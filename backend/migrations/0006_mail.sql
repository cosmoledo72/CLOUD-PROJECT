-- Iteration 6: outgoing mail, invitations and password reset.

-- Per-portal SMTP server. When disabled, the system-wide server from the
-- environment (SMTP_*) is used if configured.
CREATE TABLE tenant_mail_settings (
    tenant_id     UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    enabled       BOOLEAN NOT NULL DEFAULT false,
    host          TEXT NOT NULL DEFAULT '',
    port          INTEGER NOT NULL DEFAULT 587 CHECK (port BETWEEN 1 AND 65535),
    security      TEXT NOT NULL DEFAULT 'starttls' CHECK (security IN ('none', 'starttls', 'tls')),
    username      TEXT NOT NULL DEFAULT '',
    -- AES-256-GCM ciphertext (nonce || data), key derived from APP_SECRET.
    password_enc  BYTEA,
    from_email    TEXT NOT NULL DEFAULT '',
    from_name     TEXT NOT NULL DEFAULT '',
    reply_to      TEXT NOT NULL DEFAULT '',
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Outgoing mail queue processed by a background worker with retries.
CREATE TABLE mail_outbox (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,
    to_email        TEXT NOT NULL,
    subject         TEXT NOT NULL,
    body_text       TEXT NOT NULL,
    body_html       TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'sent', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at         TIMESTAMPTZ
);
CREATE INDEX mail_outbox_due ON mail_outbox (next_attempt_at) WHERE status = 'queued';
CREATE INDEX mail_outbox_tenant ON mail_outbox (tenant_id, created_at DESC);

-- One-time links: invitations and password resets. Only the SHA-256 hash of
-- the token is stored.
CREATE TABLE user_tokens (
    token_hash  BYTEA PRIMARY KEY,
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('invite', 'reset')),
    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ
);
CREATE INDEX user_tokens_user ON user_tokens (user_id, kind);

-- Invited users have no usable password until they accept the invitation.
ALTER TABLE users ADD COLUMN invite_pending BOOLEAN NOT NULL DEFAULT false;
