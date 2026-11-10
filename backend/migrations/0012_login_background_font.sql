-- Login page: optional white plate under the logo and a background image
-- behind it, darkened by an overlay so the text stays readable.
ALTER TABLE tenant_branding ADD COLUMN login_logo_plate BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE tenant_branding ADD COLUMN login_bg_key TEXT;
ALTER TABLE tenant_branding ADD COLUMN login_bg_mime TEXT;
-- Darkening of the background image, percent (0 = none).
ALTER TABLE tenant_branding ADD COLUMN login_bg_overlay SMALLINT NOT NULL DEFAULT 45
    CHECK (login_bg_overlay BETWEEN 0 AND 85);

-- Portal font (a key from the frontend catalog) and base text size.
ALTER TABLE tenant_branding ADD COLUMN font_family TEXT NOT NULL DEFAULT 'system';
ALTER TABLE tenant_branding ADD COLUMN font_size TEXT NOT NULL DEFAULT 'default'
    CHECK (font_size IN ('small', 'default', 'large'));
