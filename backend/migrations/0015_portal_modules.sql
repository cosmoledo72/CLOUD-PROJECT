-- Portal modules that the administrator switched off (Администрирование → Модули).
-- Empty = every module on; data of a switched-off module is kept.
ALTER TABLE tenants ADD COLUMN disabled_modules TEXT[] NOT NULL DEFAULT '{}';
