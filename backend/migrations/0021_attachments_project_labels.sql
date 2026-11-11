-- Iteration 19: task attachments (C3), project labels and badges (F6).

-- Files attached to a task. The file itself lives in the project folder
-- (uploads go to «Задачи/#N Название»); detaching keeps the file.
CREATE TABLE task_attachments (
    task_id   UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    file_id   UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    added_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (task_id, file_id)
);
CREATE INDEX task_attachments_file ON task_attachments (file_id);

-- The folder a task's uploads go to (created on the first upload).
ALTER TABLE tasks ADD COLUMN folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;

-- A file is "live" when neither it nor any folder above it is in the trash.
-- Attachments of trashed files are hidden and come back on restore.
CREATE FUNCTION file_is_live(fid UUID) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
    WITH RECURSIVE up AS (
        SELECT d.id, d.parent_id, d.deleted_at FROM files f JOIN folders d ON d.id = f.folder_id
         WHERE f.id = fid AND f.deleted_at IS NULL
        UNION ALL
        SELECT p.id, p.parent_id, p.deleted_at FROM folders p JOIN up ON p.id = up.parent_id
    )
    SELECT EXISTS (SELECT 1 FROM up) AND NOT EXISTS (SELECT 1 FROM up WHERE deleted_at IS NOT NULL)
$$;

-- Project labels: one catalogue per portal, separate from task labels.
CREATE TABLE project_labels (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 40),
    color      TEXT NOT NULL DEFAULT '#64748b' CHECK (color ~ '^#[0-9a-fA-F]{6}$'),
    sort_order INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX project_labels_name ON project_labels (tenant_id, lower(name));

CREATE TABLE project_label_links (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    label_id   UUID NOT NULL REFERENCES project_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (project_id, label_id)
);
CREATE INDEX project_label_links_label ON project_label_links (label_id);

-- Project badge: a coloured square with the first letter of the name, or an
-- icon. NULL = automatic (colour derived from the name, letter).
ALTER TABLE projects
    ADD COLUMN badge_color TEXT CHECK (badge_color ~ '^#[0-9a-fA-F]{6}$'),
    ADD COLUMN badge_icon  TEXT CHECK (length(badge_icon) BETWEEN 1 AND 40);
