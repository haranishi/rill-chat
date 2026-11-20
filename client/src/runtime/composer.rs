use super::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt::Write;
use web_sys::{HtmlButtonElement, HtmlTextAreaElement};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Main,
    Thread,
}

impl Scope {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "main" => Some(Self::Main),
            "thread" => Some(Self::Thread),
            _ => None,
        }
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Thread => "thread",
        }
    }
    fn area(self) -> Option<HtmlTextAreaElement> {
        dom::get(&format!("compose-{}", self.name()))?
            .dyn_into()
            .ok()
    }
    fn key(self, app: &App) -> Option<String> {
        let parent = match self {
            Self::Main => 0,
            Self::Thread => app.thread.as_ref()?.id,
        };
        Some(format!("{}{}.{parent}", prefix(app), app.room.as_ref()?.id))
    }
}

fn prefix(app: &App) -> String {
    format!(
        "rill.draft.{}.{}.",
        app.state.workspace.id,
        app.state.me.as_ref().map_or(0, |me| me.id)
    )
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(super) struct Draft {
    body: String,
    client_id: Option<String>,
}

fn draft(key: &str) -> Draft {
    read(|app| app.drafts.get(key).cloned())
        .or_else(|| {
            let raw = storage()?.get_item(key).ok()??;
            serde_json::from_str(&raw).ok()
        })
        .unwrap_or_default()
}

fn store(key: &str, draft: Draft) {
    if draft.body.is_empty() {
        change(|app| app.drafts.remove(key));
        if let Some(storage) = storage() {
            let _ = storage.remove_item(key);
        }
    } else {
        if let Some(storage) = storage()
            && let Ok(value) = serde_json::to_string(&draft)
        {
            let _ = storage.set_item(key, &value);
        }
        change(|app| app.drafts.insert(key.to_owned(), draft));
    }
}

pub(super) fn clear_user_drafts() {
    let prefix = read(prefix);
    change(|app| app.drafts.retain(|key, _| !key.starts_with(&prefix)));
    if let Some(storage) = storage() {
        let keys: Vec<_> = (0..storage.length().unwrap_or_default())
            .filter_map(|index| storage.key(index).ok().flatten())
            .filter(|key| key.starts_with(&prefix))
            .collect();
        for key in keys {
            let _ = storage.remove_item(&key);
        }
    }
}

pub(super) fn random_id() -> Option<String> {
    let mut bytes = [0; 16];
    dom::window()
        .crypto()
        .ok()?
        .get_random_values_with_u8_array(&mut bytes)
        .ok()?;
    let mut key = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(key, "{byte:02x}");
    }
    Some(key)
}

pub(super) fn mount(scope: Scope) {
    let model = read(|app| {
        let room = app.room.as_ref()?;
        let label = format!(
            "{}{}",
            if room.kind == "dm" { "" } else { "#" },
            view::room_label(&app.state, room)
        );
        Some((
            scope.key(app)?,
            view::composer_markup(scope.name(), &label, app.sample),
        ))
    });
    let Some((key, markup)) = model else { return };
    dom::set_html(
        if scope == Scope::Main {
            "composer-region"
        } else {
            "thread-composer"
        },
        &markup,
    );
    if let Some(area) = scope.area() {
        area.set_value(&draft(&key).body);
        update(scope, &key, &area);
    }
}

fn update(scope: Scope, key: &str, area: &HtmlTextAreaElement) {
    let disabled =
        read(|app| !app.sample && (app.sending.contains(key) || area.value().trim().is_empty()));
    if let Some(send) = dom::get(&format!("send-{}", scope.name()))
        .and_then(|el| el.dyn_into::<HtmlButtonElement>().ok())
    {
        send.set_disabled(disabled);
    }
    let _ = area.style().set_property("height", "auto");
    let _ = area
        .style()
        .set_property("height", &format!("{}px", area.scroll_height().min(118)));
}

pub(super) fn input(scope: Scope) {
    let Some(key) = read(|app| scope.key(app)) else {
        return;
    };
    if let Some(area) = scope.area() {
        store(
            &key,
            Draft {
                body: area.value(),
                client_id: None,
            },
        );
        update(scope, &key, &area);
    }
}

pub(super) fn insert(scope: Scope, emoji: &str) {
    if let Some(area) = scope.area() {
        let start = area.selection_start().ok().flatten().unwrap_or(0);
        let end = area.selection_end().ok().flatten().unwrap_or(start);
        if area
            .set_range_text_with_start_and_end_and_mode(emoji, start, end, "end")
            .is_ok()
        {
            input(scope);
        }
        let _ = area.focus();
    }
}

pub(super) fn submit(scope: Scope) {
    if sample() {
        return dialogs::auth("login");
    }
    let context = read(|app| {
        let key = scope.key(app)?;
        let channel = app.room.as_ref()?.id;
        if app.sending.contains(&key) {
            return None;
        }
        let parent = match scope {
            Scope::Main => None,
            Scope::Thread => Some(app.thread.as_ref()?.id),
        };
        Some((key, channel, parent, app.auth_epoch))
    });
    let Some((key, channel, parent, epoch)) = context else {
        return;
    };
    let Some(area) = scope.area() else { return };
    let body = area.value();
    if body.trim().is_empty() {
        return;
    }
    if body.chars().count() > 4000 {
        return notify("メッセージは4,000文字以内で入力してください。");
    }
    let saved = draft(&key);
    let retry_id = if saved.body == body {
        saved.client_id
    } else {
        None
    };
    let Some(client_id) = retry_id.or_else(random_id) else {
        return notify("送信用の番号を準備できません。もう一度お試しください。");
    };
    store(
        &key,
        Draft {
            body: body.clone(),
            client_id: Some(client_id.clone()),
        },
    );
    change(|app| app.sending.insert(key.clone()));
    update(scope, &key, &area);
    spawn_local(async move {
        let result = api::post::<Message>(
            &format!("/channels/{channel}/messages"),
            &json!({
                "body": body, "client_id": client_id, "parent_id": parent
            }),
        )
        .await;
        if !read(|app| app.auth_epoch == epoch) {
            return;
        }
        change(|app| app.sending.remove(&key));
        match result {
            Ok(post) => {
                if draft(&key).client_id.as_deref() == Some(&client_id) {
                    store(&key, Draft::default());
                    if read(|app| scope.key(app).as_ref() == Some(&key))
                        && let Some(area) = scope.area()
                        && area.value() == body
                    {
                        area.set_value("");
                    }
                }
                events::receive(post);
            }
            Err(error) => ui_error(error),
        }
        if read(|app| scope.key(app).as_ref() == Some(&key))
            && let Some(area) = scope.area()
        {
            update(scope, &key, &area);
        }
    });
}
