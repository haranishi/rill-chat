use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    extract::State,
    http::HeaderValue,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use serde_json::json;
use tokio::sync::broadcast;

use crate::{
    AppState,
    auth::SessionKey,
    model::{Audience, Member, Message, Recipients, Result, now_ms},
};

#[derive(Clone)]
pub struct Update {
    audience: Audience,
    kind: &'static str,
    data: Arc<str>,
}

impl Update {
    pub fn refresh(audience: Audience, channel: Option<i64>) -> Self {
        let data = channel.map_or_else(|| json!({}), |id| json!({"channel_id":id}));
        Self {
            audience,
            kind: "refresh",
            data: data.to_string().into(),
        }
    }
    pub fn message_refresh(audience: Audience, channel: i64, message: i64) -> Self {
        Self {
            audience,
            kind: "refresh",
            data: json!({"channel_id":channel,"message_id":message})
                .to_string()
                .into(),
        }
    }
    pub fn saved(member: &Member, channel: i64, message: i64) -> Self {
        Self {
            audience: Audience::member(member),
            kind: "refresh",
            data: json!({"channel_id":channel,"message_id":message,"saved_changed":true})
                .to_string()
                .into(),
        }
    }
    pub fn message(audience: Audience, msg: &Message) -> Self {
        // Only new messages are broadcast here. They have no saves or reactions yet.
        Self {
            audience,
            kind: "message",
            data: serde_json::to_string(msg)
                .expect("serialize message primitives")
                .into(),
        }
    }
}

struct Feed {
    sessions: usize,
    sender: broadcast::Sender<Update>,
}
type OnlineMembers = HashMap<i64, Feed>;

#[derive(Clone, Default)]
pub struct Hub(Arc<Mutex<HashMap<i64, OnlineMembers>>>);

impl Hub {
    fn subscribe(&self, member: &Member) -> Subscription {
        let mut workspaces = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let feed = workspaces
            .entry(member.workspace)
            .or_default()
            .entry(member.user.id)
            .or_insert_with(|| Feed {
                sessions: 0,
                sender: broadcast::channel(128).0,
            });
        feed.sessions += 1;
        Subscription {
            hub: self.clone(),
            workspace: member.workspace,
            user: member.user.id,
            receiver: feed.sender.subscribe(),
        }
    }

    pub fn publish(&self, update: Update) {
        let workspaces = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(online) = workspaces.get(&update.audience.workspace) else {
            return;
        };
        let send = |feed: &Feed| {
            let _ = feed.sender.send(update.clone());
        };
        match update.audience.recipients {
            Recipients::User(id) => online.get(&id).into_iter().for_each(send),
            Recipients::Direct(a, b) => {
                [a, b].iter().filter_map(|id| online.get(id)).for_each(send);
            }
            Recipients::Workspace => online.values().for_each(send),
        }
    }
}

struct Subscription {
    hub: Hub,
    workspace: i64,
    user: i64,
    receiver: broadcast::Receiver<Update>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut workspaces = self.hub.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(online) = workspaces.get_mut(&self.workspace) {
            if let Some(feed) = online.get_mut(&self.user) {
                feed.sessions -= 1;
                if feed.sessions == 0 {
                    online.remove(&self.user);
                }
            }
            if online.is_empty() {
                workspaces.remove(&self.workspace);
            }
        }
    }
}

pub async fn events(State(s): State<AppState>, SessionKey(key): SessionKey) -> Result<Response> {
    let mut revocations = s.revocations.subscribe();
    let mut stopping = s.stopping.clone();
    let hub = s.updates.clone();
    // Acquire the subscription under the same lock that orders commits and their events.
    let (mut subscription, expires_at) =
        s.db.for_user(key.clone(), move |_, member| {
            Ok((hub.subscribe(&member), member.expires_at))
        })
        .await?;
    let remaining = Duration::from_millis((expires_at - now_ms()).max(0) as u64);
    let expires = tokio::time::Instant::now() + remaining;
    let stream = async_stream::stream! {
        let deadline = tokio::time::sleep_until(expires);
        tokio::pin!(deadline);
        yield Ok::<_, Infallible>(Event::default().event("ready").data("{}"));

        loop {
            tokio::select! {
                biased;
                _ = &mut deadline => break,
                _ = stopping.changed() => break,
                changed = revocations.changed() => {
                    if changed.is_err() || s.db.for_user(key.clone(), |_, _| Ok(())).await.is_err() {
                        break;
                    }
                }
                result = subscription.receiver.recv() => match result {
                    Ok(update) => yield Ok(Event::default().event(update.kind).data(update.data.as_ref())),
                    Err(broadcast::error::RecvError::Lagged(_)) => yield Ok(Event::default().event("resync").data("{}")),
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    };
    let mut response = Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response();
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(response)
}
