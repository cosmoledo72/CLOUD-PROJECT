//! Interface settings that follow the user across browsers: which fields the
//! project list shows, in what order, grouping and so on. Each user keeps
//! their own; an administrator may set the portal default. The server stores
//! the JSON as is (the interface validates its own format) and only checks
//! the key and the size.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiResult},
    state::AppState,
};

/// Known setting keys.
pub const KEYS: &[&str] = &["projects_list"];
/// Largest stored value, bytes of JSON.
pub const MAX_BYTES: usize = 16 * 1024;

fn check_key(key: &str) -> ApiResult<()> {
    if KEYS.contains(&key) { Ok(()) } else { bad_request(format!("unknown setting '{key}'")) }
}

fn check_value(v: &Value) -> ApiResult<()> {
    if !v.is_object() {
        return bad_request("setting must be a JSON object");
    }
    if serde_json::to_vec(v).map(|b| b.len()).unwrap_or(usize::MAX) > MAX_BYTES {
        return bad_request("setting is too large");
    }
    Ok(())
}

async fn load(state: &AppState, user: &CurrentUser, key: &str) -> ApiResult<(Option<Value>, Option<Value>)> {
    let own: Option<Value> = sqlx::query_scalar("SELECT value FROM view_settings WHERE user_id = $1 AND key = $2")
        .bind(user.user_id)
        .bind(key)
        .fetch_optional(&state.db)
        .await?;
    let dflt: Option<Value> = sqlx::query_scalar("SELECT value FROM view_settings WHERE tenant_id = $1 AND user_id IS NULL AND key = $2")
        .bind(user.tenant_id)
        .bind(key)
        .fetch_optional(&state.db)
        .await?;
    Ok((own, dflt))
}

/// `{value, own, default}`: `value` is the user's own setting, else the portal
/// default, else null (the interface then uses its built-in default).
pub async fn get(State(state): State<AppState>, user: CurrentUser, Path(key): Path<String>) -> ApiResult<Json<Value>> {
    check_key(&key)?;
    let (own, dflt) = load(&state, &user, &key).await?;
    Ok(Json(json!({ "value": own.clone().or(dflt.clone()), "own": own.is_some(), "default": dflt })))
}

#[derive(Deserialize)]
pub struct Input {
    pub value: Value,
}

pub async fn put(State(state): State<AppState>, user: CurrentUser, Path(key): Path<String>, Json(req): Json<Input>) -> ApiResult<Json<Value>> {
    check_key(&key)?;
    check_value(&req.value)?;
    sqlx::query(
        "INSERT INTO view_settings (tenant_id, user_id, key, value) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, key) WHERE user_id IS NOT NULL DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(&key)
    .bind(&req.value)
    .execute(&state.db)
    .await?;
    get(State(state), user, Path(key)).await
}

/// Drop the user's own setting: back to the portal default.
pub async fn reset(State(state): State<AppState>, user: CurrentUser, Path(key): Path<String>) -> ApiResult<Json<Value>> {
    check_key(&key)?;
    sqlx::query("DELETE FROM view_settings WHERE user_id = $1 AND key = $2").bind(user.user_id).bind(&key).execute(&state.db).await?;
    get(State(state), user, Path(key)).await
}

/// Portal default (administrators). `null` removes it.
pub async fn put_default(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(req): Json<Input>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    check_key(&key)?;
    if req.value.is_null() {
        sqlx::query("DELETE FROM view_settings WHERE tenant_id = $1 AND user_id IS NULL AND key = $2")
            .bind(user.tenant_id)
            .bind(&key)
            .execute(&state.db)
            .await?;
    } else {
        check_value(&req.value)?;
        sqlx::query(
            "INSERT INTO view_settings (tenant_id, user_id, key, value) VALUES ($1, NULL, $2, $3)
             ON CONFLICT (tenant_id, key) WHERE user_id IS NULL DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(user.tenant_id)
        .bind(&key)
        .bind(&req.value)
        .execute(&state.db)
        .await?;
    }
    state
        .audit(user.tenant_id, Some(user.user_id), "portal.view_default", Some("tenant"), Some(user.tenant_id), json!({ "key": key, "cleared": req.value.is_null() }))
        .await;
    get(State(state), user, Path(key)).await
}
