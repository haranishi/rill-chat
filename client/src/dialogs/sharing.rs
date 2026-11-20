use super::{FEEDBACK, current, generation, show};
use crate::{
    api, dom, runtime,
    view::{day_label, icon, time_label},
};
use js_sys::Reflect;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Clipboard, HtmlInputElement, HtmlTextAreaElement, ShareData};

pub(super) const SOURCE: &str = "https://github.com/haranishi/rill-chat";
const DESCRIPTION: &str = "Rill — Rust 製のオープンソース・チームチャット";

#[derive(Deserialize)]
struct Invite {
    code: String,
    expires_at: i64,
}

pub(super) fn invite() {
    show(
        "メンバーを招待",
        &format!(
            r#"<p class="dialog-lead">招待コードを発行し、参加してほしい人にこのサイトのアドレスと一緒に渡してください。有効期間は24時間です。</p><form id="invite-form">{FEEDBACK}<button type="submit" id="make-invite" class="primary">{}招待コードを発行</button></form><div id="invite-result"></div>"#,
            icon("key")
        ),
        "",
    );
}

pub(super) async fn issue_invite(id: u64) -> Result<(), api::Error> {
    let invite: Invite = api::post("/invites", &json!({})).await?;
    if current(id) {
        dom::hidden("invite-form", true);
        dom::set_html(
            "invite-result",
            &format!(
                r#"<label class="field"><span>招待コード</span><input id="invite-code" readonly></label><p class="muted">有効期限 {} {}</p><button class="primary" id="copy-invite">{}コードをコピー</button>"#,
                day_label(invite.expires_at),
                time_label(invite.expires_at),
                icon("copy")
            ),
        );
        if let Some(el) =
            dom::get("invite-code").and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
        {
            el.set_value(&invite.code);
        }
        dom::focus("invite-code");
    }
    Ok(())
}

pub(super) fn copy(text: String) {
    let id = generation();
    let navigator = dom::window().navigator();
    let clipboard = Reflect::get(&navigator, &JsValue::from_str("clipboard"))
        .ok()
        .and_then(|v| v.dyn_into::<Clipboard>().ok());
    spawn_local(async move {
        if let Some(api) = clipboard
            && JsFuture::from(api.write_text(&text)).await.is_ok()
        {
            runtime::notify("コピーしました");
            return;
        }
        if current(id) {
            show(
                "テキストをコピー",
                r#"<label class="field"><span>選択したテキストをコピーしてください。</span><textarea id="copy-value" readonly></textarea></label>"#,
                "",
            );
            if let Some(el) =
                dom::get("copy-value").and_then(|e| e.dyn_into::<HtmlTextAreaElement>().ok())
            {
                el.set_value(&text);
                let _ = el.focus();
                el.select();
            }
        }
    });
}

pub(super) fn product() {
    let nav = dom::window().navigator();
    let native = if Reflect::get(&nav, &JsValue::from_str("share")).is_ok_and(|v| v.is_function()) {
        r#"<button id="native-share">端末で共有</button>"#
    } else {
        ""
    };
    let url: String = js_sys::encode_uri_component(SOURCE).into();
    let text: String = js_sys::encode_uri_component(DESCRIPTION).into();
    show(
        "Rill について",
        &format!(
            r#"<div class="product-intro"><img src="/mark.svg" alt="" width="48" height="48"><p>{DESCRIPTION}</p></div><a class="secondary" href="{SOURCE}" target="_blank" rel="noopener noreferrer">GitHub でソースを見る{}</a><h3>プロジェクトを共有</h3><div class="share-list"><a href="https://x.com/intent/post?text={text}&amp;url={url}" target="_blank" rel="noopener noreferrer">X</a><a href="https://social-plugins.line.me/lineit/share?url={url}" target="_blank" rel="noopener noreferrer">LINE</a><button id="copy-project">リンクをコピー</button>{native}</div>"#,
            icon("external")
        ),
        "",
    );
}

pub(super) fn share() {
    let data = ShareData::new();
    data.set_title(DESCRIPTION);
    data.set_url(SOURCE);
    let pending = dom::window().navigator().share_with_data(&data);
    spawn_local(async move {
        if let Err(err) = JsFuture::from(pending).await
            && Reflect::get(&err, &JsValue::from_str("name"))
                .ok()
                .and_then(|n| n.as_string())
                .as_deref()
                != Some("AbortError")
        {
            runtime::notify("共有できませんでした。リンクをコピーしてください。");
        }
    });
}
