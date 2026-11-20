ALTER TABLE messages ADD COLUMN edited_at INTEGER;
ALTER TABLE messages ADD COLUMN original_body_hash BLOB;

CREATE TABLE saved_messages (
    user_id INTEGER NOT NULL REFERENCES users(id),
    message_id INTEGER NOT NULL REFERENCES messages(id),
    PRIMARY KEY (user_id, message_id)
) WITHOUT ROWID;

CREATE TRIGGER reindex_message AFTER UPDATE OF body ON messages
WHEN old.body IS NOT new.body BEGIN
    INSERT INTO message_search(message_search, rowid, body) VALUES ('delete', old.id, old.body);
    INSERT INTO message_search(rowid, body) VALUES (new.id, new.body);
END;

PRAGMA user_version=2;
