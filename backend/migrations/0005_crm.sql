-- CRM: counterparties, contacts, sales pipeline, deals and interaction history.

-- Per-user CRM access: none (no CRM), user (own and shared records), manager
-- (all records, pipeline settings). Portal owners/admins act as managers.
ALTER TABLE users ADD COLUMN crm_role TEXT NOT NULL DEFAULT 'user'
    CHECK (crm_role IN ('none', 'user', 'manager'));

CREATE TABLE crm_companies (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name           TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 300),
    full_name      TEXT NOT NULL DEFAULT '',
    inn            TEXT CHECK (inn ~ '^([0-9]{10}|[0-9]{12})$'),
    kpp            TEXT CHECK (kpp ~ '^[0-9]{4}[0-9A-Z]{2}[0-9]{3}$'),
    ogrn           TEXT CHECK (ogrn ~ '^([0-9]{13}|[0-9]{15})$'),
    legal_address  TEXT NOT NULL DEFAULT '',
    postal_address TEXT NOT NULL DEFAULT '',
    phone          TEXT NOT NULL DEFAULT '',
    email          TEXT NOT NULL DEFAULT '',
    website        TEXT NOT NULL DEFAULT '',
    director       TEXT NOT NULL DEFAULT '',
    bank_name      TEXT NOT NULL DEFAULT '',
    bik            TEXT CHECK (bik ~ '^[0-9]{9}$'),
    account        TEXT CHECK (account ~ '^[0-9]{20}$'),
    corr_account   TEXT CHECK (corr_account ~ '^[0-9]{20}$'),
    notes          TEXT NOT NULL DEFAULT '',
    tags           TEXT[] NOT NULL DEFAULT '{}',
    responsible_id UUID REFERENCES users(id) ON DELETE SET NULL,
    -- all: every CRM user; responsible: responsible person only; group: members of group_id.
    visibility     TEXT NOT NULL DEFAULT 'all' CHECK (visibility IN ('all', 'responsible', 'group')),
    group_id       UUID REFERENCES groups(id) ON DELETE SET NULL,
    created_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX crm_companies_tenant_idx ON crm_companies (tenant_id, lower(name));
-- A legal entity is identified by INN + KPP (branches share the INN).
CREATE UNIQUE INDEX crm_companies_inn_uq ON crm_companies (tenant_id, inn, COALESCE(kpp, '')) WHERE inn IS NOT NULL;

CREATE TABLE crm_contacts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    company_id  UUID REFERENCES crm_companies(id) ON DELETE SET NULL,
    last_name   TEXT NOT NULL DEFAULT '',
    first_name  TEXT NOT NULL DEFAULT '',
    middle_name TEXT NOT NULL DEFAULT '',
    position    TEXT NOT NULL DEFAULT '',
    phone       TEXT NOT NULL DEFAULT '',
    email       TEXT NOT NULL DEFAULT '',
    notes       TEXT NOT NULL DEFAULT '',
    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(last_name || first_name) > 0)
);
CREATE INDEX crm_contacts_company_idx ON crm_contacts (tenant_id, company_id);

CREATE TABLE crm_stages (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 60),
    -- open stages form the pipeline; won/lost close the deal.
    kind        TEXT NOT NULL DEFAULT 'open' CHECK (kind IN ('open', 'won', 'lost')),
    probability SMALLINT NOT NULL DEFAULT 0 CHECK (probability BETWEEN 0 AND 100),
    color       TEXT NOT NULL DEFAULT '#64748b' CHECK (color ~ '^#[0-9a-f]{6}$'),
    sort_order  DOUBLE PRECISION NOT NULL DEFAULT 0
);
CREATE INDEX crm_stages_tenant_idx ON crm_stages (tenant_id, sort_order);

CREATE TABLE crm_deals (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    title          TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 300),
    company_id     UUID REFERENCES crm_companies(id) ON DELETE SET NULL,
    contact_id     UUID REFERENCES crm_contacts(id) ON DELETE SET NULL,
    stage_id       UUID NOT NULL REFERENCES crm_stages(id),
    amount         NUMERIC(16, 2) NOT NULL DEFAULT 0 CHECK (amount >= 0),
    expected_close DATE,
    responsible_id UUID REFERENCES users(id) ON DELETE SET NULL,
    description    TEXT NOT NULL DEFAULT '',
    lost_reason    TEXT NOT NULL DEFAULT '',
    project_id     UUID REFERENCES projects(id) ON DELETE SET NULL,
    sort_order     DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at      TIMESTAMPTZ
);
CREATE INDEX crm_deals_tenant_idx ON crm_deals (tenant_id, stage_id, sort_order);
CREATE INDEX crm_deals_company_idx ON crm_deals (company_id);

-- Interaction history. "system" entries are written automatically (stage changes etc.).
CREATE TABLE crm_activities (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    company_id  UUID REFERENCES crm_companies(id) ON DELETE CASCADE,
    contact_id  UUID REFERENCES crm_contacts(id) ON DELETE SET NULL,
    deal_id     UUID REFERENCES crm_deals(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('note', 'call', 'meeting', 'email', 'system')),
    body        TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 10000),
    happened_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    author_id   UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (company_id IS NOT NULL OR deal_id IS NOT NULL OR contact_id IS NOT NULL)
);
CREATE INDEX crm_activities_company_idx ON crm_activities (company_id, happened_at DESC);
CREATE INDEX crm_activities_deal_idx ON crm_activities (deal_id, happened_at DESC);

-- Customer of a project and the deal it came from.
ALTER TABLE projects
    ADD COLUMN company_id UUID REFERENCES crm_companies(id) ON DELETE SET NULL,
    ADD COLUMN deal_id    UUID REFERENCES crm_deals(id) ON DELETE SET NULL;

-- Single source of truth for counterparty visibility.
CREATE FUNCTION crm_company_visible(p_visibility TEXT, p_responsible UUID, p_group UUID, p_user UUID, p_manager BOOLEAN)
RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
    SELECT p_manager
        OR p_visibility = 'all'
        OR p_responsible = p_user
        OR (p_visibility = 'group' AND EXISTS (
               SELECT 1 FROM group_members gm WHERE gm.group_id = p_group AND gm.user_id = p_user))
$$;
