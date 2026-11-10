//! Task comments with @mentions.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    notify::{self, Note},
    projects::{project_role, ProjectRole},
    state::AppState,
};

/// Maximum comment length in characters.
const MAX_LEN: usize = 10_000;

#[derive(Serialize, sqlx::FromRow)]
struct CommentRow {
    id: Uuid,
    author_id: Option<Uuid>,
    author_name: Option<String>,
    body: String,
    created_at: DateTime<Utc>,
}

struct TaskRef {
    project_id: Uuid,
    title: String,
    assignee_id: Option<Uuid>,
    created_by: Option<Uuid>,
}

async fn task_ref(state: &AppState, user: &CurrentUser, task_id: Uuid) -> ApiResult<TaskRef> {
    let row: (Uuid, String, Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT project_id, title, assignee_id, created_by FROM tasks WHERE id = $1 AND tenant_id = $2")
            .bind(task_id)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    Ok(TaskRef { project_id: row.0, title: row.1, assignee_id: row.2, created_by: row.3 })
}

pub async fn list(State(state): State<AppState>, user: CurrentUser, Path(task_id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let t = task_ref(&state, &user, task_id).await?;
    project_role(&state, &user, t.project_id).await?;
    let rows = sqlx::query_as::<_, CommentRow>(
        "SELECT c.id, c.author_id, u.display_name AS author_name, c.body, c.created_at
           FROM task_comments c LEFT JOIN users u ON u.id = c.author_id
          WHERE c.task_id = $1 ORDER BY c.created_at",
    )
    .bind(task_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "comments": rows })))
}

#[derive(Deserialize)]
pub struct NewComment {
    pub body: String,
    /// Users picked from the @mention list.
    #[serde(default)]
    pub mentions: Vec<Uuid>,
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(task_id): Path<Uuid>,
    Json(req): Json<NewComment>,
) -> ApiResult<Json<Value>> {
    let t = task_ref(&state, &user, task_id).await?;
    // Viewers may read but not discuss; assignees always may.
    let role = project_role(&state, &user, t.project_id).await?;
    if t.assignee_id != Some(user.user_id) && role < ProjectRole::Member {
        return Err(ApiError::Forbidden);
    }
    let body = req.body.trim();
    if body.is_empty() {
        return bad_request("comment is empty");
    }
    if body.chars().count() > MAX_LEN {
        return bad_request("comment is too long");
    }
    let id: Uuid = sqlx::query_scalar("INSERT INTO task_comments (tenant_id, task_id, author_id, body) VALUES ($1, $2, $3, $4) RETURNING id")
        .bind(user.tenant_id)
        .bind(task_id)
        .bind(user.user_id)
        .bind(body)
        .fetch_one(&state.db)
        .await?;

    let link = format!("/projects/{}?task={task_id}", t.project_id);
    let excerpt = excerpt(body);
    // Mentioned users who can see the task get a mention instead of a plain comment notice.
    let mentioned = notify::task_audience(&state, user.tenant_id, task_id, &req.mentions).await?;
    notify::notify(
        &state,
        user.tenant_id,
        Some(user.user_id),
        &mentioned,
        Note {
            kind: "mention",
            title: format!("{} упомянул(а) вас в задаче «{}»", user.display_name, t.title),
            body: excerpt.clone(),
            link: link.clone(),
            entity_type: Some("task"),
            entity_id: Some(task_id),
            ..Default::default()
        },
    )
    .await;
    // Participants: assignee, author and earlier commenters.
    let mut participants: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT author_id FROM task_comments WHERE task_id = $1 AND author_id IS NOT NULL")
        .bind(task_id)
        .fetch_all(&state.db)
        .await?;
    participants.extend(t.assignee_id);
    participants.extend(t.created_by);
    participants.retain(|u| !mentioned.contains(u));
    let participants = notify::task_audience(&state, user.tenant_id, task_id, &participants).await?;
    notify::notify(
        &state,
        user.tenant_id,
        Some(user.user_id),
        &participants,
        Note {
            kind: "task_comment",
            title: format!("Комментарий к задаче «{}»", t.title),
            body: format!("{}: {excerpt}", user.display_name),
            link,
            entity_type: Some("task"),
            entity_id: Some(task_id),
            ..Default::default()
        },
    )
    .await;
    Ok(Json(json!({ "id": id })))
}

pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (task_id, author): (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT task_id, author_id FROM task_comments WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let t = task_ref(&state, &user, task_id).await?;
    let role = project_role(&state, &user, t.project_id).await?;
    if author != Some(user.user_id) && role < ProjectRole::Manager {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("DELETE FROM task_comments WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

/// First line-ish part of a text for notification bodies.
pub fn excerpt(s: &str) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 200 {
        format!("{}…", flat.chars().take(200).collect::<String>())
    } else {
        flat
    }
}
