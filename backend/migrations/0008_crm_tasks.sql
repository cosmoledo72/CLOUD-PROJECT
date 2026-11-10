-- Iteration 8: CRM to-dos ("call back", "send the quote") with reminders.

CREATE TABLE crm_tasks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL DEFAULT 'call' CHECK (kind IN ('call', 'meeting', 'email', 'todo')),
    title       TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 300),
    notes       TEXT NOT NULL DEFAULT '',
    -- Day in the portal time zone; the time is optional ("during the day").
    due_date    DATE NOT NULL,
    due_time    TIME,
    company_id  UUID REFERENCES crm_companies(id) ON DELETE CASCADE,
    deal_id     UUID REFERENCES crm_deals(id) ON DELETE CASCADE,
    contact_id  UUID REFERENCES crm_contacts(id) ON DELETE SET NULL,
    assignee_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    done_at     TIMESTAMPTZ,
    done_by     UUID REFERENCES users(id) ON DELETE SET NULL,
    result      TEXT NOT NULL DEFAULT ''
);
CREATE INDEX crm_tasks_assignee_open ON crm_tasks (assignee_id, due_date) WHERE done_at IS NULL;
CREATE INDEX crm_tasks_due_open ON crm_tasks (due_date) WHERE done_at IS NULL;
CREATE INDEX crm_tasks_company ON crm_tasks (company_id);
CREATE INDEX crm_tasks_deal ON crm_tasks (deal_id);
