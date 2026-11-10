//! Authentication: password hashing, opaque session tokens and the
//! `CurrentUser` extractor.

pub mod handlers;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

pub const SESSION_COOKIE: &str = "wh_sid";

/// Authenticated user of the current request.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CurrentUser {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    /// CRM access: none | user | manager.
    pub crm_role: String,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        matches!(self.role.as_str(), "owner" | "admin")
    }
    pub fn is_guest(&self) -> bool {
        self.role == "guest"
    }
    /// May use the CRM at all (guests never can).
    pub fn crm_enabled(&self) -> bool {
        !self.is_guest() && self.crm_role != "none"
    }
    /// Sees every CRM record and manages pipeline settings.
    pub fn crm_manager(&self) -> bool {
        self.crm_enabled() && (self.is_admin() || self.crm_role == "manager")
    }
    pub fn require_crm(&self) -> ApiResult<()> {
        if self.crm_enabled() { Ok(()) } else { Err(ApiError::Forbidden) }
    }
    pub fn require_admin(&self) -> ApiResult<()> {
        if self.is_admin() { Ok(()) } else { Err(ApiError::Forbidden) }
    }
}

// ---------------------------------------------------------------------------
// Passwords (Argon2id). Hashing is CPU-heavy, so it runs on the blocking pool.
// ---------------------------------------------------------------------------

pub async fn hash_password(password: String) -> ApiResult<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| anyhow::anyhow!("hash failure: {e}"))
    })
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .map_err(ApiError::Internal)
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// A real Argon2 hash used to equalise timing for unknown accounts.
pub async fn dummy_hash() -> String {
    static HASH: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    HASH.get_or_init(|| async { hash_password("timing-equaliser".into()).await.unwrap_or_default() })
        .await
        .clone()
}

/// Basic password policy; extend per tenant settings later.
pub fn validate_password(p: &str) -> ApiResult<()> {
    if p.chars().count() < 10 {
        return Err(ApiError::BadRequest("password must be at least 10 characters".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Session tokens
// ---------------------------------------------------------------------------

/// Generate a random 256-bit token (hex) and its SHA-256 hash for storage.
pub fn new_token() -> (String, Vec<u8>) {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let hash = token_hash(&token);
    (token, hash)
}

pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub fn session_cookie(token: String, secure: bool, max_age_hours: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .expires(time_duration_hours(max_age_hours))
        .build()
}

pub fn clear_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, "")).path("/").expires(time_duration_hours(0)).build()
}

fn time_duration_hours(h: i64) -> axum_extra::extract::cookie::Expiration {
    let at = std::time::SystemTime::now() + std::time::Duration::from_secs((h.max(0) as u64) * 3600);
    axum_extra::extract::cookie::Expiration::DateTime(at.into())
}

/// Extract the raw session token from the cookie or an `Authorization: Bearer` header
/// (the latter is used by the desktop client and API integrations).
pub fn token_from_parts(parts: &Parts) -> Option<String> {
    token_from_headers(&parts.headers)
}

pub fn token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(h) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Some(t) = h.to_str().ok().and_then(|v| v.strip_prefix("Bearer ")) {
            return Some(t.trim().to_string());
        }
    }
    CookieJar::from_headers(headers)
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .filter(|v| !v.is_empty())
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = token_from_parts(parts).ok_or(ApiError::Unauthorized)?;
        let user = sqlx::query_as::<_, CurrentUser>(
            "SELECT u.id AS user_id, u.tenant_id, u.email::text AS email, u.display_name, u.role, u.crm_role
               FROM sessions s
               JOIN users u   ON u.id = s.user_id
               JOIN tenants t ON t.id = s.tenant_id
              WHERE s.token_hash = $1
                AND s.stage = 'active'
                AND s.expires_at > now()
                AND u.is_active
                AND t.status = 'active'",
        )
        .bind(token_hash(&token))
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
        Ok(user)
    }
}

/// Random UUID-like id without hyphens, used for ONLYOFFICE document keys.
pub fn random_key() -> String {
    Uuid::new_v4().simple().to_string()
}
