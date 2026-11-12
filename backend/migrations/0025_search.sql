-- Iteration 25 (F8 + C5): portal-wide search. Stemmed full-text vectors
-- (Russian) on the searchable records, trigram indexes on names for typos and
-- substrings, and the text extracted from documents (DOCX, XLSX, PPTX, PDF,
-- TXT/CSV/MD) kept per file.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Text of the current version of a file; `version` tells the indexer what is stale.
CREATE TABLE file_texts (
    file_id    UUID PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    version    INT NOT NULL,
    content    TEXT NOT NULL DEFAULT '',
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    search     TSVECTOR GENERATED ALWAYS AS (to_tsvector('russian', content)) STORED
);
CREATE INDEX file_texts_search ON file_texts USING gin (search);

ALTER TABLE files ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (
    to_tsvector('russian', regexp_replace(name, '[._-]+', ' ', 'g'))
) STORED;
CREATE INDEX files_search ON files USING gin (search);
CREATE INDEX files_name_trgm ON files USING gin (lower(name) gin_trgm_ops);

ALTER TABLE projects ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (
    setweight(to_tsvector('russian', name), 'A') || setweight(to_tsvector('russian', description), 'B')
) STORED;
CREATE INDEX projects_search ON projects USING gin (search);
CREATE INDEX projects_name_trgm ON projects USING gin (lower(name) gin_trgm_ops);
-- Requisite values (code, object, address…) as text.
CREATE INDEX project_field_values_trgm ON project_field_values USING gin (lower(value #>> '{}') gin_trgm_ops);

ALTER TABLE tasks ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (
    setweight(to_tsvector('russian', title), 'A') || setweight(to_tsvector('russian', description), 'B')
) STORED;
CREATE INDEX tasks_search ON tasks USING gin (search);
CREATE INDEX tasks_title_trgm ON tasks USING gin (lower(title) gin_trgm_ops);

ALTER TABLE task_comments ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (to_tsvector('russian', body)) STORED;
CREATE INDEX task_comments_search ON task_comments USING gin (search);

ALTER TABLE chat_messages ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (to_tsvector('russian', body)) STORED;
CREATE INDEX chat_messages_search ON chat_messages USING gin (search);

ALTER TABLE crm_companies ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (
    setweight(to_tsvector('russian', name || ' ' || full_name), 'A') || to_tsvector('simple', COALESCE(inn, ''))
) STORED;
CREATE INDEX crm_companies_search ON crm_companies USING gin (search);
CREATE INDEX crm_companies_name_trgm ON crm_companies USING gin (lower(name || ' ' || full_name) gin_trgm_ops);

ALTER TABLE crm_contacts ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (
    to_tsvector('russian', last_name || ' ' || first_name || ' ' || middle_name || ' ' || position || ' ' || notes)
    || to_tsvector('simple', email || ' ' || phone)
) STORED;
CREATE INDEX crm_contacts_search ON crm_contacts USING gin (search);
CREATE INDEX crm_contacts_name_trgm ON crm_contacts USING gin (lower(last_name || ' ' || first_name || ' ' || middle_name) gin_trgm_ops);

ALTER TABLE crm_deals ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (to_tsvector('russian', title)) STORED;
CREATE INDEX crm_deals_search ON crm_deals USING gin (search);
CREATE INDEX crm_deals_title_trgm ON crm_deals USING gin (lower(title) gin_trgm_ops);

CREATE INDEX kb_pages_title_trgm ON kb_pages USING gin (lower(title) gin_trgm_ops);
CREATE INDEX users_name_trgm ON users USING gin (lower(display_name) gin_trgm_ops);
