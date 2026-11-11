//! Project labels (F6): a portal-wide catalogue of coloured labels
//! («Ожидаем оплату», «На экспертизе») assigned to projects. Separate from
//! task labels. Project managers create labels on the fly and assign them to
//! their projects; renaming and deleting is for portal administrators.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    projects::{project_role, ProjectRole},
    state::AppState,
    task_board::{check_color, check_label_name, LabelInput},
};

const MAX_LABELS: i64 = 100;
/// Labels on one project.
const MAX_PER_PROJECT: usize = 10;

#[derive(Serialize, sqlx::FromRow)]
struct LabelRow {
    id: Uuid,
    name: String,
    color: String,
    sort_order: i32,
    project_count: i64,
}

/// May the user add labels to the catalogue: administrators and managers of
/// at least one project.
async fn can_create(state: &AppState, user: &CurrentUser) -> ApiResult<bool> {
    if user.is_admin() {
        return Ok(true);
    }
    if user.is_guest() {
        return Ok(false);
    }
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM project_members pm JOIN projects p ON p.id = pm.project_id
                         WHERE pm.user_id = $1 AND pm.role = 'manager' AND p.tenant_id = $2)",
    )
    .bind(user.user_id)
    .bind(user.tenant_id)
    .fetch_one(&state.db)
    .await?)
}

fn name_conflict(e: sqlx::Error) -> ApiError {
    match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("метка с таким названием уже есть".into()),
        other => other,
    }
}

/// `GET /api/project-labels`
pub async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query_as::<_, LabelRow>(
        "SELECT l.id, l.name, l.color, l.sort_order,
                (SELECT count(*) FROM project_label_links k WHERE k.label_id = l.id) AS project_count
           FROM project_labels l WHERE l.tenant_id = $1 ORDER BY l.sort_order, lower(l.name)",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "labels": rows, "can_create": can_create(&state, &user).await?, "can_manage": user.is_admin() })))
}

/// `POST /api/project-labels`
pub async fn create(State(state): State<AppState>, user: CurrentUser, Json(req): Json<LabelInput>) -> ApiResult<Json<Value>> {
    if !can_create(&state, &user).await? {
        return Err(ApiError::Forbidden);
    }
    let name = check_label_name(req.name.as_deref().unwrap_or(""))?;
    let color = check_color(req.color.as_deref().unwrap_or("#64748b"))?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM project_labels WHERE tenant_id = $1").bind(user.tenant_id).fetch_one(&state.db).await?;
    if count >= MAX_LABELS {
        return bad_request("слишком много меток");
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_labels (tenant_id, name, color, sort_order)
         VALUES ($1, $2, $3, COALESCE($4, (SELECT COALESCE(max(sort_order), 0) + 1 FROM project_labels WHERE tenant_id = $1)))
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&name)
    .bind(&color)
    .bind(req.sort_order)
    .fetch_one(&state.db)
    .await
    .map_err(name_conflict)?;
    Ok(Json(json!({ "id": id, "name": name, "color": color })))
}

/// `PATCH /api/project-labels/{id}` — administrators.
pub async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<LabelInput>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let name = req.name.as_deref().map(check_label_name).transpose()?;
    let color = req.color.as_deref().map(check_color).transpose()?;
    let n = sqlx::query(
        "UPDATE project_labels SET name = COALESCE($3, name), color = COALESCE($4, color), sort_order = COALESCE($5, sort_order)
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(name)
    .bind(color)
    .bind(req.sort_order)
    .execute(&state.db)
    .await
    .map_err(name_conflict)?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /api/project-labels/{id}` — administrators; the label leaves every project.
pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let n = sqlx::query("DELETE FROM project_labels WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    state.audit(user.tenant_id, Some(user.user_id), "project_label.deleted", Some("project_label"), Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SetLabels {
    pub label_ids: Vec<Uuid>,
}

/// `PUT /api/projects/{id}/labels` — replace the labels of a project (its managers).
pub async fn set_for_project(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetLabels>,
) -> ApiResult<Json<Value>> {
    if project_role(&state, &user, id).await? < ProjectRole::Manager {
        return Err(ApiError::Forbidden);
    }
    let mut ids = req.label_ids;
    ids.sort();
    ids.dedup();
    if ids.len() > MAX_PER_PROJECT {
        return bad_request("не больше 10 меток на проект");
    }
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM project_labels WHERE id = ANY($1) AND tenant_id = $2")
        .bind(&ids)
        .bind(user.tenant_id)
        .fetch_one(&state.db)
        .await?;
    if known as usize != ids.len() {
        return bad_request("unknown label");
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM project_label_links WHERE project_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO project_label_links (project_id, label_id) SELECT $1, unnest($2::uuid[])")
        .bind(id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "label_ids": ids })))
}
