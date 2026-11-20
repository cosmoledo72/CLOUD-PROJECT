-- Server-wide backup schedule, edited in «Администрирование → Резервные копии».
-- A single row; NULL hour means BACKUP_HOUR from the environment. The time zone
-- is not stored here: the backup service uses the time zone of the backup portal
-- (tenants.timezone of BACKUP_PORTAL), read by scripts/backup.sh.
CREATE TABLE backup_settings (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    hour SMALLINT CHECK (hour BETWEEN 0 AND 23),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL
);
