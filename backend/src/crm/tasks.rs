//! CRM to-dos: calls, meetings, emails and other steps planned for a
//! company or a deal, with reminders. Completing one records the result in
//! the interaction history.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{company_visible_sql, deals::load_deal, visible_company};
use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    notify::{self, Note},
    state::AppState,
};

fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

pub const KINDS: [&str; 4] = ["call", "meeting", "email", "todo"];

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "call" => "Звонок",
        "meeting" => "Встреча",
        "email" => "Письмо",
        _ => "Дело",
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct TaskRow {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub notes: String,
    pub due_date: NaiveDate,
    pub due_time: Option<NaiveTime>,
    pub company_id: Option<Uuid>,
    pub company_name: Option<String>,
    pub deal_id: Option<Uuid>,
    pub deal_title: Option<String>,
    pub contact_id: Option<Uuid>,
    pub contact_name: Option<String>,
    pub contact_phone: Option<String>,
    pub assignee_id: Uuid,
    pub assignee_name: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub done_at: Option<DateTime<Utc>>,
    pub done_by_name: Option<String>,
    pub result: String,
    pub can_edit: bool,
}

/// `$1` = current user, `$2` = manager flag, `$3` = tenant.
fn select(extra_where: &str, order: &str) -> String {
    format!(
        "SELECT t.id, t.kind, t.title, t.notes, t.due_date, t.due_time, t.company_id, c.name AS company_name,
                t.deal_id, d.title AS deal_title, t.contact_id,
                NULLIF(concat_ws(' ', ct.last_name, ct.first_name), '') AS contact_name, NULLIF(ct.phone, '') AS contact_phone,
                t.assignee_id, ua.display_name AS assignee_name, t.created_by, t.created_at, t.done_at,
                ud.display_name AS done_by_name, t.result,
                ($2 OR t.assignee_id = $1 OR t.created_by = $1 OR c.responsible_id = $1) AS can_edit
           FROM crm_tasks t
           LEFT JOIN crm_companies c ON c.id = t.company_id
           LEFT JOIN crm_deals d ON d.id = t.deal_id
           LEFT JOIN crm_contacts ct ON ct.id = t.contact_id
           LEFT JOIN users ua ON ua.id = t.assignee_id
           LEFT JOIN users ud ON ud.id = t.done_by
          WHERE t.tenant_id = $3
            AND (t.assignee_id = $1 OR t.created_by = $1 OR t.company_id IS NULL OR {vis})
            {extra_where}
          ORDER BY {order}",
        vis = company_visible_sql("c", 1, 2)
    )
}

async fn load(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<TaskRow> {
    user.require_crm()?;
    sqlx::query_as::<_, TaskRow>(&select("AND t.id = $4", "t.id"))
        .bind(user.user_id)
        .bind(user.crm_manager())
        .bind(user.tenant_id)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Today in the portal time zone.
pub async fn portal_today(state: &AppState, tenant_id: Uuid) -> ApiResult<NaiveDate> {
    Ok(sqlx::query_scalar("SELECT (now() AT TIME ZONE timezone)::date FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await?)
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// mine (default) | all
    pub scope: Option<String>,
    /// open (default) | done
    pub status: Option<String>,
    pub company_id: Option<Uuid>,
    pub deal_id: Option<Uuid>,
    pub assignee_id: Option<Uuid>,
}

pub async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let done = q.status.as_deref() == Some("done");
    // Records of a company or deal show everyone's tasks; the to-do list shows mine by default.
    let mine = q.company_id.is_none() && q.deal_id.is_none() && q.assignee_id.is_none() && q.scope.as_deref() != Some("all");
    let extra = format!(
        "AND ({}) AND ($4::uuid IS NULL OR t.company_id = $4) AND ($5::uuid IS NULL OR t.deal_id = $5)
         AND ($6::uuid IS NULL OR t.assignee_id = $6) AND (NOT $7 OR t.assignee_id = $1)",
        if done { "t.done_at IS NOT NULL" } else { "t.done_at IS NULL" }
    );
    let order = if done { "t.done_at DESC LIMIT 200" } else { "t.due_date, t.due_time NULLS LAST, t.created_at LIMIT 1000" };
    let rows = sqlx::query_as::<_, TaskRow>(&select(&extra, order))
        .bind(user.user_id)
        .bind(user.crm_manager())
        .bind(user.tenant_id)
        .bind(q.company_id)
        .bind(q.deal_id)
        .bind(q.assignee_id)
        .bind(mine)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({ "tasks": rows, "today": portal_today(&state, user.tenant_id).await? })))
}

/// Counters for the menu: my open tasks that are overdue or due today.
pub async fn summary(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let today = portal_today(&state, user.tenant_id).await?;
    let (overdue, due_today): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE due_date < $2), count(*) FILTER (WHERE due_date = $2)
           FROM crm_tasks WHERE assignee_id = $1 AND done_at IS NULL",
    )
    .bind(user.user_id)
    .bind(today)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({ "overdue": overdue, "today": due_today })))
}

fn parse_time(s: &str) -> ApiResult<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S"))
        .map_err(|_| ApiError::BadRequest("время указывается как ЧЧ:ММ".into()))
}

/// Resolve and check the links of a task: the deal fixes the company; the
/// contact must belong to the company.
async fn links(
    state: &AppState,
    user: &CurrentUser,
    company_id: Option<Uuid>,
    deal_id: Option<Uuid>,
    contact_id: Option<Uuid>,
) -> ApiResult<Option<Uuid>> {
    let company = match deal_id {
        Some(did) => {
            let deal = load_deal(state, user, did).await?;
            if company_id.is_some() && company_id != deal.company_id {
                return bad_request("дело сделки привязывается к контрагенту этой сделки");
            }
            deal.company_id
        }
        None => company_id,
    };
    if let Some(cid) = company {
        visible_company(state, user, cid).await?;
    }
    if let Some(ct) = contact_id {
        let cc: Option<Option<Uuid>> = sqlx::query_scalar("SELECT company_id FROM crm_contacts WHERE id = $1 AND tenant_id = $2")
            .bind(ct)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?;
        let cc = cc.ok_or_else(|| ApiError::BadRequest("unknown contact".into()))?;
        if company.is_some() && cc.is_some() && cc != company {
            return bad_request("контактное лицо относится к другому контрагенту");
        }
    }
    Ok(company)
}

/// The assignee must use the CRM and be able to see the company.
async fn check_assignee(state: &AppState, user: &CurrentUser, assignee: Uuid, company: Option<Uuid>) -> ApiResult<()> {
    let ok = notify::crm_audience(state, user.tenant_id, company, &[assignee]).await?;
    let active: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND is_active)")
        .bind(assignee)
        .fetch_one(&state.db)
        .await?;
    if ok.is_empty() || !active {
        return bad_request("исполнитель не работает в CRM или не видит этого контрагента");
    }
    Ok(())
}

fn link_of(t: &TaskRow) -> String {
    match (t.deal_id, t.company_id) {
        (Some(d), _) => format!("/crm/deals/{d}"),
        (None, Some(c)) => format!("/crm/companies/{c}"),
        _ => "/crm/tasks".into(),
    }
}

fn when(t: &TaskRow) -> String {
    let date = t.due_date.format("%d.%m.%Y");
    match t.due_time {
        Some(tm) => format!("{date} в {}", tm.format("%H:%M")),
        None => date.to_string(),
    }
}

fn subject(t: &TaskRow) -> String {
    match (&t.deal_title, &t.company_name) {
        (Some(d), Some(c)) => format!("Сделка «{d}», {}", crate::mail::quoted(c)),
        (Some(d), None) => format!("Сделка «{d}»"),
        (None, Some(c)) => crate::mail::quoted(c),
        _ => String::new(),
    }
}

async fn notify_assigned(state: &AppState, user: &CurrentUser, t: &TaskRow) {
    let subj = subject(t);
    let mut parts = vec![];
    if !subj.is_empty() {
        parts.push(subj);
    }
    parts.push(format!("{}, срок {}", kind_label(&t.kind), when(t)));
    parts.push(format!("Поручил(а): {}", user.display_name));
    notify::notify(
        state,
        user.tenant_id,
        Some(user.user_id),
        &[t.assignee_id],
        Note {
            kind: "crm_task_assigned",
            title: format!("Вам поручено: {}", t.title),
            body: parts.join(". "),
            link: link_of(t),
            entity_type: Some("crm_task"),
            entity_id: Some(t.id),
            ..Default::default()
        },
    )
    .await;
}

#[derive(Deserialize)]
pub struct NewTask {
    #[serde(default = "default_kind")]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    pub due_date: NaiveDate,
    pub due_time: Option<String>,
    pub company_id: Option<Uuid>,
    pub deal_id: Option<Uuid>,
    pub contact_id: Option<Uuid>,
    pub assignee_id: Option<Uuid>,
}

fn default_kind() -> String {
    "call".into()
}

fn check_text(kind: &str, title: &str) -> ApiResult<()> {
    if !KINDS.contains(&kind) {
        return bad_request("kind must be call, meeting, email or todo");
    }
    let n = title.trim().chars().count();
    if n == 0 || n > 300 {
        return bad_request("укажите, что нужно сделать (до 300 символов)");
    }
    Ok(())
}

pub async fn create(State(state): State<AppState>, user: CurrentUser, Json(req): Json<NewTask>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    check_text(&req.kind, &req.title)?;
    let due_time = req.due_time.as_deref().filter(|s| !s.is_empty()).map(parse_time).transpose()?;
    let company = links(&state, &user, req.company_id, req.deal_id, req.contact_id).await?;
    let assignee = req.assignee_id.unwrap_or(user.user_id);
    check_assignee(&state, &user, assignee, company).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO crm_tasks (tenant_id, kind, title, notes, due_date, due_time, company_id, deal_id, contact_id, assignee_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&req.kind)
    .bind(req.title.trim())
    .bind(req.notes.trim())
    .bind(req.due_date)
    .bind(due_time)
    .bind(company)
    .bind(req.deal_id)
    .bind(req.contact_id)
    .bind(assignee)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    let t = load(&state, &user, id).await?;
    notify_assigned(&state, &user, &t).await;
    Ok(Json(json!({ "task": t })))
}

#[derive(Deserialize)]
pub struct UpdateTask {
    pub kind: Option<String>,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub due_date: Option<NaiveDate>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub contact_id: Option<Option<Uuid>>,
    pub assignee_id: Option<Uuid>,
    /// Mark done (with `result`) or reopen.
    pub done: Option<bool>,
    pub result: Option<String>,
}

pub async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<UpdateTask>) -> ApiResult<Json<Value>> {
    let t = load(&state, &user, id).await?;
    if !t.can_edit {
        return Err(ApiError::Forbidden);
    }
    let kind = req.kind.clone().unwrap_or_else(|| t.kind.clone());
    let title = req.title.clone().unwrap_or_else(|| t.title.clone());
    check_text(&kind, &title)?;
    let due_time = match &req.due_time {
        Some(Some(s)) if !s.is_empty() => Some(parse_time(s)?),
        Some(_) => None,
        None => t.due_time,
    };
    let contact = req.contact_id.unwrap_or(t.contact_id);
    if contact != t.contact_id {
        links(&state, &user, t.company_id, None, contact).await?;
    }
    let assignee = req.assignee_id.unwrap_or(t.assignee_id);
    if assignee != t.assignee_id {
        check_assignee(&state, &user, assignee, t.company_id).await?;
    }
    let result = req.result.clone().map(|r| r.trim().to_string()).unwrap_or_else(|| t.result.clone());
    if result.chars().count() > 5000 {
        return bad_request("слишком длинный результат");
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE crm_tasks SET kind = $2, title = $3, notes = $4, due_date = $5, due_time = $6, contact_id = $7,
                assignee_id = $8, result = $9,
                done_at = CASE WHEN $10::bool IS NULL THEN done_at WHEN $10 THEN COALESCE(done_at, now()) ELSE NULL END,
                done_by = CASE WHEN $10::bool IS NULL THEN done_by WHEN $10 THEN COALESCE(done_by, $11) ELSE NULL END
          WHERE id = $1",
    )
    .bind(id)
    .bind(&kind)
    .bind(title.trim())
    .bind(req.notes.as_deref().map(str::trim).unwrap_or(&t.notes))
    .bind(req.due_date.unwrap_or(t.due_date))
    .bind(due_time)
    .bind(contact)
    .bind(assignee)
    .bind(&result)
    .bind(req.done)
    .bind(user.user_id)
    .execute(&mut *tx)
    .await?;
    // Completing writes the outcome into the interaction history.
    if req.done == Some(true) && t.done_at.is_none() && (t.company_id.is_some() || t.deal_id.is_some()) {
        let activity_kind = match kind.as_str() {
            "call" | "meeting" | "email" => kind.as_str(),
            _ => "note",
        };
        let body = if result.is_empty() { format!("Выполнено: {}", title.trim()) } else { format!("{}: {result}", title.trim()) };
        sqlx::query(
            "INSERT INTO crm_activities (tenant_id, company_id, deal_id, contact_id, kind, body, happened_at, author_id)
             VALUES ($1, $2, $3, $4, $5, $6, now(), $7)",
        )
        .bind(user.tenant_id)
        .bind(t.company_id)
        .bind(t.deal_id)
        .bind(contact)
        .bind(activity_kind)
        .bind(body)
        .bind(user.user_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let updated = load(&state, &user, id).await?;
    if assignee != t.assignee_id {
        notify_assigned(&state, &user, &updated).await;
    }
    Ok(Json(json!({ "task": updated })))
}

pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load(&state, &user, id).await?;
    if !t.can_edit {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("DELETE FROM crm_tasks WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Reminders (called from the notification worker)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct DueRow {
    id: Uuid,
    tenant_id: Uuid,
    kind: String,
    title: String,
    due_date: NaiveDate,
    due_time: Option<NaiveTime>,
    company_id: Option<Uuid>,
    company_name: Option<String>,
    deal_id: Option<Uuid>,
    deal_title: Option<String>,
    assignee_id: Uuid,
    local_now: chrono::NaiveDateTime,
}

/// Minutes before a timed task when its reminder is sent.
pub const LEAD_MINUTES: i64 = 15;

pub async fn scan_reminders(state: &AppState) -> anyhow::Result<()> {
    let rows = sqlx::query_as::<_, DueRow>(
        "SELECT t.id, t.tenant_id, t.kind, t.title, t.due_date, t.due_time, t.company_id, c.name AS company_name,
                t.deal_id, d.title AS deal_title, t.assignee_id, (now() AT TIME ZONE ten.timezone) AS local_now
           FROM crm_tasks t
           JOIN tenants ten ON ten.id = t.tenant_id
           LEFT JOIN crm_companies c ON c.id = t.company_id
           LEFT JOIN crm_deals d ON d.id = t.deal_id
          WHERE t.done_at IS NULL AND ten.status = 'active' AND NOT ('crm' = ANY(ten.disabled_modules))
            AND t.due_date BETWEEN current_date - 9 AND current_date + 2",
    )
    .fetch_all(&state.db)
    .await?;
    for r in rows {
        let today = r.local_now.date();
        let hour_ok = chrono::Timelike::hour(&r.local_now.time()) as i32 >= state.cfg.due_reminder_hour;
        let (nkind, title, key) = if r.due_date < today {
            if r.due_date < today - chrono::Duration::days(7) || !hour_ok {
                continue;
            }
            ("crm_task_overdue", format!("Просрочено: {}", r.title), format!("overdue:{}:{}", r.id, r.due_date))
        } else if r.due_date == today {
            match r.due_time {
                Some(tm) => {
                    let at = r.due_date.and_time(tm);
                    if r.local_now < at - chrono::Duration::minutes(LEAD_MINUTES) {
                        continue;
                    }
                    ("crm_task_due", format!("В {}: {}", tm.format("%H:%M"), r.title), format!("at:{}:{}T{}", r.id, r.due_date, tm))
                }
                None if hour_ok => ("crm_task_due", format!("Сегодня: {}", r.title), format!("day:{}:{}", r.id, r.due_date)),
                None => continue,
            }
        } else {
            continue;
        };
        let row = TaskRow {
            id: r.id,
            kind: r.kind.clone(),
            title: r.title.clone(),
            notes: String::new(),
            due_date: r.due_date,
            due_time: r.due_time,
            company_id: r.company_id,
            company_name: r.company_name.clone(),
            deal_id: r.deal_id,
            deal_title: r.deal_title.clone(),
            contact_id: None,
            contact_name: None,
            contact_phone: None,
            assignee_id: r.assignee_id,
            assignee_name: None,
            created_by: None,
            created_at: Utc::now(),
            done_at: None,
            done_by_name: None,
            result: String::new(),
            can_edit: false,
        };
        let subj = subject(&row);
        let body = if subj.is_empty() { format!("{}, срок {}", kind_label(&r.kind), when(&row)) } else { format!("{subj}. Срок {}", when(&row)) };
        notify::notify(
            state,
            r.tenant_id,
            None,
            &[r.assignee_id],
            Note {
                kind: nkind,
                title,
                body,
                link: link_of(&row),
                entity_type: Some("crm_task"),
                entity_id: Some(r.id),
                dedupe_key: Some(key),
            },
        )
        .await;
    }
    Ok(())
}
