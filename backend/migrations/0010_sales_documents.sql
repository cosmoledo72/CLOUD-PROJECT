-- Iteration 9: sales documents (quotes, contracts, invoices, acts) generated
-- from templates, with the portal's own requisites and automatic numbering.

-- Own requisites of the organisation (the seller in invoices and acts).
CREATE TABLE tenant_requisites (
    tenant_id      UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    full_name      TEXT NOT NULL DEFAULT '',
    inn            TEXT NOT NULL DEFAULT '',
    kpp            TEXT NOT NULL DEFAULT '',
    ogrn           TEXT NOT NULL DEFAULT '',
    legal_address  TEXT NOT NULL DEFAULT '',
    postal_address TEXT NOT NULL DEFAULT '',
    phone          TEXT NOT NULL DEFAULT '',
    email          TEXT NOT NULL DEFAULT '',
    website        TEXT NOT NULL DEFAULT '',
    director       TEXT NOT NULL DEFAULT '',
    director_title TEXT NOT NULL DEFAULT 'Генеральный директор',
    accountant     TEXT NOT NULL DEFAULT '',
    bank_name      TEXT NOT NULL DEFAULT '',
    bik            TEXT NOT NULL DEFAULT '',
    account        TEXT NOT NULL DEFAULT '',
    corr_account   TEXT NOT NULL DEFAULT '',
    -- Default VAT for new documents: none (not a VAT payer) or a rate in percent.
    vat_rate       TEXT NOT NULL DEFAULT 'none',
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Templates are either for projects (project requisites) or for sales
-- documents of a deal (client, deal, document, organisation).
ALTER TABLE doc_templates ADD COLUMN scope TEXT NOT NULL DEFAULT 'project' CHECK (scope IN ('project', 'sales'));
ALTER TABLE doc_templates ADD COLUMN doc_kind TEXT CHECK (doc_kind IN ('quote', 'contract', 'invoice', 'act'));

-- Each company gets its own document folder, governed by CRM visibility.
ALTER TABLE folders DROP CONSTRAINT folders_kind_check;
ALTER TABLE folders ADD CONSTRAINT folders_kind_check CHECK (kind IN ('my', 'common', 'project', 'regular', 'crm'));
ALTER TABLE crm_companies ADD COLUMN folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;

-- Number formats and counters per kind and calendar year.
CREATE TABLE crm_doc_numbering (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind      TEXT NOT NULL CHECK (kind IN ('quote', 'contract', 'invoice', 'act')),
    pattern   TEXT NOT NULL CHECK (length(pattern) BETWEEN 1 AND 60),
    PRIMARY KEY (tenant_id, kind)
);
CREATE TABLE crm_doc_counters (
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    year        INTEGER NOT NULL,
    last_number INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, kind, year)
);

CREATE TABLE crm_documents (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('quote', 'contract', 'invoice', 'act')),
    number      TEXT NOT NULL CHECK (length(number) BETWEEN 1 AND 60),
    doc_date    DATE NOT NULL,
    company_id  UUID REFERENCES crm_companies(id) ON DELETE SET NULL,
    company_name TEXT NOT NULL,
    deal_id     UUID REFERENCES crm_deals(id) ON DELETE SET NULL,
    -- Contract the invoice or act is based on.
    basis_id    UUID REFERENCES crm_documents(id) ON DELETE SET NULL,
    subject     TEXT NOT NULL DEFAULT '',
    amount      NUMERIC(16, 2) NOT NULL DEFAULT 0,
    vat_rate    TEXT NOT NULL DEFAULT 'none',
    vat_amount  NUMERIC(16, 2) NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'sent', 'signed', 'cancelled')),
    file_id     UUID REFERENCES files(id) ON DELETE SET NULL,
    template_id UUID REFERENCES doc_templates(id) ON DELETE SET NULL,
    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- A number is unique within its kind and year (invoices restart every year).
CREATE UNIQUE INDEX crm_documents_number_uq ON crm_documents (tenant_id, kind, extract(year FROM doc_date), lower(number))
    WHERE status <> 'cancelled';
CREATE INDEX crm_documents_deal ON crm_documents (deal_id);
CREATE INDEX crm_documents_company ON crm_documents (company_id);
