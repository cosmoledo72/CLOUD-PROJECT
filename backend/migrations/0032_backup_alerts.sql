-- Mail to the owner of the backup portal when backups need attention
-- (iteration 32). The keys of the problems already reported are kept here so
-- that one letter goes out per change, not one per check, also after a restart.
ALTER TABLE backup_settings
    ADD COLUMN alert_keys TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN alert_changed_at TIMESTAMPTZ;
