-- Login page panel: tone of the background (dark — light text and a darkening
-- veil; light — dark text and a lightening veil) and the base colour under a
-- transparent background image ('brand' gradient, 'light', 'white' or '#rrggbb').
ALTER TABLE tenant_branding
    ADD COLUMN login_bg_tone TEXT NOT NULL DEFAULT 'dark' CHECK (login_bg_tone IN ('dark', 'light')),
    ADD COLUMN login_bg_base TEXT NOT NULL DEFAULT 'brand'
        CHECK (login_bg_base IN ('brand', 'light', 'white') OR login_bg_base ~ '^#[0-9a-f]{6}$');
