CREATE TABLE IF NOT EXISTS identities (
    account_id CHAR(32) PRIMARY KEY,
    public_key JSONB NOT NULL,
    encrypted_recovery_bundle JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS sessions (
    session_id UUID PRIMARY KEY,
    account_id CHAR(32) NOT NULL REFERENCES identities(account_id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS sessions_account_id_idx ON sessions(account_id);
CREATE INDEX IF NOT EXISTS sessions_expires_at_idx ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS encrypted_messages (
    message_id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL,
    sender_account_id CHAR(32) NOT NULL REFERENCES identities(account_id),
    recipient_account_id CHAR(32) NOT NULL REFERENCES identities(account_id),
    ciphertext JSONB NOT NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS encrypted_messages_recipient_idx
    ON encrypted_messages(recipient_account_id, created_at);
CREATE INDEX IF NOT EXISTS encrypted_messages_expiry_idx
    ON encrypted_messages(expires_at);
