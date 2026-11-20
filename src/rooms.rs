use axum::{Json, extract::State};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::SessionKey,
    events::Update,
    model::{ApiError, Audience, Channel, Member, Result, User, label},
};

const CHANNELS:&str="SELECT c.id,COALESCE(peer.name,c.name) AS name,c.topic,peer.id AS peer_id
    FROM channels c LEFT JOIN users peer ON peer.id=CASE WHEN c.dm_a=?1 THEN c.dm_b WHEN c.dm_b=?1 THEN c.dm_a END
    WHERE c.workspace_id=?2 AND (c.dm_a IS NULL OR c.dm_a=?1 OR c.dm_b=?1)";

fn channel_row(r: &Row<'_>) -> rusqlite::Result<Channel> {
    let peer_id: Option<i64> = r.get("peer_id")?;
    Ok(Channel {
        id: r.get("id")?,
        name: r.get("name")?,
        topic: r.get("topic")?,
        kind: if peer_id.is_some() { "dm" } else { "channel" },
        peer_id,
    })
}

fn channel_by_id(c: &Connection, m: &Member, id: i64) -> Result<Channel> {
    Ok(c.query_row(
        &format!("{CHANNELS} AND c.id=?3"),
        params![m.user.id, m.workspace, id],
        channel_row,
    )?)
}

pub async fn state(State(s): State<AppState>, SessionKey(key): SessionKey) -> Result<Json<Value>> {
    s.db.for_user(key, |connection, member| {
        let workspace = connection.query_row(
            "SELECT id,name,owner_id FROM workspaces WHERE id=?",
            [member.workspace],
            |row| {
                Ok(json!({
                    "id": row.get::<_,i64>(0)?,
                    "name": row.get::<_,String>(1)?,
                    "owner_id": row.get::<_,i64>(2)?,
                }))
            },
        )?;
        let members = connection
            .prepare_cached("SELECT id,name,handle FROM users WHERE workspace_id=? ORDER BY id")?
            .query_map([member.workspace], User::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let channels = connection
            .prepare_cached(&format!("{CHANNELS} ORDER BY c.id"))?
            .query_map((member.user.id, member.workspace), channel_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Json(json!({
            "workspace": workspace, "members": members, "channels": channels, "me": member.user
        })))
    })
    .await
}

#[derive(Deserialize)]
pub struct NewChannel {
    name: String,
    #[serde(default)]
    topic: String,
}

pub async fn add_channel(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Json(input): Json<NewChannel>,
) -> Result<Json<Channel>> {
    let name = label(
        &input.name,
        48,
        "チャンネル名は1〜48文字で入力してください。",
    )?;
    let topic = input.topic.trim().to_owned();
    if topic.chars().count() > 180 || topic.chars().any(char::is_control) {
        return Err(ApiError::bad(
            "説明は改行を含めず180文字以内で入力してください。",
        ));
    }
    let bus = s.updates.clone();
    s.db.for_user(key, move |c, m| {
        let wrote = c.execute(
            "INSERT INTO channels(workspace_id,name,topic) VALUES(?,?,?)
            ON CONFLICT(workspace_id,name COLLATE NOCASE) WHERE dm_a IS NULL DO NOTHING",
            params![m.workspace, name, topic],
        )?;
        if wrote == 0 {
            return Err(ApiError::conflict("同じ名前のチャンネルがあります。"));
        }
        let channel = channel_by_id(c, &m, c.last_insert_rowid())?;
        bus.publish(Update::refresh(Audience::workspace(m.workspace), None));
        Ok(Json(channel))
    })
    .await
}

#[derive(Deserialize)]
pub struct Direct {
    user_id: i64,
}

pub async fn direct(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Json(input): Json<Direct>,
) -> Result<Json<Channel>> {
    let bus = s.updates.clone();
    s.db.for_user(key, move |c, m| {
        let peer = input.user_id;
        if peer == m.user.id {
            return Err(ApiError::bad("相手のメンバーを選んでください。"));
        }
        let exists = c
            .query_row(
                "SELECT 1 FROM users WHERE workspace_id=? AND id=?",
                (m.workspace, peer),
                |_| Ok(()),
            )
            .optional()?;
        if exists.is_none() {
            return Err(ApiError::missing());
        }
        let (a, b) = (peer.min(m.user.id), peer.max(m.user.id));
        let wrote = c.execute(
            "INSERT INTO channels(workspace_id,name,topic,dm_a,dm_b) VALUES(?,'','',?,?)
            ON CONFLICT(dm_a,dm_b) WHERE dm_a IS NOT NULL DO NOTHING",
            params![m.workspace, a, b],
        )?;
        let id = c.query_row(
            "SELECT id FROM channels WHERE dm_a=? AND dm_b=?",
            (a, b),
            |r| r.get(0),
        )?;
        let ch = channel_by_id(c, &m, id)?;
        if wrote > 0 {
            bus.publish(Update::refresh(Audience::direct(m.workspace, a, b), None));
        }
        Ok(Json(ch))
    })
    .await
}
