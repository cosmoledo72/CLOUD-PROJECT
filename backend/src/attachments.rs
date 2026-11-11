//! Files attached to tasks (C3).
//!
//! An attachment is a link between a task and a file of the project folder.
//! Uploads from the task card go to «Задачи/#N Название» inside the project
//! folder, so they are also visible on the project's «Файлы» tab; existing
//! files of the project folder can be attached as well. Detaching removes the
//! link only. Attachments of files in the trash are hidden until restored.

use axum::{
    extract::{Multipart, Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    acl::{self, Access},
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    files,
    projects::{load_task, project_role, ProjectRole, TaskRow},
    state::AppState,
};

/// Attachments per task.
const MAX_ATTACHMENTS: i64 = 100;
/// Name of the folder (inside the project folder) that holds task uploads.
const TASKS_FOLDER: &str = "Задачи";

#[derive(Serialize, sqlx::FromRow)]
pub struct AttachmentRow {
    file_id: Uuid,
    folder_id: Uuid,
    name: String,
    extension: String,
    size_bytes: i64,
    updated_at: DateTime<Utc>,
    added_by: Option<Uuid>,
    added_by_name: Option<String>,
    added_at: DateTime<Utc>,
}

pub async fn list_for(state: &AppState, task_id: Uuid) -> ApiResult<Vec<AttachmentRow>> {
    Ok(sqlx::query_as::<_, AttachmentRow>(
        "SELECT f.id AS file_id, f.folder_id, f.name, f.extension, f.size_bytes, f.updated_at,
                a.added_by, u.display_name AS added_by_name, a.added_at
           FROM task_attachments a JOIN files f ON f.id = a.file_id LEFT JOIN users u ON u.id = a.added_by
          WHERE a.task_id = $1 AND file_is_live(f.id)
          ORDER BY a.added_at, lower(f.name)",
    )
    .bind(task_id)
    .fetch_all(&state.db)
    .await?)
}

async fn can_edit(state: &AppState, user: &CurrentUser, t: &TaskRow) -> ApiResult<bool> {
    let role = project_role(state, user, t.project_id).await?;
    Ok(role >= ProjectRole::Member || t.assignee_id == Some(user.user_id))
}

/// `GET /api/tasks/{id}/attachments`
pub async fn list(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    let editable = can_edit(&state, &user, &t).await?;
    Ok(Json(json!({ "attachments": list_for(&state, id).await?, "can_edit": editable })))
}

/// A folder name made of the task number and title: «#234 Обмерные работы».
fn task_folder_name(number: i64, title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = clean.chars().take(80).collect();
    let name = format!("#{number} {}", short.trim_end_matches(['.', ' ']));
    name.trim().to_string()
}

/// A live child folder by name, created when missing.
async fn ensure_child(state: &AppState, user: &CurrentUser, parent: Uuid, name: &str) -> ApiResult<Uuid> {
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM folders WHERE parent_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL")
            .bind(parent)
            .bind(name)
            .fetch_optional(&state.db)
            .await?;
    if let Some(id) = found {
        return Ok(id);
    }
    Ok(sqlx::query_scalar("INSERT INTO folders (tenant_id, parent_id, name, owner_id) VALUES ($1, $2, $3, $4) RETURNING id")
        .bind(user.tenant_id)
        .bind(parent)
        .bind(name)
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?)
}

/// Is the folder (and everything above it) out of the trash?
async fn folder_is_live(state: &AppState, folder_id: Uuid) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE up AS (
             SELECT id, parent_id, deleted_at FROM folders WHERE id = $1
             UNION ALL
             SELECT p.id, p.parent_id, p.deleted_at FROM folders p JOIN up ON p.id = up.parent_id)
         SELECT EXISTS (SELECT 1 FROM up) AND NOT EXISTS (SELECT 1 FROM up WHERE deleted_at IS NOT NULL)",
    )
    .bind(folder_id)
    .fetch_one(&state.db)
    .await?)
}

/// Is `folder_id` the project folder or somewhere inside it?
async fn inside(state: &AppState, folder_id: Uuid, root: Uuid) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE up AS (
             SELECT id, parent_id FROM folders WHERE id = $1
             UNION ALL
             SELECT p.id, p.parent_id FROM folders p JOIN up ON p.id = up.parent_id)
         SELECT EXISTS (SELECT 1 FROM up WHERE id = $2)",
    )
    .bind(folder_id)
    .bind(root)
    .fetch_one(&state.db)
    .await?)
}

async fn project_folder(state: &AppState, project_id: Uuid) -> ApiResult<Uuid> {
    let folder: Option<Uuid> = sqlx::query_scalar(
        "SELECT p.folder_id FROM projects p JOIN folders f ON f.id = p.folder_id WHERE p.id = $1 AND f.deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await?
    .flatten();
    folder.ok_or_else(|| ApiError::BadRequest("у проекта нет папки документов".into()))
}

/// The folder for the task's uploads: «<project folder>/Задачи/#N Название».
/// Reused while it is live; created anew when it went to the trash.
async fn task_folder(state: &AppState, user: &CurrentUser, t: &TaskRow) -> ApiResult<Uuid> {
    let current: Option<Uuid> = sqlx::query_scalar("SELECT folder_id FROM tasks WHERE id = $1").bind(t.id).fetch_one(&state.db).await?;
    if let Some(id) = current {
        if folder_is_live(state, id).await? {
            return Ok(id);
        }
    }
    let root = project_folder(state, t.project_id).await?;
    let tasks = ensure_child(state, user, root, TASKS_FOLDER).await?;
    let id = ensure_child(state, user, tasks, &task_folder_name(t.number, &t.title)).await?;
    sqlx::query("UPDATE tasks SET folder_id = $2 WHERE id = $1").bind(t.id).bind(id).execute(&state.db).await?;
    Ok(id)
}

/// «report.pdf» → «report (2).pdf» until the name is free in the folder.
async fn free_name(state: &AppState, folder_id: Uuid, name: &str) -> ApiResult<String> {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1..1000 {
        let candidate = if n == 1 { name.to_string() } else { format!("{stem} ({n}){ext}") };
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM files WHERE folder_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL)")
                .bind(folder_id)
                .bind(&candidate)
                .fetch_one(&state.db)
                .await?;
        if !taken {
            return Ok(candidate);
        }
    }
    Err(ApiError::Conflict("слишком много файлов с таким именем".into()))
}

async fn check_room(state: &AppState, task_id: Uuid, adding: i64) -> ApiResult<()> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM task_attachments WHERE task_id = $1").bind(task_id).fetch_one(&state.db).await?;
    if n + adding > MAX_ATTACHMENTS {
        return bad_request("слишком много вложений у задачи");
    }
    Ok(())
}

async fn link(state: &AppState, user: &CurrentUser, task_id: Uuid, file_id: Uuid) -> ApiResult<bool> {
    let n = sqlx::query(
        "INSERT INTO task_attachments (task_id, file_id, tenant_id, added_by) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
    )
    .bind(task_id)
    .bind(file_id)
    .bind(user.tenant_id)
    .bind(user.user_id)
    .execute(&state.db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

async fn touch(state: &AppState, task_id: Uuid) -> ApiResult<()> {
    sqlx::query("UPDATE tasks SET updated_at = now() WHERE id = $1").bind(task_id).execute(&state.db).await?;
    Ok(())
}

/// `POST /api/tasks/{id}/attachments` (multipart) — upload files into the
/// task's folder and attach them. The right to edit the task is enough.
pub async fn upload(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, mut multipart: Multipart) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    if !can_edit(&state, &user, &t).await? {
        return Err(ApiError::Forbidden);
    }
    let mut folder: Option<Uuid> = None;
    let mut added = 0;
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        let Some(file_name) = field.file_name().map(str::to_string) else { continue };
        let name = files::validate_name(&file_name)?;
        let data = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
        check_room(&state, id, 1).await?;
        let folder_id = match folder {
            Some(f) => f,
            None => {
                let f = task_folder(&state, &user, &t).await?;
                folder = Some(f);
                f
            }
        };
        let name = free_name(&state, folder_id, &name).await?;
        let file_id = files::create_file(&state, &user, folder_id, &name, data).await?;
        link(&state, &user, id, file_id).await?;
        added += 1;
    }
    if added == 0 {
        return bad_request("файл не передан");
    }
    touch(&state, id).await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "task.attachment_added", Some("task"), Some(id), json!({ "files": added }))
        .await;
    Ok(Json(json!({ "attachments": list_for(&state, id).await?, "added": added })))
}

#[derive(Deserialize)]
pub struct LinkRequest {
    pub file_ids: Vec<Uuid>,
}

/// `POST /api/tasks/{id}/attachments/link` — attach files that already lie in
/// the project folder. The user must be able to read each file.
pub async fn link_existing(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<LinkRequest>,
) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    if !can_edit(&state, &user, &t).await? {
        return Err(ApiError::Forbidden);
    }
    let mut ids = req.file_ids;
    ids.sort();
    ids.dedup();
    if ids.is_empty() || ids.len() > 50 {
        return bad_request("выберите от 1 до 50 файлов");
    }
    check_room(&state, id, ids.len() as i64).await?;
    let root = project_folder(&state, t.project_id).await?;
    for file_id in &ids {
        let (access, folder_id) = acl::file_access(&state.db, &user, *file_id).await?;
        access.require(Access::Read)?;
        if !inside(&state, folder_id, root).await? {
            return bad_request("прикрепить можно только файлы из папки проекта");
        }
    }
    let mut added = 0;
    for file_id in &ids {
        if link(&state, &user, id, *file_id).await? {
            added += 1;
        }
    }
    if added > 0 {
        touch(&state, id).await?;
        state
            .audit(user.tenant_id, Some(user.user_id), "task.attachment_added", Some("task"), Some(id), json!({ "files": added, "linked": true }))
            .await;
    }
    Ok(Json(json!({ "attachments": list_for(&state, id).await?, "added": added })))
}

/// `DELETE /api/tasks/{id}/attachments/{file_id}` — detach; the file stays in
/// its folder. Allowed to whoever attached it and to project managers.
pub async fn detach(State(state): State<AppState>, user: CurrentUser, Path((id, file_id)): Path<(Uuid, Uuid)>) -> ApiResult<Json<Value>> {
    let t = load_task(&state, &user, id).await?;
    let role = project_role(&state, &user, t.project_id).await?;
    let added_by: Option<Option<Uuid>> = sqlx::query_scalar("SELECT added_by FROM task_attachments WHERE task_id = $1 AND file_id = $2")
        .bind(id)
        .bind(file_id)
        .fetch_optional(&state.db)
        .await?;
    let added_by = added_by.ok_or(ApiError::NotFound)?;
    if role < ProjectRole::Manager && added_by != Some(user.user_id) {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("DELETE FROM task_attachments WHERE task_id = $1 AND file_id = $2").bind(id).bind(file_id).execute(&state.db).await?;
    touch(&state, id).await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "task.attachment_removed", Some("task"), Some(id), json!({ "file_id": file_id }))
        .await;
    Ok(Json(json!({ "attachments": list_for(&state, id).await? })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_folder_names_are_safe() {
        assert_eq!(task_folder_name(234, "Обмерные работы"), "#234 Обмерные работы");
        assert_eq!(task_folder_name(7, "Отчёт: том 1/2 «АР» ..."), "#7 Отчёт том 1 2 «АР»");
        assert_eq!(task_folder_name(1, &"я".repeat(300)).chars().count(), 3 + 80);
        assert_eq!(task_folder_name(5, "  ???  "), "#5");
    }
}
