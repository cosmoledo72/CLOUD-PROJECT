-- Iteration 24 (F3): time tracking — a running timer per employee, editable
-- time entries with the hour rate fixed at entry time, labour pay per task
-- (hourly or a fixed amount for the volume), employees' hour rates.

ALTER TABLE time_entries
    ADD COLUMN source     TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'timer')),
    -- Cost of one hour of the employee when the entry was made (null: no rate).
    ADD COLUMN hour_rate  NUMERIC(12, 2),
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
CREATE INDEX time_entries_tenant_date ON time_entries (tenant_id, work_date);
CREATE INDEX time_entries_task ON time_entries (task_id);
CREATE INDEX time_entries_user_date ON time_entries (user_id, work_date);

-- One running timer per employee.
CREATE TABLE time_timers (
    user_id    UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Cost of an employee's hour, for labour costs of hourly tasks.
ALTER TABLE users ADD COLUMN hour_rate NUMERIC(12, 2) CHECK (hour_rate IS NULL OR hour_rate >= 0);

-- How the work on a task is paid: by the hour (hours × rate) or a fixed amount for the volume.
ALTER TABLE tasks
    ADD COLUMN pay_mode   TEXT NOT NULL DEFAULT 'hourly' CHECK (pay_mode IN ('hourly', 'fixed')),
    ADD COLUMN pay_amount NUMERIC(14, 2) CHECK (pay_amount IS NULL OR pay_amount >= 0);

-- When a task was done: fixed pay counts as labour cost from that day.
ALTER TABLE tasks ADD COLUMN done_at TIMESTAMPTZ;
UPDATE tasks SET done_at = updated_at WHERE status = 'done';
CREATE FUNCTION tasks_done_at() RETURNS trigger AS $$
BEGIN
    IF NEW.status = 'done' AND (TG_OP = 'INSERT' OR OLD.status IS DISTINCT FROM 'done') THEN
        NEW.done_at := now();
    ELSIF NEW.status <> 'done' THEN
        NEW.done_at := NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER tasks_done_at BEFORE INSERT OR UPDATE OF status ON tasks
    FOR EACH ROW EXECUTE FUNCTION tasks_done_at();
