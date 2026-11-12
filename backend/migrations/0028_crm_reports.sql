-- CRM reports (B11): every stage a deal entered, for the funnel and the time
-- spent on stages. Rebuilt for existing deals from the system entries of the
-- interaction history ("Создана сделка … на этапе «X»", "Этап сделки …: A → B").

CREATE TABLE crm_deal_stage_log (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    deal_id    UUID NOT NULL REFERENCES crm_deals(id) ON DELETE CASCADE,
    stage_id   UUID NOT NULL REFERENCES crm_stages(id) ON DELETE CASCADE,
    entered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    changed_by UUID REFERENCES users(id) ON DELETE SET NULL
);
CREATE INDEX crm_deal_stage_log_deal ON crm_deal_stage_log (deal_id, entered_at);
CREATE INDEX crm_deal_stage_log_tenant ON crm_deal_stage_log (tenant_id, stage_id);

-- Stage of creation.
INSERT INTO crm_deal_stage_log (tenant_id, deal_id, stage_id, entered_at, changed_by)
SELECT d.tenant_id, d.id, s.id, d.created_at, d.created_by
  FROM crm_deals d
  JOIN LATERAL (
        SELECT substring(a.body FROM 'на этапе «(.*)»$') AS name
          FROM crm_activities a
         WHERE a.deal_id = d.id AND a.kind = 'system' AND a.body LIKE 'Создана сделка %'
         ORDER BY a.happened_at LIMIT 1) c ON true
  JOIN crm_stages s ON s.tenant_id = d.tenant_id AND s.name = c.name;

-- Stage changes.
INSERT INTO crm_deal_stage_log (tenant_id, deal_id, stage_id, entered_at, changed_by)
SELECT d.tenant_id, d.id, s.id, a.happened_at, a.author_id
  FROM crm_activities a
  JOIN crm_deals d ON d.id = a.deal_id
  JOIN crm_stages s ON s.tenant_id = d.tenant_id
                   AND s.name = split_part(split_part(a.body, ' → ', 2), '. Причина: ', 1)
 WHERE a.kind = 'system' AND a.body LIKE 'Этап сделки %' AND a.body LIKE '% → %';

-- The current stage, when the history does not end with it (renamed stages,
-- deals older than the history).
INSERT INTO crm_deal_stage_log (tenant_id, deal_id, stage_id, entered_at, changed_by)
SELECT d.tenant_id, d.id, d.stage_id, COALESCE(d.closed_at, d.created_at), d.responsible_id
  FROM crm_deals d
 WHERE d.stage_id IS DISTINCT FROM (
        SELECT l.stage_id FROM crm_deal_stage_log l WHERE l.deal_id = d.id ORDER BY l.entered_at DESC LIMIT 1);
