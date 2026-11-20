use super::*;

async fn edit(app: &App, cookie: &str, message: i64, body: &str, expected: &Value) -> Reply {
    app.post(
        &format!("/messages/{message}/edit"),
        cookie,
        json!({"body":body,"expected_edited_at":expected}),
    )
    .await
}

async fn save(app: &App, cookie: &str, message: i64, active: bool) -> Reply {
    app.post(
        &format!("/messages/{message}/saved"),
        cookie,
        json!({"active":active}),
    )
    .await
}

#[tokio::test]
async fn edits_check_author_and_version_and_preserve_search_and_send_retries() {
    let app = App::new();
    let owner = app.create("sender").await;
    let peer = app.join("reader", &app.invite(&owner).await).await;
    let state = app.get("/state", &owner).await.body;
    let room = id(&state["channels"][0]);
    let original = "以前の図書館で集合";
    let replacement = "明日の映画祭で集合";
    let posted = app
        .send(room, &owner, original, "idempotency-key", None)
        .await
        .body;
    let message = id(&posted);
    assert_eq!(posted["saved"], false);
    assert!(posted.get("edited_at").unwrap().is_null());
    let mut stream = app.events(&peer).await;
    assert_eq!(
        edit(&app, &peer, message, original, &Value::Null)
            .await
            .status,
        403
    );
    assert_eq!(
        app.post(
            &format!("/messages/{message}/edit"),
            &owner,
            json!({"body":replacement})
        )
        .await
        .status,
        422
    );
    assert_eq!(
        edit(&app, &owner, message, "   ", &Value::Null)
            .await
            .status,
        400
    );
    let changed = edit(&app, &owner, message, replacement, &Value::Null).await;
    assert_eq!(changed.status, 200);
    assert!(changed.body["edited_at"].as_i64().unwrap() > posted["created_at"].as_i64().unwrap());
    assert_eq!(
        next_event(&mut stream).await.unwrap(),
        (
            "refresh".into(),
            json!({"channel_id":room,"message_id":message})
        )
    );
    assert_eq!(
        edit(&app, &owner, message, replacement, &Value::Null)
            .await
            .body,
        changed.body
    );
    assert_eq!(
        app.get("/search?q=%E5%9B%B3%E6%9B%B8%E9%A4%A8", &peer)
            .await
            .body,
        json!([])
    ); // 図書館
    assert_eq!(
        app.get("/search?q=%E6%98%A0%E7%94%BB%E7%A5%AD", &peer)
            .await
            .body,
        json!([changed.body])
    ); // 映画祭
    assert_eq!(
        app.send(room, &owner, original, "idempotency-key", None)
            .await
            .body,
        changed.body
    );
    assert_eq!(
        app.send(room, &owner, replacement, "idempotency-key", None)
            .await
            .status,
        409
    );
    let (first, second) = tokio::join!(
        edit(
            &app,
            &owner,
            message,
            "First correction",
            &changed.body["edited_at"]
        ),
        edit(
            &app,
            &owner,
            message,
            "Second correction",
            &changed.body["edited_at"]
        )
    );
    let (winner, stale) = if first.status == 200 {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(winner.status, 200);
    assert_eq!(stale.status, 409);
    assert!(
        winner.body["edited_at"].as_i64().unwrap() > changed.body["edited_at"].as_i64().unwrap()
    );
    assert_eq!(next_event(&mut stream).await.unwrap().0, "refresh");
    let marker = app
        .send(room, &owner, "After edit", "marker", None)
        .await
        .body;
    assert_eq!(
        next_event(&mut stream).await.unwrap(),
        ("message".into(), marker)
    );
}

#[tokio::test]
async fn saves_and_edit_refreshes_respect_conversation_and_user_boundaries() {
    let app = App::new();
    let owner = app.create("author").await;
    let invite = app.invite(&owner).await;
    let reader = app.join("reader", &invite).await;
    let bystander = app.join("third", &invite).await;
    let second_login = app
        .post(
            "/auth/login",
            "",
            json!({"handle":"reader","password":PASSWORD}),
        )
        .await
        .cookie;
    let outsider = app.create("outside").await;
    let state = app.get("/state", &reader).await.body;
    let room = id(&state["channels"][0]);
    let dm = id(&app
        .post("/dms", &owner, json!({"user_id":id(&state["me"])}))
        .await
        .body);
    let root = id(&app
        .send(dm, &owner, "DM discussion", "root", None)
        .await
        .body);
    let reply = id(&app
        .send(dm, &owner, "Private reply", "reply", Some(root))
        .await
        .body);
    let mut one = app.events(&reader).await;
    let mut two = app.events(&second_login).await;
    let mut sender = app.events(&owner).await;
    let mut third = app.events(&bystander).await;
    let saved = save(&app, &reader, reply, true).await.body;
    assert_eq!(saved["saved"], true);
    assert_eq!(saved["parent_id"], root);
    assert_eq!(saved["channel_id"], dm);
    assert_eq!(save(&app, &reader, reply, true).await.body, saved);
    let private_refresh = (
        "refresh".to_string(),
        json!({"channel_id":dm,"message_id":reply,"saved_changed":true}),
    );
    assert_eq!(next_event(&mut one).await.unwrap(), private_refresh);
    assert_eq!(next_event(&mut two).await.unwrap(), private_refresh);
    for other in [&owner, &bystander, &outsider] {
        assert_eq!(app.get("/saved", other).await.body, json!([]));
    }
    for hidden in [&bystander, &outsider] {
        assert_eq!(save(&app, hidden, reply, true).await.status, 404);
        assert_eq!(
            edit(&app, hidden, reply, "hidden", &Value::Null)
                .await
                .status,
            404
        );
    }
    assert_eq!(app.get("/saved", "").await.status, 401);
    assert_eq!(save(&app, "", reply, true).await.status, 401);
    let changed = edit(&app, &owner, reply, "Revised private reply", &Value::Null)
        .await
        .body;
    assert_eq!(changed["saved"], false);
    let conversation_refresh = (
        "refresh".to_string(),
        json!({"channel_id":dm,"message_id":reply}),
    );
    for entitled in [&mut one, &mut two, &mut sender] {
        assert_eq!(next_event(entitled).await.unwrap(), conversation_refresh);
    }
    let marker = app
        .send(room, &owner, "Room update", "marker", None)
        .await
        .body;
    assert_eq!(
        next_event(&mut third).await.unwrap(),
        ("message".into(), marker)
    );
    let list = app.get("/saved", &second_login).await.body;
    let centered = format!("/channels/{dm}/messages?parent_id={root}&around={reply}");
    assert_eq!(list[0]["saved"], true);
    assert_eq!(list[0]["body"], changed["body"]);
    assert_eq!(app.get(&centered, &reader).await.body, list);
    assert_eq!(app.get(&centered, &owner).await.body[0]["saved"], false);
    assert_eq!(app.get("/search?q=Revised", &reader).await.body, list);
    assert_eq!(
        save(&app, &second_login, reply, false).await.body["saved"],
        false
    );
    assert_eq!(app.get("/saved", &reader).await.body, json!([]));
}

#[tokio::test]
async fn saved_pages_order_by_message_id_across_a_restart() {
    let mut app = App::new();
    let owner = app.create("saver").await;
    let room = id(&app.get("/state", &owner).await.body["channels"][0]);
    let mut ids = Vec::new();
    for number in 0..52 {
        ids.push(id(&app
            .send(room, &owner, "For later", &number.to_string(), None)
            .await
            .body));
    }
    for &message in ids.iter().rev() {
        assert_eq!(save(&app, &owner, message, true).await.status, 200);
    }
    app.service = rill_chat::service(&app.path, false).unwrap();
    let first = app.get("/saved", &owner).await.body;
    let returned: Vec<_> = first.as_array().unwrap().iter().map(id).collect();
    assert_eq!(
        returned,
        ids.iter().rev().take(50).copied().collect::<Vec<_>>()
    );
    let next = app
        .get(&format!("/saved?before={}", returned[49]), &owner)
        .await
        .body;
    assert_eq!(
        next.as_array().unwrap().iter().map(id).collect::<Vec<_>>(),
        [ids[1], ids[0]]
    );
}

#[test]
fn version_one_migration_preserves_content_and_rolls_back_incomplete_upgrades() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("version-one.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("../../src/schema.sql"))
        .unwrap();
    connection.execute_batch(
        "INSERT INTO workspaces(id,name) VALUES(1,'Legacy workspace');
         INSERT INTO users(id,workspace_id,handle,name,password_hash) VALUES(1,1,'writer','Writer','fixture');
         INSERT INTO channels(id,workspace_id,name,topic) VALUES(1,1,'news','');
         INSERT INTO messages(id,channel_id,author_id,body,client_id,created_at) VALUES(1,1,1,'昨日の図書館','legacy',1);
         CREATE TABLE saved_messages(conflicting_fixture INTEGER);"
    ).unwrap();
    assert!(rill_chat::service(&path, false).is_err());
    let version = || {
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap()
    };
    assert_eq!(version(), 1);
    assert!(
        connection
            .prepare("SELECT edited_at FROM messages")
            .is_err()
    );
    connection.execute("DROP TABLE saved_messages", []).unwrap();
    for _ in 0..2 {
        drop(rill_chat::service(&path, false).unwrap());
        assert_eq!(version(), 2);
        let row: (i64, Option<i64>, Option<Vec<u8>>) = connection.query_row(
            "SELECT m.id,m.edited_at,m.original_body_hash FROM messages m JOIN message_search ON message_search.rowid=m.id WHERE message_search MATCH '図書館'",
            [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        ).unwrap();
        assert_eq!(row, (1, None, None));
    }
}
