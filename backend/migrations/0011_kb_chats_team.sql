-- Iteration 10: knowledge base, chats, team directory.

-- Team directory: what colleagues see about each other.
ALTER TABLE users ADD COLUMN position TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN phone TEXT NOT NULL DEFAULT '';

-- Knowledge base: a tree of Markdown pages with full-text search.
CREATE TABLE kb_pages (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    parent_id  UUID REFERENCES kb_pages(id) ON DELETE CASCADE,
    title      TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    icon       TEXT NOT NULL DEFAULT '',
    body       TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    search     TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('russian', title), 'A') || setweight(to_tsvector('russian', body), 'B')
    ) STORED
);
CREATE INDEX kb_pages_tree ON kb_pages (tenant_id, parent_id, sort_order);
CREATE INDEX kb_pages_search ON kb_pages USING gin (search);

-- Chats: a channel per project, group channels and direct messages.
CREATE TABLE chat_channels (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('project', 'group', 'direct')),
    project_id      UUID UNIQUE REFERENCES projects(id) ON DELETE CASCADE,
    -- "<smaller user id>:<larger user id>" for direct chats.
    direct_key      TEXT,
    name            TEXT NOT NULL DEFAULT '',
    created_by      UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_message_at TIMESTAMPTZ
);
CREATE UNIQUE INDEX chat_channels_direct ON chat_channels (tenant_id, direct_key) WHERE direct_key IS NOT NULL;

-- Explicit members of group and direct chats; for project chats the row only
-- keeps the read position (access follows project membership).
CREATE TABLE chat_members (
    channel_id   UUID NOT NULL REFERENCES chat_channels(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    last_read_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    joined_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (channel_id, user_id)
);
CREATE INDEX chat_members_user ON chat_members (user_id);

CREATE TABLE chat_messages (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    channel_id UUID NOT NULL REFERENCES chat_channels(id) ON DELETE CASCADE,
    author_id  UUID REFERENCES users(id) ON DELETE SET NULL,
    body       TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 10000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at  TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);
CREATE INDEX chat_messages_channel ON chat_messages (channel_id, created_at DESC);
