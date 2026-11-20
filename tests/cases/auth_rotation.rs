use super::*;

#[tokio::test]
async fn a_login_with_the_old_password_cannot_outlive_a_concurrent_rotation() {
    let app = App::new();
    let current = app.create("writer").await;
    let (login, rotation) = tokio::join!(
        app.post(
            "/auth/login",
            "",
            json!({"handle":"writer","password":PASSWORD})
        ),
        app.post(
            "/auth/password",
            &current,
            json!({"current_password":PASSWORD,"new_password":"a different test password"})
        )
    );
    assert_eq!(rotation.status, 200);
    assert_eq!(app.get("/state", &rotation.cookie).await.status, 200);
    assert_eq!(app.get("/state", &current).await.status, 401);
    if login.status == 200 {
        assert_eq!(app.get("/state", &login.cookie).await.status, 401);
    } else {
        assert_eq!(login.status, 401);
    }
}

#[tokio::test]
async fn changing_password_rotates_all_sessions_and_closes_only_that_accounts_streams() {
    let mut app = App::new();
    let owner = app.create("writer").await;
    let peer = app.join("reader", &app.invite(&owner).await).await;
    let login = json!({"handle":"writer","password":PASSWORD});
    let other = app.post("/auth/login", "", login.clone()).await.cookie;
    let mut owner_stream = app.events(&owner).await;
    let mut other_stream = app.events(&other).await;
    let mut peer_stream = app.events(&peer).await;
    let new_password = "replacement for the test only";
    for (current, new, status) in [
        ("incorrect test password", new_password, 401),
        (PASSWORD, "short", 400),
        (PASSWORD, PASSWORD, 400),
    ] {
        let denied = app
            .post(
                "/auth/password",
                &owner,
                json!({"current_password":current,"new_password":new}),
            )
            .await;
        assert_eq!(denied.status, status);
        assert!(denied.cookie.is_empty());
    }
    assert_eq!(app.get("/state", &other).await.status, 200);
    let update = json!({"current_password":PASSWORD,"new_password":new_password});
    assert_eq!(
        app.post("/auth/password", "", update.clone()).await.status,
        401
    );
    let changed = app.post("/auth/password", &owner, update).await;
    assert_eq!(changed.status, 200);
    assert!(!changed.cookie.is_empty());
    assert_ne!(changed.cookie, owner);
    assert!(next_event(&mut owner_stream).await.is_none());
    assert!(next_event(&mut other_stream).await.is_none());
    for stale in [&owner, &other] {
        assert_eq!(app.get("/state", stale).await.status, 401);
    }
    let state = app.get("/state", &changed.cookie).await.body;
    assert_eq!(state["me"]["handle"], "writer");
    let room = id(&state["channels"][0]);
    assert_eq!(
        app.send(room, &changed.cookie, "変更後の投稿", "after-change", None)
            .await
            .status,
        200
    );
    assert_eq!(next_event(&mut peer_stream).await.unwrap().0, "message");
    drop(peer_stream);
    app.service = rill_chat::service(&app.path, false).unwrap();
    assert_eq!(app.get("/state", &changed.cookie).await.status, 200);
    assert_eq!(app.get("/state", &other).await.status, 401);
    assert_eq!(app.post("/auth/login", "", login).await.status, 401);
    assert_eq!(
        app.post(
            "/auth/login",
            "",
            json!({"handle":"writer","password":new_password})
        )
        .await
        .status,
        200
    );
}
