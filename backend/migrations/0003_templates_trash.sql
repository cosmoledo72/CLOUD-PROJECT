-- Document templates with requisite placeholders, and trash metadata.

CREATE TABLE doc_templates (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name         TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    description  TEXT NOT NULL DEFAULT '',
    category     TEXT NOT NULL DEFAULT 'Общие',
    extension    TEXT NOT NULL CHECK (extension IN ('docx', 'xlsx')),
    storage_key  TEXT NOT NULL,
    size_bytes   BIGINT NOT NULL,
    -- Placeholder keys found in the file, e.g. ["project.code", "date"].
    placeholders JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_by   UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);

-- Who moved an item to the trash (deleted_at already exists).
ALTER TABLE folders ADD COLUMN deleted_by UUID REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE files   ADD COLUMN deleted_by UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX folders_deleted_idx ON folders (tenant_id, deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX files_deleted_idx   ON files   (tenant_id, deleted_at) WHERE deleted_at IS NOT NULL;
