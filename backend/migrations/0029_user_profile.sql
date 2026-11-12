-- Employee card (iteration 28б): personal and work data, requisites for
-- documents, photo and signature, last visit; custom requisites defined by the
-- administrator.

ALTER TABLE users
    ADD COLUMN last_name         TEXT NOT NULL DEFAULT '',
    ADD COLUMN first_name        TEXT NOT NULL DEFAULT '',
    ADD COLUMN middle_name       TEXT NOT NULL DEFAULT '',
    ADD COLUMN birth_date        DATE,
    ADD COLUMN department        TEXT NOT NULL DEFAULT '',
    ADD COLUMN manager_id        UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN hired_on          DATE,
    ADD COLUMN phone_mobile      TEXT NOT NULL DEFAULT '',
    ADD COLUMN telegram          TEXT NOT NULL DEFAULT '',
    -- Requisites for documents.
    ADD COLUMN doc_short_name    TEXT NOT NULL DEFAULT '',
    ADD COLUMN doc_position      TEXT NOT NULL DEFAULT '',
    ADD COLUMN name_genitive     TEXT NOT NULL DEFAULT '',
    ADD COLUMN name_dative       TEXT NOT NULL DEFAULT '',
    ADD COLUMN nrs_number        TEXT NOT NULL DEFAULT '',
    ADD COLUMN qual_cert         TEXT NOT NULL DEFAULT '',
    ADD COLUMN power_of_attorney TEXT NOT NULL DEFAULT '',
    -- Photo (square, shown instead of initials) and signature image (PNG).
    ADD COLUMN avatar_key        TEXT,
    ADD COLUMN avatar_at         TIMESTAMPTZ,
    ADD COLUMN signature_key     TEXT,
    ADD COLUMN signature_at      TIMESTAMPTZ,
    ADD COLUMN last_seen_at      TIMESTAMPTZ;

-- "Имя Фамилия" becomes first and last name; other names are left to fill in.
UPDATE users
   SET first_name = split_part(btrim(display_name), ' ', 1),
       last_name  = split_part(btrim(display_name), ' ', 2)
 WHERE array_length(regexp_split_to_array(btrim(display_name), '\s+'), 1) = 2;

CREATE TABLE user_field_defs (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    label      TEXT NOT NULL CHECK (length(label) BETWEEN 1 AND 120),
    field_type TEXT NOT NULL CHECK (field_type IN ('text', 'textarea', 'date', 'number')),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX user_field_defs_tenant ON user_field_defs (tenant_id, sort_order);

CREATE TABLE user_field_values (
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    field_id   UUID NOT NULL REFERENCES user_field_defs(id) ON DELETE CASCADE,
    value      TEXT NOT NULL,
    PRIMARY KEY (user_id, field_id)
);
