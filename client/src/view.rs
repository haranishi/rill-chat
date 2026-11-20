mod icons;
mod text;

use crate::model::{Channel, Message, State, User};
pub use icons::icon;
use js_sys::Date;
use std::fmt::Write;
pub use text::{esc, render_swatches};
use wasm_bindgen::JsValue;

pub const PALETTE: [&str; 4] = ["👍", "🎉", "❤️", "👀"];

pub fn room_label(state: &State, room: &Channel) -> String {
    if room.kind == "dm" {
        return state
            .members
            .iter()
            .find(|p| Some(p.id) == room.peer_id)
            .map_or("ダイレクトメッセージ", |p| p.name.as_str())
            .to_owned();
    }
    room.name.clone()
}

pub fn avatar(person: Option<&User>, size: &str) -> String {
    let initial = person
        .map_or("ゲスト", |p| p.name.as_str())
        .trim()
        .chars()
        .next()
        .unwrap_or('?');
    let tone = person.map_or(6, |p| (p.id.rem_euclid(6) + 5) % 6);
    format!(
        r#"<span class="avatar tone-{tone} {size}" aria-hidden="true">{}</span>"#,
        esc(&initial.to_string())
    )
}

fn date(ms: i64) -> Date {
    Date::new(&JsValue::from_f64(ms as f64))
}

pub fn day_label(ms: i64) -> String {
    let d = date(ms);
    let day = ["日", "月", "火", "水", "木", "金", "土"][d.get_day() as usize];
    format!(
        "{}年{}月{}日({day})",
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date()
    )
}

pub fn time_label(ms: i64) -> String {
    let d = date(ms);
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}

pub fn day_divider(ms: i64) -> String {
    format!(
        r#"<div class="date-divider"><span>{}</span></div>"#,
        day_label(ms)
    )
}

fn action(attribute: &str, id: i64, label: &str, glyph: &str, pressed: Option<bool>) -> String {
    let pressed = pressed
        .map(|v| format!(r#" aria-pressed="{v}""#))
        .unwrap_or_default();
    format!(
        r#"<button type="button" class="mini-icon" data-{attribute}="{id}" aria-label="{label}" title="{label}"{pressed}>{}</button>"#,
        icon(glyph)
    )
}

pub fn post_markup(post: &Message, state: &State, in_thread: bool, parent: bool) -> String {
    let author = state.members.iter().find(|p| p.id == post.author_id);
    let name = esc(author.map_or("メンバー", |p| p.name.as_str()));
    let id = post.id;
    let parent_class = if parent { " thread-parent" } else { "" };
    let tools_class = if state.me.is_some() {
        " with-actions"
    } else {
        ""
    };
    let stamp: String = date(post.created_at).to_iso_string().into();
    let mut html = format!(
        r#"<article class="post{parent_class}{tools_class}" data-mid="{id}">{}<div class="post-content"><div class="post-byline"><strong>{name}</strong><time datetime="{stamp}" title="{}">{}</time>"#,
        avatar(author, ""),
        day_label(post.created_at),
        time_label(post.created_at)
    );
    if let Some(edited) = post.edited_at {
        let _ = write!(
            html,
            r#"<small class="post-edited" title="{} {}">編集済み</small>"#,
            day_label(edited),
            time_label(edited)
        );
    }
    if post.saved {
        let _ = write!(
            html,
            r#"<span class="saved-mark" role="img" aria-label="保存済み" title="保存済み">{}</span>"#,
            icon("bookmark")
        );
    }
    let _ = write!(
        html,
        r#"</div><div class="post-body">{}</div>"#,
        text::message_body(&post.body)
    );
    if !post.reactions.is_empty() {
        html.push_str(r#"<div class="reactions">"#);
        for r in &post.reactions {
            let mine = if r.mine { " mine" } else { "" };
            let emoji = esc(&r.emoji);
            let _ = write!(
                html,
                r#"<button class="reaction{mine}" data-react="{id}" data-emoji="{emoji}" aria-pressed="{}" aria-label="{emoji} リアクション {}件"><span>{emoji}</span>{}</button>"#,
                r.mine, r.count, r.count
            );
        }
        html.push_str("</div>");
    }
    if !in_thread && post.reply_count > 0 {
        let _ = write!(
            html,
            r#"<button class="reply-link" data-reply="{id}">{}<span>{}件の返信</span>{}</button>"#,
            icon("thread"),
            post.reply_count,
            icon("right")
        );
    }
    html.push_str(r#"</div><div class="post-tools">"#);
    html.push_str(&action(
        "emoji-for",
        id,
        "リアクションを追加",
        "smile",
        None,
    ));
    if !in_thread {
        html.push_str(&action("reply", id, "スレッドで返信", "thread", None));
    }
    if let Some(me) = &state.me {
        let label = if post.saved {
            "保存を解除"
        } else {
            "後で読むために保存"
        };
        html.push_str(&action("save", id, label, "bookmark", Some(post.saved)));
        if me.id == post.author_id {
            html.push_str(&action("edit", id, "メッセージを編集", "edit", None));
        }
    }
    html.push_str("</div></article>");
    html
}

pub fn empty_room(state: &State, room: &Channel) -> String {
    let (title, text, glyph) = if room.kind == "dm" {
        (
            format!("{}さんへの最初のメッセージ", room_label(state, room)),
            "この会話を読めるのは二人だけです。",
            "lock",
        )
    } else {
        (
            format!("#{} で会話を始めましょう", room.name),
            "下の入力欄から、チームに最初のひとことをどうぞ。",
            "hash",
        )
    };
    format!(
        r#"<div class="empty-room"><span class="empty-glyph">{}</span><h2>{}</h2><p>{text}</p></div>"#,
        icon(glyph),
        esc(&title)
    )
}

pub fn composer_markup(scope: &str, label: &str, sample: bool) -> String {
    let placeholder = if sample {
        "ログインしてメッセージを送る".into()
    } else if scope == "thread" {
        "スレッドに返信…".into()
    } else {
        format!("{label} にメッセージ")
    };
    let (extra, readonly, hint, send_label) = if sample {
        (
            " preview-composer",
            " readonly",
            format!("{}閲覧用デモ", icon("book")),
            "ログインして投稿",
        )
    } else {
        (
            "",
            "",
            "Enter で送信 · Shift + Enter で改行".to_owned(),
            "送信",
        )
    };
    let placeholder = esc(&placeholder);
    format!(
        r#"<form id="form-{scope}" class="composer{extra}"><textarea id="compose-{scope}" rows="1" maxlength="8000" aria-label="{placeholder}" placeholder="{placeholder}"{readonly}></textarea><div class="composer-tools"><div class="composer-left"><button type="button" data-compose-emoji="{scope}" class="mini-icon" aria-label="絵文字を挿入" title="絵文字を挿入">{}</button><span class="composer-hint">{hint}</span></div><button id="send-{scope}" class="send-button" type="submit" aria-label="{send_label}">{}</button></div></form>"#,
        icon("smile"),
        icon("send")
    )
}
