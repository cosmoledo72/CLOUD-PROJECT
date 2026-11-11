-- The «Наименование объекта» requisite and the project name are now one value
-- (kept equal by the API). Align existing projects once: the requisite wins.

CREATE FUNCTION pg_tmp_folder_name(n TEXT) RETURNS TEXT LANGUAGE plpgsql AS $$
DECLARE r TEXT := btrim(translate(n, E'/\\', '--'));
BEGIN
    WHILE octet_length(r) > 255 LOOP
        r := left(r, -1);
    END LOOP;
    r := btrim(r);
    IF r = '' OR r = '.' OR r = '..' THEN r := 'Проект'; END IF;
    RETURN r;
END $$;

WITH name_defs AS (
    -- First active text field per portal that is the object name (same rule as fields::name_def).
    SELECT DISTINCT ON (tenant_id) id, tenant_id
      FROM project_field_defs
     WHERE NOT is_archived AND field_type IN ('text', 'textarea')
       AND (key = 'object_name' OR lower(btrim(label)) = 'наименование объекта')
     ORDER BY tenant_id, sort_order, created_at
), wanted AS (
    SELECT p.id, p.folder_id,
           left(regexp_replace(btrim(v.value #>> '{}'), '\s+', ' ', 'g'), 1000) AS name
      FROM projects p
      JOIN name_defs d ON d.tenant_id = p.tenant_id
      JOIN project_field_values v ON v.project_id = p.id AND v.field_id = d.id
     WHERE jsonb_typeof(v.value) = 'string' AND btrim(v.value #>> '{}') <> ''
), renamed AS (
    UPDATE projects p SET name = w.name FROM wanted w WHERE p.id = w.id AND p.name <> w.name
    RETURNING p.id, p.folder_id, p.name
)
UPDATE folders f SET name = pg_tmp_folder_name(r.name)
  FROM renamed r WHERE f.id = r.folder_id AND f.kind = 'project';

-- Projects without the requisite value get it from their name, so both sides match.
INSERT INTO project_field_values (project_id, field_id, value, updated_at)
SELECT p.id, d.id, to_jsonb(p.name), now()
  FROM projects p
  JOIN (SELECT DISTINCT ON (tenant_id) id, tenant_id FROM project_field_defs
         WHERE NOT is_archived AND field_type IN ('text', 'textarea')
           AND (key = 'object_name' OR lower(btrim(label)) = 'наименование объекта')
         ORDER BY tenant_id, sort_order, created_at) d ON d.tenant_id = p.tenant_id
ON CONFLICT (project_id, field_id) DO NOTHING;

DROP FUNCTION pg_tmp_folder_name(TEXT);
