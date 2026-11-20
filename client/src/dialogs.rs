mod account;
mod messages;
mod results;
mod sharing;

use crate::{
    api, dom, runtime,
    view::{esc, icon},
};
use serde_json::json;
use std::cell::Cell;
use std::fmt::Write;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    Element, Event, FormData, HtmlDialogElement, HtmlElement, HtmlFormElement, HtmlInputElement,
    KeyboardEvent,
};

thread_local! { static GENERATION: Cell<u64> = const { Cell::new(0) }; }
const FEEDBACK: &str = r#"<p class="form-error" role="alert" hidden></p>"#;

pub use account::auth;
pub fn members() {
    people(false);
}
pub fn refresh_saved_if_open() {
    results::refresh_if_open();
}

fn modal() -> Option<HtmlDialogElement> {
    dom::get("dialog")?.dyn_into().ok()
}
fn generation() -> u64 {
    GENERATION.get()
}
fn invalidate() {
    GENERATION.set(generation().wrapping_add(1));
    results::clear();
}
fn current(id: u64) -> bool {
    generation() == id && modal().is_some_and(|el| el.open())
}

fn show(title: &str, html: &str, class_name: &str) -> u64 {
    invalidate();
    if let Some(dialog) = modal() {
        dialog.set_class_name(class_name);
        dom::set_html(
            "dialog-body",
            &format!(
                r#"<div class="dialog-heading"><h2 id="dialog-title">{}</h2><button type="button" class="icon-button" data-close aria-label="閉じる">{}</button></div>{html}"#,
                esc(title),
                icon("x")
            ),
        );
        let _ = dialog.set_attribute("aria-labelledby", "dialog-title");
        if !dialog.open() {
            let _ = dialog.show_modal();
        }
    }
    generation()
}

fn close() {
    if let Some(dialog) = modal() {
        dialog.close();
    }
    invalidate();
}

fn close_if_current(id: u64) {
    if current(id) {
        close();
    }
}

fn field(label: &str, name: &str, attrs: &str) -> String {
    format!(
        r#"<label class="field"><span>{label}</span><input name="{name}" {attrs} required></label>"#
    )
}
fn primary(label: &str) -> String {
    format!(
        r#"<button class="primary" type="submit">{label}{}</button>"#,
        icon("arrow")
    )
}
fn val(data: &FormData, name: &str) -> String {
    data.get(name).as_string().unwrap_or_default()
}
fn focus_first() {
    if let Some(input) = dom::get("dialog-body")
        .and_then(|el| el.query_selector("input").ok().flatten())
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    {
        let _ = input.focus();
    }
}
fn error(message: &str) {
    if let Some(el) =
        dom::get("dialog-body").and_then(|el| el.query_selector(".form-error").ok().flatten())
    {
        el.set_text_content(Some(message));
        let _ = el.remove_attribute("hidden");
    }
}
fn clear_error() {
    if let Some(el) =
        dom::get("dialog-body").and_then(|el| el.query_selector(".form-error").ok().flatten())
    {
        let _ = el.set_attribute("hidden", "");
        el.set_text_content(None);
    }
}
fn disabled(el: &Element, on: bool) {
    if on {
        let _ = el.set_attribute("disabled", "");
    } else {
        let _ = el.remove_attribute("disabled");
    }
}

fn channel() {
    if runtime::sample() {
        return auth("create");
    }
    let name = field(
        "チャンネル名",
        "name",
        r#"maxlength="48" placeholder="例）design""#,
    );
    show(
        "チャンネルを作成",
        &format!(
            r#"<form id="channel-form">{name}<label class="field"><span>トピック <small>任意</small></span><input name="topic" maxlength="180" placeholder="何について話す場所ですか？"></label>{FEEDBACK}{}</form>"#,
            primary("作成する")
        ),
        "",
    );
    focus_first();
}

fn people(for_dm: bool) {
    let state = runtime::state();
    let mut rows = String::new();
    for user in &state.members {
        if for_dm && state.me.as_ref().is_some_and(|me| me.id == user.id) {
            continue;
        }
        let (tag, attr, trailing) = if for_dm {
            (
                "button",
                format!(r#" data-person="{}""#, user.id),
                icon("arrow"),
            )
        } else {
            (
                "div",
                String::new(),
                if state.workspace.owner_id == user.id {
                    r#"<span class="role">オーナー</span>"#.into()
                } else {
                    String::new()
                },
            )
        };
        let _ = write!(
            rows,
            r#"<{tag}{attr} class="person-row">{}<span class="person-text"><b>{}</b><small>@{}</small></span>{trailing}</{tag}>"#,
            crate::view::avatar(Some(user), ""),
            esc(&user.name),
            esc(&user.handle)
        );
    }
    if rows.is_empty() {
        rows.push_str(r#"<p class="muted">メンバーを招待すると、ここから DM を始められます。</p>"#);
    }
    show(
        if for_dm {
            "DM の相手を選ぶ"
        } else {
            "ワークスペースのメンバー"
        },
        &format!(r#"<div class="people-list">{rows}</div>{FEEDBACK}"#),
        "",
    );
}

fn start_dm(button: Element, user_id: i64) {
    if runtime::sample() {
        return auth("join");
    }
    let id = generation();
    disabled(&button, true);
    spawn_local(async move {
        match api::post("/dms", &json!({"user_id":user_id})).await {
            Ok(room) if current(id) => {
                runtime::add_room(room).await;
                close_if_current(id);
            }
            Err(err) if current(id) => error(&err.message),
            _ => {}
        }
        disabled(&button, false);
    });
}

fn submit(event: Event) {
    let Some(form) = event
        .target()
        .and_then(|el| el.dyn_into::<HtmlFormElement>().ok())
    else {
        return;
    };
    event.prevent_default();
    let Ok(Some(button)) = form.query_selector("[type=submit]") else {
        return;
    };
    if button.has_attribute("disabled") {
        return;
    }
    let Ok(data) = FormData::new_with_form(&form) else {
        return;
    };
    let id = generation();
    clear_error();
    disabled(&button, true);
    spawn_local(async move {
        let result = match form.id().as_str() {
            "auth-form" => account::authenticate(&form, &data, id).await,
            "logout-form" => account::logout(id).await,
            "password-form" => account::change_password(&data, id).await,
            "channel-form" => match api::post(
                "/channels",
                &json!({"name":val(&data,"name"),"topic":val(&data,"topic")}),
            )
            .await
            {
                Ok(room) => {
                    if current(id) {
                        runtime::add_room(room).await;
                        close_if_current(id);
                    }
                    Ok(())
                }
                Err(err) => Err(err),
            },
            "edit-form" => messages::submit_edit(&form, &data, id).await,
            "invite-form" => sharing::issue_invite(id).await,
            _ => Ok(()),
        };
        if current(id) {
            if let Err(err) = result {
                error(&err.message);
            }
            disabled(&button, form.has_attribute("data-stale"));
        }
    });
}

fn clicked(event: Event) {
    let Some(target) = event.target().and_then(|v| v.dyn_into::<Element>().ok()) else {
        return;
    };
    if target.id() == "dialog" {
        return close();
    }
    let Ok(Some(el)) = target.closest("button") else {
        return;
    };
    match el.id().as_str() {
        "search-open" => results::search(),
        "team-action" if runtime::sample() => auth("create"),
        "team-action" => sharing::invite(),
        "account" => account::profile(),
        "change-password" => account::password(),
        "new-channel" => channel(),
        "new-dm" => people(true),
        "product" => sharing::product(),
        "saved-open" => results::saved(),
        "saved-older" => results::load_saved(),
        "saved-retry" => results::refresh_if_open(),
        "review-edit" => messages::review_edit(),
        "retry-edit" => messages::retry_latest(),
        "copy-invite" => {
            if let Some(input) =
                dom::get("invite-code").and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
            {
                sharing::copy(input.value());
            }
        }
        "copy-project" => sharing::copy(sharing::SOURCE.into()),
        "native-share" => sharing::share(),
        _ => {
            if el.has_attribute("data-close") {
                close();
            } else if el.has_attribute("data-first-dm") {
                people(true);
            } else if el.has_attribute("data-members") {
                members();
            } else if let Some(mode) = el.get_attribute("data-auth") {
                auth(&mode);
            } else if let Some(id) = attr_id(&el, "data-person") {
                start_dm(el, id);
            } else if let Some(id) = attr_id(&el, "data-save") {
                messages::toggle_save(el, id);
            } else if let Some(id) = attr_id(&el, "data-edit") {
                messages::edit(id);
            } else if let Some(id) = attr_id(&el, "data-result") {
                results::jump(id);
            }
        }
    }
}

fn attr_id(el: &Element, name: &str) -> Option<i64> {
    el.get_attribute(name)?.parse().ok()
}

pub fn start() {
    let doc = dom::document();
    dom::on(&doc, "click", clicked);
    if let Some(dialog) = modal() {
        dom::on(&dialog, "submit", submit);
        dom::on(&dialog, "input", |event| {
            if let Some(input) = event
                .target()
                .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
                && input.id() == "query"
            {
                results::query(input.value());
            }
        });
        dom::on(&dialog, "close", |_| {
            if !modal().is_some_and(|v| v.open()) {
                invalidate();
                dom::set_html("dialog-body", "");
            }
        });
    }
    dom::on(&doc, "keydown", |event| {
        let Some(key) = event.dyn_ref::<KeyboardEvent>() else {
            return;
        };
        if (key.ctrl_key() || key.meta_key()) && key.key().eq_ignore_ascii_case("k") {
            event.prevent_default();
            if !modal().is_some_and(|v| v.open()) {
                results::search();
            }
        }
    });
}
