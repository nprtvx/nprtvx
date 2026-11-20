CREATE TABLE IF NOT EXISTS relay_messages (
    message_id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    mls_message BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS relay_messages_group_idx ON relay_messages(group_id, created_at);
