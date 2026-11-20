use super::*;
use std::collections::BTreeMap;

const PAGE: usize = 50;

#[derive(Default)]
pub(super) struct Hints {
    serial: u64,
    ids: VecDeque<i64>,
}

impl Hints {
    pub(super) fn record(&mut self, id: i64) {
        self.serial += 1;
        if self.ids.len() == 128 {
            self.ids.pop_front();
        }
        self.ids.push_back(id);
    }

    fn affected(&self, before: u64, page: &[Message]) -> Vec<i64> {
        let count = self.serial.saturating_sub(before) as usize;
        if count == 0 {
            return Vec::new();
        }
        let overflowed = count > self.ids.len();
        page.iter()
            .filter(|p| overflowed || self.ids.iter().rev().take(count).any(|id| *id == p.id))
            .map(|post| post.id)
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Read {
    Latest,
    Before(i64),
    Around(i64),
}

async fn page(
    room: i64,
    parent: Option<i64>,
    read: Read,
    sample: bool,
) -> Result<Vec<Message>, api::Error> {
    if sample {
        let mut posts: Vec<_> = demo::messages()
            .into_iter()
            .filter(|m| m.channel_id == room && m.parent_id == parent)
            .collect();
        posts.sort_unstable_by_key(|m| m.id);
        return Ok(posts);
    }
    let mut query = Vec::with_capacity(2);
    if let Some(parent) = parent {
        query.push(format!("parent_id={parent}"));
    }
    match read {
        Read::Latest => {}
        Read::Before(id) => query.push(format!("before={id}")),
        Read::Around(id) => query.push(format!("around={id}")),
    }
    let mut path = format!("/channels/{room}/messages");
    if !query.is_empty() {
        path.push('?');
        path.push_str(&query.join("&"));
    }
    let room_epoch = super::read(|app| app.room_epoch);
    // A message may be edited or saved before its first history page is in
    // the DOM. Re-read that page when an invalidation crossed its query.
    let mut attempts = 0;
    loop {
        attempts += 1;
        let marker = super::read(|app| app.hints.serial);
        let rows: Vec<Message> = api::get(&path).await?;
        let affected = super::read(|app| {
            if app.room_epoch == room_epoch {
                app.hints.affected(marker, &rows)
            } else {
                Vec::new()
            }
        });
        if affected.is_empty() {
            return Ok(rows);
        }
        // Under continuous edits, paint a bounded snapshot and converge each
        // affected row after the caller has installed it.
        if attempts == 3 {
            dom::timeout(0, move || {
                if super::read(|app| app.room_epoch == room_epoch) {
                    for id in affected {
                        spawn_local(refresh_one(id));
                    }
                }
            });
            return Ok(rows);
        }
    }
}

// New events delivered while a page is in flight survive the snapshot. Older pages
// overlap the existing list at their boundary, where the already-rendered row wins.
fn merge(older: Vec<Message>, newer: Vec<Message>) -> Vec<Message> {
    older
        .into_iter()
        .chain(newer)
        .map(|p| (p.id, p))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

struct Fence {
    newest: i64,
    revision: u64,
}

impl Fence {
    fn new(messages: &[Message], revision: u64) -> Self {
        Self {
            newest: messages.last().map_or(0, |m| m.id),
            revision,
        }
    }
}

fn snapshot(
    current: Vec<Message>,
    mut page: Vec<Message>,
    fence: Fence,
    reset: bool,
    anchor: Option<i64>,
    changes: &HashMap<i64, u64>,
) -> Vec<Message> {
    // An edit, save or reaction may finish after the server read this page.
    for incoming in &mut page {
        if let Ok(index) = current.binary_search_by_key(&incoming.id, |p| p.id) {
            let local = &current[index];
            if local.edited_at > incoming.edited_at
                || changes
                    .get(&incoming.id)
                    .is_some_and(|revision| *revision > fence.revision)
            {
                incoming.clone_from(local);
            }
        }
    }
    if !reset {
        return merge(current, page);
    }
    let live = current
        .into_iter()
        .filter(|p| anchor.is_none() && p.id > fence.newest)
        .collect();
    merge(page, live)
}

pub(super) async fn open_room(id: i64, force: bool, around: Option<i64>) {
    controls::navigation(false);
    let panel = default_panel();
    let selected = change(|app| {
        let room = app.state.channels.iter().find(|c| c.id == id)?.clone();
        if !force && around.is_none() && app.room.as_ref().is_some_and(|r| r.id == id) {
            return None;
        }
        app.room_epoch += 1;
        app.clear_thread();
        app.room = Some(room);
        app.posts.clear();
        app.unread.remove(&id);
        app.more = false;
        app.older_pending = false;
        app.anchor = around;
        app.refreshes.clear();
        app.changed_posts.clear();
        app.hints = Hints::default();
        if app.panel == Panel::Thread {
            app.panel = panel;
        }
        Some(app.state.me.as_ref().map(|me| me.id))
    });
    let Some(me) = selected else { return };
    if let (Some(me), Some(storage)) = (me, storage()) {
        let _ = storage.set_item(&format!("rill.room.{me}"), &id.to_string());
    }
    controls::clear_emoji();
    render::nav();
    render::header();
    render::panel();
    composer::mount(composer::Scope::Main);
    dom::hidden("load-older", true);
    dom::set_html("messages", r#"<div class="loading">読み込み中…</div>"#);
    if let Some(button) = dom::get("to-latest") {
        button.remove();
    }
    load_room(true, false).await;
}

pub(super) async fn load_room(initial: bool, reset: bool) {
    let context = change(|app| {
        let id = app.room.as_ref()?.id;
        app.room_request += 1;
        Some((
            id,
            app.room_epoch,
            app.room_request,
            app.anchor,
            app.sample,
            Fence::new(&app.posts, app.revision),
        ))
    });
    let Some((id, epoch, request, anchor, sample, fence)) = context else {
        return;
    };
    let Some(timeline) = dom::get("timeline") else {
        return;
    };
    let result = page(id, None, anchor.map_or(Read::Latest, Read::Around), sample).await;
    if !read(|app| app.room_epoch == epoch && app.room_request == request) {
        return;
    }
    match result {
        Ok(page) => {
            let length = page.len();
            let first = page.first().map(|m| m.id);
            // Read the scroll position after the request so scrolling while waiting
            // doesn't move the reader back to where the request started.
            let top = timeline.scroll_top();
            let near = near_bottom(&timeline);
            change(|app| {
                app.posts = snapshot(
                    std::mem::take(&mut app.posts),
                    page,
                    fence,
                    reset,
                    anchor,
                    &app.changed_posts,
                );
                if reset || initial || first == app.posts.first().map(|m| m.id) {
                    app.more = !sample && (length == PAGE || anchor.is_some() && length > 0);
                }
            });
            render::timeline(!sample && anchor.is_none() && (initial || near));
            if !initial && !near {
                timeline.set_scroll_top(top);
            }
            render::info();
        }
        Err(error) if error.status == 401 => ui_error(error),
        Err(error) if read(|app| app.posts.is_empty()) => {
            render::load_failure("messages", &error.message, "retry-room");
        }
        Err(error) => notify(&error.message),
    }
}

pub(super) async fn older_posts() {
    let context = change(|app| {
        if !app.more || app.older_pending || app.sample {
            return None;
        }
        let id = app.room.as_ref()?.id;
        let first = app.posts.first()?.id;
        app.older_pending = true;
        Some((id, first, app.room_epoch))
    });
    let Some((id, first, epoch)) = context else {
        return;
    };
    let result = page(id, None, Read::Before(first), false).await;
    if read(|app| app.room_epoch != epoch) {
        return;
    }
    let same_start = change(|app| {
        app.older_pending = false;
        app.posts.first().is_some_and(|row| row.id == first)
    });
    if !same_start {
        return;
    }
    match result {
        Ok(rows) => {
            let Some(timeline) = dom::get("timeline") else {
                return;
            };
            let previous_height = timeline.scroll_height();
            let top = timeline.scroll_top();
            change(|app| {
                app.more = rows.len() == PAGE;
                app.posts = merge(rows, std::mem::take(&mut app.posts));
            });
            render::timeline(false);
            timeline.set_scroll_top(top + timeline.scroll_height() - previous_height);
        }
        Err(error) => ui_error(error),
    }
}

pub(super) async fn refresh_one(id: i64) {
    let context = change(|app| {
        let post = app.message(id)?;
        let (channel, parent) = (post.channel_id, post.parent_id);
        let seq = app.refreshes.entry(id).or_default();
        *seq += 1;
        Some((channel, parent, *seq, app.room_epoch, app.sample))
    });
    let Some((channel, parent, seq, epoch, sample)) = context else {
        return;
    };
    loop {
        let began = message_request(id);
        let result = page(channel, parent, Read::Around(id), sample).await;
        if !read(|app| {
            app.room_epoch == epoch
                && app.refreshes.get(&id) == Some(&seq)
                && app.message(id).is_some()
        }) {
            return;
        }
        match result {
            Ok(rows) => {
                if message_request(id).revision != began.revision {
                    // A write completed after this read began. Fetch the current
                    // viewer projection again so the shared row converges.
                    continue;
                }
                if let Some(post) = rows.into_iter().find(|p| p.id == id) {
                    update_message(post);
                }
            }
            Err(error) if error.status == 401 => ui_error(error),
            Err(_) => {} // A later stream handshake will reconcile failed invalidations.
        }
        return;
    }
}

pub(super) async fn open_thread(id: i64, around: Option<i64>) {
    let context = change(|app| {
        let room = app.room.as_ref()?.id;
        // Reserve this selection before its parent lookup so rapidly switching
        // threads cannot reactivate an earlier, slower selection.
        app.thread_epoch += 1;
        Some((
            room,
            app.room_epoch,
            app.thread_epoch,
            app.posts.iter().find(|p| p.id == id).cloned(),
            app.sample,
        ))
    });
    let Some((room, epoch, request, mut parent, sample)) = context else {
        return;
    };
    if parent.is_none() {
        let result = page(room, None, Read::Around(id), sample).await;
        if !read(|app| app.room_epoch == epoch && app.thread_epoch == request) {
            return;
        }
        match result {
            Ok(posts) => parent = posts.into_iter().find(|p| p.id == id),
            Err(error) => return ui_error(error),
        }
    }
    let Some(parent) = parent else {
        return notify("このメッセージを開けません。");
    };
    change(|app| {
        app.panel = Panel::Thread;
        app.thread = Some(parent);
        app.replies.clear();
        app.more_replies = false;
        app.older_replies_pending = false;
        app.reply_anchor = around;
    });
    render::panel();
    if !wide() {
        dom::focus("panel-close");
    }
    load_replies(false, false).await;
}

pub(super) async fn load_replies(older: bool, reset: bool) {
    let context = change(|app| {
        let room = app.room.as_ref()?.id;
        let parent = app.thread.as_ref()?.id;
        let query = if older {
            if !app.more_replies || app.older_replies_pending {
                return None;
            }
            let first = app.replies.first()?.id;
            app.older_replies_pending = true;
            Read::Before(first)
        } else {
            app.thread_request += 1;
            app.reply_anchor.map_or(Read::Latest, Read::Around)
        };
        Some((
            room,
            parent,
            app.thread_epoch,
            app.thread_request,
            query,
            app.reply_anchor,
            app.sample,
            Fence::new(&app.replies, app.revision),
        ))
    });
    let Some((room, parent, epoch, request, query, anchor, sample, fence)) = context else {
        return;
    };
    let result = page(room, Some(parent), query, sample).await;
    if !read(|app| app.thread_epoch == epoch) {
        return;
    }
    if let Read::Before(first) = query {
        let same_start = change(|app| {
            app.older_replies_pending = false;
            app.replies.first().is_some_and(|row| row.id == first)
        });
        if !same_start {
            return;
        }
    } else if !read(|app| app.thread_request == request) {
        return;
    }
    let Some(scroller) = dom::get("thread-scroll") else {
        return;
    };
    match result {
        Ok(rows) => {
            let height = scroller.scroll_height();
            let top = scroller.scroll_top();
            let near = near_bottom(&scroller);
            change(|app| {
                let len = rows.len();
                let first = rows.first().map(|p| p.id);
                let current = std::mem::take(&mut app.replies);
                app.replies = if older {
                    merge(rows, current)
                } else {
                    snapshot(current, rows, fence, reset, anchor, &app.changed_posts)
                };
                if reset || older || first == app.replies.first().map(|p| p.id) {
                    app.more_replies =
                        !sample && (len == PAGE || !older && anchor.is_some() && len > 0);
                }
            });
            render::replies();
            if older {
                scroller.set_scroll_top(top + scroller.scroll_height() - height);
            } else if reset && near && anchor.is_none() {
                scroll_bottom(&scroller);
            }
        }
        Err(error) if error.status == 401 => ui_error(error),
        Err(error) if read(|app| app.replies.is_empty()) => {
            render::load_failure("thread-posts", &error.message, "retry-thread");
        }
        Err(error) => notify(&error.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: i64) -> Message {
        Message {
            id,
            ..Default::default()
        }
    }

    #[test]
    fn an_edit_during_an_unpainted_history_request_invalidates_only_its_page() {
        let mut hints = Hints::default();
        hints.record(1);
        let marker = hints.serial;
        hints.record(8);
        hints.record(43);
        assert_eq!(hints.affected(marker, &[msg(1), msg(8)]), vec![8]);
        assert!(hints.affected(marker, &[msg(2), msg(3)]).is_empty());
        for id in 44..=180 {
            hints.record(id);
        }
        assert_eq!(hints.affected(marker, &[msg(2), msg(3)]), vec![2, 3]);
    }

    #[test]
    fn reconnect_keeps_the_contiguous_snapshot_and_in_flight_delivery() {
        let current = vec![msg(1), msg(2), msg(80)];
        let fence = Fence::new(&current[..2], 0);
        let page: Vec<_> = (30..=79).map(msg).collect();
        let result = snapshot(current, page, fence, true, None, &HashMap::new());
        assert_eq!(
            result.iter().map(|p| p.id).collect::<Vec<_>>(),
            (30..=80).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_late_history_response_preserves_a_completed_private_save_and_edit() {
        let mut saved = msg(4);
        saved.saved = true;
        let mut edited = msg(5);
        edited.body = "new wording".into();
        edited.edited_at = Some(100);
        let fence = Fence {
            newest: 5,
            revision: 7,
        };
        let current = vec![saved, edited];
        let result = snapshot(
            current,
            vec![msg(4), msg(5)],
            fence,
            true,
            Some(5),
            &HashMap::from([(4, 8)]),
        );
        assert!(result[0].saved);
        assert_eq!(result[1].body, "new wording");
    }
}
