-- Admin panel: portal branding and configurable project requisites.

-- ---------------------------------------------------------------------------
-- Portal branding (one row per tenant; absent row = defaults)
-- ---------------------------------------------------------------------------

CREATE TABLE tenant_branding (
    tenant_id      UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    -- Name shown in the sidebar and on the login page; NULL = tenants.name.
    display_name   TEXT,
    accent_color   TEXT NOT NULL DEFAULT '#1f4e79' CHECK (accent_color ~ '^#[0-9a-f]{6}$'),
    sidebar_style  TEXT NOT NULL DEFAULT 'light' CHECK (sidebar_style IN ('light', 'dark', 'accent')),
    color_scheme   TEXT NOT NULL DEFAULT 'auto' CHECK (color_scheme IN ('auto', 'light', 'dark')),
    corner_style   TEXT NOT NULL DEFAULT 'default' CHECK (corner_style IN ('sharp', 'default', 'round')),
    login_title    TEXT NOT NULL DEFAULT '',
    login_subtitle TEXT NOT NULL DEFAULT '',
    -- Wide logo for the sidebar and the login page.
    logo_key       TEXT,
    logo_mime      TEXT,
    -- Show the logo on a white plate (for dark logos on dark/accent sidebars).
    logo_plate     BOOLEAN NOT NULL DEFAULT false,
    -- Square icon used as favicon and next to the portal name.
    icon_key       TEXT,
    icon_mime      TEXT,
    -- Bumped on every asset change; used as a cache-busting URL parameter.
    asset_version  INT NOT NULL DEFAULT 0,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- Project requisites: admin-defined typed fields filled in per project
-- (project code, client, contract, stage, chief engineer, ...).
-- ---------------------------------------------------------------------------

CREATE TABLE project_field_defs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- Stable machine key, usable later as a document template placeholder.
    key         TEXT NOT NULL CHECK (key ~ '^[a-z][a-z0-9_]{0,39}$'),
    label       TEXT NOT NULL CHECK (length(label) BETWEEN 1 AND 120),
    section     TEXT NOT NULL DEFAULT 'Общие сведения',
    field_type  TEXT NOT NULL CHECK (field_type IN ('text', 'textarea', 'number', 'date', 'select', 'user', 'checkbox')),
    -- For 'select': JSON array of option strings.
    options     JSONB NOT NULL DEFAULT '[]'::jsonb,
    help_text   TEXT NOT NULL DEFAULT '',
    is_required BOOLEAN NOT NULL DEFAULT false,
    -- Show the value on project cards and in project lists.
    show_in_list BOOLEAN NOT NULL DEFAULT false,
    -- Archived fields are hidden from forms; their values are kept.
    is_archived BOOLEAN NOT NULL DEFAULT false,
    sort_order  DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, key)
);
CREATE INDEX project_field_defs_tenant_idx ON project_field_defs (tenant_id, sort_order);

CREATE TABLE project_field_values (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    field_id   UUID NOT NULL REFERENCES project_field_defs(id) ON DELETE CASCADE,
    value      JSONB NOT NULL,
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, field_id)
);

CREATE INDEX audit_action_idx ON audit_log (tenant_id, action);
