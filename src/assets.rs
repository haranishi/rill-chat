use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use include_dir::{Dir, include_dir};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path, sync::LazyLock};

static WEB: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web");
static BROWSER: Dir<'_> = include_dir!("$OUT_DIR/browser");
static ASSETS: LazyLock<HashMap<&'static Path, Asset>> = LazyLock::new(|| {
    let mut entries = HashMap::new();
    index(&WEB, &mut entries);
    index(&BROWSER, &mut entries);
    entries
});

struct Asset {
    mime: &'static str,
    identity: Representation,
    gzip: Option<Representation>,
}

struct Representation {
    bytes: &'static [u8],
    etag: HeaderValue,
}

impl Representation {
    fn new(bytes: &'static [u8]) -> Self {
        let hash = hex::encode(Sha256::digest(bytes));
        let etag = HeaderValue::from_str(&format!("\"{hash}\"")).expect("hex ETag");
        Self { bytes, etag }
    }
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
            Some("wasm") => "application/wasm",
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
        let gzip = dir
            .get_file(format!("{}.gz", path.display()))
            .map(|file| Representation::new(file.contents()));
        entries.insert(
            path,
            Asset {
                mime,
                identity: Representation::new(file.contents()),
                gzip,
            },
        );
    }
    for nested in dir.dirs() {
        index(nested, entries);
    }
}

// Identity is implicitly acceptable except when explicitly excluded. Prefer gzip on a tie.
fn encoding_quality(headers: &HeaderMap) -> (f32, f32) {
    let (mut identity, mut gzip, mut wildcard) = (None, None, None);
    for entry in headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|s| s.split(','))
    {
        let mut fields = entry.split(';').map(str::trim);
        let name = fields.next().unwrap_or_default();
        let q = fields
            .filter_map(|field| field.split_once('='))
            .find(|(k, _)| k.trim().eq_ignore_ascii_case("q"))
            .map_or(1.0, |(_, v)| {
                v.trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|x| (0.0..=1.0).contains(x))
                    .unwrap_or(0.0)
            });
        if name.eq_ignore_ascii_case("gzip") {
            gzip = Some(q);
        } else if name.eq_ignore_ascii_case("identity") {
            identity = Some(q);
        } else if name == "*" {
            wildcard = Some(q);
        }
    }
    (
        identity.unwrap_or(if wildcard == Some(0.0) { 0.0 } else { 1.0 }),
        gzip.or(wildcard).unwrap_or(0.0),
    )
}

pub async fn serve(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let Some(asset) = ASSETS.get(Path::new(path)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (plain, compressed) = encoding_quality(&headers);
    let encoded = asset
        .gzip
        .as_ref()
        .filter(|_| compressed > 0.0 && compressed >= plain);
    if encoded.is_none() && plain == 0.0 {
        return (
            StatusCode::NOT_ACCEPTABLE,
            [(header::VARY, "Accept-Encoding")],
        )
            .into_response();
    }
    let representation = encoded.unwrap_or(&asset.identity);
    let cached = headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .any(|value| {
            value == "*"
                || value.strip_prefix("W/").unwrap_or(value).as_bytes()
                    == representation.etag.as_bytes()
        });
    let mut res = if cached {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (
            [(header::CONTENT_TYPE, asset.mime)],
            Body::from(representation.bytes),
        )
            .into_response()
    };
    res.headers_mut()
        .insert(header::ETAG, representation.etag.clone());
    if encoded.is_some() {
        res.headers_mut()
            .insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    }
    if asset.gzip.is_some() {
        res.headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    }
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, no-cache"),
    );
    res
}
