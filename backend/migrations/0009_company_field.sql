-- Requisite type "company": a counterparty picked from the CRM directory.
-- Value: {"id": <crm company id or null>, "name": "...", "inn": "..."}; id is
-- null when the customer was typed in without being in the directory.

ALTER TABLE project_field_defs DROP CONSTRAINT project_field_defs_field_type_check;
ALTER TABLE project_field_defs ADD CONSTRAINT project_field_defs_field_type_check
    CHECK (field_type IN ('text', 'textarea', 'number', 'date', 'select', 'user', 'checkbox', 'company'));

-- The "Заказчик" requisite of the ready-made set becomes a directory field;
-- projects already linked to a company keep that link.
UPDATE project_field_values v
   SET value = jsonb_build_object(
           'id', p.company_id,
           'name', v.value #>> '{}',
           'inn', (SELECT c.inn FROM crm_companies c WHERE c.id = p.company_id))
  FROM project_field_defs d, projects p
 WHERE d.id = v.field_id AND p.id = v.project_id
   AND d.key = 'client' AND d.field_type = 'text' AND jsonb_typeof(v.value) = 'string';

UPDATE project_field_defs SET field_type = 'company' WHERE key = 'client' AND field_type = 'text';
