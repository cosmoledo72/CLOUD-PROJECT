-- Iteration 7: notifications (in-app and email), task comments, portal time zone.

-- Local time of the portal: due-date reminders are sent by its calendar.
ALTER TABLE tenants ADD COLUMN timezone TEXT NOT NULL DEFAULT 'Europe/Moscow';

CREATE TABLE notifications (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL,
    title        TEXT NOT NULL,
    body         TEXT NOT NULL DEFAULT '',
    -- In-app path to open, e.g. /projects/<id>?task=<id>.
    link         TEXT NOT NULL DEFAULT '',
    actor_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    entity_type  TEXT,
    entity_id    UUID,
    -- Prevents repeats of scheduled reminders (e.g. one per task and due date).
    dedupe_key   TEXT,
    show_in_app  BOOLEAN NOT NULL DEFAULT true,
    email_wanted BOOLEAN NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    read_at      TIMESTAMPTZ,
    emailed_at   TIMESTAMPTZ
);
CREATE INDEX notifications_user ON notifications (user_id, created_at DESC);
CREATE INDEX notifications_unread ON notifications (user_id) WHERE read_at IS NULL AND show_in_app;
CREATE UNIQUE INDEX notifications_dedupe ON notifications (user_id, kind, dedupe_key) WHERE dedupe_key IS NOT NULL;
CREATE INDEX notifications_email_due ON notifications (created_at)
    WHERE email_wanted AND emailed_at IS NULL AND read_at IS NULL;

-- Per-user channel choice for each kind; absent row = default of the kind.
CREATE TABLE notification_prefs (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind    TEXT NOT NULL,
    in_app  BOOLEAN NOT NULL,
    email   BOOLEAN NOT NULL,
    PRIMARY KEY (user_id, kind)
);

CREATE TABLE task_comments (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    author_id  UUID REFERENCES users(id) ON DELETE SET NULL,
    body       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX task_comments_task ON task_comments (task_id, created_at);

-- Due-date reminder scan.
CREATE INDEX tasks_due_open ON tasks (due_date) WHERE status <> 'done' AND assignee_id IS NOT NULL;
