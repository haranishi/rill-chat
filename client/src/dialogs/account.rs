use super::{FEEDBACK, close_if_current, field, focus_first, primary, show, val};
use crate::{
    api, runtime,
    view::{avatar, esc, icon},
};
use serde_json::{Value, json};
use web_sys::{FormData, HtmlFormElement};

pub fn auth(mode: &str) {
    let (mode, title, lead, submit) = match mode {
        "join" => (
            "join",
            "チームの招待に参加",
            "チームから届いた招待コードで、アカウントを作成します。",
            "チームに参加",
        ),
        "login" => (
            "login",
            "おかえりなさい",
            "登録したユーザー名とパスワードを入力してください。",
            "ログイン",
        ),
        _ => (
            "create",
            "ワークスペースを作る",
            "チームの名前と、あなたのログイン情報を登録してください。",
            "作成して始める",
        ),
    };
    let tabs: String = [
        ("create", "新規作成"),
        ("join", "招待で参加"),
        ("login", "ログイン"),
    ]
    .into_iter()
    .map(|(key, label)| {
        format!(
            r#"<button data-auth="{key}" aria-pressed="{}">{label}</button>"#,
            key == mode
        )
    })
    .collect();
    let mut fields = String::new();
    if mode == "create" {
        fields.push_str(&field(
            "ワークスペース名",
            "workspace",
            r#"maxlength="48" placeholder="例）デザイン部" autocomplete="organization""#,
        ));
    }
    if mode == "join" {
        fields.push_str(&field(
            "招待コード",
            "code",
            r#"maxlength="256" autocomplete="off" spellcheck="false" autocapitalize="off""#,
        ));
    }
    if mode != "login" {
        fields.push_str(&field(
            "表示名",
            "name",
            r#"maxlength="48" autocomplete="nickname" placeholder="会話で使う名前""#,
        ));
    }
    fields.push_str(&field("ユーザー名","handle",r#"minlength="3" maxlength="32" pattern="[a-zA-Z0-9_\-]{3,32}" autocomplete="username" autocapitalize="none" spellcheck="false" placeholder="英数字・ハイフン・下線で3〜32字""#));
    fields.push_str(&field(
        "パスワード",
        "password",
        if mode == "login" {
            r#"type="password" autocomplete="current-password""#
        } else {
            r#"type="password" minlength="10" autocomplete="new-password" placeholder="10文字以上""#
        },
    ));
    show(
        title,
        &format!(
            r#"<div class="auth-tabs" role="group" aria-label="アカウント操作">{tabs}</div><p class="dialog-lead">{lead}</p><form id="auth-form" data-mode="{mode}">{fields}{FEEDBACK}{}</form>"#,
            primary(submit)
        ),
        "",
    );
    focus_first();
}

pub(super) async fn authenticate(
    form: &HtmlFormElement,
    data: &FormData,
    generation: u64,
) -> Result<(), api::Error> {
    let mode = form.get_attribute("data-mode").unwrap_or_default();
    let mut body =
        json!({"handle":val(data,"handle").trim().to_lowercase(),"password":val(data,"password")});
    match mode.as_str() {
        "create" => {
            body["workspace"] = val(data, "workspace").into();
            body["name"] = val(data, "name").into();
        }
        "join" => {
            body["code"] = val(data, "code").into();
            body["name"] = val(data, "name").into();
        }
        "login" => {}
        _ => return Ok(()),
    }
    api::post::<Value>(&format!("/auth/{mode}"), &body).await?;
    runtime::enter().await?;
    close_if_current(generation);
    Ok(())
}

pub(super) fn profile() {
    let state = runtime::state();
    let Some(me) = state.me.as_ref() else {
        return auth("login");
    };
    let role = if state.workspace.owner_id == me.id {
        "オーナー"
    } else {
        "メンバー"
    };
    show(
        "アカウント",
        &format!(
            r#"<div class="person-row profile-card">{}<span class="person-text"><b>{}</b><small>@{}</small></span></div><dl class="profile-list"><dt>ワークスペース</dt><dd>{}</dd><dt>役割</dt><dd>{role}</dd></dl><div class="profile-actions"><button id="change-password" type="button" class="secondary">{}パスワードを変更</button><form id="logout-form">{FEEDBACK}<button class="secondary" id="logout" type="submit">{}ログアウト</button></form></div>"#,
            avatar(Some(me), ""),
            esc(&me.name),
            esc(&me.handle),
            esc(&state.workspace.name),
            icon("key"),
            icon("exit")
        ),
        "",
    );
}

pub(super) async fn logout(generation: u64) -> Result<(), api::Error> {
    api::post::<Value>("/auth/logout", &json!({})).await?;
    close_if_current(generation);
    runtime::signed_out();
    Ok(())
}

pub(super) fn password() {
    if runtime::sample() {
        return auth("login");
    }
    let old = field(
        "現在のパスワード",
        "current_password",
        r#"type="password" autocomplete="current-password""#,
    );
    let new = field(
        "新しいパスワード",
        "new_password",
        r#"type="password" minlength="10" autocomplete="new-password" placeholder="10文字以上""#,
    );
    show(
        "パスワードを変更",
        &format!(
            r#"<p class="dialog-lead">変更後もこのブラウザーで作業を続けられます。他の端末では新しいパスワードでログインしてください。</p><form id="password-form">{old}{new}{FEEDBACK}{}</form>"#,
            primary("パスワードを変更")
        ),
        "",
    );
    focus_first();
}

pub(super) async fn change_password(data: &FormData, generation: u64) -> Result<(), api::Error> {
    let body = json!({"current_password":val(data,"current_password"),"new_password":val(data,"new_password")});
    let rotation = runtime::password_change_started();
    let result = api::post::<Value>("/auth/password", &body).await;
    runtime::password_change_finished(rotation);
    result?;
    close_if_current(generation);
    runtime::notify("パスワードを変更しました。");
    Ok(())
}
