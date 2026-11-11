//! "All tasks" section and the task card: tasks across projects, task labels
//! and checklists. Project-scoped task editing stays in `projects.rs`.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    notify::{self, Note},
    projects::{load_task, project_name, project_role, ProjectRole, TaskRow, TASK_SELECT},
    state::AppState,
};

/// Upper bound of tasks returned by one board request.
const BOARD_LIMIT: i64 = 3000;
const MAX_LABELS: usize = 200;
const MAX_CHECKLIST: i64 = 200;

fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// Parse a comma-separated list of UUIDs; the token `none` sets the flag
/// (used for "no assignee").
fn parse_ids(raw: Option<&str>) -> ApiResult<(Vec<Uuid>, bool)> {
    let mut ids = Vec::new();
    let mut none = false;
    for part in raw.unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if part == "none" {
            none = true;
        } else {
            ids.push(Uuid::parse_str(part).map_err(|_| ApiError::BadRequest(format!("invalid id: {part}")))?);
        }
    }
    Ok((ids, none))
}

fn parse_priorities(raw: Option<&str>) -> ApiResult<Vec<i16>> {
    raw.unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| match s.parse::<i16>() {
            Ok(p @ 1..=4) => Ok(p),
            _ => Err(ApiError::BadRequest("priority must be 1..4".into())),
        })
        .collect()
}

fn parse_statuses(raw: Option<&str>) -> ApiResult<Vec<String>> {
    raw.unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if matches!(s, "open" | "in_progress" | "review" | "done") {
                Ok(s.to_string())
            } else {
                Err(ApiError::BadRequest(format!("invalid status: {s}")))
            }
        })
        .collect()
}

/// Task number typed as `#234` or `234`.
fn parse_number(q: &str) -> Option<i64> {
    q.trim().trim_start_matches('#').parse().ok().filter(|n| *n > 0)
}

/// May the user edit the task (fields, labels, checklist)?
async fn can_edit(state: &AppState, user: &CurrentUser, t: &TaskRow) -> ApiResult<bool> {
    let role = project_role(state, user, t.project_id).await?;
    Ok(role >= ProjectRole::Member || t.assignee_id == Some(user.user_id))
}

async fn require_edit(state: &AppState, user: &CurrentUser, t: &TaskRow) -> ApiResult<()> {
    if can_edit(state, user, t).await? { Ok(()) } else { Err(ApiError::Forbidden) }
}

// ---------------------------------------------------------------------------
// Board: tasks of every visible project
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BoardQuery {
    /// Comma-separated project ids; empty = all visible projects.
    pub project: Option<String>,
    /// Comma-separated user ids; `none` = without assignee.
    pub assignee: Option<String>,
    /// Comma-separated label ids; a task matches if it has any of them.
    pub label: Option<String>,
    /// Comma-separated priorities 1..4.
    pub priority: Option<String>,
    /// Comma-separated statuses.
    pub status: Option<String>,
    /// Only tasks whose dates overlap [from, to] (tasks without dates are excluded when set).
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Title substring or task number.
    pub q: Option<String>,
    /// Include paused and closed projects.
    #[serde(default)]
    pub closed: bool,
}

#[derive(Serialize, sqlx::FromRow)]
struct BoardProject {
    id: Uuid,
    name: String,
    status: String,
    code: Option<String>,
    role: String,
    badge_color: Option<String>,
    badge_icon: Option<String>,
}

pub async fn board(State(state): State<AppState>, user: CurrentUser, Query(q): Query<BoardQuery>) -> ApiResult<Json<Value>> {
    let (projects, _) = parse_ids(q.project.as_deref())?;
    let (assignees, no_assignee) = parse_ids(q.assignee.as_deref())?;
    let (labels, _) = parse_ids(q.label.as_deref())?;
    let priorities = parse_priorities(q.priority.as_deref())?;
    let statuses = parse_statuses(q.status.as_deref())?;
    if let (Some(f), Some(t)) = (q.from, q.to) {
        if f > t {
            return bad_request("from must not be after to");
        }
    }
    let text = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let number = text.as_deref().and_then(parse_number);

    // Projects the user can see; tenant admins see every project.
    let visible = sqlx::query_as::<_, BoardProject>(
        "SELECT p.id, p.name, p.status,
                (SELECT v.value #>> '{}' FROM project_field_values v JOIN project_field_defs d ON d.id = v.field_id
                  WHERE v.project_id = p.id AND d.key = 'code') AS code,
                CASE WHEN $3 THEN 'manager' ELSE pm.role END AS role, p.badge_color, p.badge_icon
           FROM projects p LEFT JOIN project_members pm ON pm.project_id = p.id AND pm.user_id = $2
          WHERE p.tenant_id = $1 AND ($3 OR pm.user_id IS NOT NULL) AND ($4 OR p.status = 'active')
          ORDER BY lower(p.name)",
    )
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.is_admin())
    .bind(q.closed)
    .fetch_all(&state.db)
    .await?;
    let mut scope: Vec<Uuid> = visible.iter().map(|p| p.id).collect();
    if !projects.is_empty() {
        scope.retain(|id| projects.contains(id));
    }

    let tasks = sqlx::query_as::<_, TaskRow>(&format!(
        "{TASK_SELECT}
          WHERE t.tenant_id = $1 AND t.project_id = ANY($2)
            AND (cardinality($3::uuid[]) = 0 AND NOT $4 OR t.assignee_id = ANY($3) OR ($4 AND t.assignee_id IS NULL))
            AND (cardinality($5::uuid[]) = 0 OR EXISTS (SELECT 1 FROM task_label_links l WHERE l.task_id = t.id AND l.label_id = ANY($5)))
            AND (cardinality($6::smallint[]) = 0 OR t.priority = ANY($6))
            AND (cardinality($7::text[]) = 0 OR t.status = ANY($7))
            AND ($8::date IS NULL OR COALESCE(t.due_date, t.start_date) >= $8)
            AND ($9::date IS NULL OR COALESCE(t.start_date, t.due_date) <= $9)
            AND ($10::text IS NULL OR t.title ILIKE '%' || $10 || '%' OR t.number = $11)
          ORDER BY lower(p.name), t.sort_order, t.created_at
          LIMIT $12"
    ))
    .bind(user.tenant_id)
    .bind(&scope)
    .bind(&assignees)
    .bind(no_assignee)
    .bind(&labels)
    .bind(&priorities)
    .bind(&statuses)
    .bind(q.from)
    .bind(q.to)
    .bind(text.as_deref().map(|s| s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")))
    .bind(number)
    .bind(BOARD_LIMIT + 1)
    .fetch_all(&state.db)
    .await?;
    let truncated = tasks.len() as i64 > BOARD_LIMIT;
    let tasks: Vec<TaskRow> = tasks.into_iter().take(BOARD_LIMIT as usize).collect();

    // Dependencies among the returned tasks, for the Gantt view.
    let ids: Vec<Uuid> = tasks.iter().map(|t| t.id).collect();
    let deps = sqlx::query_as::<_, crate::projects::DependencyRow>(
        "SELECT task_id, depends_on_id, kind, lag_days FROM task_dependencies WHERE task_id = ANY($1) AND depends_on_id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    let today = crate::crm::tasks::portal_today(&state, user.tenant_id).await?;
    Ok(Json(json!({
        "tasks": tasks,
        "dependencies": deps,
        "projects": visible,
        "today": today,
        "truncated": truncated,
    })))
}

// ---------------------------------------------------------------------------
// Task card
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct ChecklistItem {
    id: Uuid,
    body: String,
    done: bool,
    done_by: Option<Uuid>,
    done_at: Option<DateTime<Utc>>,
    assignee_id: Option<Uuid>,
    assignee_name: Option<String>,
    due_date: Option<NaiveDate>,
    sort_order: f64,
}

const ITEM_SELECT: &str = "SELECT c.id, c.body, c.done, c.done_by, c.done_at, c.assignee_id, u.display_name AS assignee_name,
        c.due_date, c.sort_order
   FROM task_checklist_items c LEFT JOIN users u ON u.id = c.assignee_id";

async fn checklist(state: &AppState, task_id: Uuid) -> ApiResult<Vec<ChecklistItem>> {
    Ok(sqlx::query_as::<_, ChecklistItem>(&format!("{ITEM_SELECT} WHERE c.task_id = $1 ORDER BY c.sort_order, c.created_at"))
        .bind(task_id)
        .fetch_all(&state.db)
        .await?)
}

#[derive(Serialize, sqlx::FromRow)]
struct CardMember {
    user_id: Uuid,
    display_name: String,
    role: String,
}

/// Full task card: task, checklist, project members, the caller's rights.
pub async fn get_task(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    let role = project_role(&state, &user, t.project_id).await?;
    let members = sqlx::query_as::<_, CardMember>(
        "SELECT u.id AS user_id, u.display_name, pm.role FROM project_members pm JOIN users u ON u.id = pm.user_id
          WHERE pm.project_id = $1 AND u.is_active ORDER BY u.display_name",
    )
    .bind(t.project_id)
    .fetch_all(&state.db)
    .await?;
    let editable = role >= ProjectRole::Member || t.assignee_id == Some(user.user_id);
    let author: Option<(Option<Uuid>, Option<String>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT t.created_by, u.display_name, t.created_at FROM tasks t LEFT JOIN users u ON u.id = t.created_by WHERE t.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (created_by, created_by_name, created_at) = author.map(|(a, b, c)| (a, b, Some(c))).unwrap_or((None, None, None));
    Ok(Json(json!({
        "task": t,
        "checklist": checklist(&state, id).await?,
        "attachments": crate::attachments::list_for(&state, id).await?,
        "can_detach_any": role >= ProjectRole::Manager,
        "members": members,
        "can_edit": editable,
        "can_manage": role >= ProjectRole::Member,
        "created_by": created_by,
        "created_by_name": created_by_name,
        "created_at": created_at,
    })))
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct LabelRow {
    id: Uuid,
    name: String,
    color: String,
    sort_order: i32,
    task_count: i64,
}

pub async fn list_labels(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query_as::<_, LabelRow>(
        "SELECT l.id, l.name, l.color, l.sort_order,
                (SELECT count(*) FROM task_label_links k WHERE k.label_id = l.id) AS task_count
           FROM task_labels l WHERE l.tenant_id = $1 ORDER BY l.sort_order, lower(l.name)",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "labels": rows, "can_manage": user.is_admin() })))
}

#[derive(Deserialize)]
pub struct LabelInput {
    pub name: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
}

pub fn check_label_name(name: &str) -> ApiResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 40 {
        return bad_request("название метки — от 1 до 40 символов");
    }
    Ok(n.to_string())
}

pub fn check_color(c: &str) -> ApiResult<String> {
    let c = c.trim();
    if c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(c.to_lowercase())
    } else {
        bad_request("цвет задаётся в виде #rrggbb")
    }
}

/// Any staff member may add a label (they are picked on the fly in the card);
/// renaming and deleting is for portal admins.
pub async fn create_label(State(state): State<AppState>, user: CurrentUser, Json(req): Json<LabelInput>) -> ApiResult<Json<Value>> {
    if user.is_guest() {
        return Err(ApiError::Forbidden);
    }
    let name = check_label_name(req.name.as_deref().unwrap_or(""))?;
    let color = check_color(req.color.as_deref().unwrap_or("#64748b"))?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_labels WHERE tenant_id = $1").bind(user.tenant_id).fetch_one(&state.db).await?;
    if count as usize >= MAX_LABELS {
        return bad_request("слишком много меток");
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO task_labels (tenant_id, name, color, sort_order)
         VALUES ($1, $2, $3, COALESCE($4, (SELECT COALESCE(max(sort_order), 0) + 1 FROM task_labels WHERE tenant_id = $1)))
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&name)
    .bind(&color)
    .bind(req.sort_order)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("метка с таким названием уже есть".into()),
        other => other,
    })?;
    Ok(Json(json!({ "id": id, "name": name, "color": color })))
}

pub async fn update_label(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<LabelInput>,
) -> ApiResult<Json<Value>> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    let name = req.name.as_deref().map(check_label_name).transpose()?;
    let color = req.color.as_deref().map(check_color).transpose()?;
    let n = sqlx::query(
        "UPDATE task_labels SET name = COALESCE($3, name), color = COALESCE($4, color), sort_order = COALESCE($5, sort_order)
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(name)
    .bind(color)
    .bind(req.sort_order)
    .execute(&state.db)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("метка с таким названием уже есть".into()),
        other => other,
    })?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_label(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    let n = sqlx::query("DELETE FROM task_labels WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    state.audit(user.tenant_id, Some(user.user_id), "task_label.deleted", Some("task_label"), Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SetLabels {
    pub label_ids: Vec<Uuid>,
}

/// Replace the labels of a task.
pub async fn set_task_labels(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetLabels>,
) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    require_edit(&state, &user, &t).await?;
    let mut ids = req.label_ids;
    ids.sort();
    ids.dedup();
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM task_labels WHERE id = ANY($1) AND tenant_id = $2")
        .bind(&ids)
        .bind(user.tenant_id)
        .fetch_one(&state.db)
        .await?;
    if known as usize != ids.len() {
        return bad_request("unknown label");
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM task_label_links WHERE task_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO task_label_links (task_id, label_id) SELECT $1, unnest($2::uuid[])")
        .bind(id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE tasks SET updated_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({ "task": load_task(&state, &user, id).await? })))
}

// ---------------------------------------------------------------------------
// Checklist
// ---------------------------------------------------------------------------

async fn check_member(state: &AppState, project_id: Uuid, user_id: Option<Uuid>) -> ApiResult<()> {
    if let Some(u) = user_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM project_members WHERE project_id = $1 AND user_id = $2)")
            .bind(project_id)
            .bind(u)
            .fetch_one(&state.db)
            .await?;
        if !ok {
            return bad_request("assignee must be a project member");
        }
    }
    Ok(())
}

fn check_body(body: &str) -> ApiResult<String> {
    let b = body.trim();
    if b.is_empty() || b.chars().count() > 500 {
        return bad_request("пункт чек-листа — от 1 до 500 символов");
    }
    Ok(b.to_string())
}

async fn notify_item(state: &AppState, user: &CurrentUser, t: &TaskRow, body: &str, assignee: Uuid) {
    let project = project_name(state, t.project_id).await;
    notify::notify(
        state,
        user.tenant_id,
        Some(user.user_id),
        &[assignee],
        Note {
            kind: "checklist_assigned",
            title: format!("Вам поручен пункт чек-листа: {body}"),
            body: format!("Задача #{} «{}», проект «{project}». Поручил(а): {}", t.number, t.title, user.display_name),
            link: format!("/projects/{}?task={}", t.project_id, t.id),
            entity_type: Some("task"),
            entity_id: Some(t.id),
            ..Default::default()
        },
    )
    .await;
}

#[derive(Deserialize)]
pub struct NewItem {
    pub body: String,
    pub assignee_id: Option<Uuid>,
    pub due_date: Option<NaiveDate>,
}

pub async fn add_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(task_id): Path<Uuid>,
    Json(req): Json<NewItem>,
) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, task_id).await?;
    require_edit(&state, &user, &t).await?;
    let body = check_body(&req.body)?;
    check_member(&state, t.project_id, req.assignee_id).await?;
    if t.checklist_total >= MAX_CHECKLIST {
        return bad_request("слишком много пунктов в чек-листе");
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO task_checklist_items (tenant_id, task_id, body, assignee_id, due_date, created_by, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, COALESCE((SELECT max(sort_order) FROM task_checklist_items WHERE task_id = $2), 0) + 1)
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(task_id)
    .bind(&body)
    .bind(req.assignee_id)
    .bind(req.due_date)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    if let Some(a) = req.assignee_id {
        notify_item(&state, &user, &t, &body, a).await;
    }
    Ok(Json(json!({ "id": id, "checklist": checklist(&state, task_id).await? })))
}

#[derive(Deserialize)]
pub struct UpdateItem {
    pub body: Option<String>,
    pub done: Option<bool>,
    pub sort_order: Option<f64>,
    #[serde(default, deserialize_with = "double_option")]
    pub assignee_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_date: Option<Option<NaiveDate>>,
}

async fn load_item(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<(Uuid, String, Option<Uuid>)> {
    sqlx::query_as("SELECT task_id, body, assignee_id FROM task_checklist_items WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn update_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateItem>,
) -> ApiResult<Json<Value>> {
    let (task_id, old_body, old_assignee) = load_item(&state, &user, id).await?;
    let t = load_task(&state, &user, task_id).await?;
    let editor = can_edit(&state, &user, &t).await?;
    // The item's own assignee may tick it off even without edit rights on the task.
    let only_done = req.body.is_none() && req.sort_order.is_none() && req.assignee_id.is_none() && req.due_date.is_none();
    if !editor && !(only_done && old_assignee == Some(user.user_id)) {
        return Err(ApiError::Forbidden);
    }
    let body = req.body.as_deref().map(check_body).transpose()?;
    if let Some(a) = req.assignee_id {
        check_member(&state, t.project_id, a).await?;
    }
    sqlx::query(
        "UPDATE task_checklist_items SET
                body = COALESCE($2, body),
                done = COALESCE($3, done),
                done_by = CASE WHEN $3 IS NULL THEN done_by WHEN $3 THEN COALESCE(CASE WHEN done THEN done_by END, $4) ELSE NULL END,
                done_at = CASE WHEN $3 IS NULL THEN done_at WHEN $3 THEN COALESCE(CASE WHEN done THEN done_at END, now()) ELSE NULL END,
                sort_order = COALESCE($5, sort_order),
                assignee_id = CASE WHEN $6 THEN $7 ELSE assignee_id END,
                due_date = CASE WHEN $8 THEN $9 ELSE due_date END
          WHERE id = $1",
    )
    .bind(id)
    .bind(&body)
    .bind(req.done)
    .bind(user.user_id)
    .bind(req.sort_order)
    .bind(req.assignee_id.is_some())
    .bind(req.assignee_id.flatten())
    .bind(req.due_date.is_some())
    .bind(req.due_date.flatten())
    .execute(&state.db)
    .await?;
    if let Some(a) = req.assignee_id.flatten().filter(|a| Some(*a) != old_assignee) {
        notify_item(&state, &user, &t, body.as_deref().unwrap_or(&old_body), a).await;
    }
    Ok(Json(json!({ "checklist": checklist(&state, task_id).await? })))
}

pub async fn delete_item(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (task_id, _, _) = load_item(&state, &user, id).await?;
    let t = load_task(&state, &user, task_id).await?;
    require_edit(&state, &user, &t).await?;
    sqlx::query("DELETE FROM task_checklist_items WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "checklist": checklist(&state, task_id).await? })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_none_token() {
        let id = Uuid::new_v4();
        let (ids, none) = parse_ids(Some(&format!(" {id}, none ,"))).unwrap();
        assert_eq!(ids, vec![id]);
        assert!(none);
        assert_eq!(parse_ids(None).unwrap(), (vec![], false));
        assert!(parse_ids(Some("bogus")).is_err());
    }

    #[test]
    fn priorities_and_statuses() {
        assert_eq!(parse_priorities(Some("4,1")).unwrap(), vec![4, 1]);
        assert!(parse_priorities(Some("5")).is_err());
        assert_eq!(parse_statuses(Some("open,done")).unwrap(), vec!["open", "done"]);
        assert!(parse_statuses(Some("closed")).is_err());
    }

    #[test]
    fn task_numbers_and_colors() {
        assert_eq!(parse_number("#234"), Some(234));
        assert_eq!(parse_number(" 17 "), Some(17));
        assert_eq!(parse_number("отчёт"), None);
        assert_eq!(parse_number("#0"), None);
        assert_eq!(check_color("#AABBCC").unwrap(), "#aabbcc");
        assert!(check_color("red").is_err());
        assert!(check_label_name("  ").is_err());
        assert!(check_label_name(&"я".repeat(41)).is_err());
    }
}
