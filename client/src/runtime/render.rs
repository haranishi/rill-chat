use super::*;
use std::fmt::Write;

pub(super) fn connection(is_live: bool) {
    let sample = change(|app| {
        app.live = is_live;
        app.sample
    });
    let (text, class) = if sample {
        ("ログイン", "")
    } else if is_live {
        ("接続済み", "connected")
    } else {
        ("再接続中…", "reconnecting")
    };
    dom::set_text("self-detail", text);
    if let Some(detail) = dom::get("self-detail") {
        detail.set_class_name(class);
    }
}

fn nav_button(app: &App, room: &Channel) -> String {
    let active = app.room.as_ref().is_some_and(|r| r.id == room.id);
    let glyph = if room.kind == "dm" {
        view::avatar(
            app.state
                .members
                .iter()
                .find(|u| Some(u.id) == room.peer_id),
            "tiny",
        )
    } else {
        r##"<span class="nav-hash">#</span>"##.into()
    };
    let unread = if app.unread.contains(&room.id) {
        r#"<span class="unread-dot" aria-label="未読"></span>"#
    } else {
        ""
    };
    format!(
        r#"<button data-room="{}" class="room-link{}" aria-current="{}">{}<span class="room-link-text">{}</span>{}</button>"#,
        room.id,
        if active { " selected" } else { "" },
        if active { "page" } else { "false" },
        glyph,
        view::esc(&view::room_label(&app.state, room)),
        unread
    )
}

pub(super) fn nav() {
    let live = read(|app| {
        let state = &app.state;
        dom::set_text("workspace-name", &state.workspace.name);
        dom::set_text(
            "workspace-caption",
            if app.sample {
                "デモワークスペース"
            } else {
                "ワークスペース"
            },
        );
        dom::set_text("crumb-workspace", &state.workspace.name);
        dom::hidden("preview-tag", !app.sample);
        let mut channels = String::new();
        let mut dms = String::new();
        for room in &state.channels {
            if room.kind == "dm" {
                dms.push_str(&nav_button(app, room));
            } else {
                channels.push_str(&nav_button(app, room));
            }
        }
        if dms.is_empty() {
            dms.push_str(r#"<button class="first-dm" data-first-dm>会話を始める</button>"#);
        }
        dom::set_html("channels", &channels);
        dom::set_html("dms", &dms);
        dom::set_html("self-avatar", &view::avatar(state.me.as_ref(), ""));
        dom::set_text(
            "self-name",
            state.me.as_ref().map_or("ゲスト", |me| me.name.as_str()),
        );
        let owner = state
            .me
            .as_ref()
            .is_some_and(|me| me.id == state.workspace.owner_id);
        dom::hidden("team-action", !app.sample && !owner);
        if let Some(label) = dom::get("team-action").and_then(|b| b.last_element_child()) {
            label.set_text_content(Some(if app.sample {
                "チームで使う"
            } else {
                "メンバーを招待"
            }));
        }
        app.live
    });
    connection(live);
}

pub(super) fn header() {
    read(|app| {
        let Some(room) = app.room.as_ref() else {
            return;
        };
        let dm = room.kind == "dm";
        let name = view::room_label(&app.state, room);
        let topic = if dm {
            "二人だけの会話"
        } else if room.topic.is_empty() {
            "ワークスペースのチャンネル"
        } else {
            &room.topic
        };
        dom::set_html(
            "room-kind",
            &if dm { view::icon("lock") } else { "#".into() },
        );
        dom::set_text("room-name", &name);
        dom::set_text("room-topic", topic);
        dom::set_text(
            "crumb-section",
            if dm {
                "ダイレクトメッセージ"
            } else {
                "チャンネル"
            },
        );
        attr(
            "panel-toggle",
            "aria-label",
            if dm {
                "会話情報"
            } else {
                "チャンネル情報"
            },
        );
        dom::document().set_title(&format!(
            "{}{} · {} | Rill",
            if dm { "" } else { "# " },
            name,
            app.state.workspace.name
        ));
    });
}

fn history_markup(posts: &[Message], state: &State, in_thread: bool) -> String {
    let mut html = String::new();
    let mut previous_day = String::new();
    for post in posts {
        if !in_thread {
            let day = view::day_label(post.created_at);
            if day != previous_day {
                html.push_str(&view::day_divider(post.created_at));
                previous_day = day;
            }
        }
        html.push_str(&view::post_markup(post, state, in_thread, false));
    }
    html
}

pub(super) fn timeline(bottom: bool) {
    let Some(box_el) = dom::get("messages") else {
        return;
    };
    let (html, more, anchored) = read(|app| {
        let html = if app.posts.is_empty() {
            app.room
                .as_ref()
                .map_or_else(String::new, |room| view::empty_room(&app.state, room))
        } else {
            history_markup(&app.posts, &app.state, false)
        };
        (html, app.more, app.anchor.is_some())
    });
    box_el.set_inner_html(&html);
    view::render_swatches(&box_el);
    dom::hidden("load-older", !more);
    match (anchored, dom::get("to-latest")) {
        (true, None) => {
            if let Some(conversation) = dom::get("conversation") {
                let _ = conversation.insert_adjacent_html(
                    "beforeend",
                    r#"<button id="to-latest" class="latest-button">最新の会話へ</button>"#,
                );
            }
        }
        (false, Some(button)) => button.remove(),
        _ => {}
    }
    if bottom && let Some(timeline) = dom::get("timeline") {
        scroll_bottom(&timeline);
    }
}

// A live message normally touches just the new article. Rebuild the list only for
// out-of-order delivery, such as overlapping HTTP and EventSource responses.
pub(super) fn append(post: &Message, previous: Option<&Message>, in_thread: bool) {
    let Some(box_el) = dom::get(if in_thread {
        "thread-posts"
    } else {
        "messages"
    }) else {
        return;
    };
    let mut html = String::new();
    if !in_thread
        && previous
            .is_none_or(|p| view::day_label(p.created_at) != view::day_label(post.created_at))
    {
        html.push_str(&view::day_divider(post.created_at));
    }
    if previous.is_none() {
        box_el.set_inner_html("");
    }
    html.push_str(&read(|app| {
        view::post_markup(post, &app.state, in_thread, false)
    }));
    let _ = box_el.insert_adjacent_html("beforeend", &html);
    if let Some(el) = box_el.last_element_child() {
        view::render_swatches(&el);
    }
}

pub(super) fn replace_post(post: &Message) {
    let selector = format!("[data-mid='{}']", post.id);
    let doc = dom::document();
    let Ok(nodes) = doc.query_selector_all(&selector) else {
        return;
    };
    for index in 0..nodes.length() {
        let Some(old) = nodes
            .item(index)
            .and_then(|node| node.dyn_into::<Element>().ok())
        else {
            continue;
        };
        let parent = old.class_list().contains("thread-parent");
        let in_thread = old.closest("#context-panel").ok().flatten().is_some();
        let focus = doc.active_element().filter(|el| old.contains(Some(el)));
        let active_selector = focus.as_ref().and_then(|button| {
            ["data-save", "data-edit", "data-reply", "data-emoji-for"]
                .into_iter()
                .find(|name| button.has_attribute(name))
                .map(|name| format!("[{name}]"))
                .or_else(|| {
                    button
                        .get_attribute("data-emoji")
                        .map(|emoji| format!("[data-emoji='{}']", view::esc(&emoji)))
                })
        });
        let Ok(container) = doc.create_element("div") else {
            continue;
        };
        container.set_inner_html(&read(|app| {
            view::post_markup(post, &app.state, in_thread, parent)
        }));
        if let Some(new_node) = container.first_element_child() {
            view::render_swatches(&new_node);
            let _ = old.replace_with_with_node_1(&new_node);
            if let Some(selector) = active_selector
                && let Ok(Some(button)) = new_node.query_selector(&selector)
            {
                focus_element(&button);
            }
        }
    }
}

fn heading(label: &str) -> String {
    format!(
        r#"<div class="panel-heading"><h2>{label}</h2><button id="panel-close" class="icon-button" aria-label="パネルを閉じる">{}</button></div>"#,
        view::icon("x")
    )
}

pub(super) fn info() {
    let markup = read(|app| {
        let room = app.room.as_ref()?;
        if app.panel != Panel::Info {
            return None;
        }
        let dm = room.kind == "dm";
        let self_id = app.state.me.as_ref().map_or(1, |me| me.id);
        let members: Vec<_> = app
            .state
            .members
            .iter()
            .filter(|p| !dm || p.id == self_id || Some(p.id) == room.peer_id)
            .collect();
        let mut html = heading(if dm {
            "会話情報"
        } else {
            "チャンネル情報"
        });
        let _ = write!(
            html,
            r#"<div class="details-scroll"><div class="room-portrait"><img src="/river.png" alt="" width="880" height="416"></div><div class="access-note">{}{}</div><section class="detail-group"><div class="detail-label"><h3>メンバー <span>{}</span></h3>"#,
            view::icon(if dm { "lock" } else { "users" }),
            if dm {
                "この会話は二人だけに表示されます"
            } else {
                "ワークスペースの全員が参加できます"
            },
            members.len()
        );
        if !dm {
            let _ = write!(
                html,
                r#"<button class="mini-icon" data-members aria-label="メンバーをすべて表示">{}</button>"#,
                view::icon("right")
            );
        }
        html.push_str(r#"</div><ul class="people-list">"#);
        for member in members.iter().take(5) {
            let _ = write!(
                html,
                r#"<li class="person-row">{}<span class="person-text"><b>{}</b><small>@{}</small></span>{}</li>"#,
                view::avatar(Some(member), "small"),
                view::esc(&member.name),
                view::esc(&member.handle),
                if member.id == app.state.workspace.owner_id {
                    r#"<span class="role">オーナー</span>"#
                } else {
                    ""
                }
            );
        }
        let _ = write!(
            html,
            r#"</ul></section><section class="detail-group"><div class="detail-label"><h3>最近のスレッド</h3>{}</div>"#,
            view::icon("thread")
        );
        let mut threads = app
            .posts
            .iter()
            .rev()
            .filter(|post| post.reply_count > 0)
            .take(3)
            .peekable();
        if threads.peek().is_none() {
            html.push_str(r#"<p class="subtle">メッセージに返信すると、ここに表示されます。</p>"#);
        }
        for post in threads {
            let _ = write!(
                html,
                r#"<button class="thread-card" data-reply="{}"><span>{}</span><small>{}件の返信 {}</small></button>"#,
                post.id,
                view::esc(post.body.lines().next().unwrap_or("")),
                post.reply_count,
                view::icon("right")
            );
        }
        html.push_str("</section></div>");
        Some(html)
    });
    if let Some(markup) = markup {
        dom::set_html("context-panel", &markup);
    }
}

pub(super) fn replies() {
    if let Some(box_el) = dom::get("thread-posts") {
        let (html, more) = read(|app| {
            let html = if app.replies.is_empty() {
                r#"<p class="no-replies">最初の返信を書きましょう。</p>"#.into()
            } else {
                history_markup(&app.replies, &app.state, true)
            };
            (html, app.more_replies)
        });
        box_el.set_inner_html(&html);
        view::render_swatches(&box_el);
        dom::hidden("older-replies", !more);
    }
}

pub(super) fn panel() {
    let mode = read(|app| {
        if app.room.is_some() {
            app.panel
        } else {
            Panel::Closed
        }
    });
    let show = mode != Panel::Closed;
    dom::hidden("context-panel", !show);
    if let Some(panes) = dom::get("panes") {
        let _ = panes.class_list().toggle_with_force("with-panel", show);
    }
    attr(
        "panel-toggle",
        "aria-expanded",
        if show { "true" } else { "false" },
    );
    match mode {
        Panel::Closed => dom::set_html("context-panel", ""),
        Panel::Info => info(),
        Panel::Thread => {
            let origin = read(|app| {
                app.thread
                    .as_ref()
                    .map(|post| view::post_markup(post, &app.state, true, true))
            });
            if let Some(origin) = origin {
                dom::set_html(
                    "context-panel",
                    &format!(
                        r#"{}<div id="thread-scroll"><div class="thread-origin">{origin}</div><div class="thread-divider">返信</div><button id="older-replies" class="text-button" hidden>以前の返信を読む</button><div id="thread-posts"></div></div><div id="thread-composer"></div>"#,
                        heading("スレッド")
                    ),
                );
                if let Some(panel) = dom::get("context-panel") {
                    view::render_swatches(&panel);
                }
                replies();
                composer::mount(composer::Scope::Thread);
            }
        }
    }
}

pub(super) fn load_failure(container: &str, message: &str, retry_id: &str) {
    dom::set_html(
        container,
        &format!(
            r#"<p class="inline-error">{}</p><button id="{retry_id}" class="text-button">もう一度読み込む</button>"#,
            view::esc(message)
        ),
    );
}
