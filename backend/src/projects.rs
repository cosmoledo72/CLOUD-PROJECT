//! Projects, tasks, Gantt dependencies and time tracking.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    scheduling::{self, Dep, Node, Span},
    error::{bad_request, ApiError, ApiResult},
    files::validate_name,
    state::AppState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectRole {
    Viewer,
    Member,
    Manager,
}

impl ProjectRole {
    fn require(self, needed: ProjectRole) -> ApiResult<()> {
        if self >= needed { Ok(()) } else { Err(ApiError::Forbidden) }
    }
}

/// Role of the current user in a project. Tenant admins act as managers.
pub async fn project_role(state: &AppState, user: &CurrentUser, project_id: Uuid) -> ApiResult<ProjectRole> {
    let exists: Option<Option<String>> = sqlx::query_scalar(
        "SELECT pm.role FROM projects p LEFT JOIN project_members pm ON pm.project_id = p.id AND pm.user_id = $3
          WHERE p.id = $1 AND p.tenant_id = $2",
    )
    .bind(project_id)
    .bind(user.tenant_id)
    .bind(user.user_id)
    .fetch_optional(&state.db)
    .await?;
    let role = exists.ok_or(ApiError::NotFound)?;
    if user.is_admin() {
        return Ok(ProjectRole::Manager);
    }
    match role.as_deref() {
        Some("manager") => Ok(ProjectRole::Manager),
        Some("member") => Ok(ProjectRole::Member),
        Some("viewer") => Ok(ProjectRole::Viewer),
        _ => Err(ApiError::NotFound),
    }
}

/// Deserialize a field that distinguishes "absent" (None) from "null" (Some(None)).
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct ProjectRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: String,
    pub owner_id: Uuid,
    pub folder_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub task_total: i64,
    pub task_done: i64,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    /// Requisite values as `{ key: value }`.
    pub fields: Value,
    /// Move dependent tasks automatically when dates change.
    pub auto_schedule: bool,
    pub baseline_saved_at: Option<DateTime<Utc>>,
    /// Customer (CRM company) and the deal the project came from.
    pub company_id: Option<Uuid>,
    pub company_name: Option<String>,
    pub deal_id: Option<Uuid>,
}

const PROJECT_SELECT: &str = "SELECT p.id, p.name, p.description, p.status, p.owner_id, p.folder_id, p.created_at,
        (SELECT count(*) FROM tasks t WHERE t.project_id = p.id) AS task_total,
        (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.status = 'done') AS task_done,
        (SELECT min(t.start_date) FROM tasks t WHERE t.project_id = p.id) AS start_date,
        (SELECT max(t.due_date) FROM tasks t WHERE t.project_id = p.id) AS end_date,
        (SELECT COALESCE(jsonb_object_agg(d.key, v.value), '{}'::jsonb)
           FROM project_field_values v JOIN project_field_defs d ON d.id = v.field_id
          WHERE v.project_id = p.id) AS fields,
        p.auto_schedule, p.baseline_saved_at,
        p.company_id, (SELECT c.name FROM crm_companies c WHERE c.id = p.company_id) AS company_name, p.deal_id
   FROM projects p";

pub async fn list_projects(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query_as::<_, ProjectRow>(&format!(
        "{PROJECT_SELECT} WHERE p.tenant_id = $1
            AND ($3 OR EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.user_id = $2))
          ORDER BY p.status, lower(p.name)"
    ))
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.is_admin())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "projects": rows })))
}

#[derive(Deserialize)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Initial requisite values `{ key: value }`; required requisites must be present.
    #[serde(default)]
    pub fields: serde_json::Map<String, Value>,
}

pub async fn create_project(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CreateProject>) -> ApiResult<Json<Value>> {
    let (id, folder_id) = create_project_inner(&state, &user, &req.name, &req.description, &req.fields, None, None).await?;
    Ok(Json(json!({ "id": id, "folder_id": folder_id })))
}

/// Create a project with its document folder and requisites in one transaction.
/// `company_id` / `deal_id` link it to the CRM. Returns (project id, folder id).
pub async fn create_project_inner(
    state: &AppState,
    user: &CurrentUser,
    name: &str,
    description: &str,
    fields: &serde_json::Map<String, Value>,
    company_id: Option<Uuid>,
    deal_id: Option<Uuid>,
) -> ApiResult<(Uuid, Uuid)> {
    if user.is_guest() {
        return Err(ApiError::Forbidden);
    }
    let name = validate_name(name)?;
    let mut tx = state.db.begin().await?;
    // Every project gets its own document root, governed by project roles.
    let folder_id: Uuid = sqlx::query_scalar(
        "INSERT INTO folders (tenant_id, kind, name, owner_id) VALUES ($1, 'project', $2, $3) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&name)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (tenant_id, name, description, owner_id, folder_id, company_id, deal_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&name)
    .bind(description.trim())
    .bind(user.user_id)
    .bind(folder_id)
    .bind(company_id)
    .bind(deal_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, 'manager')")
        .bind(id)
        .bind(user.user_id)
        .execute(&mut *tx)
        .await?;
    crate::fields::save_values(state, &mut tx, user.tenant_id, user.user_id, id, fields).await?;
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "project.created", Some("project"), Some(id), json!({ "name": name })).await;
    Ok((id, folder_id))
}

#[derive(Serialize, sqlx::FromRow)]
struct MemberRow {
    user_id: Uuid,
    display_name: String,
    email: String,
    role: String,
}

pub async fn get_project(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let role = project_role(&state, &user, id).await?;
    let project = sqlx::query_as::<_, ProjectRow>(&format!("{PROJECT_SELECT} WHERE p.id = $1"))
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    let members = sqlx::query_as::<_, MemberRow>(
        "SELECT u.id AS user_id, u.display_name, u.email::text AS email, pm.role
           FROM project_members pm JOIN users u ON u.id = pm.user_id WHERE pm.project_id = $1 ORDER BY u.display_name",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let my_role = match role {
        ProjectRole::Manager => "manager",
        ProjectRole::Member => "member",
        ProjectRole::Viewer => "viewer",
    };
    Ok(Json(json!({ "project": project, "members": members, "my_role": my_role })))
}

#[derive(Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub auto_schedule: Option<bool>,
}

pub async fn update_project(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProject>,
) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?.require(ProjectRole::Manager)?;
    let name = req.name.as_deref().map(validate_name).transpose()?;
    sqlx::query(
        "UPDATE projects SET name = COALESCE($2, name), description = COALESCE($3, description), status = COALESCE($4, status),
                auto_schedule = COALESCE($5, auto_schedule)
          WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(req.description)
    .bind(req.status)
    .bind(req.auto_schedule)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_project(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?.require(ProjectRole::Manager)?;
    let mut tx = state.db.begin().await?;
    let folder: Option<Uuid> = sqlx::query_scalar("DELETE FROM projects WHERE id = $1 RETURNING folder_id")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(f) = folder {
        sqlx::query("UPDATE folders SET deleted_at = now(), deleted_by = $2 WHERE id = $1")
            .bind(f)
            .bind(user.user_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "project.deleted", Some("project"), Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct MemberRequest {
    pub user_id: Uuid,
    pub role: String,
}

pub async fn upsert_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<MemberRequest>,
) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?.require(ProjectRole::Manager)?;
    if !matches!(req.role.as_str(), "manager" | "member" | "viewer") {
        return bad_request("role must be manager, member or viewer");
    }
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1 AND tenant_id = $2")
        .bind(req.user_id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::BadRequest("unknown user".into()))?;
    let added: bool = sqlx::query_scalar(
        "INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (project_id, user_id) DO UPDATE SET role = EXCLUDED.role
         RETURNING (xmax = 0)",
    )
    .bind(id)
    .bind(req.user_id)
    .bind(&req.role)
    .fetch_one(&state.db)
    .await?;
    if added {
        let project = project_name(&state, id).await;
        crate::notify::notify(
            &state,
            user.tenant_id,
            Some(user.user_id),
            &[req.user_id],
            crate::notify::Note {
                kind: "project_added",
                title: format!("Вас добавили в проект «{project}»"),
                body: format!("Добавил(а): {}", user.display_name),
                link: format!("/projects/{id}"),
                entity_type: Some("project"),
                entity_id: Some(id),
                ..Default::default()
            },
        )
        .await;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, member_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?.require(ProjectRole::Manager)?;
    sqlx::query("DELETE FROM project_members WHERE project_id = $1 AND user_id = $2")
        .bind(id)
        .bind(member_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct TaskRow {
    pub id: Uuid,
    /// Portal-wide task number shown as `#234`.
    pub number: i64,
    pub project_id: Uuid,
    pub project_name: String,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: i16,
    pub is_milestone: bool,
    pub assignee_id: Option<Uuid>,
    pub assignee_name: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub progress: i16,
    pub sort_order: f64,
    pub spent_minutes: i64,
    pub updated_at: DateTime<Utc>,
    /// Ids of attached task labels.
    pub label_ids: Vec<Uuid>,
    pub checklist_total: i64,
    pub checklist_done: i64,
    pub comment_count: i64,
}

pub const TASK_SELECT: &str = "SELECT t.id, t.number, t.project_id, p.name AS project_name, t.parent_id, t.title, t.description, t.status, t.priority, t.is_milestone,
        t.assignee_id, u.display_name AS assignee_name, t.start_date, t.due_date, t.progress, t.sort_order,
        COALESCE((SELECT sum(minutes) FROM time_entries te WHERE te.task_id = t.id), 0)::bigint AS spent_minutes,
        t.updated_at,
        ARRAY(SELECT l.label_id FROM task_label_links l JOIN task_labels tl ON tl.id = l.label_id
               WHERE l.task_id = t.id ORDER BY tl.sort_order, lower(tl.name)) AS label_ids,
        (SELECT count(*) FROM task_checklist_items c WHERE c.task_id = t.id) AS checklist_total,
        (SELECT count(*) FROM task_checklist_items c WHERE c.task_id = t.id AND c.done) AS checklist_done,
        (SELECT count(*) FROM task_comments c WHERE c.task_id = t.id) AS comment_count
   FROM tasks t JOIN projects p ON p.id = t.project_id LEFT JOIN users u ON u.id = t.assignee_id";

#[derive(Serialize, sqlx::FromRow)]
pub struct DependencyRow {
    pub task_id: Uuid,
    pub depends_on_id: Uuid,
    pub kind: String,
    pub lag_days: i32,
}

/// Tasks and dependencies of a project — the full Gantt payload.
pub async fn list_tasks(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?;
    let tasks = sqlx::query_as::<_, TaskRow>(&format!("{TASK_SELECT} WHERE t.project_id = $1 ORDER BY t.sort_order, t.created_at"))
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let deps = sqlx::query_as::<_, DependencyRow>(
        "SELECT d.task_id, d.depends_on_id, d.kind, d.lag_days FROM task_dependencies d
           JOIN tasks t ON t.id = d.task_id WHERE t.project_id = $1",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let (nodes, graph) = load_graph(&state, id).await?;
    let cp = scheduling::critical_path(&nodes, &graph);
    let mut critical: Vec<Uuid> = cp.critical.into_iter().collect();
    critical.sort();
    let baseline = sqlx::query_as::<_, BaselineRow>("SELECT task_id, start_date, due_date FROM task_baselines WHERE project_id = $1")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({
        "tasks": tasks,
        "dependencies": deps,
        "schedule": { "critical": critical, "slack": cp.slack, "project_end": cp.project_end },
        "baseline": baseline,
    })))
}

pub async fn load_task(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<TaskRow> {
    sqlx::query_as::<_, TaskRow>(&format!("{TASK_SELECT} WHERE t.id = $1 AND t.tenant_id = $2"))
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

async fn check_assignee(state: &AppState, project_id: Uuid, assignee: Option<Uuid>) -> ApiResult<()> {
    if let Some(a) = assignee {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM project_members WHERE project_id = $1 AND user_id = $2)")
            .bind(project_id)
            .bind(a)
            .fetch_one(&state.db)
            .await?;
        if !ok {
            return bad_request("assignee must be a project member");
        }
    }
    Ok(())
}

fn check_dates(start: Option<NaiveDate>, due: Option<NaiveDate>) -> ApiResult<()> {
    if let (Some(s), Some(d)) = (start, due) {
        if s > d {
            return bad_request("start date must not be after due date");
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub parent_id: Option<Uuid>,
    pub assignee_id: Option<Uuid>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    #[serde(default)]
    pub is_milestone: bool,
    pub priority: Option<i16>,
}

pub async fn create_task(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<Uuid>,
    Json(req): Json<CreateTask>,
) -> ApiResult<Json<Value>> {
    project_role(&state, &user, project_id).await?.require(ProjectRole::Member)?;
    let title = req.title.trim();
    if title.is_empty() {
        return bad_request("title is required");
    }
    check_dates(req.start_date, req.due_date)?;
    check_assignee(&state, project_id, req.assignee_id).await?;
    if let Some(p) = req.parent_id {
        let same: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tasks WHERE id = $1 AND project_id = $2)")
            .bind(p)
            .bind(project_id)
            .fetch_one(&state.db)
            .await?;
        if !same {
            return bad_request("parent task must belong to the same project");
        }
    }
    // A milestone is a zero-length point in time.
    let (start, due) = if req.is_milestone { (req.due_date.or(req.start_date), req.due_date.or(req.start_date)) } else { (req.start_date, req.due_date) };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tasks (tenant_id, project_id, parent_id, title, description, assignee_id, start_date, due_date,
                            is_milestone, priority, created_by, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                 COALESCE((SELECT max(sort_order) FROM tasks WHERE project_id = $2), 0) + 1)
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(project_id)
    .bind(req.parent_id)
    .bind(title)
    .bind(req.description.trim())
    .bind(req.assignee_id)
    .bind(start)
    .bind(due)
    .bind(req.is_milestone)
    .bind(req.priority.unwrap_or(2).clamp(1, 4))
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    if let Some(a) = req.assignee_id {
        notify_assigned(&state, &user, project_id, id, title, a).await;
    }
    Ok(Json(json!({ "task": load_task(&state, &user, id).await? })))
}

pub async fn project_name(state: &AppState, project_id: Uuid) -> String {
    sqlx::query_scalar("SELECT name FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or_default()
}

async fn notify_assigned(state: &AppState, user: &CurrentUser, project_id: Uuid, task_id: Uuid, title: &str, assignee: Uuid) {
    let project = project_name(state, project_id).await;
    crate::notify::notify(
        state,
        user.tenant_id,
        Some(user.user_id),
        &[assignee],
        crate::notify::Note {
            kind: "task_assigned",
            title: format!("Вам назначена задача «{title}»"),
            body: format!("Проект «{project}». Назначил(а): {}", user.display_name),
            link: format!("/projects/{project_id}?task={task_id}"),
            entity_type: Some("task"),
            entity_id: Some(task_id),
            ..Default::default()
        },
    )
    .await;
}

#[derive(Deserialize)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<i16>,
    pub progress: Option<i16>,
    pub sort_order: Option<f64>,
    pub is_milestone: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub assignee_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub start_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_date: Option<Option<NaiveDate>>,
}

pub async fn update_task(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTask>,
) -> ApiResult<Json<Value>> {
    let mut t = load_task(&state, &user, id).await?;
    let before = (t.start_date, t.due_date, t.is_milestone);
    let (assignee_before, status_before) = (t.assignee_id, t.status.clone());
    let role = project_role(&state, &user, t.project_id).await?;
    // Assignees may update their own tasks; otherwise members and above.
    if t.assignee_id != Some(user.user_id) {
        role.require(ProjectRole::Member)?;
    }

    if let Some(v) = req.title {
        let v = v.trim().to_string();
        if v.is_empty() {
            return bad_request("title is required");
        }
        t.title = v;
    }
    if let Some(v) = req.description { t.description = v; }
    if let Some(v) = req.status {
        if !matches!(v.as_str(), "open" | "in_progress" | "review" | "done") {
            return bad_request("invalid status");
        }
        if v == "done" { t.progress = 100; }
        t.status = v;
    }
    if let Some(v) = req.priority { t.priority = v.clamp(1, 4); }
    if let Some(v) = req.progress { t.progress = v.clamp(0, 100); }
    if let Some(v) = req.sort_order { t.sort_order = v; }
    if let Some(v) = req.is_milestone { t.is_milestone = v; }
    if let Some(v) = req.assignee_id {
        check_assignee(&state, t.project_id, v).await?;
        t.assignee_id = v;
    }
    if let Some(v) = req.start_date { t.start_date = v; }
    if let Some(v) = req.due_date { t.due_date = v; }
    if t.is_milestone {
        t.start_date = t.due_date.or(t.start_date);
        t.due_date = t.start_date;
    }
    check_dates(t.start_date, t.due_date)?;
    let dates_changed = (t.start_date, t.due_date, t.is_milestone) != before;

    sqlx::query(
        "UPDATE tasks SET title = $2, description = $3, status = $4, priority = $5, progress = $6, sort_order = $7,
                          is_milestone = $8, assignee_id = $9, start_date = $10, due_date = $11, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&t.title)
    .bind(&t.description)
    .bind(&t.status)
    .bind(t.priority)
    .bind(t.progress)
    .bind(t.sort_order)
    .bind(t.is_milestone)
    .bind(t.assignee_id)
    .bind(t.start_date)
    .bind(t.due_date)
    .execute(&state.db)
    .await?;
    if let Some(a) = t.assignee_id.filter(|a| Some(*a) != assignee_before) {
        notify_assigned(&state, &user, t.project_id, id, &t.title, a).await;
    }
    if t.status == "done" && status_before != "done" {
        let author: Option<Uuid> = sqlx::query_scalar("SELECT created_by FROM tasks WHERE id = $1").bind(id).fetch_one(&state.db).await?;
        if let Some(author) = author {
            let project = project_name(&state, t.project_id).await;
            crate::notify::notify(
                &state,
                user.tenant_id,
                Some(user.user_id),
                &[author],
                crate::notify::Note {
                    kind: "task_done",
                    title: format!("Задача выполнена: {}", t.title),
                    body: format!("Проект «{project}». Отметил(а): {}", user.display_name),
                    link: format!("/projects/{}?task={id}", t.project_id),
                    entity_type: Some("task"),
                    entity_id: Some(id),
                    ..Default::default()
                },
            )
            .await;
        }
    }
    let shifted = if dates_changed { auto_shift(&state, &user, t.project_id, &[id], false).await? } else { vec![] };
    Ok(Json(json!({ "task": load_task(&state, &user, id).await?, "shifted": shifted })))
}

pub async fn delete_task(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    project_role(&state, &user, t.project_id).await?.require(ProjectRole::Member)?;
    sqlx::query("DELETE FROM tasks WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Dependencies
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateDependency {
    pub depends_on_id: Uuid,
    #[serde(default = "default_dep_kind")]
    pub kind: String,
    #[serde(default)]
    pub lag_days: i32,
}

fn default_dep_kind() -> String {
    "FS".into()
}

pub async fn add_dependency(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateDependency>,
) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    project_role(&state, &user, t.project_id).await?.require(ProjectRole::Member)?;
    if !matches!(req.kind.as_str(), "FS" | "SS" | "FF" | "SF") {
        return bad_request("kind must be FS, SS, FF or SF");
    }
    let other = load_task(&state, &user, req.depends_on_id).await?;
    if other.project_id != t.project_id {
        return bad_request("dependencies must stay within one project");
    }
    // Reject cycles: does the predecessor (transitively) depend on this task?
    let cycle: bool = sqlx::query_scalar(
        "WITH RECURSIVE reach AS (
             SELECT depends_on_id AS id FROM task_dependencies WHERE task_id = $1
             UNION
             SELECT d.depends_on_id FROM task_dependencies d JOIN reach r ON d.task_id = r.id
         ) SELECT $1 = $2 OR EXISTS (SELECT 1 FROM reach WHERE id = $2)",
    )
    .bind(req.depends_on_id)
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    if cycle {
        return bad_request("this dependency would create a cycle");
    }
    sqlx::query(
        "INSERT INTO task_dependencies (task_id, depends_on_id, kind, lag_days) VALUES ($1, $2, $3, $4)
         ON CONFLICT (task_id, depends_on_id) DO UPDATE SET kind = EXCLUDED.kind, lag_days = EXCLUDED.lag_days",
    )
    .bind(id)
    .bind(req.depends_on_id)
    .bind(&req.kind)
    .bind(req.lag_days)
    .execute(&state.db)
    .await?;
    let shifted = auto_shift(&state, &user, t.project_id, &[req.depends_on_id], false).await?;
    Ok(Json(json!({ "ok": true, "shifted": shifted })))
}

pub async fn remove_dependency(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, dep_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    project_role(&state, &user, t.project_id).await?.require(ProjectRole::Member)?;
    sqlx::query("DELETE FROM task_dependencies WHERE task_id = $1 AND depends_on_id = $2")
        .bind(id)
        .bind(dep_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Time tracking
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct TimeRow {
    id: Uuid,
    user_id: Uuid,
    user_name: String,
    work_date: NaiveDate,
    minutes: i32,
    note: String,
}

pub async fn list_time(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    project_role(&state, &user, t.project_id).await?;
    let rows = sqlx::query_as::<_, TimeRow>(
        "SELECT te.id, te.user_id, u.display_name AS user_name, te.work_date, te.minutes, te.note
           FROM time_entries te JOIN users u ON u.id = te.user_id WHERE te.task_id = $1 ORDER BY te.work_date DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "entries": rows })))
}

#[derive(Deserialize)]
pub struct CreateTime {
    pub work_date: NaiveDate,
    pub minutes: i32,
    #[serde(default)]
    pub note: String,
}

pub async fn add_time(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateTime>,
) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    project_role(&state, &user, t.project_id).await?.require(ProjectRole::Member)?;
    if req.minutes <= 0 || req.minutes > 24 * 60 {
        return bad_request("minutes must be within 1..1440");
    }
    let tid: Uuid = sqlx::query_scalar(
        "INSERT INTO time_entries (tenant_id, task_id, user_id, work_date, minutes, note) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(id)
    .bind(user.user_id)
    .bind(req.work_date)
    .bind(req.minutes)
    .bind(req.note.trim())
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({ "id": tid })))
}

// ---------------------------------------------------------------------------
// Scheduling: auto-shift, reschedule and baseline
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct BaselineRow {
    task_id: Uuid,
    start_date: Option<NaiveDate>,
    due_date: Option<NaiveDate>,
}

/// Tasks and dependencies of a project as a scheduling graph.
async fn load_graph(state: &AppState, project_id: Uuid) -> ApiResult<(std::collections::HashMap<Uuid, Node>, Vec<Dep>)> {
    let rows: Vec<(Uuid, Option<NaiveDate>, Option<NaiveDate>, String, bool)> =
        sqlx::query_as("SELECT id, start_date, due_date, status, is_milestone FROM tasks WHERE project_id = $1")
            .bind(project_id)
            .fetch_all(&state.db)
            .await?;
    let nodes = rows
        .into_iter()
        .map(|(id, s, d, status, milestone)| (id, Node { span: Span::from_dates(s, d), done: status == "done", milestone }))
        .collect();
    let deps: Vec<(Uuid, Uuid, String, i32)> = sqlx::query_as(
        "SELECT d.task_id, d.depends_on_id, d.kind, d.lag_days FROM task_dependencies d JOIN tasks t ON t.id = d.task_id
          WHERE t.project_id = $1",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;
    let deps = deps.into_iter().map(|(task, on, kind, lag)| Dep { task, depends_on: on, kind, lag: lag as i64 }).collect();
    Ok((nodes, deps))
}

/// Move successors of `changed` (or of every task when `all`) so that all
/// dependencies hold. Respects the project's auto-schedule switch unless
/// `all` is set (explicit "reschedule" request). Returns the moved tasks.
async fn auto_shift(state: &AppState, user: &CurrentUser, project_id: Uuid, changed: &[Uuid], all: bool) -> ApiResult<Vec<TaskRow>> {
    if !all {
        let enabled: bool = sqlx::query_scalar("SELECT auto_schedule FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(&state.db)
            .await?;
        if !enabled {
            return Ok(vec![]);
        }
    }
    let (nodes, deps) = load_graph(state, project_id).await?;
    let moved = if all { scheduling::propagate_all(&nodes, &deps) } else { scheduling::propagate(&nodes, &deps, changed) };
    if moved.is_empty() {
        return Ok(vec![]);
    }
    let mut tx = state.db.begin().await?;
    for (id, span) in &moved {
        let milestone = nodes.get(id).is_some_and(|n| n.milestone);
        // Keep "due only" tasks as they were entered: shift the due date, leave start empty.
        sqlx::query(
            "UPDATE tasks SET start_date = CASE WHEN start_date IS NULL AND NOT $4 THEN NULL ELSE $2 END,
                              due_date = CASE WHEN due_date IS NULL THEN NULL ELSE $3 END, updated_at = now()
              WHERE id = $1",
        )
        .bind(id)
        .bind(span.start)
        .bind(span.due)
        .bind(milestone)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let ids: Vec<Uuid> = moved.keys().copied().collect();
    let rows = sqlx::query_as::<_, TaskRow>(&format!("{TASK_SELECT} WHERE t.id = ANY($1) AND t.tenant_id = $2 ORDER BY t.sort_order"))
        .bind(&ids)
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "project.rescheduled", Some("project"), Some(project_id), json!({ "moved": rows.len() }))
        .await;
    Ok(rows)
}

/// Fix every violated dependency in the project (explicit action).
pub async fn reschedule(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?.require(ProjectRole::Member)?;
    let shifted = auto_shift(&state, &user, id, &[], true).await?;
    Ok(Json(json!({ "shifted": shifted })))
}

/// Save the current dates of all tasks as the project's baseline.
pub async fn save_baseline(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?.require(ProjectRole::Manager)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM task_baselines WHERE project_id = $1").bind(id).execute(&mut *tx).await?;
    let n = sqlx::query(
        "INSERT INTO task_baselines (task_id, project_id, start_date, due_date)
         SELECT id, project_id, start_date, due_date FROM tasks WHERE project_id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query("UPDATE projects SET baseline_saved_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "project.baseline_saved", Some("project"), Some(id), json!({ "tasks": n })).await;
    Ok(Json(json!({ "tasks": n })))
}

pub async fn clear_baseline(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?.require(ProjectRole::Manager)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM task_baselines WHERE project_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE projects SET baseline_saved_at = NULL WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
