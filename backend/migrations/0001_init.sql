-- WorkHub initial schema.
-- Multi-tenancy model: shared database, shared schema, every tenant-owned row
-- carries tenant_id. All application queries filter by tenant_id. Postgres
-- row-level security as a second line of defence is planned (SPEC, phase 2).

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS citext;

-- ---------------------------------------------------------------------------
-- Tenancy and identity
-- ---------------------------------------------------------------------------

CREATE TABLE tenants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug        CITEXT NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,62}$'),
    name        TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    quota_bytes BIGINT NOT NULL DEFAULT 10737418240, -- 10 GiB
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email         CITEXT NOT NULL,
    display_name  TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'member'
                  CHECK (role IN ('owner', 'admin', 'member', 'guest')),
    totp_secret   TEXT,                 -- base32, set during enrolment
    totp_enabled  BOOLEAN NOT NULL DEFAULT false,
    is_active     BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, email)
);

CREATE TABLE groups (
    id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name      TEXT NOT NULL,
    UNIQUE (tenant_id, name)
);

CREATE TABLE group_members (
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);

-- Opaque session tokens; only the SHA-256 hash is stored.
CREATE TABLE sessions (
    token_hash  BYTEA PRIMARY KEY,
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- 'pending_mfa' sessions only allow the TOTP verification call.
    stage       TEXT NOT NULL CHECK (stage IN ('pending_mfa', 'active')),
    mfa_attempts SMALLINT NOT NULL DEFAULT 0,
    user_agent  TEXT,
    ip          TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE audit_log (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    action      TEXT NOT NULL,
    target_type TEXT,
    target_id   UUID,
    details     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_tenant_time_idx ON audit_log (tenant_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- Documents
-- ---------------------------------------------------------------------------

-- Root folders have parent_id NULL and a kind: 'my' (personal) or 'common'.
CREATE TABLE folders (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    parent_id  UUID REFERENCES folders(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL DEFAULT 'regular' CHECK (kind IN ('my', 'common', 'project', 'regular')),
    name       TEXT NOT NULL,
    owner_id   UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);
CREATE INDEX folders_parent_idx ON folders (tenant_id, parent_id);
CREATE UNIQUE INDEX folders_name_uq ON folders (parent_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE files (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    folder_id       UUID NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    extension       TEXT NOT NULL,
    owner_id        UUID NOT NULL REFERENCES users(id),
    current_version INT NOT NULL DEFAULT 1,
    size_bytes      BIGINT NOT NULL DEFAULT 0,
    -- ONLYOFFICE document key of the current co-editing session. All users
    -- opening the file share this key; it is reset after a final save so the
    -- next session starts from the newly stored version.
    edit_key        TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at      TIMESTAMPTZ
);
CREATE INDEX files_folder_idx ON files (tenant_id, folder_id);
CREATE UNIQUE INDEX files_name_uq ON files (folder_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE file_versions (
    file_id     UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    version     INT NOT NULL,
    storage_key TEXT NOT NULL,          -- key in the blob store
    size_bytes  BIGINT NOT NULL,
    sha256      BYTEA NOT NULL,
    author_id   UUID REFERENCES users(id),
    changes_key TEXT,                   -- ONLYOFFICE changes archive, if any
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (file_id, version)
);

-- Explicit grants. Access on a folder is inherited by everything below it.
-- Exactly one of user_id / group_id is set; both NULL = everyone in tenant.
CREATE TABLE shares (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL CHECK (resource_type IN ('folder', 'file')),
    resource_id   UUID NOT NULL,
    user_id       UUID REFERENCES users(id) ON DELETE CASCADE,
    group_id      UUID REFERENCES groups(id) ON DELETE CASCADE,
    access        SMALLINT NOT NULL CHECK (access BETWEEN 1 AND 4), -- 1 read, 2 comment, 3 edit, 4 full
    created_by    UUID REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (NOT (user_id IS NOT NULL AND group_id IS NOT NULL))
);
CREATE INDEX shares_resource_idx ON shares (resource_type, resource_id);

-- ---------------------------------------------------------------------------
-- Projects and tasks
-- ---------------------------------------------------------------------------

CREATE TABLE projects (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'closed')),
    owner_id    UUID NOT NULL REFERENCES users(id),
    folder_id   UUID REFERENCES folders(id) ON DELETE SET NULL, -- project documents
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX projects_tenant_idx ON projects (tenant_id);

CREATE TABLE project_members (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('manager', 'member', 'viewer')),
    PRIMARY KEY (project_id, user_id)
);

CREATE TABLE tasks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    parent_id   UUID REFERENCES tasks(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'in_progress', 'review', 'done')),
    priority    SMALLINT NOT NULL DEFAULT 2 CHECK (priority BETWEEN 1 AND 4),
    is_milestone BOOLEAN NOT NULL DEFAULT false,
    assignee_id UUID REFERENCES users(id) ON DELETE SET NULL,
    start_date  DATE,
    due_date    DATE,
    progress    SMALLINT NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
    sort_order  DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_by  UUID REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (start_date IS NULL OR due_date IS NULL OR start_date <= due_date)
);
CREATE INDEX tasks_project_idx ON tasks (tenant_id, project_id, sort_order);

-- Gantt dependencies. FS = finish-to-start (default), SS, FF, SF.
CREATE TABLE task_dependencies (
    task_id       UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    kind          TEXT NOT NULL DEFAULT 'FS' CHECK (kind IN ('FS', 'SS', 'FF', 'SF')),
    lag_days      INT NOT NULL DEFAULT 0,
    PRIMARY KEY (task_id, depends_on_id),
    CHECK (task_id <> depends_on_id)
);

CREATE TABLE time_entries (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users(id),
    work_date  DATE NOT NULL,
    minutes    INT NOT NULL CHECK (minutes > 0),
    note       TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
