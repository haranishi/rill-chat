use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct State {
    pub me: Option<User>,
    pub workspace: Workspace,
    pub channels: Vec<Channel>,
    pub members: Vec<User>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Workspace {
    pub id: i64,
    pub name: String,
    pub owner_id: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub handle: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Channel {
    pub id: i64,
    pub name: String,
    pub topic: String,
    pub kind: String,
    pub peer_id: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Message {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub parent_id: Option<i64>,
    pub body: String,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    pub reply_count: i64,
    #[serde(default)]
    pub saved: bool,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Reaction {
    pub emoji: String,
    pub count: i64,
    #[serde(default)]
    pub mine: bool,
}
