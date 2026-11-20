use crate::{
    AppState,
    auth::SessionKey,
    model::{ApiError, Audience, HistoryQuery, Message, Reaction, Result, SavedQuery, SearchQuery},
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use rusqlite::{Connection, OptionalExtension, Params, Row, params};
use std::collections::HashMap;

// Bind the viewer as ?1 in each message projection. Private saves stay local to the reader.
const COLUMNS: &str = "m.id,m.channel_id,m.author_id,m.body,m.parent_id,m.created_at,m.edited_at,
        (SELECT count(*) FROM messages child WHERE child.channel_id=m.channel_id AND child.parent_id=m.id),
        EXISTS(SELECT 1 FROM saved_messages s WHERE s.user_id=?1 AND s.message_id=m.id)";

fn msg(r: &Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        channel_id: r.get(1)?,
        author_id: r.get(2)?,
        body: r.get(3)?,
        parent_id: r.get(4)?,
        created_at: r.get(5)?,
        edited_at: r.get(6)?,
        reply_count: r.get(7)?,
        saved: r.get(8)?,
        reactions: Vec::new(),
    })
}

fn collect(c: &Connection, suffix: &str, params: impl Params) -> Result<Vec<Message>> {
    let query = format!("SELECT {COLUMNS} FROM messages m {suffix}");
    let rows = c
        .prepare_cached(&query)?
        .query_map(params, msg)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn add_reactions(c: &Connection, viewer: i64, rows: &mut [Message]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let ids = rows.iter().map(|r| r.id).collect::<Vec<_>>();
    let indexes: HashMap<_, _> = ids
        .iter()
        .copied()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let keys = serde_json::to_string(&ids).map_err(|_| ApiError::internal())?;
    let mut stmt=c.prepare_cached("SELECT message_id,emoji,count(*),max(user_id=?1) FROM reactions
        WHERE message_id IN (SELECT value FROM json_each(?2)) GROUP BY message_id,emoji ORDER BY message_id,emoji")?;
    let mut aggregates = stmt.query((viewer, keys))?;
    while let Some(r) = aggregates.next()? {
        let id: i64 = r.get(0)?;
        let index = indexes[&id];
        rows[index].reactions.push(Reaction {
            emoji: r.get(1)?,
            count: r.get(2)?,
            mine: r.get(3)?,
        });
    }
    Ok(())
}

pub fn message(c: &Connection, viewer: i64, id: i64) -> Result<Message> {
    let mut row = c
        .query_row(
            &format!("SELECT {COLUMNS} FROM messages m WHERE m.id=?2"),
            [viewer, id],
            msg,
        )
        .optional()?
        .ok_or_else(ApiError::missing)?;
    add_reactions(c, viewer, std::slice::from_mut(&mut row))?;
    Ok(row)
}

pub fn check_parent(c: &Connection, channel: i64, parent: Option<i64>) -> Result<()> {
    if let Some(id) = parent {
        c.query_row(
            "SELECT id FROM messages WHERE channel_id=? AND id=? AND parent_id IS NULL",
            (channel, id),
            |_| Ok(()),
        )
        .optional()?
        .ok_or_else(ApiError::missing)?;
    }
    Ok(())
}

pub async fn history(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Path(channel): Path<i64>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<Message>>> {
    if q.before.is_some() && q.around.is_some() {
        return Err(ApiError::bad("履歴の位置を一つ指定してください。"));
    }
    s.db.for_user(key, move |c, m| {
        Audience::channel(c, &m, channel)?;
        check_parent(c, channel, q.parent_id)?;
        let mut rows;
        if let Some(around) = q.around {
            c.query_row(
                "SELECT id FROM messages WHERE id=? AND channel_id=? AND parent_id IS ?",
                params![around, channel, q.parent_id],
                |_| Ok(()),
            )
            .optional()?
            .ok_or_else(ApiError::missing)?;
            rows = collect(
                c,
                "WHERE m.channel_id=?2 AND m.parent_id IS ?3 AND m.id<?4 ORDER BY m.id DESC LIMIT 24",
                params![m.user.id, channel, q.parent_id, around],
            )?;
            rows.reverse();
            rows.extend(collect(
                c,
                "WHERE m.channel_id=?2 AND m.parent_id IS ?3 AND m.id>=?4 ORDER BY m.id LIMIT ?5",
                params![m.user.id, channel, q.parent_id, around, 50 - rows.len() as i64],
            )?);
        } else {
            rows = collect(
                c,
                "WHERE m.channel_id=?2 AND m.parent_id IS ?3 AND m.id<?4 ORDER BY m.id DESC LIMIT 50",
                params![m.user.id, channel, q.parent_id, q.before.unwrap_or(i64::MAX)],
            )?;
            rows.reverse();
        }
        add_reactions(c, m.user.id, &mut rows)?;
        Ok(Json(rows))
    })
    .await
}

pub async fn search(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<Message>>> {
    let term = crate::model::label(&q.q, 128, "検索語は1〜128文字で入力してください。")?;
    s.db.for_user(key, move |connection, member| {
        // SQLite's trigram tokenizer cannot produce tokens for one- or two-character terms.
        let mut rows = if term.chars().count() >= 3 {
            let phrase = format!("\"{}\"", term.replace('"', "\"\""));
            collect(
                connection,
                "JOIN message_search ON message_search.rowid=m.id
                 JOIN channels ch ON ch.id=m.channel_id
                 WHERE message_search MATCH ?2 AND ch.workspace_id=?3
                   AND (ch.dm_a IS NULL OR ch.dm_a=?1 OR ch.dm_b=?1)
                 ORDER BY m.id DESC LIMIT 50",
                params![member.user.id, phrase, member.workspace],
            )?
        } else {
            collect(
                connection,
                "JOIN channels ch ON ch.id=m.channel_id
                 WHERE ch.workspace_id=?3 AND (ch.dm_a IS NULL OR ch.dm_a=?1 OR ch.dm_b=?1)
                   AND instr(lower(m.body),lower(?2))>0 ORDER BY m.id DESC LIMIT 50",
                params![member.user.id, term, member.workspace],
            )?
        };
        add_reactions(connection, member.user.id, &mut rows)?;
        Ok(Json(rows))
    })
    .await
}

pub async fn saved(
    State(s): State<AppState>,
    SessionKey(key): SessionKey,
    Query(q): Query<SavedQuery>,
) -> Result<Json<Vec<Message>>> {
    s.db.for_user(key, move |connection, member| {
        let mut rows = collect(
            connection,
            "JOIN saved_messages pick ON pick.message_id=m.id
             JOIN channels ch ON ch.id=m.channel_id
             WHERE pick.user_id=?1 AND pick.message_id<?2 AND ch.workspace_id=?3
               AND (ch.dm_a IS NULL OR ch.dm_a=?1 OR ch.dm_b=?1)
             ORDER BY pick.message_id DESC LIMIT 50",
            params![
                member.user.id,
                q.before.unwrap_or(i64::MAX),
                member.workspace
            ],
        )?;
        add_reactions(connection, member.user.id, &mut rows)?;
        Ok(Json(rows))
    })
    .await
}
