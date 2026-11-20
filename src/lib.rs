mod assets;
mod auth;
mod db;
mod events;
mod history;
mod messages;
mod model;
mod rooms;

use crate::model::ApiError;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, Method, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use std::{path::Path, sync::Arc};
use tokio::sync::{Semaphore, watch};

#[derive(Clone)]
pub(crate) struct AppState {
    db: db::Database,
    updates: events::Hub,
    revocations: watch::Sender<u64>,
    stopping: watch::Receiver<bool>,
    hash_slots: Arc<Semaphore>,
    dummy_password: Arc<str>,
    secure: bool,
}

pub struct Service {
    pub router: Router,
    pub shutdown: watch::Sender<bool>,
}

pub fn service(
    path: &Path,
    secure: bool,
) -> std::result::Result<Service, Box<dyn std::error::Error>> {
    let (shutdown, stopping) = watch::channel(false);
    let state = AppState {
        db: db::Database::open(path)?,
        updates: events::Hub::default(),
        revocations: watch::channel(0).0,
        stopping,
        hash_slots: Arc::new(Semaphore::new(4)),
        dummy_password: auth::dummy_hash()
            .map_err(|_| "Password hashing unavailable")?
            .into(),
        secure,
    };
    let api = Router::new()
        .route("/auth/create", post(auth::create))
        .route("/auth/join", post(auth::join))
        .route("/auth/login", post(auth::login))
        .route("/auth/password", post(auth::change_password))
        .route("/auth/logout", post(auth::logout))
        .route("/state", get(rooms::state))
        .route("/invites", post(auth::invite))
        .route("/channels", post(rooms::add_channel))
        .route("/dms", post(rooms::direct))
        .route(
            "/channels/{id}/messages",
            get(history::history).post(messages::send),
        )
        .route("/messages/{id}/reactions", post(messages::react))
        .route("/messages/{id}/edit", post(messages::edit))
        .route("/messages/{id}/saved", post(messages::save))
        .route("/saved", get(history::saved))
        .route("/search", get(history::search))
        .route("/events", get(events::events))
        .fallback(|| async { ApiError::missing() })
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(secure, api_headers));
    let router = Router::new()
        .nest("/api", api)
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .fallback(get(assets::serve))
        .layer(middleware::from_fn(response_headers))
        .with_state(state);
    Ok(Service { router, shutdown })
}

fn reject_mutation(request: &Request, secure: bool) -> Option<ApiError> {
    if request.method() != Method::POST {
        return None;
    }
    let headers = request.headers();
    let denied = || {
        ApiError(
            StatusCode::FORBIDDEN,
            "同じサイトから操作し直してください。",
        )
    };
    if headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site")
    {
        return Some(denied());
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().ok().and_then(|s| s.parse::<Uri>().ok());
        let host = headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| request.uri().authority().map(|a| a.as_str()));
        if !origin.as_ref().is_some_and(|uri| {
            (uri.scheme_str() == Some("https") || (!secure && uri.scheme_str() == Some("http")))
                && uri
                    .authority()
                    .zip(host)
                    .is_some_and(|(a, h)| a.as_str().eq_ignore_ascii_case(h))
        }) {
            return Some(denied());
        }
    }
    let json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"));
    if !json {
        return Some(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "JSON形式で送信してください。",
        ));
    }
    None
}

async fn api_headers(State(secure): State<bool>, request: Request, next: Next) -> Response {
    let mut response = if let Some(rejection) = reject_mutation(&request, secure) {
        rejection.into_response()
    } else {
        next.run(request).await
    };
    if response.status().is_client_error()
        && !response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|h| h.to_str().unwrap_or("").starts_with("application/json"))
    {
        response = ApiError(
            response.status(),
            "リクエストの項目または形式を確認してください。",
        )
        .into_response();
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn response_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("content-security-policy",HeaderValue::from_static(
        "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'self'"
    ));
    response
}
