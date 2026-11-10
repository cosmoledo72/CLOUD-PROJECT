//! Sales pipeline: stages, deals, interaction history, and the link between
//! deals/companies and projects.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{can_edit, check_user, company_visible_sql, require_edit, system_activity, visible_company};
use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    fields::load_defs,
    projects::{create_project_inner, project_role, ProjectRole},
    state::AppState,
};

fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct StageRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub probability: i16,
    pub color: String,
    pub sort_order: f64,
}

/// Create the default pipeline for a portal on first use.
async fn ensure_stages(state: &AppState, tenant_id: Uuid) -> ApiResult<()> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM crm_stages WHERE tenant_id = $1").bind(tenant_id).fetch_one(&state.db).await?;
    if n > 0 {
        return Ok(());
    }
    let defaults = [
        ("Запрос", "open", 10, "#64748b"),
        ("Коммерческое предложение", "open", 30, "#2563eb"),
        ("Тендер", "open", 40, "#7c3aed"),
        ("Согласование договора", "open", 70, "#d97706"),
        ("Выиграна", "won", 100, "#16a34a"),
        ("Проиграна", "lost", 0, "#dc2626"),
    ];
    let mut tx = state.db.begin().await?;
    // Re-check under a lock so that concurrent first requests do not duplicate stages.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::text))").bind(tenant_id).execute(&mut *tx).await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM crm_stages WHERE tenant_id = $1").bind(tenant_id).fetch_one(&mut *tx).await?;
    if n == 0 {
        for (i, (name, kind, prob, color)) in defaults.iter().enumerate() {
            sqlx::query("INSERT INTO crm_stages (tenant_id, name, kind, probability, color, sort_order) VALUES ($1, $2, $3, $4, $5, $6)")
                .bind(tenant_id)
                .bind(name)
                .bind(kind)
                .bind(*prob as i16)
                .bind(color)
                .bind(i as f64)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn load_stages(state: &AppState, tenant_id: Uuid) -> ApiResult<Vec<StageRow>> {
    ensure_stages(state, tenant_id).await?;
    Ok(sqlx::query_as::<_, StageRow>(
        "SELECT id, name, kind, probability, color, sort_order FROM crm_stages WHERE tenant_id = $1 ORDER BY sort_order",
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?)
}

pub async fn list_stages(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    Ok(Json(json!({ "stages": load_stages(&state, user.tenant_id).await? })))
}

#[derive(Deserialize)]
pub struct StageInput {
    pub id: Option<Uuid>,
    pub name: String,
    pub kind: String,
    pub probability: i16,
    pub color: String,
}

#[derive(Deserialize)]
pub struct StagesRequest {
    pub stages: Vec<StageInput>,
}

/// Replace the pipeline (managers). Stages that still hold deals cannot be removed.
pub async fn save_stages(State(state): State<AppState>, user: CurrentUser, Json(req): Json<StagesRequest>) -> ApiResult<Json<Value>> {
    if !user.crm_manager() {
        return Err(ApiError::Forbidden);
    }
    let count = |k: &str| req.stages.iter().filter(|s| s.kind == k).count();
    if count("open") == 0 || count("won") == 0 || count("lost") == 0 {
        return bad_request("the pipeline needs at least one open, one won and one lost stage");
    }
    for s in &req.stages {
        let name = s.name.trim();
        let color_ok = s.color.len() == 7 && s.color.starts_with('#') && s.color[1..].chars().all(|c| c.is_ascii_hexdigit());
        if name.is_empty() || name.chars().count() > 60 || !matches!(s.kind.as_str(), "open" | "won" | "lost")
            || !(0..=100).contains(&s.probability) || !color_ok
        {
            return bad_request(format!("invalid stage «{name}»"));
        }
    }
    ensure_stages(&state, user.tenant_id).await?;
    let mut tx = state.db.begin().await?;
    let keep: Vec<Uuid> = req.stages.iter().filter_map(|s| s.id).collect();
    let removed: Vec<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT s.id, s.name, (SELECT count(*) FROM crm_deals d WHERE d.stage_id = s.id)
           FROM crm_stages s WHERE s.tenant_id = $1 AND NOT (s.id = ANY($2))",
    )
    .bind(user.tenant_id)
    .bind(&keep)
    .fetch_all(&mut *tx)
    .await?;
    if let Some((_, name, n)) = removed.iter().find(|r| r.2 > 0) {
        return bad_request(format!("stage «{name}» still has {n} deal(s); move them before deleting the stage"));
    }
    sqlx::query("DELETE FROM crm_stages WHERE tenant_id = $1 AND NOT (id = ANY($2))")
        .bind(user.tenant_id)
        .bind(&keep)
        .execute(&mut *tx)
        .await?;
    for (i, s) in req.stages.iter().enumerate() {
        let color = s.color.to_lowercase();
        match s.id {
            Some(id) => {
                let n = sqlx::query(
                    "UPDATE crm_stages SET name = $3, kind = $4, probability = $5, color = $6, sort_order = $7
                      WHERE id = $1 AND tenant_id = $2",
                )
                .bind(id)
                .bind(user.tenant_id)
                .bind(s.name.trim())
                .bind(&s.kind)
                .bind(s.probability)
                .bind(&color)
                .bind(i as f64)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if n == 0 {
                    return Err(ApiError::NotFound);
                }
            }
            None => {
                sqlx::query("INSERT INTO crm_stages (tenant_id, name, kind, probability, color, sort_order) VALUES ($1, $2, $3, $4, $5, $6)")
                    .bind(user.tenant_id)
                    .bind(s.name.trim())
                    .bind(&s.kind)
                    .bind(s.probability)
                    .bind(&color)
                    .bind(i as f64)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "crm.stages_updated", None, None, json!({ "count": req.stages.len() })).await;
    Ok(Json(json!({ "stages": load_stages(&state, user.tenant_id).await? })))
}

// ---------------------------------------------------------------------------
// Deals
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct DealRow {
    pub id: Uuid,
    pub title: String,
    pub company_id: Option<Uuid>,
    pub company_name: Option<String>,
    pub contact_id: Option<Uuid>,
    pub contact_name: Option<String>,
    pub stage_id: Uuid,
    pub stage_kind: String,
    pub probability: i16,
    pub amount: f64,
    pub expected_close: Option<NaiveDate>,
    pub responsible_id: Option<Uuid>,
    pub responsible_name: Option<String>,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub description: String,
    pub lost_reason: String,
    pub sort_order: f64,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    /// Nearest open CRM task of the deal.
    pub next_task_date: Option<NaiveDate>,
    pub next_task_title: Option<String>,
}

const DEAL_SELECT: &str = "SELECT d.id, d.title, d.company_id, c.name AS company_name, d.contact_id,
        NULLIF(concat_ws(' ', ct.last_name, ct.first_name), '') AS contact_name,
        d.stage_id, s.kind AS stage_kind, s.probability, d.amount::float8 AS amount, d.expected_close,
        d.responsible_id, u.display_name AS responsible_name, d.project_id, p.name AS project_name,
        d.description, d.lost_reason, d.sort_order, d.created_by, d.created_at, d.updated_at, d.closed_at,
        nt.due_date AS next_task_date, nt.title AS next_task_title
   FROM crm_deals d
   LEFT JOIN LATERAL (SELECT t.due_date, t.title FROM crm_tasks t WHERE t.deal_id = d.id AND t.done_at IS NULL
                       ORDER BY t.due_date, t.due_time NULLS LAST LIMIT 1) nt ON true
   JOIN crm_stages s ON s.id = d.stage_id
   LEFT JOIN crm_companies c ON c.id = d.company_id
   LEFT JOIN crm_contacts ct ON ct.id = d.contact_id
   LEFT JOIN users u ON u.id = d.responsible_id
   LEFT JOIN projects p ON p.id = d.project_id";

/// Deal visibility: via its company, or always for its responsible person.
fn deal_visible_sql(user_param: usize, manager_param: usize) -> String {
    format!(
        "(d.company_id IS NULL OR d.responsible_id = ${u} OR {vis})",
        u = user_param,
        vis = company_visible_sql("c", user_param, manager_param)
    )
}

pub(crate) async fn load_deal(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<DealRow> {
    user.require_crm()?;
    sqlx::query_as::<_, DealRow>(&format!("{DEAL_SELECT} WHERE d.id = $1 AND d.tenant_id = $2 AND {}", deal_visible_sql(3, 4)))
        .bind(id)
        .bind(user.tenant_id)
        .bind(user.user_id)
        .bind(user.crm_manager())
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn deals_of_company(state: &AppState, user: &CurrentUser, company_id: Uuid) -> ApiResult<Vec<DealRow>> {
    Ok(sqlx::query_as::<_, DealRow>(&format!(
        "{DEAL_SELECT} WHERE d.company_id = $1 AND d.tenant_id = $2 AND {} ORDER BY d.closed_at NULLS FIRST, d.created_at DESC",
        deal_visible_sql(3, 4)
    ))
    .bind(company_id)
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.crm_manager())
    .fetch_all(&state.db)
    .await?)
}

#[derive(Deserialize)]
pub struct DealQuery {
    pub q: Option<String>,
    pub responsible_id: Option<Uuid>,
    pub company_id: Option<Uuid>,
    /// Closed deals are shown for this many days after closing (default 30).
    pub closed_days: Option<i32>,
}

pub async fn list_deals(State(state): State<AppState>, user: CurrentUser, Query(q): Query<DealQuery>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let stages = load_stages(&state, user.tenant_id).await?;
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{}%", s.to_lowercase()));
    let deals = sqlx::query_as::<_, DealRow>(&format!(
        "{DEAL_SELECT}
          WHERE d.tenant_id = $1 AND {vis}
            AND ($4::text IS NULL OR lower(d.title) LIKE $4 OR lower(c.name) LIKE $4)
            AND ($5::uuid IS NULL OR d.responsible_id = $5)
            AND ($6::uuid IS NULL OR d.company_id = $6)
            AND (d.closed_at IS NULL OR d.closed_at > now() - make_interval(days => $7))
          ORDER BY d.sort_order, d.created_at",
        vis = deal_visible_sql(2, 3)
    ))
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.crm_manager())
    .bind(search)
    .bind(q.responsible_id)
    .bind(q.company_id)
    .bind(q.closed_days.unwrap_or(30).clamp(0, 3650))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "stages": stages, "deals": deals })))
}

pub async fn get_deal(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let deal = load_deal(&state, &user, id).await?;
    let activities = activities(&state, "a.deal_id = $1", id).await?;
    let editable = can_edit(&user, deal.responsible_id, deal.created_by);
    Ok(Json(json!({ "deal": deal, "activities": activities, "can_edit": editable })))
}

async fn check_stage(state: &AppState, tenant_id: Uuid, stage_id: Uuid) -> ApiResult<(String, String)> {
    sqlx::query_as("SELECT name, kind FROM crm_stages WHERE id = $1 AND tenant_id = $2")
        .bind(stage_id)
        .bind(tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::BadRequest("unknown stage".into()))
}

/// The contact must exist in the tenant; if the deal has a company, the contact must belong to it.
async fn check_contact(state: &AppState, user: &CurrentUser, contact_id: Option<Uuid>, company_id: Option<Uuid>) -> ApiResult<()> {
    if let Some(cid) = contact_id {
        let contact_company: Option<Option<Uuid>> = sqlx::query_scalar("SELECT company_id FROM crm_contacts WHERE id = $1 AND tenant_id = $2")
            .bind(cid)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?;
        let contact_company = contact_company.ok_or_else(|| ApiError::BadRequest("unknown contact".into()))?;
        if let (Some(cc), Some(dc)) = (contact_company, company_id) {
            if cc != dc {
                return bad_request("the contact belongs to another company");
            }
        }
        if let Some(cc) = contact_company {
            visible_company(state, user, cc).await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateDeal {
    pub title: String,
    pub company_id: Option<Uuid>,
    pub contact_id: Option<Uuid>,
    pub stage_id: Option<Uuid>,
    pub amount: Option<f64>,
    pub expected_close: Option<NaiveDate>,
    pub responsible_id: Option<Uuid>,
    #[serde(default)]
    pub description: String,
}

fn check_amount(a: f64) -> ApiResult<f64> {
    if !a.is_finite() || !(0.0..1e13).contains(&a) {
        return bad_request("amount must be a non-negative number");
    }
    Ok((a * 100.0).round() / 100.0)
}

pub async fn create_deal(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CreateDeal>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let title = req.title.trim();
    if title.is_empty() {
        return bad_request("title is required");
    }
    if let Some(c) = req.company_id {
        visible_company(&state, &user, c).await?;
    }
    check_contact(&state, &user, req.contact_id, req.company_id).await?;
    let responsible = req.responsible_id.or(Some(user.user_id));
    check_user(&state, user.tenant_id, responsible).await?;
    let stages = load_stages(&state, user.tenant_id).await?;
    let stage = match req.stage_id {
        Some(id) => stages.iter().find(|s| s.id == id).cloned().ok_or_else(|| ApiError::BadRequest("unknown stage".into()))?,
        None => stages.iter().find(|s| s.kind == "open").cloned().expect("pipeline has an open stage"),
    };
    let amount = check_amount(req.amount.unwrap_or(0.0))?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO crm_deals (tenant_id, title, company_id, contact_id, stage_id, amount, expected_close, responsible_id,
                                description, created_by, sort_order, closed_at)
         VALUES ($1, $2, $3, $4, $5, $6::float8::numeric, $7, $8, $9, $10,
                 COALESCE((SELECT max(sort_order) FROM crm_deals WHERE stage_id = $5), 0) + 1,
                 CASE WHEN $11 THEN now() END)
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(title)
    .bind(req.company_id)
    .bind(req.contact_id)
    .bind(stage.id)
    .bind(amount)
    .bind(req.expected_close)
    .bind(responsible)
    .bind(req.description.trim())
    .bind(user.user_id)
    .bind(stage.kind != "open")
    .fetch_one(&state.db)
    .await?;
    system_activity(&state, &user, req.company_id, Some(id), format!("Создана сделка «{title}» на этапе «{}»", stage.name)).await;
    state.audit(user.tenant_id, Some(user.user_id), "crm.deal_created", None, Some(id), json!({ "title": title })).await;
    notify_deal(
        &state,
        &user,
        responsible,
        "deal_assigned",
        format!("Вы ответственный по сделке «{title}»"),
        format!("Назначил(а): {}. Сумма {}", user.display_name, money(req.amount.unwrap_or(0.0))),
        id,
    )
    .await;
    Ok(Json(json!({ "deal": load_deal(&state, &user, id).await? })))
}

#[derive(Deserialize)]
pub struct UpdateDeal {
    pub title: Option<String>,
    pub stage_id: Option<Uuid>,
    pub amount: Option<f64>,
    pub description: Option<String>,
    pub lost_reason: Option<String>,
    pub sort_order: Option<f64>,
    #[serde(default, deserialize_with = "double_option")]
    pub company_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub contact_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub expected_close: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub responsible_id: Option<Option<Uuid>>,
}

pub async fn update_deal(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDeal>,
) -> ApiResult<Json<Value>> {
    let d = load_deal(&state, &user, id).await?;
    require_edit(&user, d.responsible_id, d.created_by)?;

    let title = req.title.as_deref().map(str::trim).unwrap_or(&d.title).to_string();
    if title.is_empty() {
        return bad_request("title is required");
    }
    let company_id = req.company_id.unwrap_or(d.company_id);
    if company_id != d.company_id {
        if let Some(c) = company_id {
            visible_company(&state, &user, c).await?;
        }
    }
    // Changing the company drops a contact of the old company unless a new one is given.
    let contact_id = match req.contact_id {
        Some(c) => c,
        None if company_id != d.company_id => None,
        None => d.contact_id,
    };
    check_contact(&state, &user, contact_id, company_id).await?;
    let responsible = req.responsible_id.unwrap_or(d.responsible_id);
    if responsible != d.responsible_id {
        check_user(&state, user.tenant_id, responsible).await?;
    }
    let amount = match req.amount {
        Some(a) => check_amount(a)?,
        None => d.amount,
    };
    let stage_id = req.stage_id.unwrap_or(d.stage_id);
    let (new_stage_name, new_kind) = check_stage(&state, user.tenant_id, stage_id).await?;
    let stage_changed = stage_id != d.stage_id;
    let closed_at = match (stage_changed, new_kind.as_str()) {
        (false, _) => d.closed_at,
        (true, "open") => None,
        (true, _) => Some(Utc::now()),
    };

    sqlx::query(
        "UPDATE crm_deals SET title = $2, stage_id = $3, amount = $4::float8::numeric, description = $5, lost_reason = $6,
                sort_order = $7, company_id = $8, contact_id = $9, expected_close = $10, responsible_id = $11,
                closed_at = $12, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&title)
    .bind(stage_id)
    .bind(amount)
    .bind(req.description.as_deref().map(str::trim).unwrap_or(&d.description))
    .bind(req.lost_reason.as_deref().map(str::trim).unwrap_or(&d.lost_reason))
    .bind(req.sort_order.unwrap_or(d.sort_order))
    .bind(company_id)
    .bind(contact_id)
    .bind(req.expected_close.unwrap_or(d.expected_close))
    .bind(responsible)
    .bind(closed_at)
    .execute(&state.db)
    .await?;

    if stage_changed {
        let (old_name, _) = check_stage(&state, user.tenant_id, d.stage_id).await?;
        let mut text = format!("Этап сделки «{title}»: {old_name} → {new_stage_name}");
        if new_kind == "lost" {
            if let Some(r) = req.lost_reason.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
                text.push_str(&format!(". Причина: {r}"));
            }
        }
        system_activity(&state, &user, company_id, Some(id), text.clone()).await;
        // The responsible person hears about moves made by others (not when just reassigned).
        if responsible == d.responsible_id {
            notify_deal(&state, &user, responsible, "deal_stage", format!("Сделка «{title}»: {new_stage_name}"),
                format!("{} перевёл(а) сделку: {old_name} → {new_stage_name}", user.display_name), id).await;
        }
    }
    if responsible != d.responsible_id {
        notify_deal(&state, &user, responsible, "deal_assigned", format!("Вы ответственный по сделке «{title}»"),
            format!("Назначил(а): {}. Этап «{new_stage_name}»", user.display_name), id).await;
    }
    Ok(Json(json!({ "deal": load_deal(&state, &user, id).await? })))
}

async fn notify_deal(state: &AppState, user: &CurrentUser, to: Option<Uuid>, kind: &'static str, title: String, body: String, deal_id: Uuid) {
    let Some(to) = to else { return };
    crate::notify::notify(
        state,
        user.tenant_id,
        Some(user.user_id),
        &[to],
        crate::notify::Note {
            kind,
            title,
            body,
            link: format!("/crm/deals/{deal_id}"),
            entity_type: Some("deal"),
            entity_id: Some(deal_id),
            ..Default::default()
        },
    )
    .await;
}

fn money(v: f64) -> String {
    let s = format!("{:.0}", v);
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 && c != '-' {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    format!("{out}\u{a0}₽")
}

pub async fn delete_deal(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let d = load_deal(&state, &user, id).await?;
    require_edit(&user, d.responsible_id, d.created_by)?;
    sqlx::query("DELETE FROM crm_deals WHERE id = $1").bind(id).execute(&state.db).await?;
    state.audit(user.tenant_id, Some(user.user_id), "crm.deal_deleted", None, Some(id), json!({ "title": d.title })).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Interaction history
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct ActivityRow {
    pub id: Uuid,
    pub kind: String,
    pub body: String,
    pub happened_at: DateTime<Utc>,
    pub author_id: Option<Uuid>,
    pub author_name: Option<String>,
    pub company_id: Option<Uuid>,
    pub deal_id: Option<Uuid>,
    pub deal_title: Option<String>,
    pub contact_id: Option<Uuid>,
    pub contact_name: Option<String>,
}

/// Feed filtered by `condition` (`a.company_id = $1` or `a.deal_id = $1`).
pub async fn activities(state: &AppState, condition: &str, id: Uuid) -> ApiResult<Vec<ActivityRow>> {
    Ok(sqlx::query_as::<_, ActivityRow>(&format!(
        "SELECT a.id, a.kind, a.body, a.happened_at, a.author_id, u.display_name AS author_name, a.company_id, a.deal_id,
                d.title AS deal_title, a.contact_id, NULLIF(concat_ws(' ', ct.last_name, ct.first_name), '') AS contact_name
           FROM crm_activities a
           LEFT JOIN users u ON u.id = a.author_id
           LEFT JOIN crm_deals d ON d.id = a.deal_id
           LEFT JOIN crm_contacts ct ON ct.id = a.contact_id
          WHERE {condition}
          ORDER BY a.happened_at DESC, a.created_at DESC LIMIT 300"
    ))
    .bind(id)
    .fetch_all(&state.db)
    .await?)
}

#[derive(Deserialize)]
pub struct ActivityInput {
    pub company_id: Option<Uuid>,
    pub contact_id: Option<Uuid>,
    pub deal_id: Option<Uuid>,
    pub kind: String,
    pub body: String,
    pub happened_at: Option<DateTime<Utc>>,
    /// Users picked from the @mention list.
    #[serde(default)]
    pub mentions: Vec<Uuid>,
}

pub async fn create_activity(State(state): State<AppState>, user: CurrentUser, Json(req): Json<ActivityInput>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    if !matches!(req.kind.as_str(), "note" | "call" | "meeting" | "email") {
        return bad_request("kind must be note, call, meeting or email");
    }
    let body = req.body.trim();
    if body.is_empty() || body.chars().count() > 10_000 {
        return bad_request("text is required (up to 10000 chars)");
    }
    let mut company_id = req.company_id;
    if let Some(did) = req.deal_id {
        let d = load_deal(&state, &user, did).await?;
        company_id = company_id.or(d.company_id);
    }
    if let Some(cid) = req.contact_id {
        let cc: Option<Option<Uuid>> = sqlx::query_scalar("SELECT company_id FROM crm_contacts WHERE id = $1 AND tenant_id = $2")
            .bind(cid)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?;
        company_id = company_id.or(cc.ok_or(ApiError::NotFound)?);
    }
    if let Some(c) = company_id {
        visible_company(&state, &user, c).await?;
    }
    if company_id.is_none() && req.deal_id.is_none() && req.contact_id.is_none() {
        return bad_request("an activity needs a company, contact or deal");
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO crm_activities (tenant_id, company_id, contact_id, deal_id, kind, body, happened_at, author_id)
         VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, now()), $8) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(company_id)
    .bind(req.contact_id)
    .bind(req.deal_id)
    .bind(&req.kind)
    .bind(body)
    .bind(req.happened_at)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    if !req.mentions.is_empty() {
        let audience = crate::notify::crm_audience(&state, user.tenant_id, company_id, &req.mentions).await?;
        let (subject, link) = match req.deal_id {
            Some(did) => {
                let t: String = sqlx::query_scalar("SELECT title FROM crm_deals WHERE id = $1").bind(did).fetch_one(&state.db).await?;
                (format!("сделке «{t}»"), format!("/crm/deals/{did}"))
            }
            None => {
                let c = company_id.expect("company or deal is present");
                let n: String = sqlx::query_scalar("SELECT name FROM crm_companies WHERE id = $1").bind(c).fetch_one(&state.db).await?;
                (format!("карточке {}", crate::mail::quoted(&n)), format!("/crm/companies/{c}"))
            }
        };
        crate::notify::notify(
            &state,
            user.tenant_id,
            Some(user.user_id),
            &audience,
            crate::notify::Note {
                kind: "mention",
                title: format!("{} упомянул(а) вас в {subject}", user.display_name),
                body: crate::comments::excerpt(body),
                link,
                entity_type: Some("activity"),
                entity_id: Some(id),
                ..Default::default()
            },
        )
        .await;
    }
    Ok(Json(json!({ "id": id })))
}

pub async fn delete_activity(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let (author, kind): (Option<Uuid>, String) = sqlx::query_as("SELECT author_id, kind FROM crm_activities WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let allowed = user.crm_manager() || (kind != "system" && author == Some(user.user_id));
    if !allowed {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("DELETE FROM crm_activities WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

/// Requisite values that follow from a company (and deal): client name, INN, amount.
async fn customer_requisites(state: &AppState, tenant_id: Uuid, company_id: Uuid, amount: Option<f64>) -> ApiResult<Map<String, Value>> {
    let (name, inn): (String, Option<String>) = sqlx::query_as("SELECT name, inn FROM crm_companies WHERE id = $1")
        .bind(company_id)
        .fetch_one(&state.db)
        .await?;
    let defs = load_defs(state, tenant_id, false).await?;
    let mut out = Map::new();
    if let Some(cust) = crate::fields::customer_def(&defs) {
        out.insert(cust.key.clone(), json!({ "id": company_id, "name": name, "inn": inn }));
    }
    for d in defs {
        match (d.key.as_str(), d.field_type.as_str()) {
            ("client", "text" | "textarea") => {
                out.insert(d.key, json!(name));
            }
            ("client_inn", "text") => {
                if let Some(i) = &inn {
                    out.insert(d.key, json!(i));
                }
            }
            ("contract_amount", "number") => {
                if let Some(a) = amount.filter(|a| *a > 0.0) {
                    out.insert(d.key, json!(a));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct ProjectFromDeal {
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    /// Requisites entered in the dialog; they override values taken from the deal.
    #[serde(default)]
    pub fields: Map<String, Value>,
}

/// Defaults the "create project" dialog pre-fills from a deal.
pub async fn project_defaults(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let d = load_deal(&state, &user, id).await?;
    let fields = match d.company_id {
        Some(c) => customer_requisites(&state, user.tenant_id, c, Some(d.amount)).await?,
        None => Map::new(),
    };
    Ok(Json(json!({ "name": d.title, "fields": fields })))
}

pub async fn create_project_from_deal(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectFromDeal>,
) -> ApiResult<Json<Value>> {
    let d = load_deal(&state, &user, id).await?;
    require_edit(&user, d.responsible_id, d.created_by)?;
    if d.project_id.is_some() {
        return Err(ApiError::Conflict("a project has already been created for this deal".into()));
    }
    let mut fields = match d.company_id {
        Some(c) => customer_requisites(&state, user.tenant_id, c, Some(d.amount)).await?,
        None => Map::new(),
    };
    fields.extend(req.fields);
    let name = req.name.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&d.title).to_string();
    let (project_id, folder_id) =
        create_project_inner(&state, &user, &name, &req.description, &fields, d.company_id, Some(d.id)).await?;
    // The deal's responsible person joins the project as a manager.
    if let Some(r) = d.responsible_id.filter(|r| *r != user.user_id) {
        sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, 'manager') ON CONFLICT DO NOTHING")
            .bind(project_id)
            .bind(r)
            .execute(&state.db)
            .await?;
    }
    sqlx::query("UPDATE crm_deals SET project_id = $2, updated_at = now() WHERE id = $1").bind(id).bind(project_id).execute(&state.db).await?;
    system_activity(&state, &user, d.company_id, Some(id), format!("По сделке создан проект «{name}»")).await;
    Ok(Json(json!({ "project_id": project_id, "folder_id": folder_id })))
}

#[derive(Deserialize)]
pub struct LinkCompany {
    pub company_id: Option<Uuid>,
}

/// Set or clear a project's customer; fills the "client" requisites from the company card.
pub async fn link_project_company(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<Uuid>,
    Json(req): Json<LinkCompany>,
) -> ApiResult<Json<Value>> {
    if project_role(&state, &user, project_id).await? < ProjectRole::Manager {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    if let Some(c) = req.company_id {
        visible_company(&state, &user, c).await?;
        let values = customer_requisites(&state, user.tenant_id, c, None).await?;
        let defs = load_defs(&state, user.tenant_id, false).await?;
        for (key, value) in values {
            if let Some(def) = defs.iter().find(|d| d.key == key) {
                sqlx::query(
                    "INSERT INTO project_field_values (project_id, field_id, value, updated_by) VALUES ($1, $2, $3, $4)
                     ON CONFLICT (project_id, field_id) DO UPDATE SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by,
                        updated_at = now()",
                )
                .bind(project_id)
                .bind(def.id)
                .bind(value)
                .bind(user.user_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    if req.company_id.is_none() {
        let defs = load_defs(&state, user.tenant_id, false).await?;
        if let Some(cust) = crate::fields::customer_def(&defs) {
            sqlx::query("DELETE FROM project_field_values WHERE project_id = $1 AND field_id = $2 AND value ->> 'id' IS NOT NULL")
                .bind(project_id)
                .bind(cust.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query("UPDATE projects SET company_id = $2 WHERE id = $1").bind(project_id).bind(req.company_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
