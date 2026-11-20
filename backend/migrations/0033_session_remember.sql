-- «Запомнить меня» on the login page (iteration 34): a remembered session lives
-- REMEMBER_TTL_HOURS (default 30 days) instead of SESSION_TTL_HOURS (7 days).
-- Kept on the session so a login waiting for its 2FA code passes it on.
ALTER TABLE sessions ADD COLUMN remember BOOLEAN NOT NULL DEFAULT false;
