use crate::{
    api, demo, dialogs, dom,
    model::{Channel, Message, State},
    view,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, Storage};

mod composer;
mod controls;
mod events;
mod history;
mod render;
mod rotation;
pub use rotation::Rotation;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Info,
    Thread,
    Closed,
}

struct App {
    state: State,
    sample: bool,
    room: Option<Channel>,
    posts: Vec<Message>,
    thread: Option<Message>,
    replies: Vec<Message>,
    panel: Panel,
    anchor: Option<i64>,
    reply_anchor: Option<i64>,
    more: bool,
    more_replies: bool,
    older_pending: bool,
    older_replies_pending: bool,
    auth_epoch: u64,
    room_epoch: u64,
    thread_epoch: u64,
    room_request: u64,
    thread_request: u64,
    meta_request: u64,
    meta_timer: u64,
    toast_version: u64,
    live: bool,
    stream_epoch: u64,
    password_pending: bool,
    stream_verified: bool,
    handshake: u64,
    waiting_events: VecDeque<Message>,
    reconnect_attempt: u8,
    retry_scheduled: bool,
    checking_cookie: Option<u64>,
    unread: HashSet<i64>,
    seen: HashSet<i64>,
    seen_order: VecDeque<i64>,
    refreshes: HashMap<i64, u64>,
    revision: u64,
    changed_posts: HashMap<i64, u64>,
    hints: history::Hints,
    reactions: HashSet<(i64, String)>,
    drafts: HashMap<String, composer::Draft>,
    sending: HashSet<String>,
}

impl App {
    fn new() -> Self {
        Self {
            state: demo::state(),
            sample: true,
            room: None,
            posts: Vec::new(),
            thread: None,
            replies: Vec::new(),
            panel: default_panel(),
            anchor: None,
            reply_anchor: None,
            more: false,
            more_replies: false,
            older_pending: false,
            older_replies_pending: false,
            auth_epoch: 0,
            room_epoch: 0,
            thread_epoch: 0,
            room_request: 0,
            thread_request: 0,
            meta_request: 0,
            meta_timer: 0,
            toast_version: 0,
            live: false,
            stream_epoch: 0,
            password_pending: false,
            stream_verified: false,
            handshake: 0,
            waiting_events: VecDeque::new(),
            reconnect_attempt: 0,
            retry_scheduled: false,
            checking_cookie: None,
            unread: HashSet::new(),
            seen: HashSet::new(),
            seen_order: VecDeque::new(),
            refreshes: HashMap::new(),
            revision: 0,
            changed_posts: HashMap::new(),
            hints: history::Hints::default(),
            reactions: HashSet::new(),
            drafts: HashMap::new(),
            sending: HashSet::new(),
        }
    }

    fn message(&self, id: i64) -> Option<&Message> {
        self.posts
            .iter()
            .chain(self.replies.iter())
            .chain(self.thread.iter())
            .find(|post| post.id == id)
    }

    fn mark_seen(&mut self, id: i64) -> bool {
        if !self.seen.insert(id) {
            return false;
        }
        self.seen_order.push_back(id);
        if self.seen_order.len() > 1200
            && let Some(old) = self.seen_order.pop_front()
        {
            self.seen.remove(&old);
        }
        true
    }

    fn clear_thread(&mut self) {
        self.thread_epoch += 1;
        self.thread = None;
        self.replies.clear();
        self.more_replies = false;
        self.older_replies_pending = false;
        self.reply_anchor = None;
    }
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App::new());
}

fn read<R>(f: impl FnOnce(&App) -> R) -> R {
    APP.with_borrow(f)
}
fn change<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with_borrow_mut(f)
}
fn media(query: &str) -> bool {
    dom::window()
        .match_media(query)
        .ok()
        .flatten()
        .is_some_and(|media| media.matches())
}
fn wide() -> bool {
    media("(min-width: 1180px)")
}
fn mobile() -> bool {
    media("(max-width: 700px)")
}
fn default_panel() -> Panel {
    if wide() { Panel::Info } else { Panel::Closed }
}
fn storage() -> Option<Storage> {
    dom::window().session_storage().ok().flatten()
}
fn attr(id: &str, name: &str, value: &str) {
    if let Some(el) = dom::get(id) {
        let _ = el.set_attribute(name, value);
    }
}
fn scroll_bottom(el: &Element) {
    el.set_scroll_top(el.scroll_height());
}
fn near_bottom(el: &Element) -> bool {
    el.scroll_height() - el.client_height() - el.scroll_top() < 100
}
fn focus_element(el: &Element) {
    if let Some(el) = el.dyn_ref::<HtmlElement>() {
        let _ = el.focus();
    }
}
fn ui_error(error: api::Error) {
    if error.status == 401 && !sample() {
        events::check_session();
    } else {
        notify(&error.message);
    }
}

pub fn state() -> State {
    read(|app| app.state.clone())
}
pub fn sample() -> bool {
    read(|app| app.sample)
}
pub fn message_by_id(id: i64) -> Option<Message> {
    read(|app| app.message(id).cloned())
}
pub fn notify(text: &str) {
    let version = change(|app| {
        app.toast_version += 1;
        app.toast_version
    });
    dom::set_text("toast", text);
    dom::hidden("toast", false);
    dom::timeout(4200, move || {
        if read(|app| app.toast_version == version) {
            dom::hidden("toast", true);
        }
    });
}

fn reset_session(state: State, sample: bool) {
    rotation::reset();
    events::stop_stream();
    controls::clear_emoji();
    let panel = default_panel();
    change(|app| {
        app.auth_epoch += 1;
        app.room_epoch += 1;
        app.meta_request += 1;
        app.meta_timer += 1;
        app.clear_thread();
        app.state = state;
        app.sample = sample;
        app.room = None;
        app.posts.clear();
        app.anchor = None;
        app.more = false;
        app.older_pending = false;
        app.panel = panel;
        app.live = false;
        app.password_pending = false;
        app.reconnect_attempt = 0;
        app.seen.clear();
        app.seen_order.clear();
        app.unread.clear();
        app.sending.clear();
        app.reactions.clear();
        app.refreshes.clear();
        app.changed_posts.clear();
        app.hints = history::Hints::default();
    });
    controls::navigation(false);
}

pub async fn enter() -> Result<(), api::Error> {
    let epoch = read(|app| app.auth_epoch);
    let fresh: State = api::get("/state").await?;
    if read(|app| app.auth_epoch == epoch) {
        activate(fresh).await;
    }
    Ok(())
}

async fn activate(fresh: State) {
    let last = fresh.me.as_ref().and_then(|me| {
        storage()?
            .get_item(&format!("rill.room.{}", me.id))
            .ok()??
            .parse::<i64>()
            .ok()
    });
    let first = fresh
        .channels
        .iter()
        .find(|room| Some(room.id) == last)
        .or_else(|| {
            fresh
                .channels
                .iter()
                .find(|room| room.kind == "channel" && room.name == "general")
        })
        .or_else(|| fresh.channels.first())
        .map(|room| room.id);
    reset_session(fresh, false);
    let epoch = read(|app| app.auth_epoch);
    if let Some(id) = first {
        history::open_room(id, true, None).await;
    } else {
        render::nav();
    }
    if read(|app| app.auth_epoch == epoch) {
        events::connect(epoch);
    }
}

fn clear_private_view() {
    // Clear private DOM synchronously, including the draft and an open details panel.
    for id in [
        "messages",
        "thread-posts",
        "context-panel",
        "composer-region",
        "room-topic",
        "room-name",
    ] {
        dom::set_html(id, "");
    }
    dom::hidden("load-older", true);
    if let Some(latest) = dom::get("to-latest") {
        latest.remove();
    }
    if let Some(dialog) =
        dom::get("dialog").and_then(|el| el.dyn_into::<web_sys::HtmlDialogElement>().ok())
        && dialog.open()
    {
        dialog.close();
    }
    dom::set_html("dialog-body", "");
}

async fn change_identity(fresh: State) {
    composer::clear_user_drafts();
    clear_private_view();
    activate(fresh).await;
    notify("別のタブで選んだアカウントに切り替えました。");
}

pub fn signed_out() {
    composer::clear_user_drafts();
    let preview = demo::state();
    let first = preview.channels.first().map(|room| room.id);
    reset_session(preview, true);
    clear_private_view();
    render::nav();
    if let Some(id) = first {
        spawn_local(async move {
            if sample() {
                history::open_room(id, true, None).await;
            }
        });
    }
}

pub async fn add_room(room: Channel) {
    let id = room.id;
    change(|app| {
        if !app.state.channels.iter().any(|r| r.id == id) {
            app.state.channels.push(room);
        }
    });
    history::open_room(id, false, None).await;
}

pub async fn jump(post: Message) {
    let epoch = read(|app| app.auth_epoch);
    if !sample()
        && !read(|app| {
            app.state
                .channels
                .iter()
                .any(|room| room.id == post.channel_id)
        })
        && let Err(err) = events::sync_meta().await
    {
        if read(|app| app.auth_epoch == epoch) {
            ui_error(err);
        }
        return;
    }
    if read(|app| {
        app.auth_epoch != epoch
            || !app
                .state
                .channels
                .iter()
                .any(|room| room.id == post.channel_id)
    }) {
        return;
    }
    let root = post.parent_id.unwrap_or(post.id);
    history::open_room(post.channel_id, true, Some(root)).await;
    if !read(|app| {
        app.auth_epoch == epoch
            && app.room.as_ref().is_some_and(|r| r.id == post.channel_id)
            && app.anchor == Some(root)
    }) {
        return;
    }
    if post.parent_id.is_some() {
        history::open_thread(root, Some(post.id)).await;
    }
    let scope = if post.parent_id.is_some() {
        "context-panel"
    } else {
        "messages"
    };
    let target = dom::get(scope).and_then(|el| {
        el.query_selector(&format!("[data-mid='{}']", post.id))
            .ok()
            .flatten()
    });
    if let Some(target) = target {
        let options = web_sys::ScrollIntoViewOptions::new();
        options.set_block(web_sys::ScrollLogicalPosition::Center);
        target.scroll_into_view_with_scroll_into_view_options(&options);
        let _ = target.class_list().add_1("highlight");
        let _ = target.set_attribute("tabindex", "-1");
        focus_element(&target);
        dom::timeout(2100, move || {
            let _ = target.class_list().remove_1("highlight");
        });
    }
}

pub fn session_epoch() -> u64 {
    read(|app| app.auth_epoch)
}

#[derive(Clone, Copy)]
pub struct MessageRequest {
    id: i64,
    auth: u64,
    room: u64,
    revision: u64,
}

pub fn message_request(id: i64) -> MessageRequest {
    read(|app| MessageRequest {
        id,
        auth: app.auth_epoch,
        room: app.room_epoch,
        revision: app.changed_posts.get(&id).copied().unwrap_or_default(),
    })
}

// A valid response can cross a room switch or a later completed mutation.
pub fn update_requested(post: Message, started: MessageRequest) -> bool {
    if started.id != post.id || started.auth != session_epoch() {
        return false;
    }
    let current = message_request(post.id);
    if current.room != started.room || current.revision != started.revision {
        if message_by_id(post.id).is_some() {
            spawn_local(history::refresh_one(post.id));
        }
    } else {
        update_message(post);
    }
    true
}

pub fn update_message(post: Message) {
    let visible = change(|app| {
        if app.room.as_ref().map(|r| r.id) != Some(post.channel_id) {
            return false;
        }
        let mut visible = false;
        for row in app
            .posts
            .iter_mut()
            .chain(app.replies.iter_mut())
            .chain(app.thread.iter_mut())
        {
            if row.id == post.id && row.edited_at <= post.edited_at {
                *row = post.clone();
                visible = true;
            }
        }
        if visible {
            app.revision += 1;
            app.changed_posts.insert(post.id, app.revision);
        }
        visible
    });
    if visible {
        render::replace_post(&post);
        render::info();
    }
}

pub fn start() {
    rotation::install();
    controls::install();
    controls::navigation(false);
    let epoch = read(|app| app.auth_epoch);
    spawn_local(async move {
        if let Err(err) = enter().await
            && read(|app| app.auth_epoch == epoch)
        {
            signed_out();
            if err.status != 401 {
                notify("サーバーに接続できないため、デモを表示しています。");
            }
        }
    });
}

pub fn password_change_started() -> Rotation {
    rotation::start()
}

pub fn password_change_finished(rotation: Rotation) {
    rotation::finish(rotation);
}
