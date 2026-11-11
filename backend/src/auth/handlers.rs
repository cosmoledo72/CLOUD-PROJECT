//! Authentication endpoints: tenant sign-up, login with optional TOTP second
//! factor, logout, profile and TOTP enrolment.

use axum::{extract::State, http::HeaderMap, Json};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use totp_rs::{Algorithm, Secret, TOTP};
use uuid::Uuid;

use super::{
    clear_cookie, hash_password, new_token, session_cookie, token_from_headers, token_hash,
    validate_password, verify_password, CurrentUser,
};
use crate::{
    error::{bad_request, ApiError, ApiResult},
    ratelimit::{client_ip, WINDOW_15_MIN, WINDOW_1_HOUR},
    files::ensure_common_root,
    state::AppState,
};

/// Maximum wrong TOTP codes per pending login before it is discarded.
const MAX_MFA_ATTEMPTS: i16 = 5;
/// Lifetime of a password-verified login waiting for its TOTP code.
const PENDING_MFA_MINUTES: i64 = 5;

#[derive(Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub tenant_slug: String,
    pub tenant_name: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub totp_enabled: bool,
    pub crm_role: String,
    /// Effective finance access: none | view | edit (administrators: edit).
    pub finance_role: String,
    pub position: String,
    pub phone: String,
    /// Portal modules that are on (see `modules::MODULES`).
    pub modules: Vec<&'static str>,
    /// The user may open «Администрирование → Резервные копии».
    pub backups: bool,
}

async fn load_me(state: &AppState, user_id: Uuid) -> ApiResult<MeResponse> {
    #[allow(clippy::type_complexity)]
    let row: (Uuid, Uuid, String, String, String, String, String, bool, String, String, String, Vec<String>, String) = sqlx::query_as(
        "SELECT u.id, t.id, t.slug::text, t.name, u.email::text, u.display_name, u.role, u.totp_enabled,
                CASE WHEN u.role = 'guest' OR 'crm' = ANY(t.disabled_modules) THEN 'none' ELSE u.crm_role END,
                u.position, u.phone, t.disabled_modules,
                CASE WHEN u.role = 'guest' OR 'finance' = ANY(t.disabled_modules) THEN 'none'
                     WHEN u.role IN ('owner', 'admin') THEN 'edit' ELSE u.finance_role END
           FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    let backups = crate::backups::allowed(state, &row.2, &row.6);
    Ok(MeResponse {
        id: row.0,
        tenant_id: row.1,
        tenant_slug: row.2,
        tenant_name: row.3,
        email: row.4,
        display_name: row.5,
        role: row.6,
        totp_enabled: row.7,
        crm_role: row.8,
        finance_role: row.12,
        position: row.9,
        phone: row.10,
        modules: crate::modules::enabled_keys(&row.11),
        backups,
    })
}

/// Create a session row and return the cookie carrying its token.
async fn start_session(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: Uuid,
    user_id: Uuid,
    stage: &str,
) -> ApiResult<axum_extra::extract::cookie::Cookie<'static>> {
    let (token, hash) = new_token();
    let (ttl_sql, hours) = if stage == "active" {
        (format!("{} hours", state.cfg.session_ttl_hours), state.cfg.session_ttl_hours)
    } else {
        (format!("{PENDING_MFA_MINUTES} minutes"), 1)
    };
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).map(|s| s.chars().take(300).collect::<String>());
    let ip = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).map(str::to_string);
    sqlx::query(
        "INSERT INTO sessions (token_hash, tenant_id, user_id, stage, user_agent, ip, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, now() + $7::interval)",
    )
    .bind(&hash)
    .bind(tenant_id)
    .bind(user_id)
    .bind(stage)
    .bind(ua)
    .bind(ip)
    .bind(ttl_sql)
    .execute(&state.db)
    .await?;
    Ok(session_cookie(token, state.cfg.cookie_secure, hours))
}

// ---------------------------------------------------------------------------
// Sign-up: creates a tenant (portal) and its owner account.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SignupRequest {
    pub tenant_slug: String,
    pub tenant_name: String,
    pub email: String,
    pub display_name: String,
    pub password: String,
}

pub async fn signup(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<SignupRequest>,
) -> ApiResult<(CookieJar, Json<MeResponse>)> {
    if !state.cfg.signup_enabled {
        return Err(ApiError::Forbidden);
    }
    let ip_key = format!("signup:{}", client_ip(&headers));
    state.limiter.check(&ip_key, state.cfg.signup_ip_limit, WINDOW_1_HOUR)?;
    state.limiter.hit(&ip_key, WINDOW_1_HOUR);
    let slug = req.tenant_slug.trim().to_lowercase();
    if req.tenant_name.trim().is_empty() || req.display_name.trim().is_empty() || !req.email.contains('@') {
        return bad_request("tenant name, display name and a valid email are required");
    }
    validate_password(&req.password)?;
    let hash = hash_password(req.password).await?;

    let mut tx = state.db.begin().await?;
    let tenant_id: Uuid = sqlx::query_scalar("INSERT INTO tenants (slug, name) VALUES ($1, $2) RETURNING id")
        .bind(&slug)
        .bind(req.tenant_name.trim())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| match ApiError::from(e) {
            ApiError::Conflict(_) => ApiError::Conflict("this portal address is taken".into()),
            ApiError::BadRequest(_) => ApiError::BadRequest(
                "portal address: 2-63 chars, lowercase latin letters, digits and '-'".into(),
            ),
            other => other,
        })?;
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, email, display_name, password_hash, role)
         VALUES ($1, $2, $3, $4, 'owner') RETURNING id",
    )
    .bind(tenant_id)
    .bind(req.email.trim())
    .bind(req.display_name.trim())
    .bind(hash)
    .fetch_one(&mut *tx)
    .await?;
    ensure_common_root(&mut tx, tenant_id, user_id).await?;
    tx.commit().await?;

    state.audit(tenant_id, Some(user_id), "tenant.created", Some("tenant"), Some(tenant_id), json!({ "slug": slug })).await;
    let cookie = start_session(&state, &headers, tenant_id, user_id, "active").await?;
    Ok((jar.add(cookie), Json(load_me(&state, user_id).await?)))
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginRequest {
    pub tenant: String,
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> ApiResult<(CookieJar, Json<Value>)> {
    // Throttle password guessing per account and per client address.
    let account_key = format!("login:{}:{}", req.tenant.trim().to_lowercase(), req.email.trim().to_lowercase());
    let ip_key = format!("login-ip:{}", client_ip(&headers));
    state.limiter.check(&account_key, state.cfg.login_account_limit, WINDOW_15_MIN)?;
    state.limiter.check(&ip_key, state.cfg.login_ip_limit, WINDOW_15_MIN)?;
    let fail = || {
        state.limiter.hit(&account_key, WINDOW_15_MIN);
        state.limiter.hit(&ip_key, WINDOW_15_MIN);
    };

    let row: Option<(Uuid, Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT u.id, u.tenant_id, u.password_hash, u.totp_enabled, u.is_active AND t.status = 'active'
           FROM users u JOIN tenants t ON t.id = u.tenant_id
          WHERE t.slug = $1 AND u.email = $2::citext",
    )
    .bind(req.tenant.trim())
    .bind(req.email.trim())
    .fetch_optional(&state.db)
    .await?;

    // Verify against a dummy hash when the user is unknown to keep timing uniform.
    let hash = match &row {
        Some(r) => r.2.clone(),
        None => super::dummy_hash().await,
    };
    let ok = verify_password(req.password, hash).await;
    let Some((user_id, tenant_id, _, totp_enabled, active)) = row else {
        fail();
        return Err(ApiError::Unauthorized);
    };
    if !ok || !active {
        fail();
        state.audit(tenant_id, Some(user_id), "auth.login_failed", None, None, json!({})).await;
        return Err(ApiError::Unauthorized);
    }

    state.limiter.reset(&account_key);
    if totp_enabled {
        let cookie = start_session(&state, &headers, tenant_id, user_id, "pending_mfa").await?;
        return Ok((jar.add(cookie), Json(json!({ "mfa_required": true }))));
    }
    let cookie = start_session(&state, &headers, tenant_id, user_id, "active").await?;
    state.audit(tenant_id, Some(user_id), "auth.login", None, None, json!({})).await;
    Ok((jar.add(cookie), Json(json!({ "mfa_required": false, "user": load_me(&state, user_id).await? }))))
}

fn build_totp(secret_b32: &str, issuer: &str, account: &str) -> ApiResult<TOTP> {
    let bytes = Secret::Encoded(secret_b32.to_string())
        .to_bytes()
        .map_err(|e| anyhow::anyhow!("bad totp secret: {e:?}"))?;
    TOTP::new(Algorithm::SHA1, 6, 1, 30, bytes, Some(issuer.to_string()), account.to_string())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("totp init: {e:?}")))
}

fn totp_ok(totp: &TOTP, code: &str) -> bool {
    let code: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
    code.len() == 6 && totp.check_current(&code).unwrap_or(false)
}

#[derive(Deserialize)]
pub struct CodeRequest {
    pub code: String,
}

/// Second login step: exchange a pending-MFA session plus a valid TOTP code
/// for a fresh active session (new token, preventing session fixation).
pub async fn verify_totp(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<CodeRequest>,
) -> ApiResult<(CookieJar, Json<MeResponse>)> {
    let token = token_from_headers(&headers).ok_or(ApiError::Unauthorized)?;
    let hash = token_hash(&token);

    let row: Option<(Uuid, Uuid, i16, Option<String>, String, String)> = sqlx::query_as(
        "SELECT s.user_id, s.tenant_id, s.mfa_attempts, u.totp_secret, u.email::text, t.name
           FROM sessions s JOIN users u ON u.id = s.user_id JOIN tenants t ON t.id = s.tenant_id
          WHERE s.token_hash = $1 AND s.stage = 'pending_mfa' AND s.expires_at > now()",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;
    let (user_id, tenant_id, attempts, secret, email, tenant_name) = row.ok_or(ApiError::Unauthorized)?;
    let secret = secret.ok_or(ApiError::Unauthorized)?;
    // Wrong codes count across pending sessions, so re-logging in does not reset the budget.
    let mfa_key = format!("mfa:{user_id}");
    state.limiter.check(&mfa_key, state.cfg.login_account_limit, WINDOW_15_MIN)?;

    if !totp_ok(&build_totp(&secret, &tenant_name, &email)?, &req.code) {
        state.limiter.hit(&mfa_key, WINDOW_15_MIN);
        if attempts + 1 >= MAX_MFA_ATTEMPTS {
            sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(&hash).execute(&state.db).await?;
        } else {
            sqlx::query("UPDATE sessions SET mfa_attempts = mfa_attempts + 1 WHERE token_hash = $1")
                .bind(&hash)
                .execute(&state.db)
                .await?;
        }
        state.audit(tenant_id, Some(user_id), "auth.mfa_failed", None, None, json!({})).await;
        return Err(ApiError::Unauthorized);
    }

    state.limiter.reset(&mfa_key);
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(&hash).execute(&state.db).await?;
    let cookie = start_session(&state, &headers, tenant_id, user_id, "active").await?;
    state.audit(tenant_id, Some(user_id), "auth.login", None, None, json!({ "mfa": true })).await;
    Ok((jar.add(cookie), Json(load_me(&state, user_id).await?)))
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap, jar: CookieJar) -> ApiResult<(CookieJar, Json<Value>)> {
    if let Some(token) = token_from_headers(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(token_hash(&token)).execute(&state.db).await?;
    }
    Ok((jar.add(clear_cookie()), Json(json!({ "ok": true }))))
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<MeResponse>> {
    Ok(Json(load_me(&state, user.user_id).await?))
}

// ---------------------------------------------------------------------------
// TOTP enrolment
// ---------------------------------------------------------------------------

/// Step 1: generate a new secret (not yet active) and return the otpauth URL
/// for the authenticator app plus a PNG-free base32 secret for manual entry.
pub async fn totp_setup(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let enabled: bool = sqlx::query_scalar("SELECT totp_enabled FROM users WHERE id = $1")
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?;
    if enabled {
        return Err(ApiError::Conflict("two-factor authentication is already enabled".into()));
    }
    let secret = Secret::generate_secret().to_encoded().to_string();
    let tenant_name: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
        .bind(user.tenant_id)
        .fetch_one(&state.db)
        .await?;
    let totp = build_totp(&secret, &tenant_name, &user.email)?;
    sqlx::query("UPDATE users SET totp_secret = $1 WHERE id = $2")
        .bind(&secret)
        .bind(user.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "secret": secret, "otpauth_url": totp.get_url() })))
}

/// Step 2: confirm enrolment with a code from the authenticator app.
pub async fn totp_enable(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CodeRequest>,
) -> ApiResult<Json<Value>> {
    let (secret, tenant_name): (Option<String>, String) = sqlx::query_as(
        "SELECT u.totp_secret, t.name FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE u.id = $1",
    )
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    let secret = secret.ok_or_else(|| ApiError::BadRequest("call /auth/totp/setup first".into()))?;
    if !totp_ok(&build_totp(&secret, &tenant_name, &user.email)?, &req.code) {
        return bad_request("invalid code");
    }
    sqlx::query("UPDATE users SET totp_enabled = true WHERE id = $1").bind(user.user_id).execute(&state.db).await?;
    state.audit(user.tenant_id, Some(user.user_id), "auth.mfa_enabled", None, None, json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct DisableTotpRequest {
    pub password: String,
}

pub async fn totp_disable(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<DisableTotpRequest>,
) -> ApiResult<Json<Value>> {
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?;
    if !verify_password(req.password, hash).await {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("UPDATE users SET totp_enabled = false, totp_secret = NULL WHERE id = $1")
        .bind(user.user_id)
        .execute(&state.db)
        .await?;
    state.audit(user.tenant_id, Some(user.user_id), "auth.mfa_disabled", None, None, json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Change own password and revoke all other sessions.
pub async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<Value>> {
    validate_password(&req.new_password)?;
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?;
    if !verify_password(req.current_password, hash).await {
        return Err(ApiError::Forbidden);
    }
    let new_hash = hash_password(req.new_password).await?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2").bind(new_hash).bind(user.user_id).execute(&state.db).await?;
    // Revocation of other sessions is done by the caller re-logging in; here we drop all.
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(user.user_id).execute(&state.db).await?;
    state.audit(user.tenant_id, Some(user.user_id), "auth.password_changed", None, None, json!({})).await;
    Ok(Json(json!({ "ok": true, "relogin_required": true })))
}
