//! One-time links sent by email: invitations of new users and password reset.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{hash_password, new_token, token_hash, validate_password, CurrentUser},
    error::{bad_request, ApiError, ApiResult},
    mail,
    ratelimit::{client_ip, WINDOW_1_HOUR},
    state::AppState,
};

/// Invitation lifetime.
pub const INVITE_DAYS: i64 = 7;
/// Password reset link lifetime.
pub const RESET_MINUTES: i64 = 60;
/// Placeholder hash of invited users: never verifies.
pub const NO_PASSWORD: &str = "!";

async fn issue_token(state: &AppState, tenant_id: Uuid, user_id: Uuid, kind: &str, ttl: &str, created_by: Option<Uuid>) -> ApiResult<String> {
    // A new link replaces older unused links of the same kind.
    sqlx::query("DELETE FROM user_tokens WHERE user_id = $1 AND kind = $2 AND used_at IS NULL")
        .bind(user_id)
        .bind(kind)
        .execute(&state.db)
        .await?;
    let (token, hash) = new_token();
    sqlx::query(
        "INSERT INTO user_tokens (token_hash, tenant_id, user_id, kind, created_by, expires_at)
         VALUES ($1, $2, $3, $4, $5, now() + $6::interval)",
    )
    .bind(hash)
    .bind(tenant_id)
    .bind(user_id)
    .bind(kind)
    .bind(created_by)
    .bind(ttl)
    .execute(&state.db)
    .await?;
    Ok(token)
}

// ---------------------------------------------------------------------------
// Invitations
// ---------------------------------------------------------------------------

/// Create an invitation link for a pending user and queue the email.
/// Returns the link (shown to the admin in case mail is not configured) and
/// whether an email was queued.
pub async fn send_invite(state: &AppState, admin: &CurrentUser, user_id: Uuid) -> ApiResult<(String, bool)> {
    let (email, name): (String, String) = sqlx::query_as(
        "SELECT email::text, display_name FROM users WHERE id = $1 AND tenant_id = $2 AND invite_pending",
    )
    .bind(user_id)
    .bind(admin.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::BadRequest("пользователь уже принял приглашение".into()))?;
    let token = issue_token(state, admin.tenant_id, user_id, "invite", &format!("{INVITE_DAYS} days"), Some(admin.user_id)).await?;
    let url = format!("{}/invite/{token}", state.cfg.public_url);
    let can_send = mail::available(state, admin.tenant_id).await;
    if can_send {
        let portal = mail::portal_name(state, admin.tenant_id).await?;
        let m = mail::render(
            &email,
            &portal,
            &format!("Приглашение в портал {}", mail::quoted(&portal)),
            &[
                format!("Здравствуйте, {name}!"),
                format!("{} приглашает вас в рабочее пространство {}: документы, проекты и задачи команды.", admin.display_name, mail::quoted(&portal)),
                "Чтобы присоединиться, задайте пароль по ссылке ниже. Логином будет этот адрес электронной почты.".into(),
            ],
            Some(("Принять приглашение", &url)),
            &format!("Ссылка действует {INVITE_DAYS} дней и может быть использована один раз."),
        );
        mail::enqueue(state, admin.tenant_id, "invite", &m).await?;
    }
    state
        .audit(admin.tenant_id, Some(admin.user_id), "user.invited", Some("user"), Some(user_id), json!({ "emailed": can_send }))
        .await;
    Ok((url, can_send))
}

/// Re-send the invitation of a user who has not joined yet.
pub async fn resend_invite(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let (url, emailed) = send_invite(&state, &user, id).await?;
    Ok(Json(json!({ "invite_url": url, "emailed": emailed })))
}

#[derive(Deserialize)]
pub struct TokenInput {
    pub token: String,
}

struct TokenInfo {
    user_id: Uuid,
    tenant_id: Uuid,
    email: String,
    display_name: String,
    tenant_slug: String,
    tenant_name: String,
}

async fn lookup(state: &AppState, token: &str, kind: &str) -> ApiResult<TokenInfo> {
    let row: Option<(Uuid, Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.tenant_id, u.email::text, u.display_name, t.slug::text, COALESCE(NULLIF(b.display_name, ''), t.name)
           FROM user_tokens k
           JOIN users u ON u.id = k.user_id
           JOIN tenants t ON t.id = k.tenant_id
           LEFT JOIN tenant_branding b ON b.tenant_id = t.id
          WHERE k.token_hash = $1 AND k.kind = $2 AND k.used_at IS NULL AND k.expires_at > now()
            AND u.is_active AND t.status = 'active'",
    )
    .bind(token_hash(token.trim()))
    .bind(kind)
    .fetch_optional(&state.db)
    .await?;
    let (user_id, tenant_id, email, display_name, tenant_slug, tenant_name) =
        row.ok_or_else(|| ApiError::BadRequest("ссылка недействительна или устарела".into()))?;
    Ok(TokenInfo { user_id, tenant_id, email, display_name, tenant_slug, tenant_name })
}

/// Mark the token used; fails if another request consumed it first.
async fn consume(tx: &mut sqlx::PgConnection, token: &str) -> ApiResult<()> {
    let n = sqlx::query("UPDATE user_tokens SET used_at = now() WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()")
        .bind(token_hash(token.trim()))
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return bad_request("ссылка недействительна или устарела");
    }
    Ok(())
}

fn throttle(state: &AppState, headers: &HeaderMap, scope: &str) -> ApiResult<()> {
    let key = format!("{scope}-ip:{}", client_ip(headers));
    state.limiter.check(&key, 30, WINDOW_1_HOUR)?;
    state.limiter.hit(&key, WINDOW_1_HOUR);
    Ok(())
}

pub async fn invite_info(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<TokenInput>) -> ApiResult<Json<Value>> {
    throttle(&state, &headers, "token-check")?;
    let t = lookup(&state, &req.token, "invite").await?;
    Ok(Json(json!({
        "email": t.email, "display_name": t.display_name, "tenant_slug": t.tenant_slug, "tenant_name": t.tenant_name,
    })))
}

#[derive(Deserialize)]
pub struct AcceptInvite {
    pub token: String,
    pub password: String,
    pub display_name: Option<String>,
}

pub async fn accept_invite(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<AcceptInvite>) -> ApiResult<Json<Value>> {
    throttle(&state, &headers, "token-use")?;
    let t = lookup(&state, &req.token, "invite").await?;
    validate_password(&req.password)?;
    let name = req.display_name.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&t.display_name).to_string();
    if name.chars().count() > 200 {
        return bad_request("слишком длинное имя");
    }
    let hash = hash_password(req.password).await?;
    let mut tx = state.db.begin().await?;
    consume(&mut tx, &req.token).await?;
    sqlx::query("UPDATE users SET password_hash = $2, display_name = $3, invite_pending = false WHERE id = $1")
        .bind(t.user_id)
        .bind(hash)
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.audit(t.tenant_id, Some(t.user_id), "user.invite_accepted", Some("user"), Some(t.user_id), json!({})).await;
    Ok(Json(json!({ "tenant_slug": t.tenant_slug, "email": t.email })))
}

// ---------------------------------------------------------------------------
// Password reset
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ForgotInput {
    pub tenant: String,
    pub email: String,
}

/// Always answers the same way, so it cannot be used to find out which
/// accounts exist; the lookup and the email happen in the background.
pub async fn forgot(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<ForgotInput>) -> ApiResult<Json<Value>> {
    let tenant = req.tenant.trim().to_lowercase();
    let email = req.email.trim().to_string();
    let ip_key = format!("forgot-ip:{}", client_ip(&headers));
    let account_key = format!("forgot:{tenant}:{}", email.to_lowercase());
    state.limiter.check(&ip_key, 10, WINDOW_1_HOUR)?;
    state.limiter.check(&account_key, 3, WINDOW_1_HOUR)?;
    state.limiter.hit(&ip_key, WINDOW_1_HOUR);
    state.limiter.hit(&account_key, WINDOW_1_HOUR);
    let st = state.clone();
    tokio::spawn(async move {
        if let Err(e) = issue_reset(&st, &tenant, &email).await {
            tracing::warn!(error = ?e, "password reset request failed");
        }
    });
    Ok(Json(json!({ "ok": true })))
}

async fn issue_reset(state: &AppState, tenant: &str, email: &str) -> ApiResult<()> {
    let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT u.id, u.tenant_id, u.email::text, u.display_name FROM users u JOIN tenants t ON t.id = u.tenant_id
          WHERE t.slug = $1 AND u.email = $2 AND u.is_active AND NOT u.invite_pending AND t.status = 'active'",
    )
    .bind(tenant)
    .bind(email)
    .fetch_optional(&state.db)
    .await?;
    let Some((user_id, tenant_id, email, name)) = row else { return Ok(()) };
    if !mail::available(state, tenant_id).await {
        state.audit(tenant_id, Some(user_id), "auth.reset_requested", None, None, json!({ "emailed": false })).await;
        return Ok(());
    }
    let token = issue_token(state, tenant_id, user_id, "reset", &format!("{RESET_MINUTES} minutes"), None).await?;
    let url = format!("{}/reset/{token}", state.cfg.public_url);
    let portal = mail::portal_name(state, tenant_id).await?;
    let m = mail::render(
        &email,
        &portal,
        "Восстановление пароля",
        &[
            format!("Здравствуйте, {name}!"),
            format!("Для вашей учётной записи в портале {} запрошена смена пароля. Задайте новый пароль по ссылке ниже.", mail::quoted(&portal)),
        ],
        Some(("Задать новый пароль", &url)),
        &format!(
            "Ссылка действует {RESET_MINUTES} минут. Если вы не запрашивали смену пароля, просто проигнорируйте это письмо — пароль останется прежним."
        ),
    );
    mail::enqueue(state, tenant_id, "reset", &m).await?;
    state.audit(tenant_id, Some(user_id), "auth.reset_requested", None, None, json!({ "emailed": true })).await;
    Ok(())
}

pub async fn reset_info(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<TokenInput>) -> ApiResult<Json<Value>> {
    throttle(&state, &headers, "token-check")?;
    let t = lookup(&state, &req.token, "reset").await?;
    Ok(Json(json!({ "email": t.email, "tenant_slug": t.tenant_slug, "tenant_name": t.tenant_name })))
}

#[derive(Deserialize)]
pub struct ResetInput {
    pub token: String,
    pub password: String,
}

pub async fn reset_password(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<ResetInput>) -> ApiResult<Json<Value>> {
    throttle(&state, &headers, "token-use")?;
    let t = lookup(&state, &req.token, "reset").await?;
    validate_password(&req.password)?;
    let hash = hash_password(req.password).await?;
    let mut tx = state.db.begin().await?;
    consume(&mut tx, &req.token).await?;
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1").bind(t.user_id).bind(hash).execute(&mut *tx).await?;
    // Sign out everywhere: whoever knew the old password loses access.
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(t.user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM user_tokens WHERE user_id = $1 AND kind = 'reset' AND used_at IS NULL")
        .bind(t.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.audit(t.tenant_id, Some(t.user_id), "auth.password_reset_by_link", None, None, json!({})).await;
    // Let the owner know, in case the change was not theirs.
    if let Ok(portal) = mail::portal_name(&state, t.tenant_id).await {
        let m = mail::render(
            &t.email,
            &portal,
            "Пароль изменён",
            &[
                format!("Здравствуйте, {}!", t.display_name),
                format!("Пароль вашей учётной записи в портале {} только что изменён по ссылке восстановления. Все сеансы завершены.", mail::quoted(&portal)),
                "Если это были не вы, срочно сообщите администратору портала.".into(),
            ],
            None,
            "",
        );
        let _ = mail::enqueue(&state, t.tenant_id, "password_changed", &m).await;
    }
    Ok(Json(json!({ "tenant_slug": t.tenant_slug, "email": t.email })))
}
