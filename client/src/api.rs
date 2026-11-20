use serde::de::DeserializeOwned;
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestCache, RequestCredentials, RequestInit, Response};

#[derive(Clone, Debug)]
pub struct Error {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(&self.message)
    }
}

fn connection_error(_: JsValue) -> Error {
    Error {
        status: 0,
        message: "接続できません。通信が戻ったら、もう一度お試しください。".into(),
    }
}

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, Error> {
    request(path, None).await
}

pub async fn post<T: DeserializeOwned>(path: &str, body: &Value) -> Result<T, Error> {
    request(path, Some(body)).await
}

async fn request<T: DeserializeOwned>(path: &str, body: Option<&Value>) -> Result<T, Error> {
    let init = RequestInit::new();
    init.set_credentials(RequestCredentials::SameOrigin);
    init.set_cache(RequestCache::NoStore);
    if let Some(body) = body {
        init.set_method("POST");
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    let req =
        Request::new_with_str_and_init(&format!("/api{path}"), &init).map_err(connection_error)?;
    req.headers()
        .set("Accept", "application/json")
        .map_err(connection_error)?;
    if body.is_some() {
        req.headers()
            .set("Content-Type", "application/json")
            .map_err(connection_error)?;
    }
    let res: Response = JsFuture::from(crate::dom::window().fetch_with_request(&req))
        .await
        .map_err(connection_error)?
        .dyn_into()
        .map_err(connection_error)?;
    let status = res.status();
    let text = JsFuture::from(res.text().map_err(connection_error)?)
        .await
        .map_err(connection_error)?
        .as_string()
        .unwrap_or_default();
    if !res.ok() {
        let error = serde_json::from_str::<Value>(&text).ok();
        let message = error
            .as_ref()
            .and_then(|v| v.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("処理に失敗しました。もう一度お試しください。")
            .to_owned();
        return Err(Error { status, message });
    }
    serde_json::from_str(if text.is_empty() { "null" } else { &text }).map_err(|_| Error {
        status,
        message: "サーバーの応答を読み取れませんでした。".into(),
    })
}
