-- Visual knowledge base editor (iteration 27, F9): pages are stored as
-- sanitized HTML (`content`); `body` keeps the plain text for search and
-- snippets. Markdown pages are converted once at server start. Page history,
-- files and images inside pages, pinned pages, archive instead of deletion.

ALTER TABLE kb_pages
    ADD COLUMN content     TEXT,
    ADD COLUMN version     INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN pinned      BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN archived_at TIMESTAMPTZ,
    ADD COLUMN archived_by UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX kb_pages_live ON kb_pages (tenant_id) WHERE archived_at IS NULL;

CREATE TABLE kb_page_versions (
    page_id    UUID NOT NULL REFERENCES kb_pages(id) ON DELETE CASCADE,
    version    INTEGER NOT NULL,
    title      TEXT NOT NULL,
    icon       TEXT NOT NULL DEFAULT '',
    content    TEXT NOT NULL,
    author_id  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (page_id, version)
);

CREATE TABLE kb_files (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    page_id     UUID NOT NULL REFERENCES kb_pages(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    extension   TEXT NOT NULL DEFAULT '',
    size_bytes  BIGINT NOT NULL,
    storage_key TEXT NOT NULL,
    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX kb_files_page ON kb_files (page_id);
