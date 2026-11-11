-- Self-registration in an existing portal and richer invitations.

-- Registration settings of the portal:
--   off     — only invitations by an administrator;
--   request — anyone may apply, an administrator approves;
--   domain  — addresses of the listed domains join at once, others apply.
ALTER TABLE tenants
    ADD COLUMN registration_mode TEXT NOT NULL DEFAULT 'request'
        CHECK (registration_mode IN ('off', 'request', 'domain')),
    ADD COLUMN registration_domains TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN registration_role TEXT NOT NULL DEFAULT 'member'
        CHECK (registration_role IN ('member', 'guest')),
    ADD COLUMN registration_group_ids UUID[] NOT NULL DEFAULT '{}';

-- Applications for an account. The password is chosen by the applicant and
-- kept hashed until the request is approved.
CREATE TABLE registration_requests (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email              CITEXT NOT NULL,
    display_name       TEXT NOT NULL,
    password_hash      TEXT NOT NULL,
    comment            TEXT NOT NULL DEFAULT '',
    status             TEXT NOT NULL DEFAULT 'unconfirmed'
                       CHECK (status IN ('unconfirmed', 'pending', 'approved', 'rejected')),
    email_confirmed    BOOLEAN NOT NULL DEFAULT false,
    confirm_token_hash BYTEA UNIQUE,
    confirm_expires_at TIMESTAMPTZ,
    ip                 TEXT NOT NULL DEFAULT '',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_by         UUID REFERENCES users(id) ON DELETE SET NULL,
    decided_at         TIMESTAMPTZ,
    reject_reason      TEXT NOT NULL DEFAULT '',
    user_id            UUID REFERENCES users(id) ON DELETE SET NULL
);
-- One open request per address in a portal.
CREATE UNIQUE INDEX registration_requests_open
    ON registration_requests (tenant_id, email) WHERE status IN ('unconfirmed', 'pending');
CREATE INDEX registration_requests_tenant ON registration_requests (tenant_id, status, created_at DESC);

-- Details of an invitation (for users with invite_pending).
CREATE TABLE invitations (
    user_id    UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invited_by UUID REFERENCES users(id) ON DELETE SET NULL,
    message    TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
