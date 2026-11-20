use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;
use tower::ServiceExt;

#[path = "cases/auth_rotation.rs"]
mod auth_rotation;
#[path = "cases/message_tools.rs"]
mod message_tools;

const PASSWORD: &str = "example-only password";
struct App {
    _temp: TempDir,
    path: PathBuf,
    service: rill_chat::Service,
}
struct Reply {
    status: StatusCode,
    body: Value,
    cookie: String,
}

impl App {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("team.sqlite3");
        let service = rill_chat::service(&path, false).unwrap();
        Self {
            _temp: temp,
            path,
            service,
        }
    }
    fn router(&self) -> &Router {
        &self.service.router
    }
    async fn create(&self, handle: &str) -> String {
        let r=self.post("/auth/create","",json!({"workspace":format!("{handle}'s studio"),"name":handle,"handle":handle,"password":PASSWORD})).await;
        assert_eq!(r.status, 200);
        r.cookie
    }
    async fn invite(&self, owner: &str) -> String {
        let r = self.post("/invites", owner, json!({})).await;
        assert_eq!(r.status, 200);
        r.body["code"].as_str().unwrap().to_owned()
    }
    async fn join(&self, handle: &str, code: &str) -> String {
        let r = self
            .post(
                "/auth/join",
                "",
                json!({"code":code,"name":handle,"handle":handle,"password":PASSWORD}),
            )
            .await;
        assert_eq!(r.status, 200);
        r.cookie
    }
    async fn call(&self, method: Method, path: &str, cookie: &str, value: Value) -> Reply {
        let mut build = Request::builder()
            .method(&method)
            .uri(format!("/api{path}"));
        if !cookie.is_empty() {
            build = build.header(header::COOKIE, cookie)
        }
        let body = if method == Method::GET {
            Body::empty()
        } else {
            build = build.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        };
        let response = self
            .router()
            .clone()
            .oneshot(build.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|h| h.to_str().unwrap().split(';').next().unwrap().to_owned())
            .unwrap_or_default();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap();
        Reply {
            status,
            body,
            cookie,
        }
    }
    async fn post(&self, path: &str, cookie: &str, value: Value) -> Reply {
        self.call(Method::POST, path, cookie, value).await
    }
    async fn get(&self, path: &str, cookie: &str) -> Reply {
        self.call(Method::GET, path, cookie, Value::Null).await
    }
    async fn send(
        &self,
        channel: i64,
        cookie: &str,
        body: &str,
        key: &str,
        parent: Option<i64>,
    ) -> Reply {
        self.post(
            &format!("/channels/{channel}/messages"),
            cookie,
            json!({"body":body,"client_id":key,"parent_id":parent}),
        )
        .await
    }
    async fn events(&self, cookie: &str) -> Body {
        let response = self
            .router()
            .clone()
            .oneshot(
                Request::get("/api/events")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let mut body = response.into_body();
        let ready = next_event(&mut body).await.unwrap();
        assert_eq!(ready.0, "ready");
        body
    }
}

async fn next_event(body: &mut Body) -> Option<(String, Value)> {
    let frame = tokio::time::timeout(Duration::from_secs(3), body.frame())
        .await
        .expect("event timed out")?
        .unwrap();
    let data = frame.into_data().unwrap();
    let lines = std::str::from_utf8(&data).unwrap();
    let kind = lines
        .lines()
        .find_map(|l| l.strip_prefix("event:"))
        .unwrap()
        .trim()
        .to_owned();
    let value = lines
        .lines()
        .find_map(|l| l.strip_prefix("data:"))
        .unwrap()
        .trim();
    Some((kind, serde_json::from_str(value).unwrap()))
}

fn id(obj: &Value) -> i64 {
    obj["id"].as_i64().unwrap()
}

#[tokio::test]
async fn sessions_and_messages_survive_restart_and_logout_revokes_one_session() {
    let mut app = App::new();
    let owner = app.create("owner").await;
    let state = app.get("/state", &owner).await.body;
    assert_eq!(state["channels"].as_array().unwrap().len(), 3);
    assert_eq!(state["members"].as_array().unwrap().len(), 1);
    let room = id(&state["channels"][0]);
    assert_eq!(
        app.send(room, &owner, "明日の打ち合わせ", "user-message", None)
            .await
            .status,
        200
    );
    app.service = rill_chat::service(&app.path, false).unwrap();
    let path = format!("/channels/{room}/messages");
    assert_eq!(
        app.get(&path, &owner).await.body[0]["body"],
        "明日の打ち合わせ"
    );
    assert_eq!(app.get(&path, "").await.status, 401);
    assert_eq!(
        app.post(
            "/auth/login",
            "",
            json!({"handle":"owner","password":"incorrect value"})
        )
        .await
        .status,
        401
    );
    let second = app
        .post(
            "/auth/login",
            "",
            json!({"handle":"OwNeR","password":PASSWORD}),
        )
        .await;
    assert_eq!(second.status, 200);
    let mut events = app.events(&second.cookie).await;
    assert_eq!(
        app.post("/auth/logout", &second.cookie, json!({}))
            .await
            .status,
        200
    );
    assert!(next_event(&mut events).await.is_none());
    assert_eq!(app.get("/state", &second.cookie).await.status, 401);
    assert_eq!(app.get("/state", &owner).await.status, 200);
}

#[tokio::test]
async fn concurrent_retries_threads_reactions_and_literal_search() {
    let app = App::new();
    let owner = app.create("artist").await;
    let code = app.invite(&owner).await;
    let peer = app.join("editor", &code).await;
    let state = app.get("/state", &owner).await.body;
    let room = id(&state["channels"][0]);
    let other = id(&state["channels"][1]);
    let send = || app.send(room, &owner, "図書館の赤い本と\nひかり", "client-key", None);
    let (a, b) = tokio::join!(send(), send());
    assert_eq!((a.status, b.status), (StatusCode::OK, StatusCode::OK));
    let root_id = id(&a.body);
    assert_eq!(id(&b.body), root_id);
    assert_eq!(
        app.send(room, &owner, "different", "client-key", None)
            .await
            .status,
        409
    );
    assert_eq!(
        app.send(other, &owner, "different room", "m2", Some(root_id))
            .await
            .status,
        404
    );
    let reply = app
        .send(room, &peer, "確認しました", "reply", Some(root_id))
        .await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        app.send(room, &owner, "nested", "nested", Some(id(&reply.body)))
            .await
            .status,
        404
    );
    let list = format!("/channels/{room}/messages");
    let roots = app.get(&list, &owner).await.body;
    assert_eq!(roots.as_array().unwrap().len(), 1);
    assert_eq!(roots[0]["reply_count"], 1);
    assert_eq!(
        app.get(&format!("{list}?parent_id={root_id}"), &peer)
            .await
            .body[0]["body"],
        "確認しました"
    );
    let react = format!("/messages/{root_id}/reactions");
    for _ in 0..2 {
        let r = app
            .post(&react, &owner, json!({"emoji":"🎉","active":true}))
            .await;
        assert_eq!(
            r.body["reactions"][0],
            json!({"emoji":"🎉","count":1,"mine":true})
        );
    }
    assert_eq!(
        app.get(&list, &peer).await.body[0]["reactions"][0]["mine"],
        false
    );
    assert_eq!(
        app.post(&react, &owner, json!({"emoji":"🎉","active":false}))
            .await
            .body["reactions"],
        json!([])
    );
    for q in ["%E5%9B%B3%E6%9B%B8%E9%A4%A8", "%E5%9B%B3%E6%9B%B8"] {
        let hits = app.get(&format!("/search?q={q}"), &peer).await.body;
        assert_eq!(hits.as_array().unwrap().len(), 1);
        assert_eq!(id(&hits[0]), root_id);
    }
    assert_eq!(app.get("/search?q=%25", &peer).await.body, json!([]));
}

#[tokio::test]
async fn private_messages_are_scoped_in_state_search_mutations_and_event_streams() {
    let app = App::new();
    let owner = app.create("alice").await;
    let invite = app.invite(&owner).await;
    let bob = app.join("bob", &invite).await;
    let carol = app.join("carol", &invite).await;
    let outsider = app.create("other").await;
    let state = app.get("/state", &bob).await.body;
    let other_state = app.get("/state", &outsider).await.body;
    let room = id(&state["channels"][0]);
    let other_room = id(&other_state["channels"][0]);
    let dm = app
        .post("/dms", &owner, json!({"user_id":id(&state["me"])}))
        .await;
    assert_eq!(dm.status, 200);
    let dm = id(&dm.body);
    let same = app
        .post("/dms", &bob, json!({"user_id":id(&state["members"][0])}))
        .await;
    assert_eq!(id(&same.body), dm);
    assert_eq!(
        app.post("/dms", &outsider, json!({"user_id":id(&state["me"])}))
            .await
            .status,
        404
    );
    assert_eq!(app.post("/invites", &bob, json!({})).await.status, 403);
    let mut b_events = app.events(&bob).await;
    let mut c_events = app.events(&carol).await;
    let mut o_events = app.events(&outsider).await;
    let private = app.send(dm, &owner, "private drawing", "m1", None).await;
    let msg = id(&private.body);
    assert_eq!(
        app.send(room, &owner, "team update", "m2", None)
            .await
            .status,
        200
    );
    assert_eq!(
        app.send(other_room, &outsider, "other team", "mark", None)
            .await
            .status,
        200
    );
    assert_eq!(id(&next_event(&mut b_events).await.unwrap().1), msg);
    assert_eq!(
        next_event(&mut c_events).await.unwrap().1["body"],
        "team update"
    );
    assert_eq!(
        next_event(&mut o_events).await.unwrap().1["body"],
        "other team"
    );
    assert_eq!(
        app.get("/search?q=drawing", &bob)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for unauthorized in [&carol, &outsider] {
        let list = app.get("/state", unauthorized).await.body;
        assert!(
            list["channels"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| id(c) != dm)
        );
        assert_eq!(
            app.get(&format!("/channels/{dm}/messages"), unauthorized)
                .await
                .status,
            404
        );
        assert_eq!(
            app.send(dm, unauthorized, "unavailable", "mine", None)
                .await
                .status,
            404
        );
        assert_eq!(
            app.get("/search?q=drawing", unauthorized).await.body,
            json!([])
        );
        assert_eq!(
            app.post(
                &format!("/messages/{msg}/reactions"),
                unauthorized,
                json!({"emoji":"👍","active":true})
            )
            .await
            .status,
            404
        );
    }
    assert_eq!(
        app.get(&format!("/channels/{room}/messages"), &outsider)
            .await
            .status,
        404
    );
    assert_eq!(app.get("/search?q=update", &outsider).await.body, json!([]));
}

#[tokio::test]
async fn paginated_history_and_reconnect_recover_after_a_lagged_stream() {
    let app = App::new();
    let owner = app.create("writer").await;
    let state = app.get("/state", &owner).await.body;
    let room = id(&state["channels"][0]);
    let mut stalled = app.events(&owner).await;
    let mut ids = Vec::new();
    for n in 1..=140 {
        let reply = app
            .send(room, &owner, "line", &format!("line-{n}"), None)
            .await;
        ids.push(id(&reply.body));
    }
    assert_eq!(next_event(&mut stalled).await.unwrap().0, "resync");
    drop(stalled);
    let mut reconnected = app.events(&owner).await;
    let path = format!("/channels/{room}/messages");
    let newest = app.get(&path, &owner).await.body;
    assert_eq!(newest.as_array().unwrap().len(), 50);
    assert_eq!(id(&newest[0]), ids[90]);
    let older = app
        .get(&format!("{path}?before={}", ids[10]), &owner)
        .await
        .body;
    assert_eq!(older.as_array().unwrap().len(), 10);
    assert_eq!(id(&older[9]), ids[9]);
    let around = app
        .get(&format!("{path}?around={}", ids[28]), &owner)
        .await
        .body;
    assert_eq!(id(&around[24]), ids[28]);
    assert_eq!(around.as_array().unwrap().len(), 50);
    let new = app
        .send(room, &owner, "next", "after-reconnect", None)
        .await;
    assert_eq!(
        id(&next_event(&mut reconnected).await.unwrap().1),
        id(&new.body)
    );
}

#[tokio::test]
async fn expiry_csrf_validation_and_secure_cookie() {
    let mut app = App::new();
    app.service = rill_chat::service(&app.path, true).unwrap();
    let signup = json!({"workspace":"studio","name":"Owner","handle":"owner","password":PASSWORD});
    for (origin, kind, expected) in [
        ("https://other.invalid", "application/json", 403),
        ("http://team.invalid", "application/json", 403),
        ("https://team.invalid", "text/plain", 415),
    ] {
        let req = Request::post("/api/auth/create")
            .header(header::ORIGIN, origin)
            .header(header::HOST, "team.invalid")
            .header(header::CONTENT_TYPE, kind)
            .body(Body::from(signup.to_string()))
            .unwrap();
        assert_eq!(
            app.router().clone().oneshot(req).await.unwrap().status(),
            expected
        );
    }
    let req = Request::post("/api/auth/create")
        .header(header::ORIGIN, "https://team.invalid")
        .header(header::HOST, "team.invalid")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(signup.to_string()))
        .unwrap();
    let res = app.router().clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), 200);
    let cookie = res.headers()[header::SET_COOKIE].to_str().unwrap();
    for flag in ["HttpOnly", "Secure", "SameSite=Strict"] {
        assert!(cookie.contains(flag))
    }
    let owner = cookie.split(';').next().unwrap();
    let code = app.invite(owner).await;
    let db = rusqlite::Connection::open(&app.path).unwrap();
    db.execute("UPDATE invites SET expires_at=0", []).unwrap();
    assert_eq!(
        app.post(
            "/auth/join",
            "",
            json!({"name":"member","handle":"member","password":PASSWORD,"code":code})
        )
        .await
        .status,
        400
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    db.execute("UPDATE sessions SET expires_at=?", [now + 250])
        .unwrap();
    let mut events = app.events(owner).await;
    assert!(next_event(&mut events).await.is_none());
    assert_eq!(app.get("/state", owner).await.status, 401);
}

#[tokio::test]
async fn static_assets_support_conditional_fetches_and_private_api_is_never_cached() {
    let app = App::new();
    let first = app
        .router()
        .clone()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let etag = first.headers()[header::ETAG].clone();
    let repeated = app
        .router()
        .clone()
        .oneshot(
            Request::get("/")
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated.status(), 304);
    assert!(
        repeated
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );
    let private = app
        .router()
        .clone()
        .oneshot(Request::get("/api/state").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(private.status(), 401);
    assert_eq!(private.headers()[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn wasm_receives_representation_specific_validators() {
    let app = App::new();
    let fetch = |encoding: &'static str, etag: Option<header::HeaderValue>| {
        let mut request =
            Request::get("/rill_client_bg.wasm").header(header::ACCEPT_ENCODING, encoding);
        if let Some(tag) = etag {
            request = request.header(header::IF_NONE_MATCH, tag);
        }
        app.router()
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
    };
    let raw = fetch("gzip;q=0", None).await.unwrap();
    assert_eq!(raw.headers()[header::CONTENT_TYPE], "application/wasm");
    assert_eq!(raw.headers()[header::VARY], "Accept-Encoding");
    let plain_tag = raw.headers()[header::ETAG].clone();
    let plain = raw.into_body().collect().await.unwrap().to_bytes();
    assert!(plain.starts_with(b"\0asm"));
    let compact = fetch("gzip", Some(plain_tag.clone())).await.unwrap();
    assert_eq!(compact.status(), 200);
    assert_eq!(compact.headers()[header::CONTENT_ENCODING], "gzip");
    let zip_tag = compact.headers()[header::ETAG].clone();
    assert_ne!(plain_tag, zip_tag);
    let zipped = compact.into_body().collect().await.unwrap().to_bytes();
    assert!(zipped.starts_with(&[0x1f, 0x8b]));
    assert!(zipped.len() < plain.len());
    assert_eq!(fetch("gzip", Some(zip_tag)).await.unwrap().status(), 304);
    assert_eq!(fetch("*;q=0", None).await.unwrap().status(), 406);
}
