//! Tenant administration: dashboard metrics, audit log, portal settings,
//! account recovery actions and group maintenance. All endpoints require a
//! tenant owner or admin.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{hash_password, validate_password, CurrentUser},
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct Overview {
    users_total: i64,
    users_active: i64,
    users_mfa: i64,
    users_guests: i64,
    sessions_active: i64,
    files_total: i64,
    storage_used: i64,
    storage_quota: i64,
    projects_active: i64,
    projects_total: i64,
    tasks_open: i64,
    tasks_overdue: i64,
    docs_editing: i64,
}

pub async fn overview(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let o = sqlx::query_as::<_, Overview>(
        "SELECT
            (SELECT count(*) FROM users WHERE tenant_id = $1) AS users_total,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND is_active) AS users_active,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND totp_enabled) AS users_mfa,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND role = 'guest') AS users_guests,
            (SELECT count(DISTINCT user_id) FROM sessions WHERE tenant_id = $1 AND stage = 'active' AND expires_at > now()) AS sessions_active,
            (SELECT count(*) FROM files WHERE tenant_id = $1 AND deleted_at IS NULL) AS files_total,
            (SELECT COALESCE(sum(fv.size_bytes), 0)::bigint FROM file_versions fv JOIN files f ON f.id = fv.file_id
              WHERE f.tenant_id = $1) AS storage_used,
            (SELECT quota_bytes FROM tenants WHERE id = $1) AS storage_quota,
            (SELECT count(*) FROM projects WHERE tenant_id = $1 AND status = 'active') AS projects_active,
            (SELECT count(*) FROM projects WHERE tenant_id = $1) AS projects_total,
            (SELECT count(*) FROM tasks WHERE tenant_id = $1 AND status <> 'done') AS tasks_open,
            (SELECT count(*) FROM tasks WHERE tenant_id = $1 AND status <> 'done' AND due_date < current_date) AS tasks_overdue,
            (SELECT count(*) FROM files WHERE tenant_id = $1 AND edit_key IS NOT NULL AND deleted_at IS NULL) AS docs_editing",
    )
    .bind(user.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let recent = load_audit(&state, user.tenant_id, &AuditQuery::default(), 8).await?;
    Ok(Json(json!({ "stats": o, "recent": recent })))
}

// ---------------------------------------------------------------------------
// Audit log
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct AuditQuery {
    /// Action prefix, e.g. `auth.` or `file.deleted`.
    pub action: Option<String>,
    pub user_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Keyset pagination: return entries with id < before.
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
struct AuditRow {
    id: i64,
    user_id: Option<Uuid>,
    user_name: Option<String>,
    action: String,
    target_type: Option<String>,
    target_id: Option<Uuid>,
    target_name: Option<String>,
    details: Value,
    created_at: DateTime<Utc>,
}

async fn load_audit(state: &AppState, tenant_id: Uuid, q: &AuditQuery, limit: i64) -> ApiResult<Vec<AuditRow>> {
    Ok(sqlx::query_as::<_, AuditRow>(
        "SELECT a.id, a.user_id, u.display_name AS user_name, a.action, a.target_type, a.target_id,
                COALESCE(f.name, fo.name, p.name, tu.display_name) AS target_name, a.details, a.created_at
           FROM audit_log a
           LEFT JOIN users u ON u.id = a.user_id
           LEFT JOIN files f ON a.target_type = 'file' AND f.id = a.target_id
           LEFT JOIN folders fo ON a.target_type = 'folder' AND fo.id = a.target_id
           LEFT JOIN projects p ON a.target_type = 'project' AND p.id = a.target_id
           LEFT JOIN users tu ON a.target_type = 'user' AND tu.id = a.target_id
          WHERE a.tenant_id = $1
            AND ($2::text IS NULL OR a.action LIKE $2 || '%')
            AND ($3::uuid IS NULL OR a.user_id = $3)
            AND ($4::date IS NULL OR a.created_at >= $4)
            AND ($5::date IS NULL OR a.created_at < $5 + 1)
            AND ($6::bigint IS NULL OR a.id < $6)
          ORDER BY a.id DESC
          LIMIT $7",
    )
    .bind(tenant_id)
    .bind(q.action.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(q.user_id)
    .bind(q.from)
    .bind(q.to)
    .bind(q.before)
    .bind(limit)
    .fetch_all(&state.db)
    .await?)
}

pub async fn audit(State(state): State<AppState>, user: CurrentUser, Query(q): Query<AuditQuery>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let rows = load_audit(&state, user.tenant_id, &q, limit + 1).await?;
    let has_more = rows.len() as i64 > limit;
    let rows: Vec<AuditRow> = rows.into_iter().take(limit as usize).collect();
    let next = if has_more { rows.last().map(|r| r.id) } else { None };
    Ok(Json(json!({ "entries": rows, "next_before": next })))
}

// ---------------------------------------------------------------------------
// Portal settings
// ---------------------------------------------------------------------------

pub async fn get_portal(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let (slug, name, quota, created_at, timezone): (String, String, i64, DateTime<Utc>, String) =
        sqlx::query_as("SELECT slug::text, name, quota_bytes, created_at, timezone FROM tenants WHERE id = $1")
            .bind(user.tenant_id)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(json!({
        "slug": slug,
        "name": name,
        "quota_bytes": quota,
        "created_at": created_at,
        "timezone": timezone,
        "signup_enabled": state.cfg.signup_enabled,
        "session_ttl_hours": state.cfg.session_ttl_hours,
        "max_upload_mb": state.cfg.max_upload_bytes / 1024 / 1024,
    })))
}

#[derive(Deserialize)]
pub struct UpdatePortal {
    pub name: String,
    /// IANA time zone, e.g. Europe/Moscow; unchanged when absent.
    pub timezone: Option<String>,
}

pub async fn update_portal(State(state): State<AppState>, user: CurrentUser, Json(req): Json<UpdatePortal>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return bad_request("portal name is required (up to 200 chars)");
    }
    if let Some(tz) = &req.timezone {
        let known: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)")
            .bind(tz)
            .fetch_one(&state.db)
            .await?;
        if !known {
            return bad_request("unknown time zone");
        }
    }
    sqlx::query("UPDATE tenants SET name = $2, timezone = COALESCE($3, timezone) WHERE id = $1")
        .bind(user.tenant_id)
        .bind(name)
        .bind(&req.timezone)
        .execute(&state.db)
        .await?;
    state.audit(user.tenant_id, Some(user.user_id), "portal.updated", None, None, json!({ "name": name, "timezone": req.timezone })).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Account recovery
// ---------------------------------------------------------------------------

/// Owners may only be managed by themselves; admins cannot manage other admins
/// unless they are the owner.
async fn manageable_target(state: &AppState, user: &CurrentUser, target: Uuid) -> ApiResult<()> {
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1 AND tenant_id = $2")
        .bind(target)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if target == user.user_id {
        return bad_request("use the profile page to manage your own account");
    }
    let allowed = match role.as_str() {
        "owner" => false,
        "admin" => user.role == "owner",
        _ => true,
    };
    if allowed { Ok(()) } else { Err(ApiError::Forbidden) }
}

#[derive(Deserialize)]
pub struct ResetPassword {
    pub password: String,
}

/// Set a new password for a user and end all their sessions.
pub async fn reset_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ResetPassword>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    manageable_target(&state, &user, id).await?;
    validate_password(&req.password)?;
    let hash = hash_password(req.password).await?;
    // Setting a password also completes a pending invitation.
    sqlx::query("UPDATE users SET password_hash = $2, invite_pending = false WHERE id = $1").bind(id).bind(hash).execute(&state.db).await?;
    sqlx::query("DELETE FROM user_tokens WHERE user_id = $1 AND used_at IS NULL").bind(id).execute(&state.db).await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(id).execute(&state.db).await?;
    state.audit(user.tenant_id, Some(user.user_id), "user.password_reset", Some("user"), Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

/// Disable two-factor authentication for a user who lost their device.
pub async fn reset_mfa(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    manageable_target(&state, &user, id).await?;
    sqlx::query("UPDATE users SET totp_enabled = false, totp_secret = NULL WHERE id = $1").bind(id).execute(&state.db).await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(id).execute(&state.db).await?;
    state.audit(user.tenant_id, Some(user.user_id), "user.mfa_reset", Some("user"), Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

/// End all sessions of a user (e.g. a lost laptop).
pub async fn revoke_sessions(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    manageable_target(&state, &user, id).await?;
    let n = sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(id).execute(&state.db).await?.rows_affected();
    state.audit(user.tenant_id, Some(user.user_id), "user.sessions_revoked", Some("user"), Some(id), json!({ "count": n })).await;
    Ok(Json(json!({ "revoked": n })))
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RenameGroup {
    pub name: String,
}

pub async fn rename_group(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RenameGroup>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let name = req.name.trim();
    if name.is_empty() {
        return bad_request("name is required");
    }
    let n = sqlx::query("UPDATE groups SET name = $3 WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .bind(name)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_group(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("DELETE FROM groups WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    // Shares granted to the group are removed by ON DELETE CASCADE.
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "group.deleted", None, Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}
