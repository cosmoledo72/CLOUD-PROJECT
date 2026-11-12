-- Public links (iteration 26, F10 + C4): a file, a folder or a knowledge base
-- page opened without an account, by an unguessable token; optional password
-- and expiry; counters of opens and downloads.

CREATE TABLE public_links (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    resource_type  TEXT NOT NULL CHECK (resource_type IN ('file', 'folder', 'kb_page')),
    resource_id    UUID NOT NULL,
    token          TEXT NOT NULL UNIQUE,
    password_hash  TEXT,
    expires_at     TIMESTAMPTZ,
    created_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at     TIMESTAMPTZ,
    revoked_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    opens          INT NOT NULL DEFAULT 0,
    downloads      INT NOT NULL DEFAULT 0,
    last_opened_at TIMESTAMPTZ
);

-- One working link per resource.
CREATE UNIQUE INDEX public_links_active_uq ON public_links (resource_type, resource_id) WHERE revoked_at IS NULL;
CREATE INDEX public_links_tenant_idx ON public_links (tenant_id, created_at DESC);

-- Administrators can switch public links off for the whole portal.
ALTER TABLE tenants ADD COLUMN public_links_enabled BOOLEAN NOT NULL DEFAULT true;
