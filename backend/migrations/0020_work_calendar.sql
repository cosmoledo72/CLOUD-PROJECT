-- Working calendar of the Russian Federation (iteration 18, C1).
--
-- calendar_days: the official production calendar shipped with the portal —
-- days that differ from the regular Mon–Fri week: non-working holidays
-- ('holiday'), transferred days off ('off'), working weekends ('work') and
-- pre-holiday shortened working days ('short', one hour shorter).
-- Sources: Government decrees on transferring days off for 2025–2027.
--
-- tenant_calendar_days: the portal's own changes on top of it (a corporate
-- day off, a year not yet in the shipped data). A row here wins.
--
-- projects.day_mode: how task durations count — 'work' (working days by the
-- calendar) or 'calendar' (every day). Existing projects keep calendar days;
-- new projects use working days.

CREATE TABLE calendar_days (
    day   DATE PRIMARY KEY,
    kind  TEXT NOT NULL CHECK (kind IN ('holiday', 'off', 'work', 'short')),
    title TEXT NOT NULL DEFAULT ''
);

CREATE TABLE tenant_calendar_days (
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    day        DATE NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('holiday', 'off', 'work', 'short')),
    title      TEXT NOT NULL DEFAULT '',
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, day)
);

ALTER TABLE projects ADD COLUMN day_mode TEXT NOT NULL DEFAULT 'calendar'
    CHECK (day_mode IN ('work', 'calendar'));
ALTER TABLE projects ALTER COLUMN day_mode SET DEFAULT 'work';

INSERT INTO calendar_days (day, kind, title) VALUES
  ('2025-01-01', 'holiday', 'Новогодние каникулы'),
  ('2025-01-02', 'holiday', 'Новогодние каникулы'),
  ('2025-01-03', 'holiday', 'Новогодние каникулы'),
  ('2025-01-04', 'holiday', 'Новогодние каникулы'),
  ('2025-01-05', 'holiday', 'Новогодние каникулы'),
  ('2025-01-06', 'holiday', 'Новогодние каникулы'),
  ('2025-01-07', 'holiday', 'Рождество Христово'),
  ('2025-01-08', 'holiday', 'Новогодние каникулы'),
  ('2025-02-23', 'holiday', 'День защитника Отечества'),
  ('2025-03-07', 'short', 'Предпраздничный день, на час короче'),
  ('2025-03-08', 'holiday', 'Международный женский день'),
  ('2025-04-30', 'short', 'Предпраздничный день, на час короче'),
  ('2025-05-01', 'holiday', 'Праздник Весны и Труда'),
  ('2025-05-02', 'off', 'Перенос с 4 января'),
  ('2025-05-08', 'off', 'Перенос с 23 февраля'),
  ('2025-05-09', 'holiday', 'День Победы'),
  ('2025-06-11', 'short', 'Предпраздничный день, на час короче'),
  ('2025-06-12', 'holiday', 'День России'),
  ('2025-06-13', 'off', 'Перенос с 8 марта'),
  ('2025-11-01', 'short', 'Рабочая суббота, сокращённый день'),
  ('2025-11-03', 'off', 'Перенос с 1 ноября'),
  ('2025-11-04', 'holiday', 'День народного единства'),
  ('2025-12-31', 'off', 'Перенос с 5 января'),
  ('2026-01-01', 'holiday', 'Новогодние каникулы'),
  ('2026-01-02', 'holiday', 'Новогодние каникулы'),
  ('2026-01-03', 'holiday', 'Новогодние каникулы'),
  ('2026-01-04', 'holiday', 'Новогодние каникулы'),
  ('2026-01-05', 'holiday', 'Новогодние каникулы'),
  ('2026-01-06', 'holiday', 'Новогодние каникулы'),
  ('2026-01-07', 'holiday', 'Рождество Христово'),
  ('2026-01-08', 'holiday', 'Новогодние каникулы'),
  ('2026-01-09', 'off', 'Перенос с 3 января'),
  ('2026-02-23', 'holiday', 'День защитника Отечества'),
  ('2026-03-08', 'holiday', 'Международный женский день'),
  ('2026-03-09', 'off', 'Перенос с 8 марта'),
  ('2026-04-30', 'short', 'Предпраздничный день, на час короче'),
  ('2026-05-01', 'holiday', 'Праздник Весны и Труда'),
  ('2026-05-08', 'short', 'Предпраздничный день, на час короче'),
  ('2026-05-09', 'holiday', 'День Победы'),
  ('2026-05-11', 'off', 'Перенос с 9 мая'),
  ('2026-06-11', 'short', 'Предпраздничный день, на час короче'),
  ('2026-06-12', 'holiday', 'День России'),
  ('2026-11-03', 'short', 'Предпраздничный день, на час короче'),
  ('2026-11-04', 'holiday', 'День народного единства'),
  ('2026-12-31', 'off', 'Перенос с 4 января'),
  ('2027-01-01', 'holiday', 'Новогодние каникулы'),
  ('2027-01-02', 'holiday', 'Новогодние каникулы'),
  ('2027-01-03', 'holiday', 'Новогодние каникулы'),
  ('2027-01-04', 'holiday', 'Новогодние каникулы'),
  ('2027-01-05', 'holiday', 'Новогодние каникулы'),
  ('2027-01-06', 'holiday', 'Новогодние каникулы'),
  ('2027-01-07', 'holiday', 'Рождество Христово'),
  ('2027-01-08', 'holiday', 'Новогодние каникулы'),
  ('2027-02-20', 'short', 'Рабочая суббота, сокращённый день'),
  ('2027-02-22', 'off', 'Перенос с 20 февраля'),
  ('2027-02-23', 'holiday', 'День защитника Отечества'),
  ('2027-03-08', 'holiday', 'Международный женский день'),
  ('2027-04-30', 'short', 'Предпраздничный день, на час короче'),
  ('2027-05-01', 'holiday', 'Праздник Весны и Труда'),
  ('2027-05-03', 'off', 'Перенос с 1 мая'),
  ('2027-05-09', 'holiday', 'День Победы'),
  ('2027-05-10', 'off', 'Перенос с 9 мая'),
  ('2027-06-11', 'short', 'Предпраздничный день, на час короче'),
  ('2027-06-12', 'holiday', 'День России'),
  ('2027-06-14', 'off', 'Перенос с 12 июня'),
  ('2027-11-03', 'short', 'Предпраздничный день, на час короче'),
  ('2027-11-04', 'holiday', 'День народного единства'),
  ('2027-11-05', 'off', 'Перенос с 2 января'),
  ('2027-12-31', 'off', 'Перенос с 3 января');
