//! Team directory: colleagues with their groups, contacts and current load,
//! plus self-service editing of the profile card.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
};

#[derive(Serialize, sqlx::FromRow)]
struct Member {
    id: Uuid,
    display_name: String,
    email: String,
    role: String,
    position: String,
    phone: String,
    is_active: bool,
    group_ids: Vec<Uuid>,
    open_tasks: i64,
    overdue_tasks: i64,
    projects: i64,
}

#[derive(Serialize, sqlx::FromRow)]
struct Group {
    id: Uuid,
    name: String,
    members: i64,
}

pub async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    if user.role == "guest" {
        return Err(ApiError::Forbidden);
    }
    let members = sqlx::query_as::<_, Member>(
        "SELECT u.id, u.display_name, u.email::text AS email, u.role, u.position, u.phone, u.is_active,
                COALESCE((SELECT array_agg(gm.group_id) FROM group_members gm WHERE gm.user_id = u.id), '{}') AS group_ids,
                (SELECT count(*) FROM tasks t WHERE t.assignee_id = u.id AND t.status <> 'done') AS open_tasks,
                (SELECT count(*) FROM tasks t WHERE t.assignee_id = u.id AND t.status <> 'done' AND t.due_date < current_date) AS overdue_tasks,
                (SELECT count(*) FROM project_members pm JOIN projects p ON p.id = pm.project_id
                  WHERE pm.user_id = u.id AND p.status <> 'closed') AS projects
           FROM users u
          WHERE u.tenant_id = $1 AND (u.is_active OR $2)
          ORDER BY lower(u.display_name)",
    )
    .bind(user.tenant_id)
    .bind(user.is_admin())
    .fetch_all(&state.db)
    .await?;
    let groups = sqlx::query_as::<_, Group>(
        "SELECT g.id, g.name, (SELECT count(*) FROM group_members gm JOIN users u ON u.id = gm.user_id
                                WHERE gm.group_id = g.id AND u.is_active) AS members
           FROM groups g WHERE g.tenant_id = $1 ORDER BY lower(g.name)",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "members": members, "groups": groups })))
}

#[derive(Deserialize)]
pub struct ProfileInput {
    pub display_name: String,
    #[serde(default)]
    pub position: String,
    #[serde(default)]
    pub phone: String,
}

/// Update the caller's own card (name, position, phone).
pub async fn update_profile(State(state): State<AppState>, user: CurrentUser, Json(req): Json<ProfileInput>) -> ApiResult<Json<Value>> {
    let name = req.display_name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return bad_request("имя — от 1 до 100 символов");
    }
    let position = req.position.trim();
    if position.chars().count() > 100 {
        return bad_request("должность — не длиннее 100 символов");
    }
    let phone = req.phone.trim();
    if phone.chars().count() > 40 || !phone.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c)) {
        return bad_request("телефон: цифры, пробелы, «+», «-» и скобки, не длиннее 40 символов");
    }
    sqlx::query("UPDATE users SET display_name = $2, position = $3, phone = $4 WHERE id = $1")
        .bind(user.user_id)
        .bind(name)
        .bind(position)
        .bind(phone)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
