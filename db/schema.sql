CREATE TABLE IF NOT EXISTS identities (
    account_id CHAR(32) PRIMARY KEY,
    username TEXT,
    password_hash TEXT,
    display_name TEXT NOT NULL DEFAULT '',
    public_key JSONB NOT NULL,
    encrypted_recovery_bundle JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS identities_username_idx
    ON identities (lower(username)) WHERE username IS NOT NULL AND username <> '';

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

CREATE TABLE IF NOT EXISTS groups (
    group_id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    owner_account_id CHAR(32) NOT NULL REFERENCES identities(account_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS group_members (
    group_id UUID NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    account_id CHAR(32) NOT NULL REFERENCES identities(account_id) ON DELETE CASCADE,
    encrypted_group_key JSONB NOT NULL,
    PRIMARY KEY (group_id, account_id)
);

CREATE TABLE IF NOT EXISTS encrypted_group_messages (
    message_id UUID PRIMARY KEY,
    group_id UUID NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    sender_account_id CHAR(32) NOT NULL REFERENCES identities(account_id),
    ciphertext JSONB NOT NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS encrypted_group_messages_group_idx
    ON encrypted_group_messages(group_id, created_at);

CREATE TABLE IF NOT EXISTS encrypted_attachments (
    attachment_id UUID PRIMARY KEY,
    sender_account_id CHAR(32) NOT NULL REFERENCES identities(account_id),
    recipient_account_id CHAR(32) REFERENCES identities(account_id),
    group_id UUID REFERENCES groups(group_id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    ciphertext JSONB NOT NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((recipient_account_id IS NOT NULL) <> (group_id IS NOT NULL))
);
