CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    received_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS messages_group_idx ON messages(group_id, received_at);
