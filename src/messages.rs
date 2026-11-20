use crate::{
    AppState,
    auth::SessionKey,
    events::Update,
    history::{check_parent, message},
    model::{ApiError, Audience, Member, Message, Result, now_ms},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
pub struct NewMessage {
    body: String,
    client_id: String,
    parent_id: Option<i64>,
}

fn validate_body(body: &str) -> Result<()> {
    if body.trim().is_empty()
        || body.chars().count() > 4000
        || body
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\r' | '\n' | '\t'))
    {
        return Err(ApiError::bad(
            "メッセージは1〜4,000文字で入力してください。",
        ));
    }
    Ok(())
}

fn message_audience(connection: &Connection, member: &Member, id: i64) -> Result<(i64, Audience)> {
    let channel = connection
        .query_row("SELECT channel_id FROM messages WHERE id=?", [id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or_else(ApiError::missing)?;
    Ok((channel, Audience::channel(connection, member, channel)?))
}

pub async fn send(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Path(channel): Path<i64>,
    Json(input): Json<NewMessage>,
) -> Result<Json<Message>> {
    validate_body(&input.body)?;
    if input.client_id.is_empty()
        || input.client_id.len() > 128
        || !input.client_id.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(ApiError::bad("送信識別子が無効です。"));
    }
    let bus = s.updates.clone();
    s.db.for_user(key, move |connection, member| {
        let audience = Audience::channel(connection, &member, channel)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = tx
            .query_row(
                "SELECT id,original_body_hash FROM messages WHERE author_id=? AND client_id=?",
                params![member.user.id, &input.client_id],
                |row| Ok((row.get(0)?, row.get::<_, Option<Vec<u8>>>(1)?)),
            )
            .optional()?;
        if let Some((id, original_hash)) = previous {
            let found = message(&tx, member.user.id, id)?;
            let same_body = match original_hash {
                Some(hash) => hash[..] == Sha256::digest(input.body.as_bytes())[..],
                None => found.body == input.body,
            };
            if found.channel_id != channel || found.parent_id != input.parent_id || !same_body {
                return Err(ApiError::conflict(
                    "この送信識別子は別のメッセージで使用済みです。",
                ));
            }
            return Ok(Json(found));
        }
        check_parent(&tx, channel, input.parent_id)?;
        let created_at = now_ms();
        tx.execute(
            "INSERT INTO messages(channel_id,author_id,parent_id,body,client_id,created_at)
             VALUES(?,?,?,?,?,?)",
            params![
                channel,
                member.user.id,
                input.parent_id,
                &input.body,
                input.client_id,
                created_at
            ],
        )?;
        let committed = Message {
            id: tx.last_insert_rowid(),
            channel_id: channel,
            author_id: member.user.id,
            parent_id: input.parent_id,
            body: input.body,
            created_at,
            edited_at: None,
            saved: false,
            reply_count: 0,
            reactions: Vec::new(),
        };
        tx.commit()?;
        bus.publish(Update::message(audience, &committed));
        Ok(Json(committed))
    })
    .await
}

#[derive(Deserialize)]
pub struct Edit {
    body: String,
    #[serde(deserialize_with = "Option::deserialize")]
    expected_edited_at: Option<i64>,
}

pub async fn edit(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Path(id): Path<i64>,
    Json(input): Json<Edit>,
) -> Result<Json<Message>> {
    validate_body(&input.body)?;
    let bus = s.updates.clone();
    s.db.for_user(key, move |connection, member| {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (channel, audience) = message_audience(&tx, &member, id)?;
        let mut current = message(&tx, member.user.id, id)?;
        if current.author_id != member.user.id {
            return Err(ApiError(StatusCode::FORBIDDEN, "自分が送信したメッセージだけ編集できます。"));
        }
        if current.body == input.body {
            return Ok(Json(current));
        }
        if current.edited_at != input.expected_edited_at {
            return Err(ApiError::conflict("このメッセージは更新されています。最新の内容を確認してください。"));
        }
        // Keep retry identity when the original text is first replaced. Normal sends avoid hashing.
        let original_hash = current.edited_at.is_none().then(|| Sha256::digest(current.body.as_bytes()).to_vec());
        let edited_at = now_ms().max(current.edited_at.unwrap_or(current.created_at).saturating_add(1));
        tx.execute(
            "UPDATE messages SET body=?,edited_at=?,original_body_hash=coalesce(original_body_hash,?) WHERE id=?",
            params![input.body, edited_at, original_hash, id],
        )?;
        tx.commit()?;
        current.body = input.body;
        current.edited_at = Some(edited_at);
        bus.publish(Update::message_refresh(audience, channel, id));
        Ok(Json(current))
    }).await
}

#[derive(Deserialize)]
pub struct Save {
    active: bool,
}

pub async fn save(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Path(id): Path<i64>,
    Json(input): Json<Save>,
) -> Result<Json<Message>> {
    let bus = s.updates.clone();
    s.db.for_user(key, move |connection, member| {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (channel, _) = message_audience(&tx, &member, id)?;
        let sql = if input.active {
            "INSERT INTO saved_messages(user_id,message_id) VALUES(?1,?2) ON CONFLICT DO NOTHING"
        } else {
            "DELETE FROM saved_messages WHERE user_id=?1 AND message_id=?2"
        };
        let changed = tx.execute(sql, (member.user.id, id))?;
        let current = message(&tx, member.user.id, id)?;
        tx.commit()?;
        if changed > 0 {
            bus.publish(Update::saved(&member, channel, id));
        }
        Ok(Json(current))
    })
    .await
}

#[derive(Deserialize)]
pub struct React {
    emoji: String,
    active: bool,
}

pub async fn react(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Path(id): Path<i64>,
    Json(input): Json<React>,
) -> Result<Json<Message>> {
    if !["👍", "🎉", "❤️", "👀"].contains(&input.emoji.as_str()) {
        return Err(ApiError::bad("リアクションを一覧から選んでください。"));
    }
    let bus = s.updates.clone();
    s.db.for_user(key, move |connection, member| {
        let (channel, audience) = message_audience(connection, &member, id)?;
        let changed = if input.active {
            connection.execute(
                "INSERT INTO reactions(message_id,user_id,emoji) VALUES(?,?,?) ON CONFLICT DO NOTHING",
                params![id, member.user.id, input.emoji],
            )?
        } else {
            connection.execute(
                "DELETE FROM reactions WHERE message_id=? AND user_id=? AND emoji=?",
                params![id, member.user.id, input.emoji],
            )?
        };
        let updated = message(connection, member.user.id, id)?;
        if changed > 0 {
            bus.publish(Update::message_refresh(audience, channel, id));
        }
        Ok(Json(updated))
    }).await
}
