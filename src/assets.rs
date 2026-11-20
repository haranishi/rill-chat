use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use include_dir::{Dir, include_dir};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path, sync::LazyLock};

static WEB: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web");
static ASSETS: LazyLock<HashMap<&'static Path, Asset>> = LazyLock::new(|| {
    let mut entries = HashMap::new();
    index(&WEB, &mut entries);
    entries
});

struct Asset {
    mime: &'static str,
    bytes: &'static [u8],
    etag: HeaderValue,
}

fn index(dir: &'static Dir, entries: &mut HashMap<&'static Path, Asset>) {
    for file in dir.files() {
        let path = file.path();
        if path
            .iter()
            .any(|part| part.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let mime = match path.extension().and_then(|s| s.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("js" | "mjs") => "text/javascript; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("ico") => "image/x-icon",
            Some("woff2") => "font/woff2",
            Some("avif") => "image/avif",
            _ => continue,
        };
        let bytes = file.contents();
        let hash = hex::encode(Sha256::digest(bytes));
        let etag =
            HeaderValue::from_str(&format!("\"{hash}\"")).expect("hex ETag is an ASCII header");
        entries.insert(path, Asset { mime, bytes, etag });
    }
    for nested in dir.dirs() {
        index(nested, entries);
    }
}

pub async fn serve(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let Some(asset) = ASSETS.get(Path::new(path)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let cached = headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .any(|value| {
            value == "*"
                || value.strip_prefix("W/").unwrap_or(value).as_bytes() == asset.etag.as_bytes()
        });
    let mut res = if cached {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (
            [(header::CONTENT_TYPE, asset.mime)],
            Body::from(asset.bytes),
        )
            .into_response()
    };
    res.headers_mut().insert(header::ETAG, asset.etag.clone());
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, no-cache"),
    );
    res
}
