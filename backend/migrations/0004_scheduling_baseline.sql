-- Scheduling: dependency-driven auto-shift switch and baseline plan.

ALTER TABLE projects
    ADD COLUMN auto_schedule BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN baseline_saved_at TIMESTAMPTZ;

-- One saved baseline per project: the approved dates of every task.
CREATE TABLE task_baselines (
    task_id    UUID PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    start_date DATE,
    due_date   DATE
);
CREATE INDEX task_baselines_project_idx ON task_baselines (project_id);
