use std::sync::Arc;

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::{
    AppState,
    events::Update,
    model::{ApiError, Audience, Result, label, now_ms},
};

const DAY: i64 = 86_400_000;
const SESSION_AGE: i64 = 30 * DAY;

fn secret() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn digest(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

pub struct SessionKey(pub String);

impl SessionKey {
    fn from_headers(headers: &HeaderMap) -> Option<String> {
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|s| s.trim().strip_prefix("rill_session="))
            .find(|s| {
                s.len() == 43
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            })
            .map(digest)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for SessionKey {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        Self::from_headers(&parts.headers)
            .map(Self)
            .ok_or_else(ApiError::unauthorized)
    }
}

fn cookie(raw: &str, age: i64, secure: bool) -> Result<Response> {
    let secure = if secure { "; Secure" } else { "" };
    let value =
        format!("rill_session={raw}; Path=/; HttpOnly; SameSite=Strict; Max-Age={age}{secure}");
    let mut response = Json(json!({})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&value).map_err(|_| ApiError::internal())?,
    );
    Ok(response)
}

fn handle(s: &str) -> Result<String> {
    let clean = s.trim().to_ascii_lowercase();
    if !(3..=32).contains(&clean.len())
        || !clean
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-'))
    {
        return Err(ApiError::bad(
            "ログインIDは半角英数字、ハイフン、下線を使って3〜32文字で入力してください。",
        ));
    }
    Ok(clean)
}

fn valid_password(s: &str) -> Result<()> {
    if !(10..=128).contains(&s.chars().count()) {
        return Err(ApiError::bad("パスワードは10〜128文字で入力してください。"));
    }
    Ok(())
}

async fn password_work<T: Send + 'static>(
    slots: Arc<Semaphore>,
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let permit = slots.try_acquire_owned().map_err(|_| ApiError::busy())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        f()
    })
    .await
    .map_err(|_| ApiError::internal())?
}

fn hash_password(raw: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(raw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| ApiError::internal())
}

pub fn dummy_hash() -> Result<String> {
    hash_password(&secret())
}

#[derive(Deserialize)]
pub struct Create {
    workspace: String,
    #[serde(flatten)]
    identity: Identity,
}
#[derive(Deserialize)]
pub struct Join {
    code: String,
    #[serde(flatten)]
    identity: Identity,
}
#[derive(Deserialize)]
struct Identity {
    name: String,
    handle: String,
    password: String,
}
#[derive(Deserialize)]
pub struct Login {
    handle: String,
    password: String,
}

enum Registration {
    Workspace(String),
    Invitation(String),
}

pub async fn create(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Create>,
) -> Result<Response> {
    let workspace = label(
        &body.workspace,
        48,
        "ワークスペース名は1〜48文字で入力してください。",
    )?;
    register(
        s,
        headers,
        body.identity,
        Registration::Workspace(workspace),
    )
    .await
}

pub async fn join(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Join>,
) -> Result<Response> {
    let raw = body.code.trim();
    if raw.len() != 43 {
        return Err(invalid_invite());
    }
    let hashed = digest(raw);
    let check = hashed.clone();
    s.db.run(move |c| invite_workspace(c, &check)).await?;
    register(s, headers, body.identity, Registration::Invitation(hashed)).await
}

fn invalid_invite() -> ApiError {
    ApiError::bad(
        "招待コードが無効か、期限が切れています。管理者から新しいコードを受け取ってください。",
    )
}

fn invite_workspace(db: &Connection, hashed: &str) -> Result<i64> {
    db.query_row(
        "SELECT workspace_id FROM invites WHERE token_hash=? AND expires_at>?",
        (hashed, now_ms()),
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(invalid_invite)
}

fn set_session(tx: &Transaction<'_>, user: i64, hashed: &str, old: Option<&str>) -> Result<()> {
    let now = now_ms();
    tx.execute("DELETE FROM sessions WHERE expires_at<=?", [now])?;
    if let Some(old) = old {
        tx.execute("DELETE FROM sessions WHERE token_hash=?", [old])?;
    }
    tx.execute(
        "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES(?,?,?)",
        params![hashed, user, now + SESSION_AGE],
    )?;
    Ok(())
}

async fn register(
    s: AppState,
    headers: HeaderMap,
    identity: Identity,
    registration: Registration,
) -> Result<Response> {
    let name = label(&identity.name, 48, "表示名は1〜48文字で入力してください。")?;
    let handle = handle(&identity.handle)?;
    valid_password(&identity.password)?;
    let password_hash = password_work(s.hash_slots.clone(), move || {
        hash_password(&identity.password)
    })
    .await?;
    let raw = secret();
    let hashed = digest(&raw);
    let old = SessionKey::from_headers(&headers);
    let replaces_session = old.is_some();
    let bus = s.updates.clone();
    s.db.run(move |db| {
        let tx = db.transaction()?;
        let taken: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE handle=?)",
            [&handle],
            |r| r.get(0),
        )?;
        if taken {
            return Err(ApiError::conflict("このログインIDはすでに使われています。"));
        }
        let create_workspace = matches!(registration, Registration::Workspace(_));
        let workspace = match registration {
            Registration::Workspace(title) => {
                tx.execute("INSERT INTO workspaces(name) VALUES(?)", [title])?;
                tx.last_insert_rowid()
            }
            Registration::Invitation(code) => invite_workspace(&tx, &code)?,
        };
        tx.execute(
            "INSERT INTO users(workspace_id,name,handle,password_hash) VALUES(?,?,?,?)",
            params![workspace, name, handle, password_hash],
        )?;
        let user = tx.last_insert_rowid();
        if create_workspace {
            tx.execute(
                "UPDATE workspaces SET owner_id=? WHERE id=?",
                (user, workspace),
            )?;
            for (channel, topic) in [
                ("general", "チーム全体のお知らせと日々の共有"),
                ("ideas", "試してみたいアイデアを持ち寄る場所"),
                ("random", "仕事の合間の雑談に"),
            ] {
                tx.execute(
                    "INSERT INTO channels(workspace_id,name,topic) VALUES(?,?,?)",
                    params![workspace, channel, topic],
                )?;
            }
        }
        set_session(&tx, user, &hashed, old.as_deref())?;
        tx.commit()?;
        bus.publish(Update::refresh(Audience::workspace(workspace), None));
        Ok(())
    })
    .await?;
    if replaces_session {
        s.revocations.send_modify(|n| *n = n.wrapping_add(1));
    }
    cookie(&raw, SESSION_AGE / 1000, s.secure)
}

struct Account {
    id: i64,
    hash: String,
    failures: i64,
    window: i64,
}

impl Account {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            hash: row.get(1)?,
            failures: row.get(2)?,
            window: row.get(3)?,
        })
    }

    fn throttled(&self) -> bool {
        self.failures >= 10 && self.window > now_ms() - 60_000
    }
}

fn verify_password(password: &str, encoded: &str) -> Result<bool> {
    let hash = PasswordHash::new(encoded).map_err(|_| ApiError::internal())?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok())
}

async fn record_failure(s: &AppState, id: i64, hash: String) -> Result<()> {
    s.db.run(move |connection| {
        let now = now_ms();
        connection.execute(
            "UPDATE users SET
                login_failures=CASE WHEN failure_window<=?1 THEN 1 ELSE login_failures+1 END,
                failure_window=CASE WHEN failure_window<=?1 THEN ?2 ELSE failure_window END
             WHERE id=?3 AND password_hash=?4",
            params![now - 60_000, now, id, hash],
        )?;
        Ok(())
    })
    .await
}

pub async fn login(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Login>,
) -> Result<Response> {
    let handle = handle(&body.handle)?;
    valid_password(&body.password)?;
    let account =
        s.db.run(move |c| {
            Ok(c.query_row(
                "SELECT id,password_hash,login_failures,failure_window FROM users WHERE handle=?",
                [handle],
                Account::read,
            )
            .optional()?)
        })
        .await?;
    if account.as_ref().is_some_and(Account::throttled) {
        return Err(ApiError::busy());
    }
    // Unknown handles do the same expensive verification without creating persistent throttle rows.
    let hash = account
        .as_ref()
        .map(|a| a.hash.as_str())
        .unwrap_or(&s.dummy_password)
        .to_owned();
    let verified = password_work(s.hash_slots.clone(), move || {
        verify_password(&body.password, &hash)
    })
    .await?;
    let bad = || {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "ログインIDまたはパスワードが違います。",
        )
    };
    let account = account.ok_or_else(bad)?;
    if !verified {
        record_failure(&s, account.id, account.hash).await?;
        return Err(bad());
    }
    let raw = secret();
    let hashed = digest(&raw);
    let old = SessionKey::from_headers(&headers);
    let replaces_session = old.is_some();
    s.db.run(move |db| {
        let tx = db.transaction()?;
        // A password change may have committed while Argon2 verified the old password.
        let matched = tx.execute(
            "UPDATE users SET login_failures=0,failure_window=0 WHERE id=? AND password_hash=?",
            params![account.id, account.hash],
        )?;
        if matched == 0 {
            return Err(bad());
        }
        set_session(&tx, account.id, &hashed, old.as_deref())?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    if replaces_session {
        s.revocations.send_modify(|n| *n = n.wrapping_add(1));
    }
    cookie(&raw, SESSION_AGE / 1000, s.secure)
}

pub async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if let Some(key) = SessionKey::from_headers(&headers) {
        s.db.run(move |c| {
            c.execute("DELETE FROM sessions WHERE token_hash=?", [key])?;
            Ok(())
        })
        .await?;
        s.revocations.send_modify(|n| *n = n.wrapping_add(1));
    }
    cookie("", 0, s.secure)
}

#[derive(Deserialize)]
pub struct ChangePassword {
    current_password: String,
    new_password: String,
}

pub async fn change_password(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Json(input): Json<ChangePassword>,
) -> Result<Response> {
    valid_password(&input.current_password)?;
    valid_password(&input.new_password)?;
    if input.current_password == input.new_password {
        return Err(ApiError::bad(
            "現在のものと異なるパスワードを入力してください。",
        ));
    }
    let account =
        s.db.for_user(key.clone(), |c, member| {
            Ok(c.query_row(
                "SELECT id,password_hash,login_failures,failure_window FROM users WHERE id=?",
                [member.user.id],
                Account::read,
            )?)
        })
        .await?;
    if account.throttled() {
        return Err(ApiError::busy());
    }
    let previous = account.hash.clone();
    let next = password_work(s.hash_slots.clone(), move || {
        if verify_password(&input.current_password, &previous)? {
            Ok(Some(hash_password(&input.new_password)?))
        } else {
            Ok(None)
        }
    })
    .await?;
    let Some(next) = next else {
        record_failure(&s, account.id, account.hash).await?;
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "現在のパスワードが違います。",
        ));
    };
    let raw = secret();
    let hashed = digest(&raw);
    let revocations = s.revocations.clone();
    s.db.for_user(key, move |c, member| {
        let tx = c.transaction()?;
        let changed = tx.execute(
            "UPDATE users SET password_hash=?,login_failures=0,failure_window=0
             WHERE id=? AND password_hash=?",
            params![next, member.user.id, account.hash],
        )?;
        if changed == 0 {
            return Err(ApiError::conflict(
                "認証情報が更新されました。もう一度ログインしてください。",
            ));
        }
        tx.execute("DELETE FROM sessions WHERE user_id=?", [member.user.id])?;
        set_session(&tx, member.user.id, &hashed, None)?;
        tx.commit()?;
        revocations.send_modify(|n| *n = n.wrapping_add(1));
        Ok(())
    })
    .await?;
    cookie(&raw, SESSION_AGE / 1000, s.secure)
}

pub async fn invite(State(s): State<AppState>, SessionKey(key): SessionKey) -> Result<Json<Value>> {
    s.db.for_user(key, move |c, m| {
        let owner: i64 = c.query_row(
            "SELECT owner_id FROM workspaces WHERE id=?",
            [m.workspace],
            |r| r.get(0),
        )?;
        if owner != m.user.id {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "招待コードはワークスペースの管理者が発行できます。",
            ));
        }
        let raw = secret();
        let now = now_ms();
        let expires_at = now + DAY;
        let tx = c.transaction()?;
        tx.execute("DELETE FROM invites WHERE expires_at<=?", [now])?;
        tx.execute(
            "INSERT INTO invites(token_hash,workspace_id,expires_at) VALUES(?,?,?)",
            params![digest(&raw), m.workspace, expires_at],
        )?;
        tx.commit()?;
        Ok(Json(json!({"code":raw,"expires_at":expires_at})))
    })
    .await
}
