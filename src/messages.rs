use crate::{
    AppState,
    auth::SessionKey,
    events::Update,
    history::{check_parent, message},
    model::{ApiError, Audience, Message, Result, now_ms},
};
use axum::{
    Json,
    extract::{Path, State},
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct NewMessage {
    body: String,
    client_id: String,
    parent_id: Option<i64>,
}

pub async fn send(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Path(channel): Path<i64>,
    Json(input): Json<NewMessage>,
) -> Result<Json<Message>> {
    if input.body.trim().is_empty()
        || input.body.chars().count() > 4000
        || input
            .body
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\r' | '\n' | '\t'))
    {
        return Err(ApiError::bad(
            "メッセージは1〜4,000文字で入力してください。",
        ));
    }
    if input.client_id.is_empty()
        || input.client_id.len() > 128
        || !input.client_id.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(ApiError::bad("送信識別子が無効です。"));
    }
    let bus = s.updates.clone();
    s.db.for_user(key, move |connection, member| {
        let audience = Audience::channel(connection, &member, channel)?;
        let tx = connection.transaction()?;
        let previous = tx
            .query_row(
                "SELECT id FROM messages WHERE author_id=? AND client_id=?",
                params![member.user.id, &input.client_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = previous {
            let found = message(&tx, member.user.id, id)?;
            if found.channel_id != channel
                || found.parent_id != input.parent_id
                || found.body != input.body
            {
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
        let channel: i64 = connection.query_row(
            "SELECT channel_id FROM messages WHERE id=?",
            [id],
            |row| row.get(0),
        ).optional()?.ok_or_else(ApiError::missing)?;
        let audience = Audience::channel(connection, &member, channel)?;
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
            bus.publish(Update::reaction(audience, channel, id));
        }
        Ok(Json(updated))
    }).await
}
