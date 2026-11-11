-- Finance: payments (income and expenses, planned and actual), their
-- categories, a finance access role per user and the invoice payment term.

-- Finance access: none | view | edit (administrators always have full access).
ALTER TABLE users ADD COLUMN finance_role TEXT NOT NULL DEFAULT 'none'
    CHECK (finance_role IN ('none', 'view', 'edit'));

-- Days to pay an invoice; after that an unpaid invoice is overdue.
ALTER TABLE tenant_requisites ADD COLUMN invoice_due_days INTEGER NOT NULL DEFAULT 10
    CHECK (invoice_due_days BETWEEN 0 AND 365);

CREATE TABLE finance_categories (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('income', 'expense')),
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    sort        INTEGER NOT NULL DEFAULT 0,
    is_archived BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX finance_categories_name ON finance_categories (tenant_id, kind, lower(name));

CREATE TABLE payments (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL CHECK (kind IN ('income', 'expense')),
    -- plan: expected (e.g. a stage of the contract payment schedule); fact: happened.
    status       TEXT NOT NULL DEFAULT 'fact' CHECK (status IN ('plan', 'fact')),
    pay_date     DATE NOT NULL,
    amount       NUMERIC(16, 2) NOT NULL CHECK (amount > 0),
    vat_rate     TEXT NOT NULL DEFAULT 'none',
    vat_amount   NUMERIC(16, 2) NOT NULL DEFAULT 0,
    category_id  UUID REFERENCES finance_categories(id) ON DELETE SET NULL,
    project_id   UUID REFERENCES projects(id) ON DELETE SET NULL,
    company_id   UUID REFERENCES crm_companies(id) ON DELETE SET NULL,
    -- Counterparty name as entered (kept when the company is deleted or CRM is off).
    company_name TEXT NOT NULL DEFAULT '',
    -- The invoice an income pays.
    invoice_id   UUID REFERENCES crm_documents(id) ON DELETE SET NULL,
    description  TEXT NOT NULL DEFAULT '',
    created_by   UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX payments_tenant_date ON payments (tenant_id, pay_date);
CREATE INDEX payments_project ON payments (project_id);
CREATE INDEX payments_invoice ON payments (invoice_id);
CREATE INDEX payments_company ON payments (company_id);
