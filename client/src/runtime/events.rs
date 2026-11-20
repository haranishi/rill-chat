use super::*;
use serde::Deserialize;
use web_sys::{Event, EventSource, MessageEvent};

struct Connection {
    source: EventSource,
    _handlers: Vec<dom::Listener>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.source.close();
    }
}
thread_local! {
    static STREAM: RefCell<Option<Connection>> = const { RefCell::new(None) };
}

pub(super) fn stop_stream() {
    change(|app| {
        app.stream_epoch += 1;
        app.stream_verified = false;
        app.waiting_events.clear();
        app.retry_scheduled = false;
        app.checking_cookie = None;
    });
    STREAM.with_borrow_mut(|stream| stream.take());
}

fn current(epoch: u64, stream: u64) -> bool {
    read(|app| {
        !app.sample
            && !app.password_pending
            && app.auth_epoch == epoch
            && app.stream_epoch == stream
    })
}

fn same_identity(fresh: &State) -> bool {
    read(|app| {
        fresh.workspace.id == app.state.workspace.id
            && fresh.me.as_ref().map(|u| u.id) == app.state.me.as_ref().map(|u| u.id)
    })
}

fn closed() -> bool {
    STREAM.with_borrow(|slot| {
        slot.as_ref()
            .is_none_or(|s| s.source.ready_state() == EventSource::CLOSED)
    })
}

fn retry_stream(epoch: u64, stream: u64) {
    let delay = change(|app| {
        if app.retry_scheduled {
            return None;
        }
        app.retry_scheduled = true;
        let delay = (500_i32 << app.reconnect_attempt.min(6)).min(30_000);
        app.reconnect_attempt = app.reconnect_attempt.saturating_add(1);
        Some(delay)
    });
    if let Some(delay) = delay {
        dom::timeout(delay, move || {
            if current(epoch, stream) && read(|app| app.retry_scheduled) {
                connect(epoch);
            }
        });
    }
}

pub(super) fn check_session() {
    let (epoch, stream) = read(|app| (app.auth_epoch, app.stream_epoch));
    if !current(epoch, stream) || change(|app| app.checking_cookie.replace(stream) == Some(stream))
    {
        return;
    }
    spawn_local(async move {
        let response = api::get::<State>("/state").await;
        if !current(epoch, stream) {
            return;
        }
        change(|app| app.checking_cookie = None);
        match response {
            Err(error) if error.status == 401 => {
                if rotation::recover_pending() {
                    return;
                }
                signed_out();
                notify("セッションが終了しました。");
            }
            Ok(fresh) if !same_identity(&fresh) => change_identity(fresh).await,
            _ if closed() => retry_stream(epoch, stream),
            _ => {}
        }
    });
}

pub(super) async fn sync_meta() -> Result<(), api::Error> {
    let (epoch, seq) = change(|app| {
        app.meta_request += 1;
        (app.auth_epoch, app.meta_request)
    });
    let result = api::get::<State>("/state").await;
    if !read(|app| !app.sample && app.auth_epoch == epoch) {
        return Ok(());
    }
    let fresh = result?;
    if !same_identity(&fresh) {
        change_identity(fresh).await;
        return Ok(());
    }
    if !read(|app| app.meta_request == seq) {
        return Ok(());
    }
    let (more_members, room, first) = change(|app| {
        let more_members = app.state.members.len() != fresh.members.len();
        let current = app.room.as_ref().map(|r| r.id);
        app.room = fresh
            .channels
            .iter()
            .find(|r| Some(r.id) == current)
            .cloned();
        app.state = fresh;
        let room = app.room.as_ref().map(|r| r.id);
        let first = app.state.channels.first().map(|r| r.id);
        (more_members, room, first)
    });
    if room.is_none() {
        if let Some(id) = first {
            history::open_room(id, true, None).await;
        } else {
            change(|app| {
                app.room_epoch += 1;
                app.clear_thread();
                app.posts.clear();
            });
            for id in ["messages", "composer-region", "room-topic", "room-name"] {
                dom::set_html(id, "");
            }
            render::nav();
            render::panel();
        }
    } else {
        render::nav();
        render::header();
        if more_members {
            render::timeline(false);
            render::replies();
        }
        render::info();
    }
    Ok(())
}

fn schedule_meta() {
    let (epoch, timer) = change(|app| {
        app.meta_timer += 1;
        (app.auth_epoch, app.meta_timer)
    });
    dom::timeout(40, move || {
        if read(|app| app.auth_epoch == epoch && app.meta_timer == timer) {
            spawn_local(async move {
                if let Err(error) = sync_meta().await
                    && read(|app| app.auth_epoch == epoch)
                {
                    ui_error(error);
                }
            });
        }
    });
}

async fn reconcile(epoch: u64, stream: u64) {
    let handshake = read(|app| app.handshake);
    let active = || current(epoch, stream) && read(|app| app.handshake == handshake);
    if let Err(error) = sync_meta().await {
        if active() {
            if error.status == 401 {
                check_session();
            } else {
                render::connection(false);
                retry_stream(epoch, stream);
            }
        }
        return;
    }
    if !active() {
        return;
    }
    let waiting = change(|app| {
        app.stream_verified = true;
        app.retry_scheduled = false;
        app.reconnect_attempt = 0;
        std::mem::take(&mut app.waiting_events)
    });
    render::connection(true);
    dialogs::refresh_saved_if_open();
    for post in waiting {
        receive(post);
    }
    let room_epoch = read(|app| app.room_epoch);
    history::load_room(false, true).await;
    if !active() || !read(|app| app.room_epoch == room_epoch) {
        return;
    }
    let thread = read(|app| app.thread.as_ref().map(|p| (p.id, app.thread_epoch)));
    if let Some((id, thread_epoch)) = thread {
        history::refresh_one(id).await;
        if active() && read(|app| app.thread_epoch == thread_epoch) {
            history::load_replies(false, true).await;
        }
    }
}

#[derive(Deserialize, Default)]
struct Hint {
    channel_id: Option<i64>,
    message_id: Option<i64>,
    #[serde(default)]
    saved_changed: bool,
}

fn payload<T: serde::de::DeserializeOwned>(event: Event) -> Option<T> {
    let text = event.dyn_into::<MessageEvent>().ok()?.data().as_string()?;
    serde_json::from_str(&text).ok()
}

pub(super) fn connect(epoch: u64) {
    stop_stream();
    let stream = read(|app| app.stream_epoch);
    if !current(epoch, stream) {
        return;
    }
    let Ok(source) = EventSource::new("/api/events") else {
        render::connection(false);
        retry_stream(epoch, stream);
        return;
    };
    let mut handlers = Vec::new();
    for kind in ["ready", "resync"] {
        handlers.push(dom::listen(source.as_ref(), kind, move |_| {
            if current(epoch, stream) {
                if kind == "ready" {
                    change(|app| {
                        app.stream_verified = false;
                        app.handshake += 1;
                    });
                }
                spawn_local(reconcile(epoch, stream));
            }
        }));
    }
    handlers.push(dom::listen(source.as_ref(), "message", move |ev| {
        if current(epoch, stream)
            && let Some(post) = payload::<Message>(ev)
        {
            let deliver = change(|app| {
                if app.stream_verified {
                    return Some(post);
                }
                // A reconnect can now be using another tab's replacement cookie.
                // Wait for the authenticated /state before painting messages.
                if app.waiting_events.len() == 128 {
                    app.waiting_events.pop_front();
                }
                app.waiting_events.push_back(post);
                None
            });
            if let Some(post) = deliver {
                receive(post);
            }
        }
    }));
    handlers.push(dom::listen(source.as_ref(), "refresh", move |ev| {
        if !current(epoch, stream) || !read(|app| app.stream_verified) {
            return;
        }
        let Some(hint) = payload::<Hint>(ev) else {
            return;
        };
        if hint.saved_changed {
            dialogs::refresh_saved_if_open();
        }
        match hint.channel_id {
            None => schedule_meta(),
            Some(id) if read(|app| app.room.as_ref().is_some_and(|r| r.id == id)) => {
                if let Some(message) = hint.message_id {
                    change(|app| app.hints.record(message));
                    spawn_local(history::refresh_one(message));
                } else {
                    spawn_local(reconcile(epoch, stream));
                }
            }
            _ => {}
        }
    }));
    handlers.push(dom::listen(source.as_ref(), "error", move |_| {
        if current(epoch, stream) {
            change(|app| {
                app.stream_verified = false;
                app.handshake += 1;
                app.waiting_events.clear();
            });
            render::connection(false);
            check_session();
        }
    }));
    STREAM.with_borrow_mut(|slot| {
        *slot = Some(Connection {
            source,
            _handlers: handlers,
        })
    });
}

enum Arrival {
    Ignore,
    Unread,
    Latest,
    Append {
        previous: Option<Message>,
        in_thread: bool,
    },
    Reorder {
        in_thread: bool,
    },
}

pub(super) fn receive(post: Message) {
    let channel = post.channel_id;
    let (arrival, own, unknown, parent) = change(|app| {
        let own = app
            .state
            .me
            .as_ref()
            .is_some_and(|me| me.id == post.author_id);
        let Some(room) = app.room.as_ref().map(|room| room.id) else {
            return (Arrival::Ignore, own, false, None);
        };
        if !app.mark_seen(post.id) {
            return (Arrival::Ignore, own, false, None);
        }
        let unknown = !app.state.members.iter().any(|u| u.id == post.author_id)
            || !app.state.channels.iter().any(|r| r.id == channel);
        if channel != room {
            let arrival = if !own && app.unread.insert(channel) {
                Arrival::Unread
            } else {
                Arrival::Ignore
            };
            return (arrival, own, unknown, None);
        }
        let parent = post.parent_id.filter(|id| app.message(*id).is_some());
        let in_thread = post.parent_id.is_some();
        if in_thread && app.thread.as_ref().map(|p| p.id) != post.parent_id {
            return (Arrival::Ignore, own, unknown, parent);
        }
        if !in_thread && app.anchor.is_some() {
            return (
                if own {
                    Arrival::Latest
                } else {
                    Arrival::Ignore
                },
                own,
                unknown,
                parent,
            );
        }
        let list = if in_thread {
            &mut app.replies
        } else {
            &mut app.posts
        };
        let arrival = match list.binary_search_by_key(&post.id, |p| p.id) {
            Ok(_) => Arrival::Ignore,
            Err(index) if index == list.len() => {
                let previous = list.last().cloned();
                list.push(post.clone());
                Arrival::Append {
                    previous,
                    in_thread,
                }
            }
            Err(index) => {
                list.insert(index, post.clone());
                Arrival::Reorder { in_thread }
            }
        };
        (arrival, own, unknown, parent)
    });
    if unknown {
        schedule_meta();
    }
    if let Some(parent) = parent {
        spawn_local(history::refresh_one(parent));
    }
    let (in_thread, previous) = match arrival {
        Arrival::Ignore => return,
        Arrival::Unread => return render::nav(),
        Arrival::Latest => return spawn_local(history::open_room(channel, true, None)),
        Arrival::Append {
            previous,
            in_thread,
        } => (in_thread, Some(previous)),
        Arrival::Reorder { in_thread } => (in_thread, None),
    };
    let Some(scroller) = dom::get(if in_thread {
        "thread-scroll"
    } else {
        "timeline"
    }) else {
        return;
    };
    let scroll = own || near_bottom(&scroller);
    if let Some(previous) = previous {
        render::append(&post, previous.as_ref(), in_thread);
    } else if in_thread {
        render::replies();
    } else {
        render::timeline(false);
    }
    if scroll {
        scroll_bottom(&scroller);
    }
}
