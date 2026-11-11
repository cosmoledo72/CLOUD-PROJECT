-- The refreshed interface is drawn for Inter: it becomes the default portal
-- font, also for portals that kept the former default ("system").
ALTER TABLE tenant_branding ALTER COLUMN font_family SET DEFAULT 'inter';
UPDATE tenant_branding SET font_family = 'inter' WHERE font_family = 'system';
