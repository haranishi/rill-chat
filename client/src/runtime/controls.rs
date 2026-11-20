use super::*;
use composer::Scope;
use serde_json::json;
use std::fmt::Write;
use web_sys::{Event, KeyboardEvent};

fn target(event: &Event) -> Option<Element> {
    event.target()?.dyn_into::<Element>().ok()
}

pub(super) fn navigation(open: bool) {
    let phone = mobile();
    if let Some(body) = dom::document().body() {
        let _ = body.class_list().toggle_with_force("nav-open", open);
    }
    dom::hidden("nav-shade", !open);
    for (id, inert) in [("sidebar", phone && !open), ("main", phone && open)] {
        if let Some(el) = dom::get(id) {
            if inert {
                let _ = el.set_attribute("inert", "");
            } else {
                let _ = el.remove_attribute("inert");
            }
        }
    }
    attr(
        "nav-open",
        "aria-expanded",
        if open { "true" } else { "false" },
    );
    if open {
        dom::focus("nav-close");
    }
}

pub(super) fn clear_emoji() {
    if let Some(menu) = dom::get("emoji-picker") {
        menu.remove();
    }
}

enum EmojiAnchor {
    Post(i64),
    Composer(Scope),
}

fn open_emoji(button: &Element, anchor: EmojiAnchor) {
    let key = match anchor {
        EmojiAnchor::Post(id) => format!("p{id}"),
        EmojiAnchor::Composer(scope) => format!("c{}", scope.name()),
    };
    let same = dom::get("emoji-picker")
        .and_then(|el| el.get_attribute("data-anchor"))
        .as_ref()
        == Some(&key);
    clear_emoji();
    if same {
        return;
    }
    if sample() {
        return dialogs::auth("login");
    }
    let doc = dom::document();
    let Some(body) = doc.body() else { return };
    let Ok(menu) = doc.create_element("div") else {
        return;
    };
    menu.set_id("emoji-picker");
    menu.set_class_name("emoji-picker");
    let _ = menu.set_attribute("data-anchor", &key);
    let _ = menu.set_attribute("role", "group");
    let _ = menu.set_attribute("aria-label", "絵文字を選択");
    let mut html = String::new();
    for emoji in view::PALETTE {
        let data = match anchor {
            EmojiAnchor::Post(id) => format!(r#"data-react="{id}" data-emoji="{emoji}""#),
            EmojiAnchor::Composer(scope) => {
                format!(r#"data-insert="{emoji}" data-scope="{}""#, scope.name())
            }
        };
        let _ = write!(
            html,
            r#"<button type="button" aria-label="{emoji}" {data}>{emoji}</button>"#
        );
    }
    menu.set_inner_html(&html);
    let rect = button.get_bounding_client_rect();
    let _ = body.append_child(&menu);
    if let Some(menu_el) = menu.dyn_ref::<HtmlElement>() {
        let viewport = f64::from(doc.document_element().map_or(0, |root| root.client_width()));
        let left = rect
            .left()
            .min(viewport - f64::from(menu_el.offset_width()) - 8.0)
            .max(8.0);
        let top = if rect.top() >= 58.0 {
            rect.top() - f64::from(menu_el.offset_height()) - 5.0
        } else {
            rect.bottom() + 4.0
        };
        let _ = menu_el.style().set_property("left", &format!("{left}px"));
        let _ = menu_el.style().set_property("top", &format!("{top}px"));
    }
    if let Some(first) = menu.first_element_child() {
        focus_element(&first);
    }
}

async fn react(id: i64, emoji: String) {
    if sample() {
        return dialogs::auth("login");
    }
    if !view::PALETTE.contains(&emoji.as_str()) {
        return;
    }
    let key = (id, emoji.clone());
    let context = change(|app| {
        let post = app.message(id)?;
        let active = !post.reactions.iter().any(|r| r.emoji == emoji && r.mine);
        app.reactions
            .insert(key.clone())
            .then_some((active, app.auth_epoch, app.room_epoch))
    });
    let Some((active, auth, room)) = context else {
        return;
    };
    let observation = message_request(id);
    let result = api::post::<Message>(
        &format!("/messages/{id}/reactions"),
        &json!({ "emoji": emoji, "active": active }),
    )
    .await;
    if !read(|app| app.auth_epoch == auth) {
        return;
    }
    change(|app| app.reactions.remove(&key));
    if read(|app| app.room_epoch == room) {
        match result {
            Ok(post) => {
                update_requested(post, observation);
            }
            Err(error) => ui_error(error),
        }
    }
}

fn close_panel() {
    change(|app| {
        app.clear_thread();
        app.panel = Panel::Closed;
    });
    clear_emoji();
    render::panel();
    dom::focus("panel-toggle");
}

fn toggle_panel() {
    let mode = change(|app| {
        app.clear_thread();
        app.panel = if app.panel == Panel::Info {
            Panel::Closed
        } else {
            Panel::Info
        };
        app.panel
    });
    clear_emoji();
    render::panel();
    if mode != Panel::Closed && !wide() {
        dom::focus("panel-close");
    }
}

fn click(event: Event) {
    let Some(target) = target(&event) else { return };
    if sample() && target.id().starts_with("compose-") {
        dialogs::auth("login");
        return;
    }
    let Some(button) = target.closest("button").ok().flatten() else {
        if target.closest("#emoji-picker").ok().flatten().is_none() {
            clear_emoji();
        }
        return;
    };
    if !button.has_attribute("data-emoji-for")
        && !button.has_attribute("data-compose-emoji")
        && button.closest("#emoji-picker").ok().flatten().is_none()
    {
        clear_emoji();
    }
    match button.id().as_str() {
        "nav-open" => return navigation(true),
        "nav-close" | "nav-shade" => {
            navigation(false);
            return dom::focus("nav-open");
        }
        "panel-close" => return close_panel(),
        "panel-toggle" => return toggle_panel(),
        "load-older" => return spawn_local(history::older_posts()),
        "older-replies" => return spawn_local(history::load_replies(true, false)),
        "retry-room" => return spawn_local(history::load_room(true, false)),
        "retry-thread" => return spawn_local(history::load_replies(false, false)),
        "to-latest" => {
            if let Some(id) = read(|app| app.room.as_ref().map(|r| r.id)) {
                spawn_local(history::open_room(id, true, None));
            }
            return;
        }
        _ => {}
    }
    let number = |name| {
        button
            .get_attribute(name)
            .and_then(|v| v.parse::<i64>().ok())
    };
    if let Some(id) = number("data-room") {
        spawn_local(history::open_room(id, false, None));
    } else if let Some(id) = number("data-reply") {
        spawn_local(history::open_thread(id, None));
    } else if let Some(id) = number("data-emoji-for") {
        open_emoji(&button, EmojiAnchor::Post(id));
    } else if let Some(scope) = button
        .get_attribute("data-compose-emoji")
        .and_then(|v| Scope::parse(&v))
    {
        open_emoji(&button, EmojiAnchor::Composer(scope));
    } else if let (Some(id), Some(emoji)) =
        (number("data-react"), button.get_attribute("data-emoji"))
    {
        clear_emoji();
        spawn_local(react(id, emoji));
    } else if let (Some(emoji), Some(scope)) = (
        button.get_attribute("data-insert"),
        button
            .get_attribute("data-scope")
            .and_then(|v| Scope::parse(&v)),
    ) {
        clear_emoji();
        composer::insert(scope, &emoji);
    }
}

fn keyboard(event: Event) {
    let Some(keyboard) = event.dyn_ref::<KeyboardEvent>() else {
        return;
    };
    if keyboard.key() == "Enter" {
        if !keyboard.shift_key()
            && !keyboard.is_composing()
            && keyboard.key_code() != 229
            && !mobile()
            && let Some(scope) = target(&event)
                .and_then(|el| el.id().strip_prefix("compose-").and_then(Scope::parse))
        {
            event.prevent_default();
            composer::submit(scope);
        }
        return;
    }
    if keyboard.key() != "Escape" || dom::get("dialog").is_some_and(|dlg| dlg.has_attribute("open"))
    {
        return;
    }
    if let Some(menu) = dom::get("emoji-picker") {
        let anchor = menu.get_attribute("data-anchor").unwrap_or_default();
        clear_emoji();
        let selector = anchor
            .strip_prefix('p')
            .map(|id| format!("[data-emoji-for='{id}']"))
            .or_else(|| {
                anchor
                    .strip_prefix('c')
                    .map(|scope| format!("[data-compose-emoji='{scope}']"))
            });
        if let Some(selector) = selector
            && let Ok(Some(anchor)) = dom::document().query_selector(&selector)
        {
            focus_element(&anchor);
        }
    } else if dom::document()
        .body()
        .is_some_and(|body| body.class_list().contains("nav-open"))
    {
        navigation(false);
        dom::focus("nav-open");
    } else if !wide() && read(|app| app.panel != Panel::Closed) {
        close_panel();
    }
}

pub(super) fn install() {
    let doc = dom::document();
    if let Ok(nodes) = doc.query_selector_all("[data-icon]") {
        for index in 0..nodes.length() {
            if let Some(el) = nodes.item(index).and_then(|n| n.dyn_into::<Element>().ok())
                && let Some(name) = el.get_attribute("data-icon")
            {
                el.set_inner_html(&view::icon(&name));
            }
        }
    }
    if let Ok(platform) = dom::window().navigator().platform()
        && !["Mac", "iPhone", "iPad"]
            .iter()
            .any(|prefix| platform.contains(prefix))
        && let Ok(Some(kbd)) = doc.query_selector("#search-open kbd")
    {
        kbd.set_text_content(Some("Ctrl K"));
    }
    dom::on(dom::window().as_ref(), "online", |_| {
        events::check_session()
    });
    dom::on(doc.as_ref(), "visibilitychange", |_| {
        if !dom::document().hidden() {
            events::check_session();
        }
    });
    dom::on(doc.as_ref(), "click", click);
    dom::on(doc.as_ref(), "keydown", keyboard);
    dom::on(doc.as_ref(), "input", |event| {
        if let Some(scope) =
            target(&event).and_then(|el| el.id().strip_prefix("compose-").and_then(Scope::parse))
        {
            composer::input(scope);
        }
    });
    dom::on(doc.as_ref(), "submit", |event| {
        if let Some(scope) =
            target(&event).and_then(|el| el.id().strip_prefix("form-").and_then(Scope::parse))
        {
            event.prevent_default();
            composer::submit(scope);
        }
    });
    if let Ok(Some(mql)) = dom::window().match_media("(min-width: 1180px)") {
        dom::on(mql.as_ref(), "change", |_| {
            let next = default_panel();
            let redraw = change(|app| {
                if app.panel == Panel::Thread {
                    return false;
                }
                app.panel = next;
                true
            });
            if redraw {
                render::panel();
            }
        });
    }
    if let Ok(Some(mql)) = dom::window().match_media("(max-width: 700px)") {
        dom::on(mql.as_ref(), "change", |_| navigation(false));
    }
}
