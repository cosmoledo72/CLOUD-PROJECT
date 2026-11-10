-- Iteration 11: "All tasks" section and the task card.
-- Portal-wide task numbers (#234), task labels and per-task checklists.

-- Task numbers: one running counter per portal, assigned on insert.
ALTER TABLE tenants ADD COLUMN task_seq BIGINT NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN number BIGINT;

-- Number existing tasks in creation order within each portal.
UPDATE tasks t SET number = n.rn
  FROM (SELECT id, row_number() OVER (PARTITION BY tenant_id ORDER BY created_at, id) AS rn FROM tasks) n
 WHERE n.id = t.id;
UPDATE tenants te SET task_seq = COALESCE((SELECT max(number) FROM tasks t WHERE t.tenant_id = te.id), 0);

ALTER TABLE tasks ALTER COLUMN number SET NOT NULL;
CREATE UNIQUE INDEX tasks_tenant_number ON tasks (tenant_id, number);

-- The counter row is locked by the UPDATE, so concurrent inserts get distinct numbers.
CREATE FUNCTION tasks_assign_number() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.number IS NULL THEN
        UPDATE tenants SET task_seq = task_seq + 1 WHERE id = NEW.tenant_id RETURNING task_seq INTO NEW.number;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER tasks_number BEFORE INSERT ON tasks FOR EACH ROW EXECUTE FUNCTION tasks_assign_number();

-- Board filters across projects.
CREATE INDEX tasks_tenant_assignee ON tasks (tenant_id, assignee_id);
CREATE INDEX tasks_tenant_due ON tasks (tenant_id, due_date);

-- Task labels: one catalogue per portal.
CREATE TABLE task_labels (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 40),
    color      TEXT NOT NULL DEFAULT '#64748b' CHECK (color ~ '^#[0-9a-fA-F]{6}$'),
    sort_order INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX task_labels_name ON task_labels (tenant_id, lower(name));

CREATE TABLE task_label_links (
    task_id  UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES task_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, label_id)
);
CREATE INDEX task_label_links_label ON task_label_links (label_id);

-- Checklist items inside a task.
CREATE TABLE task_checklist_items (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    task_id     UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    body        TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 500),
    done        BOOLEAN NOT NULL DEFAULT false,
    done_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    done_at     TIMESTAMPTZ,
    assignee_id UUID REFERENCES users(id) ON DELETE SET NULL,
    due_date    DATE,
    sort_order  DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX task_checklist_task ON task_checklist_items (task_id, sort_order);
