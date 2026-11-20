use super::{
    FEEDBACK, attr_id, close_if_current, current, disabled, error, generation, primary, results,
    show, val,
};
use crate::{api, dom, model::Message, runtime, view::esc};
use serde_json::json;
use std::{cell::RefCell, collections::HashSet};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, FormData, HtmlElement, HtmlFormElement, HtmlTextAreaElement};

thread_local! {
    static SAVING: RefCell<HashSet<(u64,i64)>> = RefCell::new(HashSet::new());
}
fn viewer() -> Option<i64> {
    runtime::state().me.map(|me| me.id)
}

pub(super) fn toggle_save(button: Element, id: i64) {
    if runtime::sample() {
        return super::auth("login");
    }
    let session = runtime::session_epoch();
    let request = runtime::message_request(id);
    if !SAVING.with_borrow_mut(|pending| pending.insert((session, id))) {
        return;
    }
    let active = button.get_attribute("aria-pressed").as_deref() != Some("true");
    let list = button.closest("#saved-results").ok().flatten().is_some();
    let scope = if button.closest("#context-panel").ok().flatten().is_some() {
        "#context-panel"
    } else {
        "#messages"
    };
    let restore_focus = dom::document().active_element().as_ref() == Some(&button);
    disabled(&button, true);
    spawn_local(async move {
        let result =
            api::post::<Message>(&format!("/messages/{id}/saved"), &json!({"active":active})).await;
        SAVING.with_borrow_mut(|pending| {
            pending.remove(&(session, id));
        });
        disabled(&button, false);
        if runtime::session_epoch() != session {
            return;
        }
        match result {
            Ok(message) => {
                let saved = message.saved;
                if !runtime::update_requested(message, request) {
                    return;
                }
                if !saved {
                    results::remove_saved(id);
                }
                runtime::notify(if saved {
                    "「あとで読む」に保存しました。"
                } else {
                    "保存を解除しました。"
                });
                if restore_focus
                    && dom::document()
                        .active_element()
                        .is_some_and(|el| el.tag_name() == "BODY")
                {
                    let selector = if list {
                        "#dialog [data-close]".to_owned()
                    } else {
                        format!(r#"{scope} [data-save="{id}"]"#)
                    };
                    if let Ok(Some(el)) = dom::document().query_selector(&selector)
                        && let Ok(el) = el.dyn_into::<HtmlElement>()
                    {
                        let _ = el.focus();
                    }
                }
            }
            Err(err) => runtime::notify(&err.message),
        }
    });
}

pub(super) fn edit(id: i64) {
    let Some(post) = runtime::message_by_id(id) else {
        return;
    };
    if viewer() != Some(post.author_id) {
        return;
    }
    let version = post.edited_at.map(|v| v.to_string()).unwrap_or_default();
    let parent = post.parent_id.map(|v| v.to_string()).unwrap_or_default();
    show(
        "メッセージを編集",
        &format!(
            r#"<form id="edit-form" data-message="{id}" data-channel="{}" data-parent="{parent}" data-version="{version}"><label class="field"><span>本文</span><textarea id="edit-body" name="body" rows="5" maxlength="8000" required></textarea></label>{FEEDBACK}<div id="edit-conflict" hidden></div>{}</form>"#,
            post.channel_id,
            primary("変更を保存")
        ),
        "",
    );
    if let Some(el) = dom::get("edit-body").and_then(|e| e.dyn_into::<HtmlTextAreaElement>().ok()) {
        el.set_value(&post.body);
        let _ = el.focus();
    }
}

pub(super) async fn submit_edit(
    form: &HtmlFormElement,
    data: &FormData,
    view: u64,
) -> Result<(), api::Error> {
    let Some(id) = attr_id(form, "data-message") else {
        return Ok(());
    };
    let request = runtime::message_request(id);
    let body = json!({"body":val(data,"body"),"expected_edited_at":attr_id(form,"data-version")});
    match api::post::<Message>(&format!("/messages/{id}/edit"), &body).await {
        Ok(post) => {
            if runtime::update_requested(post, request) {
                close_if_current(view);
                runtime::notify("変更を保存しました。");
            }
            Ok(())
        }
        Err(err) if err.status == 409 && current(view) => {
            let _ = form.set_attribute("data-stale", "");
            error("別の端末で編集されました。入力は残っています。最新の本文と見比べてください。");
            latest(form, view).await;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

pub(super) fn retry_latest() {
    let Some(form) = dom::get("edit-form").and_then(|el| el.dyn_into::<HtmlFormElement>().ok())
    else {
        return;
    };
    let id = generation();
    spawn_local(async move {
        latest(&form, id).await;
    });
}

async fn latest(form: &HtmlFormElement, view: u64) {
    let (Some(channel), Some(id)) = (attr_id(form, "data-channel"), attr_id(form, "data-message"))
    else {
        return;
    };
    let parent = attr_id(form, "data-parent")
        .map(|id| format!("&parent_id={id}"))
        .unwrap_or_default();
    dom::hidden("edit-conflict", false);
    dom::set_text("edit-conflict", "最新の本文を読み込んでいます…");
    let request = runtime::message_request(id);
    let result =
        api::get::<Vec<Message>>(&format!("/channels/{channel}/messages?around={id}{parent}"))
            .await;
    if !current(view) {
        return;
    }
    if let Ok(posts) = result
        && let Some(post) = posts.into_iter().find(|m| m.id == id)
    {
        let version = post.edited_at.map(|v| v.to_string()).unwrap_or_default();
        let body = esc(&post.body);
        if !runtime::update_requested(post, request) {
            return;
        }
        let _ = form.set_attribute("data-latest", &version);
        dom::set_html(
            "edit-conflict",
            &format!(
                r#"<p>最新の本文</p><blockquote>{body}</blockquote><button id="review-edit" type="button" class="secondary">入力を残して編集を続ける</button>"#
            ),
        );
        dom::focus("review-edit");
    } else {
        dom::set_html(
            "edit-conflict",
            r#"<p>最新の本文を読み込めませんでした。</p><button id="retry-edit" type="button" class="secondary">再読み込み</button>"#,
        );
    }
}

pub(super) fn review_edit() {
    let Some(form) = dom::get("edit-form") else {
        return;
    };
    let Some(version) = form.get_attribute("data-latest") else {
        return;
    };
    let _ = form.set_attribute("data-version", &version);
    let _ = form.remove_attribute("data-stale");
    let _ = form.remove_attribute("data-latest");
    if let Some(review) = dom::get("review-edit") {
        review.remove();
    }
    if let Ok(Some(button)) = form.query_selector("[type=submit]") {
        disabled(&button, false);
    }
    super::clear_error();
    dom::focus("edit-body");
}
