-- Iteration 22 (C2): names for the stamp of exported Gantt charts, remembered per project.
ALTER TABLE projects ADD COLUMN export_stamp JSONB NOT NULL DEFAULT '{}'::jsonb;
