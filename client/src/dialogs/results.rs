use super::{auth, close, current, disabled, generation, show};
use crate::{
    api, demo, dom,
    model::Message,
    runtime,
    view::{day_label, esc, icon, room_label},
};
use std::cell::{Cell, RefCell};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlElement;

thread_local! {
    static HITS: RefCell<Vec<Message>> = const { RefCell::new(Vec::new()) };
    static REQUEST: Cell<u64> = const { Cell::new(0) };
    static SAVED_PAGES: Cell<usize> = const { Cell::new(1) };
    static CURSOR: Cell<Option<i64>> = const { Cell::new(None) };
}
const PAGE_SIZE: usize = 50;
const SEARCH_PROMPT: &str = r#"<p class="search-empty">投稿の言葉を入力して、会話を探せます。</p>"#;

fn next_request() -> u64 {
    let next = REQUEST.get().wrapping_add(1);
    REQUEST.set(next);
    next
}
pub(super) fn clear() {
    HITS.with_borrow_mut(Vec::clear);
    next_request();
    SAVED_PAGES.set(1);
    CURSOR.set(None);
}
pub(super) fn hit(id: i64) -> Option<Message> {
    HITS.with_borrow(|hits| hits.iter().find(|m| m.id == id).cloned())
}
pub(super) fn jump(id: i64) {
    if let Some(post) = hit(id) {
        close();
        spawn_local(runtime::jump(post));
    }
}

fn rows(posts: &[Message], saved: bool) -> String {
    let state = runtime::state();
    let mut html = String::new();
    for post in posts {
        let room = state.channels.iter().find(|r| r.id == post.channel_id);
        let glyph = if room.is_some_and(|r| r.kind == "dm") {
            "lock"
        } else {
            "hash"
        };
        let name = room.map_or_else(String::new, |r| room_label(&state, r));
        let author = state
            .members
            .iter()
            .find(|p| p.id == post.author_id)
            .map_or("", |p| p.name.as_str());
        let thread = if post.parent_id.is_some() {
            " · スレッド"
        } else {
            ""
        };
        let id = post.id;
        if saved {
            html.push_str(r#"<div class="saved-row">"#);
        }
        html.push_str(&format!(r#"<button class="search-result" data-result="{id}"><span class="result-meta">{}{} · {} · {}{thread}</span><span>{}</span></button>"#, icon(glyph), esc(&name), esc(author), day_label(post.created_at), esc(&post.body)));
        if saved {
            html.push_str(&format!(r#"<button class="mini-icon" data-save="{id}" aria-pressed="true" aria-label="保存を解除" title="保存を解除">{}</button></div>"#,icon("bookmark")));
        }
    }
    html
}

pub(super) fn search() {
    show(
        "ワークスペースを検索",
        &format!(
            r#"<label class="search-box">{}<input id="query" type="search" aria-label="メッセージを検索" placeholder="メッセージを検索" autocomplete="off" maxlength="128"></label><div id="results" aria-live="polite">{SEARCH_PROMPT}</div>"#,
            icon("search")
        ),
        "search-dialog",
    );
    dom::focus("query");
}
pub(super) fn query(value: String) {
    let text = value.trim().to_owned();
    let id = generation();
    let seq = next_request();
    HITS.with_borrow_mut(Vec::clear);
    if text.is_empty() {
        return dom::set_html("results", SEARCH_PROMPT);
    }
    dom::set_html("results", r#"<p class="search-empty">検索中…</p>"#);
    dom::timeout(140, move || {
        if !current(id) || REQUEST.get() != seq {
            return;
        }
        spawn_local(async move {
            let found = if runtime::sample() {
                let text = text.to_lowercase();
                let mut posts: Vec<_> = demo::messages()
                    .into_iter()
                    .filter(|m| m.body.to_lowercase().contains(&text))
                    .collect();
                posts.sort_by_key(|m| std::cmp::Reverse(m.id));
                Ok(posts)
            } else {
                let query: String = js_sys::encode_uri_component(&text).into();
                api::get::<Vec<Message>>(&format!("/search?q={query}")).await
            };
            if !current(id) || REQUEST.get() != seq {
                return;
            }
            match found {
                Ok(posts) => {
                    let html = if posts.is_empty() {
                        r#"<p class="search-empty">一致するメッセージがありません。</p>"#.to_owned()
                    } else {
                        rows(&posts, false)
                    };
                    dom::set_html("results", &html);
                    HITS.with_borrow_mut(|hits| *hits = posts);
                }
                Err(err) => dom::set_html(
                    "results",
                    &format!(
                        r#"<p class="form-error" role="alert">{}</p>"#,
                        esc(&err.message)
                    ),
                ),
            }
        });
    });
}

pub(super) fn saved() {
    if runtime::sample() {
        return auth("login");
    }
    show(
        "あとで読む",
        r#"<div id="saved-results"></div><p class="muted" id="saved-status" role="status"></p><button id="saved-retry" class="text-button" hidden>再読み込み</button><button id="saved-older" class="text-button" hidden>さらに表示</button>"#,
        "search-dialog",
    );
    refresh_if_open();
}
pub(super) fn refresh_if_open() {
    if dom::get("saved-results").is_some() && current(generation()) {
        fetch_saved(true);
    }
}
pub(super) fn load_saved() {
    fetch_saved(false);
}

fn fetch_saved(replace: bool) {
    let id = generation();
    let seq = next_request();
    let start = if replace { None } else { CURSOR.get() };
    let pages = if replace { SAVED_PAGES.get() } else { 1 };
    if !replace {
        SAVED_PAGES.set(SAVED_PAGES.get() + 1);
    }
    if let Some(button) = dom::get("saved-older") {
        disabled(&button, true);
    }
    dom::hidden("saved-retry", true);
    dom::set_text("saved-status", "読み込み中…");
    spawn_local(async move {
        let mut cursor = start;
        let mut part = Vec::new();
        let mut more = false;
        for _ in 0..pages {
            let path = cursor.map_or_else(
                || "/saved".to_owned(),
                |before| format!("/saved?before={before}"),
            );
            let result = api::get::<Vec<Message>>(&path).await;
            if !current(id) || REQUEST.get() != seq {
                return;
            }
            let page = match result {
                Ok(page) => page,
                Err(err) => {
                    dom::set_text("saved-status", &err.message);
                    dom::hidden("saved-retry", false);
                    dom::hidden("saved-older", true);
                    return;
                }
            };
            more = page.len() == PAGE_SIZE;
            if let Some(last) = page.last() {
                cursor = Some(last.id);
            }
            part.extend(page);
            if !more {
                break;
            }
        }
        CURSOR.set(cursor);
        HITS.with_borrow_mut(|hits| {
            if replace {
                *hits = part;
            } else {
                hits.extend(part);
            }
        });
        paint_saved();
        if let Some(button) = dom::get("saved-older") {
            disabled(&button, false);
        }
        dom::hidden("saved-older", !more);
    });
}
fn paint_saved() {
    let focused = dom::document().active_element().and_then(|el| {
        el.closest("#saved-results").ok().flatten()?;
        ["data-result", "data-save"]
            .into_iter()
            .find_map(|name| super::attr_id(&el, name).map(|id| (name, id)))
    });
    HITS.with_borrow(|hits| {
        dom::set_html("saved-results", &rows(hits, true));
        dom::set_text(
            "saved-status",
            if hits.is_empty() {
                "保存したメッセージはまだありません。会話のしおりボタンで保存できます。"
            } else {
                ""
            },
        );
    });
    if let Some((attr, id)) = focused {
        let doc = dom::document();
        let next = doc
            .query_selector(&format!(r#"#saved-results [{attr}="{id}"]"#))
            .ok()
            .flatten()
            .or_else(|| doc.query_selector("#saved-results button").ok().flatten())
            .or_else(|| doc.query_selector("#dialog [data-close]").ok().flatten());
        if let Some(el) = next.and_then(|v| v.dyn_into::<HtmlElement>().ok()) {
            let _ = el.focus();
        }
    }
}
pub(super) fn remove_saved(id: i64) {
    if dom::get("saved-results").is_some() {
        HITS.with_borrow_mut(|hits| hits.retain(|m| m.id != id));
        paint_saved();
        refresh_if_open();
    }
}
