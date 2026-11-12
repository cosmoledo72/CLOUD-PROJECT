//! Time tracking (F3): a running timer per employee, time entries on tasks,
//! labour pay of a task (hourly — hours × the employee's rate fixed on each
//! entry, or a fixed amount for the volume, counted once the task is done),
//! employees' hour rates and the "Отчёты" report with an XLSX export.
//!
//! Who sees what:
//! - an employee sees and edits their own entries;
//! - a project manager sees and edits every entry of their projects;
//! - administrators and finance viewers see everything; only they see hour
//!   rates and labour costs in money (a project's labour total is also shown
//!   on its "Финансы" tab to whoever may open it).

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    money,
    projects::{load_task, project_role, ProjectRole},
    state::AppState,
};

/// Longest single entry: a day.
const MAX_MINUTES: i32 = 24 * 60;
/// Most rows in one report.
const REPORT_LIMIT: i64 = 20_000;

fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// Rates and money of labour are visible to administrators and finance viewers.
fn sees_money(user: &CurrentUser) -> bool {
    user.is_admin() || user.finance_view()
}

fn check_minutes(m: i32) -> ApiResult<()> {
    if m <= 0 || m > MAX_MINUTES {
        return bad_request("Время записи: от 1 минуты до 24 часов");
    }
    Ok(())
}

fn check_note(n: &str) -> ApiResult<String> {
    let n = n.trim();
    if n.chars().count() > 1000 {
        return bad_request("Комментарий: не больше 1000 символов");
    }
    Ok(n.to_string())
}

// ---------------------------------------------------------------------------
// Entries of a task
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct EntryRow {
    id: Uuid,
    task_id: Uuid,
    user_id: Uuid,
    user_name: String,
    work_date: NaiveDate,
    minutes: i32,
    note: String,
    source: String,
    hour_rate: Option<f64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const ENTRY_SELECT: &str = "SELECT te.id, te.task_id, te.user_id, u.display_name AS user_name, te.work_date, te.minutes, te.note,
        te.source, te.hour_rate::float8 AS hour_rate, te.created_at, te.updated_at
   FROM time_entries te JOIN users u ON u.id = te.user_id";

/// `GET /api/tasks/{id}/time` — entries of a task (newest first) for everyone in the project.
pub async fn list_for_task(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    let role = role_or_money(&state, &user, t.project_id).await?;
    let rows = sqlx::query_as::<_, EntryRow>(&format!("{ENTRY_SELECT} WHERE te.task_id = $1 ORDER BY te.work_date DESC, te.created_at DESC"))
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let money = sees_money(&user);
    let total: i64 = rows.iter().map(|r| r.minutes as i64).sum();
    let entries: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let can_edit = r.user_id == user.user_id && role >= ProjectRole::Member || role == ProjectRole::Manager;
            let mut v = serde_json::to_value(&r).unwrap_or(Value::Null);
            v["can_edit"] = json!(can_edit);
            if !money {
                v["hour_rate"] = Value::Null;
            }
            v
        })
        .collect();
    Ok(Json(json!({ "entries": entries, "total_minutes": total, "can_log": role >= ProjectRole::Member })))
}

#[derive(Deserialize)]
pub struct CreateEntry {
    pub work_date: NaiveDate,
    pub minutes: i32,
    #[serde(default)]
    pub note: String,
}

/// `POST /api/tasks/{id}/time` — log time on a task (project members).
pub async fn add(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<CreateEntry>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    project_role(&state, &user, t.project_id).await?.require(ProjectRole::Member)?;
    check_minutes(req.minutes)?;
    let note = check_note(&req.note)?;
    let eid = insert_entry(&state.db, &user, id, req.work_date, req.minutes, &note, "manual").await?;
    Ok(Json(json!({ "id": eid })))
}

async fn insert_entry<'e, E: sqlx::PgExecutor<'e>>(
    db: E,
    user: &CurrentUser,
    task_id: Uuid,
    date: NaiveDate,
    minutes: i32,
    note: &str,
    source: &str,
) -> ApiResult<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO time_entries (tenant_id, task_id, user_id, work_date, minutes, note, source, hour_rate)
         VALUES ($1, $2, $3, $4, $5, $6, $7, (SELECT hour_rate FROM users WHERE id = $3)) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(task_id)
    .bind(user.user_id)
    .bind(date)
    .bind(minutes)
    .bind(note)
    .bind(source)
    .fetch_one(db)
    .await?)
}

/// Load an entry the user may change: their own (still in the project) or any
/// entry of a project they manage.
async fn editable_entry(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<(EntryRow, Uuid)> {
    let row = sqlx::query_as::<_, EntryRow>(&format!("{ENTRY_SELECT} WHERE te.id = $1 AND te.tenant_id = $2"))
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let project_id: Uuid = sqlx::query_scalar("SELECT project_id FROM tasks WHERE id = $1").bind(row.task_id).fetch_one(&state.db).await?;
    let role = project_role(&state, user, project_id).await?;
    let ok = role == ProjectRole::Manager || (row.user_id == user.user_id && role >= ProjectRole::Member);
    if !ok {
        return Err(ApiError::Forbidden);
    }
    Ok((row, project_id))
}

#[derive(Deserialize)]
pub struct UpdateEntry {
    pub work_date: Option<NaiveDate>,
    pub minutes: Option<i32>,
    pub note: Option<String>,
}

/// `PATCH /api/time/{id}` — change an entry (own or, for project managers, any in the project).
pub async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<UpdateEntry>) -> ApiResult<Json<Value>> {
    let (old, _) = editable_entry(&state, &user, id).await?;
    if let Some(m) = req.minutes {
        check_minutes(m)?;
    }
    let note = req.note.as_deref().map(check_note).transpose()?;
    sqlx::query(
        "UPDATE time_entries SET work_date = COALESCE($2, work_date), minutes = COALESCE($3, minutes),
                note = COALESCE($4, note), updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(req.work_date)
    .bind(req.minutes)
    .bind(&note)
    .execute(&state.db)
    .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "time.updated", Some("task"), Some(old.task_id), json!({
            "entry_id": id, "owner": old.user_name,
            "from": { "work_date": old.work_date, "minutes": old.minutes },
            "to": { "work_date": req.work_date.unwrap_or(old.work_date), "minutes": req.minutes.unwrap_or(old.minutes) },
        }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /api/time/{id}`.
pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (old, _) = editable_entry(&state, &user, id).await?;
    sqlx::query("DELETE FROM time_entries WHERE id = $1").bind(id).execute(&state.db).await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "time.deleted", Some("task"), Some(old.task_id), json!({
            "entry_id": id, "owner": old.user_name, "work_date": old.work_date, "minutes": old.minutes, "note": old.note,
        }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct TimerRow {
    task_id: Uuid,
    task_number: i64,
    task_title: String,
    project_id: Uuid,
    project_name: String,
    started_at: DateTime<Utc>,
    elapsed_seconds: i64,
}

async fn current_timer(state: &AppState, user: &CurrentUser) -> ApiResult<Option<TimerRow>> {
    Ok(sqlx::query_as::<_, TimerRow>(
        "SELECT tt.task_id, t.number AS task_number, t.title AS task_title, t.project_id, p.name AS project_name, tt.started_at,
                floor(extract(epoch FROM now() - tt.started_at))::bigint AS elapsed_seconds
           FROM time_timers tt JOIN tasks t ON t.id = tt.task_id JOIN projects p ON p.id = t.project_id
          WHERE tt.user_id = $1",
    )
    .bind(user.user_id)
    .fetch_optional(&state.db)
    .await?)
}

/// `GET /api/time/timer` — the user's running timer, or `null`.
pub async fn get_timer(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "timer": current_timer(&state, &user).await? })))
}

/// Stop the running timer inside a transaction; an entry is made when it ran a minute or more.
/// Returns the entry id and its minutes.
async fn stop_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    state: &AppState,
    user: &CurrentUser,
    note: &str,
) -> ApiResult<Option<(Uuid, i32)>> {
    let running: Option<(Uuid, i64)> = sqlx::query_as(
        "DELETE FROM time_timers WHERE user_id = $1 RETURNING task_id, floor(extract(epoch FROM now() - started_at))::bigint",
    )
    .bind(user.user_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((task_id, secs)) = running else { return Ok(None) };
    let minutes = ((secs + 30) / 60).min(MAX_MINUTES as i64) as i32;
    if minutes < 1 {
        return Ok(None);
    }
    let today = crate::crm::tasks::portal_today(state, user.tenant_id).await?;
    let id = insert_entry(&mut **tx, user, task_id, today, minutes, note, "timer").await?;
    Ok(Some((id, minutes)))
}

/// `POST /api/tasks/{id}/timer` — start the timer on a task; a timer running
/// on another task is stopped first (and its time recorded).
pub async fn start_timer(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    project_role(&state, &user, t.project_id).await?.require(ProjectRole::Member)?;
    if let Some(cur) = current_timer(&state, &user).await? {
        if cur.task_id == id {
            return Ok(Json(json!({ "timer": cur, "stopped": null })));
        }
    }
    let mut tx = state.db.begin().await?;
    let stopped = stop_in_tx(&mut tx, &state, &user, "").await?;
    sqlx::query("INSERT INTO time_timers (user_id, tenant_id, task_id) VALUES ($1, $2, $3)")
        .bind(user.user_id)
        .bind(user.tenant_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({
        "timer": current_timer(&state, &user).await?,
        "stopped": stopped.map(|(eid, m)| json!({ "id": eid, "minutes": m })),
    })))
}

#[derive(Deserialize, Default)]
pub struct StopTimer {
    #[serde(default)]
    pub note: String,
}

/// `POST /api/time/timer/stop` — stop the timer and record the time.
pub async fn stop_timer(State(state): State<AppState>, user: CurrentUser, body: Option<Json<StopTimer>>) -> ApiResult<Json<Value>> {
    let note = check_note(&body.map(|b| b.0.note).unwrap_or_default())?;
    let mut tx = state.db.begin().await?;
    let stopped = stop_in_tx(&mut tx, &state, &user, &note).await?;
    tx.commit().await?;
    Ok(Json(json!({ "entry": stopped.map(|(eid, m)| json!({ "id": eid, "minutes": m })) })))
}

/// `DELETE /api/time/timer` — drop the running timer without recording.
pub async fn discard_timer(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    sqlx::query("DELETE FROM time_timers WHERE user_id = $1").bind(user.user_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Labour pay of a task
// ---------------------------------------------------------------------------

/// Role in the project; finance viewers who are not members read as viewers.
async fn role_or_money(state: &AppState, user: &CurrentUser, project_id: Uuid) -> ApiResult<ProjectRole> {
    match project_role(state, user, project_id).await {
        Err(ApiError::NotFound) if sees_money(user) => Ok(ProjectRole::Viewer),
        other => other,
    }
}

async fn pay_access(state: &AppState, user: &CurrentUser, project_id: Uuid) -> ApiResult<(bool, bool)> {
    let role = role_or_money(state, user, project_id).await?;
    let manager = role == ProjectRole::Manager;
    let see = manager || sees_money(user);
    let edit = manager || user.finance_edit();
    Ok((see, edit))
}

/// `GET /api/tasks/{id}/pay` — how the work is paid and its labour cost
/// (project managers, administrators, finance viewers).
pub async fn get_pay(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    let (see, edit) = pay_access(&state, &user, t.project_id).await?;
    if !see {
        return Err(ApiError::Forbidden);
    }
    let (mode, amount, done): (String, Option<f64>, bool) =
        sqlx::query_as("SELECT pay_mode, pay_amount::float8, status = 'done' FROM tasks WHERE id = $1").bind(id).fetch_one(&state.db).await?;
    let (minutes, cost, unrated): (i64, f64, i64) = sqlx::query_as(
        "SELECT COALESCE(sum(minutes), 0)::bigint, COALESCE(sum(minutes * hour_rate / 60.0), 0)::float8,
                COALESCE(sum(minutes) FILTER (WHERE hour_rate IS NULL), 0)::bigint
           FROM time_entries WHERE task_id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    let money = sees_money(&user);
    let labor = if mode == "fixed" {
        json!({ "accrued": if done { amount.unwrap_or(0.0) } else { 0.0 }, "expected": if done { 0.0 } else { amount.unwrap_or(0.0) } })
    } else if money {
        json!({ "accrued": money::round2(cost), "expected": 0.0, "unrated_minutes": unrated })
    } else {
        Value::Null
    };
    Ok(Json(json!({ "pay_mode": mode, "pay_amount": amount, "minutes": minutes, "labor": labor, "can_edit": edit, "sees_rates": money })))
}

#[derive(Deserialize)]
pub struct SetPay {
    pub pay_mode: String,
    #[serde(default, deserialize_with = "double_option")]
    pub pay_amount: Option<Option<f64>>,
}

/// `PUT /api/tasks/{id}/pay` — set hourly or fixed pay (project managers, finance editors).
pub async fn set_pay(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<SetPay>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    let (_, edit) = pay_access(&state, &user, t.project_id).await?;
    if !edit {
        return Err(ApiError::Forbidden);
    }
    let amount = req.pay_amount.flatten();
    match req.pay_mode.as_str() {
        "hourly" => {}
        "fixed" => match amount {
            Some(a) if a.is_finite() && (0.0..1e12).contains(&a) => {}
            _ => return bad_request("Для фиксированной оплаты укажите сумму"),
        },
        _ => return bad_request("pay_mode must be hourly or fixed"),
    }
    let amount = if req.pay_mode == "fixed" { amount.map(money::round2) } else { None };
    sqlx::query("UPDATE tasks SET pay_mode = $2, pay_amount = $3 WHERE id = $1")
        .bind(id)
        .bind(&req.pay_mode)
        .bind(amount)
        .execute(&state.db)
        .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "task.pay_changed", Some("task"), Some(id), json!({ "pay_mode": req.pay_mode, "pay_amount": amount }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

/// Labour cost of projects: hourly entries × the rate fixed on each entry,
/// plus fixed amounts of done tasks (`accrued`); fixed amounts of tasks not
/// done yet are `expected`. Returns (project id, accrued, expected).
pub async fn project_labor(state: &AppState, tenant_id: Uuid, ids: &[Uuid]) -> ApiResult<Vec<(Uuid, f64, f64)>> {
    Ok(sqlx::query_as(
        "SELECT p.id,
                (COALESCE((SELECT sum(te.minutes * te.hour_rate / 60.0) FROM time_entries te JOIN tasks t ON t.id = te.task_id
                            WHERE t.project_id = p.id AND t.pay_mode = 'hourly'), 0)
                 + COALESCE((SELECT sum(t.pay_amount) FROM tasks t WHERE t.project_id = p.id AND t.pay_mode = 'fixed' AND t.status = 'done'), 0))::float8,
                COALESCE((SELECT sum(t.pay_amount) FROM tasks t WHERE t.project_id = p.id AND t.pay_mode = 'fixed' AND t.status <> 'done'), 0)::float8
           FROM projects p WHERE p.tenant_id = $1 AND p.id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(ids)
    .fetch_all(&state.db)
    .await?)
}

// ---------------------------------------------------------------------------
// Hour rates
// ---------------------------------------------------------------------------

/// `GET /api/time/rates` — employees and their hour rates (administrators, finance viewers).
pub async fn list_rates(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    if !sees_money(&user) {
        return Err(ApiError::Forbidden);
    }
    let rows: Vec<(Uuid, String, Option<f64>, bool)> = sqlx::query_as(
        "SELECT id, display_name, hour_rate::float8, is_active FROM users WHERE tenant_id = $1 AND role <> 'guest' ORDER BY lower(display_name)",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let rates: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, rate, active)| json!({ "user_id": id, "display_name": name, "hour_rate": rate, "is_active": active }))
        .collect();
    Ok(Json(json!({ "rates": rates, "can_edit": user.is_admin() || user.finance_edit() })))
}

#[derive(Deserialize)]
pub struct SetRate {
    pub hour_rate: Option<f64>,
    /// Also put the rate on the employee's earlier entries that have none.
    #[serde(default)]
    pub apply_to_unrated: bool,
}

/// `PUT /api/time/rates/{user_id}` — set an employee's hour rate (administrators, finance editors).
/// Entries already made keep the rate they were made with.
pub async fn set_rate(State(state): State<AppState>, user: CurrentUser, Path(uid): Path<Uuid>, Json(req): Json<SetRate>) -> ApiResult<Json<Value>> {
    if !(user.is_admin() || user.finance_edit()) {
        return Err(ApiError::Forbidden);
    }
    if let Some(r) = req.hour_rate {
        if !r.is_finite() || !(0.0..1e7).contains(&r) {
            return bad_request("Ставка: число от 0");
        }
    }
    let rate = req.hour_rate.map(money::round2);
    let n = sqlx::query("UPDATE users SET hour_rate = $3 WHERE id = $1 AND tenant_id = $2")
        .bind(uid)
        .bind(user.tenant_id)
        .bind(rate)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    let mut applied = 0;
    if req.apply_to_unrated && rate.is_some() {
        applied = sqlx::query("UPDATE time_entries SET hour_rate = $2 WHERE user_id = $1 AND hour_rate IS NULL")
            .bind(uid)
            .bind(rate)
            .execute(&state.db)
            .await?
            .rows_affected();
    }
    state
        .audit(user.tenant_id, Some(user.user_id), "user.rate_changed", Some("user"), Some(uid), json!({ "hour_rate": rate, "applied_entries": applied }))
        .await;
    Ok(Json(json!({ "ok": true, "applied": applied })))
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReportQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Comma-separated ids.
    pub users: Option<String>,
    pub projects: Option<String>,
    /// Comma-separated task statuses.
    pub statuses: Option<String>,
}

fn ids(s: &Option<String>) -> ApiResult<Vec<Uuid>> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|x| Uuid::parse_str(x).map_err(|_| ApiError::BadRequest("invalid id".into())))
        .collect()
}

#[derive(Serialize, sqlx::FromRow)]
struct ReportRow {
    id: Uuid,
    work_date: NaiveDate,
    minutes: i32,
    note: String,
    source: String,
    user_id: Uuid,
    user_name: String,
    task_id: Uuid,
    task_number: i64,
    task_title: String,
    task_status: String,
    parent_id: Option<Uuid>,
    parent_title: Option<String>,
    project_id: Uuid,
    project_name: String,
    pay_mode: String,
    hour_rate: Option<f64>,
    cost: Option<f64>,
}

#[derive(Serialize, sqlx::FromRow)]
struct FixedRow {
    task_id: Uuid,
    task_number: i64,
    task_title: String,
    project_id: Uuid,
    project_name: String,
    assignee_name: Option<String>,
    pay_amount: f64,
    done_on: NaiveDate,
}

struct Report {
    rows: Vec<ReportRow>,
    fixed: Vec<FixedRow>,
    money: bool,
    truncated: bool,
}

async fn build_report(state: &AppState, user: &CurrentUser, q: &ReportQuery) -> ApiResult<Report> {
    if user.is_guest() {
        return Err(ApiError::Forbidden);
    }
    if q.to < q.from || (q.to - q.from).num_days() > 3700 {
        return bad_request("Период: дата окончания не раньше начала, не больше 10 лет");
    }
    let users = ids(&q.users)?;
    let projects = ids(&q.projects)?;
    let statuses: Vec<String> = q.statuses.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
    if statuses.iter().any(|s| !matches!(s.as_str(), "open" | "in_progress" | "review" | "done")) {
        return bad_request("unknown task status");
    }
    let money = sees_money(user);
    let all = money;
    let mut rows = sqlx::query_as::<_, ReportRow>(
        "SELECT te.id, te.work_date, te.minutes, te.note, te.source, te.user_id, u.display_name AS user_name,
                t.id AS task_id, t.number AS task_number, t.title AS task_title, t.status AS task_status,
                t.parent_id, pt.title AS parent_title, p.id AS project_id, p.name AS project_name, t.pay_mode,
                te.hour_rate::float8 AS hour_rate,
                CASE WHEN t.pay_mode = 'hourly' AND te.hour_rate IS NOT NULL THEN (te.minutes * te.hour_rate / 60.0)::float8 END AS cost
           FROM time_entries te
           JOIN tasks t ON t.id = te.task_id
           LEFT JOIN tasks pt ON pt.id = t.parent_id
           JOIN projects p ON p.id = t.project_id
           JOIN users u ON u.id = te.user_id
          WHERE te.tenant_id = $1 AND te.work_date BETWEEN $2 AND $3
            AND ($4 OR te.user_id = $5
                 OR EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.user_id = $5 AND pm.role = 'manager'))
            AND (cardinality($6::uuid[]) = 0 OR te.user_id = ANY($6))
            AND (cardinality($7::uuid[]) = 0 OR p.id = ANY($7))
            AND (cardinality($8::text[]) = 0 OR t.status = ANY($8))
          ORDER BY lower(p.name), t.number, te.work_date, lower(u.display_name)
          LIMIT $9",
    )
    .bind(user.tenant_id)
    .bind(q.from)
    .bind(q.to)
    .bind(all)
    .bind(user.user_id)
    .bind(&users)
    .bind(&projects)
    .bind(&statuses)
    .bind(REPORT_LIMIT + 1)
    .fetch_all(&state.db)
    .await?;
    let truncated = rows.len() as i64 > REPORT_LIMIT;
    rows.truncate(REPORT_LIMIT as usize);
    // Fixed pay of tasks done within the period (for those who see money).
    let fixed = if money {
        sqlx::query_as::<_, FixedRow>(
            "SELECT t.id AS task_id, t.number AS task_number, t.title AS task_title, p.id AS project_id, p.name AS project_name,
                    a.display_name AS assignee_name, t.pay_amount::float8 AS pay_amount,
                    (t.done_at AT TIME ZONE tn.timezone)::date AS done_on
               FROM tasks t JOIN projects p ON p.id = t.project_id JOIN tenants tn ON tn.id = t.tenant_id
               LEFT JOIN users a ON a.id = t.assignee_id
              WHERE t.tenant_id = $1 AND t.pay_mode = 'fixed' AND t.pay_amount IS NOT NULL AND t.done_at IS NOT NULL
                AND (t.done_at AT TIME ZONE tn.timezone)::date BETWEEN $2 AND $3
                AND (cardinality($4::uuid[]) = 0 OR t.assignee_id = ANY($4))
                AND (cardinality($5::uuid[]) = 0 OR p.id = ANY($5))
              ORDER BY lower(p.name), t.number",
        )
        .bind(user.tenant_id)
        .bind(q.from)
        .bind(q.to)
        .bind(&users)
        .bind(&projects)
        .fetch_all(&state.db)
        .await?
    } else {
        Vec::new()
    };
    if !money {
        for r in rows.iter_mut() {
            r.hour_rate = None;
            r.cost = None;
        }
    }
    Ok(Report { rows, fixed, money, truncated })
}

/// `GET /api/time/report?from&to&users&projects&statuses` — time entries for the
/// "Отчёты" section; the client groups them into a tree.
pub async fn report(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ReportQuery>) -> ApiResult<Json<Value>> {
    let r = build_report(&state, &user, &q).await?;
    let minutes: i64 = r.rows.iter().map(|x| x.minutes as i64).sum();
    let hourly: f64 = r.rows.iter().filter_map(|x| x.cost).sum();
    let fixed: f64 = r.fixed.iter().map(|x| x.pay_amount).sum();
    // Filter choices: people and projects the user may report on.
    let people: Vec<(Uuid, String)> = if r.money {
        sqlx::query_as("SELECT id, display_name FROM users WHERE tenant_id = $1 AND role <> 'guest' ORDER BY lower(display_name)")
            .bind(user.tenant_id)
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as(
            "SELECT DISTINCT u.id, u.display_name FROM users u
              WHERE u.tenant_id = $1 AND (u.id = $2 OR EXISTS (
                    SELECT 1 FROM project_members me JOIN project_members pm ON pm.project_id = me.project_id
                     WHERE me.user_id = $2 AND me.role = 'manager' AND pm.user_id = u.id))
              ORDER BY u.display_name",
        )
        .bind(user.tenant_id)
        .bind(user.user_id)
        .fetch_all(&state.db)
        .await?
    };
    let projects: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT p.id, p.name FROM projects p WHERE p.tenant_id = $1
            AND ($2 OR EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.user_id = $3))
          ORDER BY lower(p.name)",
    )
    .bind(user.tenant_id)
    .bind(r.money)
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "entries": r.rows,
        "fixed": r.fixed,
        "truncated": r.truncated,
        "sees_money": r.money,
        "totals": {
            "minutes": minutes,
            "hourly_cost": if r.money { Some(money::round2(hourly)) } else { None },
            "fixed_cost": if r.money { Some(money::round2(fixed)) } else { None },
        },
        "people": people.into_iter().map(|(id, n)| json!({ "id": id, "display_name": n })).collect::<Vec<_>>(),
        "projects": projects.into_iter().map(|(id, n)| json!({ "id": id, "name": n })).collect::<Vec<_>>(),
    })))
}

/// `GET /api/time/report.xlsx` — the same report as an Excel workbook:
/// «Записи» (every entry), «Итоги» (hours per project and employee) and, for
/// those who see money, «Фикс. оплата».
pub async fn report_xlsx(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ReportQuery>) -> ApiResult<Response> {
    let r = build_report(&state, &user, &q).await?;
    let bytes = tokio::task::spawn_blocking(move || write_xlsx(&r, q.from, q.to))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
    let name = format!("Трудозатраты {} — {}.xlsx", q.from.format("%d.%m.%Y"), q.to.format("%d.%m.%Y"));
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")),
            (header::CONTENT_DISPOSITION, crate::files::content_disposition(&name, false)),
        ],
        bytes,
    )
        .into_response())
}

fn write_xlsx(r: &Report, from: NaiveDate, to: NaiveDate) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    use rust_xlsxwriter::{Format, FormatAlign, Workbook};
    let mut wb = Workbook::new();
    let head = Format::new().set_bold().set_background_color(0xE8EDF3).set_text_wrap().set_align(FormatAlign::Top);
    let date_f = Format::new().set_num_format("dd.mm.yyyy");
    let hours_f = Format::new().set_num_format("0.00");
    let money_f = Format::new().set_num_format("# ##0.00 ₽");
    let bold = Format::new().set_bold();
    let bold_hours = Format::new().set_bold().set_num_format("0.00");
    let bold_money = Format::new().set_bold().set_num_format("# ##0.00 ₽");
    let xdate = |d: NaiveDate| rust_xlsxwriter::ExcelDateTime::from_ymd(d.year_ce().1 as u16, d.month() as u8, d.day() as u8);
    use chrono::Datelike;

    // --- Entries ---------------------------------------------------------------
    let ws = wb.add_worksheet().set_name("Записи")?;
    ws.write_string_with_format(0, 0, format!("Трудозатраты за {} — {}", from.format("%d.%m.%Y"), to.format("%d.%m.%Y")), &bold)?;
    let mut cols = vec!["Дата", "Сотрудник", "Проект", "№", "Задача", "Надзадача", "Часы", "Комментарий", "Источник"];
    if r.money {
        cols.extend(["Ставка, ₽/ч", "Стоимость, ₽"]);
    }
    for (i, c) in cols.iter().enumerate() {
        ws.write_string_with_format(2, i as u16, *c, &head)?;
    }
    let mut row = 3u32;
    for e in &r.rows {
        ws.write_datetime_with_format(row, 0, &xdate(e.work_date)?, &date_f)?;
        ws.write_string(row, 1, &e.user_name)?;
        ws.write_string(row, 2, &e.project_name)?;
        ws.write_number(row, 3, e.task_number as f64)?;
        ws.write_string(row, 4, &e.task_title)?;
        ws.write_string(row, 5, e.parent_title.as_deref().unwrap_or(""))?;
        ws.write_number_with_format(row, 6, e.minutes as f64 / 60.0, &hours_f)?;
        ws.write_string(row, 7, &e.note)?;
        ws.write_string(row, 8, if e.source == "timer" { "таймер" } else { "вручную" })?;
        if r.money {
            if let Some(h) = e.hour_rate {
                ws.write_number_with_format(row, 9, h, &money_f)?;
            }
            if e.pay_mode == "fixed" {
                ws.write_string(row, 10, "фикс. оплата")?;
            } else if let Some(c) = e.cost {
                ws.write_number_with_format(row, 10, c, &money_f)?;
            }
        }
        row += 1;
    }
    ws.write_string_with_format(row, 0, "Итого", &bold)?;
    ws.write_number_with_format(row, 6, r.rows.iter().map(|e| e.minutes as f64).sum::<f64>() / 60.0, &bold_hours)?;
    if r.money {
        ws.write_number_with_format(row, 10, r.rows.iter().filter_map(|e| e.cost).sum::<f64>(), &bold_money)?;
    }
    for (i, w) in [11.0, 22.0, 34.0, 7.0, 40.0, 26.0, 8.0, 40.0, 10.0, 12.0, 14.0].iter().enumerate().take(cols.len()) {
        ws.set_column_width(i as u16, *w)?;
    }
    ws.set_freeze_panes(3, 0)?;
    if row > 3 {
        ws.autofilter(2, 0, row - 1, (cols.len() - 1) as u16)?;
    }

    // --- Totals per project and employee ----------------------------------------
    let ws = wb.add_worksheet().set_name("Итоги")?;
    let mut sum: std::collections::BTreeMap<(String, String), (i64, f64)> = Default::default();
    for e in &r.rows {
        let s = sum.entry((e.project_name.clone(), e.user_name.clone())).or_default();
        s.0 += e.minutes as i64;
        s.1 += e.cost.unwrap_or(0.0);
    }
    let mut cols = vec!["Проект", "Сотрудник", "Часы"];
    if r.money {
        cols.push("Стоимость по часам, ₽");
    }
    for (i, c) in cols.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, *c, &head)?;
    }
    let mut row = 1u32;
    let mut last = String::new();
    for ((p, u), (m, c)) in &sum {
        if *p != last {
            ws.write_string_with_format(row, 0, p, &bold)?;
            last = p.clone();
        }
        ws.write_string(row, 1, u)?;
        ws.write_number_with_format(row, 2, *m as f64 / 60.0, &hours_f)?;
        if r.money {
            ws.write_number_with_format(row, 3, *c, &money_f)?;
        }
        row += 1;
    }
    ws.set_column_width(0, 40)?;
    ws.set_column_width(1, 24)?;
    ws.set_column_width(2, 10)?;
    ws.set_column_width(3, 20)?;

    // --- Fixed pay ----------------------------------------------------------------
    if r.money && !r.fixed.is_empty() {
        let ws = wb.add_worksheet().set_name("Фикс. оплата")?;
        for (i, c) in ["Выполнена", "Проект", "№", "Задача", "Исполнитель", "Сумма, ₽"].iter().enumerate() {
            ws.write_string_with_format(0, i as u16, *c, &head)?;
        }
        let mut row = 1u32;
        for f in &r.fixed {
            ws.write_datetime_with_format(row, 0, &xdate(f.done_on)?, &date_f)?;
            ws.write_string(row, 1, &f.project_name)?;
            ws.write_number(row, 2, f.task_number as f64)?;
            ws.write_string(row, 3, &f.task_title)?;
            ws.write_string(row, 4, f.assignee_name.as_deref().unwrap_or(""))?;
            ws.write_number_with_format(row, 5, f.pay_amount, &money_f)?;
            row += 1;
        }
        ws.write_string_with_format(row, 0, "Итого", &bold)?;
        ws.write_number_with_format(row, 5, r.fixed.iter().map(|f| f.pay_amount).sum::<f64>(), &bold_money)?;
        for (i, w) in [11.0, 34.0, 7.0, 40.0, 24.0, 14.0].iter().enumerate() {
            ws.set_column_width(i as u16, *w)?;
        }
    }
    wb.save_to_buffer()
}
