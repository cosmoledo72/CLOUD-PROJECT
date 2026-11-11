-- Interface settings: a user's own choice (e.g. columns of the project list)
-- and the portal default an administrator sets for everyone.
CREATE TABLE view_settings (
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- NULL: the portal default.
    user_id    UUID REFERENCES users(id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    value      JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX view_settings_user ON view_settings (user_id, key) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX view_settings_default ON view_settings (tenant_id, key) WHERE user_id IS NULL;
