use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);

impl ApiError {
    pub fn bad(message: &'static str) -> Self {
        Self(StatusCode::BAD_REQUEST, message)
    }
    pub fn missing() -> Self {
        Self(
            StatusCode::NOT_FOUND,
            "指定した会話またはメッセージが見つかりません。",
        )
    }
    pub fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "ログインしてください。")
    }
    pub fn conflict(message: &'static str) -> Self {
        Self(StatusCode::CONFLICT, message)
    }
    pub fn busy() -> Self {
        Self(
            StatusCode::TOO_MANY_REQUESTS,
            "少し時間を置いてからもう一度お試しください。",
        )
    }
    pub fn internal() -> Self {
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "サーバーで処理を完了できませんでした。",
        )
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        // Log only the SQLite error category. Bound parameters can contain private conversations.
        eprintln!("Rill database error ({:?})", e.sqlite_error_code());
        Self::internal()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn label(value: &str, max: usize, error: &'static str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        Err(ApiError::bad(error))
    } else {
        Ok(value.to_owned())
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub handle: String,
}

impl User {
    pub fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            name: r.get("name")?,
            handle: r.get("handle")?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Member {
    pub user: User,
    pub workspace: i64,
    pub expires_at: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct Audience {
    pub workspace: i64,
    pub recipients: Recipients,
}

#[derive(Clone, Copy, Debug)]
pub enum Recipients {
    Workspace,
    Direct(i64, i64),
    User(i64),
}

impl Audience {
    pub fn workspace(id: i64) -> Self {
        Self {
            workspace: id,
            recipients: Recipients::Workspace,
        }
    }
    pub fn direct(workspace: i64, a: i64, b: i64) -> Self {
        Self {
            workspace,
            recipients: Recipients::Direct(a, b),
        }
    }
    pub fn member(member: &Member) -> Self {
        Self {
            workspace: member.workspace,
            recipients: Recipients::User(member.user.id),
        }
    }
    pub fn channel(db: &Connection, member: &Member, channel: i64) -> Result<Self> {
        db.query_row(
            "SELECT workspace_id, dm_a, dm_b FROM channels WHERE id=?1
             AND workspace_id=?2 AND (dm_a IS NULL OR dm_a=?3 OR dm_b=?3)",
            params![channel, member.workspace, member.user.id],
            |row| {
                let a: Option<i64> = row.get(1)?;
                let b: Option<i64> = row.get(2)?;
                Ok(Self {
                    workspace: row.get(0)?,
                    recipients: match a.zip(b) {
                        Some((a, b)) => Recipients::Direct(a, b),
                        None => Recipients::Workspace,
                    },
                })
            },
        )
        .optional()?
        .ok_or_else(ApiError::missing)
    }
}

#[derive(Serialize, Debug)]
pub struct Channel {
    pub id: i64,
    pub name: String,
    pub topic: String,
    pub kind: &'static str,
    pub peer_id: Option<i64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Reaction {
    pub emoji: String,
    pub count: i64,
    pub mine: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub body: String,
    pub parent_id: Option<i64>,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    pub saved: bool,
    pub reply_count: i64,
    pub reactions: Vec<Reaction>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub before: Option<i64>,
    pub around: Option<i64>,
    pub parent_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Deserialize)]
pub struct SavedQuery {
    pub before: Option<i64>,
}
