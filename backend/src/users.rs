//! Tenant user directory and groups (admin-managed).

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{hash_password, validate_password, CurrentUser},
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
};

#[derive(Serialize, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    display_name: String,
    role: String,
    is_active: bool,
    totp_enabled: bool,
    crm_role: String,
    invite_pending: bool,
    created_at: DateTime<Utc>,
}

pub async fn list_users(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT id, email::text AS email, display_name, role, is_active, totp_enabled, crm_role, invite_pending, created_at
           FROM users WHERE tenant_id = $1 ORDER BY lower(display_name)",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "users": rows })))
}

fn valid_role(r: &str) -> bool {
    matches!(r, "admin" | "member" | "guest")
}

#[derive(Deserialize)]
pub struct CreateUser {
    pub email: String,
    pub display_name: String,
    /// Initial password handed to the user out of band. Without it the user is
    /// invited: they receive a link and choose the password themselves.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "member".into()
}

pub async fn create_user(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CreateUser>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    if !valid_role(&req.role) {
        return bad_request("role must be admin, member or guest");
    }
    // Only the portal owner may appoint administrators.
    if req.role == "admin" && user.role != "owner" {
        return Err(ApiError::Forbidden);
    }
    let email = req.email.trim();
    if email.parse::<lettre::Address>().is_err() || req.display_name.trim().is_empty() {
        return bad_request("valid email and display name are required");
    }
    let password = req.password.filter(|p| !p.is_empty());
    let invite = password.is_none();
    let hash = match password {
        Some(p) => {
            validate_password(&p)?;
            hash_password(p).await?
        }
        None => crate::accounts::NO_PASSWORD.to_string(),
    };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, email, display_name, password_hash, role, invite_pending)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(email)
    .bind(req.display_name.trim())
    .bind(hash)
    .bind(&req.role)
    .bind(invite)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("a user with this email already exists".into()),
        other => other,
    })?;
    state.audit(user.tenant_id, Some(user.user_id), "user.created", Some("user"), Some(id), json!({ "role": req.role, "invited": invite })).await;
    if invite {
        let (url, emailed) = crate::accounts::send_invite(&state, &user, id).await?;
        return Ok(Json(json!({ "id": id, "invite_url": url, "emailed": emailed })));
    }
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct UpdateUser {
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub is_active: Option<bool>,
    pub crm_role: Option<String>,
}

pub async fn update_user(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUser>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let target_role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if target_role == "owner" && (req.role.is_some() || req.is_active == Some(false)) {
        return bad_request("the portal owner cannot be demoted or deactivated");
    }
    if id == user.user_id && (req.role.is_some() || req.is_active.is_some()) {
        return bad_request("you cannot change your own role or status");
    }
    // Administrators are managed by the owner only.
    if user.role != "owner" && (target_role == "admin" || req.role.as_deref() == Some("admin")) {
        return Err(ApiError::Forbidden);
    }
    if let Some(r) = &req.crm_role {
        if !matches!(r.as_str(), "none" | "user" | "manager") {
            return bad_request("crm_role must be none, user or manager");
        }
    }
    if let Some(r) = &req.role {
        if !valid_role(r) {
            return bad_request("role must be admin, member or guest");
        }
    }
    sqlx::query(
        "UPDATE users SET display_name = COALESCE($3, display_name), role = COALESCE($4, role), is_active = COALESCE($5, is_active),
                crm_role = COALESCE($6, crm_role)
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(req.display_name.map(|s| s.trim().to_string()))
    .bind(&req.role)
    .bind(req.is_active)
    .bind(&req.crm_role)
    .execute(&state.db)
    .await?;
    if req.is_active == Some(false) {
        sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(id).execute(&state.db).await?;
    }
    state.audit(user.tenant_id, Some(user.user_id), "user.updated", Some("user"), Some(id), json!({ "role": req.role, "is_active": req.is_active })).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize, sqlx::FromRow)]
struct GroupRow {
    id: Uuid,
    name: String,
    member_ids: Vec<Uuid>,
}

pub async fn list_groups(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query_as::<_, GroupRow>(
        "SELECT g.id, g.name, COALESCE(array_agg(gm.user_id) FILTER (WHERE gm.user_id IS NOT NULL), '{}') AS member_ids
           FROM groups g LEFT JOIN group_members gm ON gm.group_id = g.id
          WHERE g.tenant_id = $1 GROUP BY g.id ORDER BY lower(g.name)",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "groups": rows })))
}

#[derive(Deserialize)]
pub struct CreateGroup {
    pub name: String,
}

pub async fn create_group(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CreateGroup>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let name = req.name.trim();
    if name.is_empty() {
        return bad_request("name is required");
    }
    let id: Uuid = sqlx::query_scalar("INSERT INTO groups (tenant_id, name) VALUES ($1, $2) RETURNING id")
        .bind(user.tenant_id)
        .bind(name)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct SetMembers {
    pub user_ids: Vec<Uuid>,
}

/// Replace the member list of a group.
pub async fn set_group_members(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetMembers>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let mut tx = state.db.begin().await?;
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM groups WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    sqlx::query("DELETE FROM group_members WHERE group_id = $1").bind(id).execute(&mut *tx).await?;
    // Only users of the same tenant are inserted.
    sqlx::query(
        "INSERT INTO group_members (group_id, user_id)
         SELECT $1, u.id FROM users u WHERE u.tenant_id = $2 AND u.id = ANY($3)",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(&req.user_ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
