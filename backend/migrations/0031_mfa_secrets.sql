-- Two-factor authentication hardening (iteration 32, D3).
--
-- TOTP secrets are kept encrypted (AES-256-GCM, key derived from APP_SECRET):
-- totp_secret_enc replaces the plain base32 column. Existing plain secrets are
-- encrypted by the server at start-up (auth::mfa::encrypt_legacy_secrets) and
-- the plain column is cleared; it stays only for that transition.
ALTER TABLE users ADD COLUMN totp_secret_enc BYTEA;

-- One-time recovery codes: only a keyed SHA-256 of each code is stored.
CREATE TABLE user_recovery_codes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash BYTEA NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, code_hash)
);
CREATE INDEX user_recovery_codes_unused ON user_recovery_codes (user_id) WHERE used_at IS NULL;
