CREATE TABLE workspaces (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id INTEGER REFERENCES users(id)
);
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL REFERENCES workspaces(id),
    handle TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    login_failures INTEGER NOT NULL DEFAULT 0,
    failure_window INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX users_workspace ON users(workspace_id, id);
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    expires_at INTEGER NOT NULL
);
CREATE INDEX sessions_expiry ON sessions(expires_at);
CREATE TABLE invites (
    token_hash TEXT PRIMARY KEY,
    workspace_id INTEGER NOT NULL REFERENCES workspaces(id),
    expires_at INTEGER NOT NULL
);
CREATE INDEX invites_expiry ON invites(expires_at);
CREATE TABLE channels (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL REFERENCES workspaces(id),
    name TEXT NOT NULL,
    topic TEXT NOT NULL,
    dm_a INTEGER REFERENCES users(id),
    dm_b INTEGER REFERENCES users(id),
    CHECK ((dm_a IS NULL AND dm_b IS NULL) OR
           (dm_a IS NOT NULL AND dm_b IS NOT NULL AND dm_a < dm_b))
);
CREATE UNIQUE INDEX channel_names ON channels(workspace_id, name COLLATE NOCASE) WHERE dm_a IS NULL;
CREATE UNIQUE INDEX direct_pair ON channels(dm_a, dm_b) WHERE dm_a IS NOT NULL;
CREATE INDEX room_list ON channels(workspace_id, id);
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL REFERENCES channels(id),
    author_id INTEGER NOT NULL REFERENCES users(id),
    parent_id INTEGER REFERENCES messages(id),
    body TEXT NOT NULL,
    client_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(author_id, client_id)
);
CREATE INDEX timeline ON messages(channel_id, parent_id, id);
CREATE TABLE reactions (
    message_id INTEGER NOT NULL REFERENCES messages(id),
    user_id INTEGER NOT NULL REFERENCES users(id),
    emoji TEXT NOT NULL,
    PRIMARY KEY (message_id, emoji, user_id)
) WITHOUT ROWID;
CREATE VIRTUAL TABLE message_search USING fts5(
    body, content='messages', content_rowid='id', tokenize='trigram'
);
CREATE TRIGGER index_message AFTER INSERT ON messages BEGIN
    INSERT INTO message_search(rowid, body) VALUES (new.id, new.body);
END;
PRAGMA user_version=1;
