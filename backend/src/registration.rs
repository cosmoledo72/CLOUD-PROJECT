//! Self-registration in an existing portal.
//!
//! The administrator chooses the mode (`tenants.registration_mode`):
//! * `off`     — only invitations;
//! * `request` — anyone may apply, an administrator approves or rejects;
//! * `domain`  — confirmed addresses of the listed domains join at once,
//!   other addresses become requests.
//!
//! The applicant's address is confirmed by an emailed link when the portal can
//! send mail; without mail every application goes straight to the
//! administrators (and the `domain` shortcut is not available, since the
//! address cannot be verified).

use axum::{
    extract::{Path, Query, State},
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
    notify::{self, Note},
    ratelimit::{client_ip, WINDOW_1_HOUR},
    state::AppState,
};

/// Lifetime of the address confirmation link.
pub const CONFIRM_HOURS: i64 = 48;
pub const MODES: &[&str] = &["off", "request", "domain"];

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct Settings {
    registration_mode: String,
    registration_domains: Vec<String>,
    registration_role: String,
    registration_group_ids: Vec<Uuid>,
}

async fn load_settings(state: &AppState, tenant_id: Uuid) -> ApiResult<Settings> {
    Ok(sqlx::query_as(
        "SELECT registration_mode, registration_domains, registration_role, registration_group_ids FROM tenants WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&state.db)
    .await?)
}

/// `example.ru` from `@Example.RU `, `example.ru` or `user@example.ru`.
pub fn normalize_domain(d: &str) -> Option<String> {
    let d = d.trim().trim_end_matches('.').to_lowercase();
    let d = d.rsplit('@').next().unwrap_or("").to_string();
    let ok = d.len() <= 253
        && d.contains('.')
        && d.split('.').all(|p| !p.is_empty() && p.len() <= 63 && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || !c.is_ascii()));
    ok.then_some(d)
}

/// Domain part of an address, lower-cased.
pub fn email_domain(email: &str) -> String {
    email.rsplit('@').next().unwrap_or("").trim().to_lowercase()
}

pub async fn get_settings(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let s = load_settings(&state, user.tenant_id).await?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM registration_requests WHERE tenant_id = $1 AND status = 'pending'")
        .bind(user.tenant_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({
        "mode": s.registration_mode,
        "domains": s.registration_domains,
        "role": s.registration_role,
        "group_ids": s.registration_group_ids,
        "mail_available": mail::available(&state, user.tenant_id).await,
        "pending": pending,
    })))
}

#[derive(Deserialize)]
pub struct SaveSettings {
    pub mode: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
}

fn default_role() -> String {
    "member".into()
}

/// Groups that belong to the portal (unknown ids are an error).
pub async fn check_groups(state: &AppState, tenant_id: Uuid, ids: &[Uuid]) -> ApiResult<Vec<Uuid>> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM groups WHERE tenant_id = $1 AND id = ANY($2)")
        .bind(tenant_id)
        .bind(&ids)
        .fetch_one(&state.db)
        .await?;
    if n as usize != ids.len() {
        return bad_request("группа не найдена");
    }
    Ok(ids)
}

pub async fn save_settings(State(state): State<AppState>, user: CurrentUser, Json(req): Json<SaveSettings>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    if !MODES.contains(&req.mode.as_str()) {
        return bad_request("unknown registration mode");
    }
    if !matches!(req.role.as_str(), "member" | "guest") {
        return bad_request("role must be member or guest");
    }
    let mut domains = Vec::new();
    for d in &req.domains {
        if d.trim().is_empty() {
            continue;
        }
        let n = normalize_domain(d).ok_or_else(|| ApiError::BadRequest(format!("неверный домен: {}", d.trim())))?;
        if !domains.contains(&n) {
            domains.push(n);
        }
    }
    if req.mode == "domain" && domains.is_empty() {
        return bad_request("укажите хотя бы один домен почты");
    }
    if domains.len() > 50 {
        return bad_request("не более 50 доменов");
    }
    let groups = check_groups(&state, user.tenant_id, &req.group_ids).await?;
    sqlx::query(
        "UPDATE tenants SET registration_mode = $2, registration_domains = $3, registration_role = $4, registration_group_ids = $5
          WHERE id = $1",
    )
    .bind(user.tenant_id)
    .bind(&req.mode)
    .bind(&domains)
    .bind(&req.role)
    .bind(&groups)
    .execute(&state.db)
    .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "portal.registration", Some("tenant"), Some(user.tenant_id),
               json!({ "mode": req.mode, "domains": domains, "role": req.role, "groups": groups.len() }))
        .await;
    get_settings(State(state), user).await
}

/// Is registration open in the portal (for the public login page)?
pub async fn is_open(state: &AppState, slug: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT registration_mode <> 'off' FROM tenants WHERE slug = $1 AND status = 'active'")
        .bind(slug.trim())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Public: apply and confirm the address
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RegisterInput {
    pub tenant: String,
    pub display_name: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub comment: String,
    /// Honeypot: a field hidden from people. Bots fill it in.
    #[serde(default)]
    pub website: String,
}

/// What the applicant is told to do next. It never depends on whether the
/// address already has an account, so the form cannot be used to probe.
fn next_step(mail: bool) -> Json<Value> {
    Json(json!({ "ok": true, "next": if mail { "confirm_email" } else { "pending" } }))
}

pub async fn register(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<RegisterInput>) -> ApiResult<Json<Value>> {
    let slug = req.tenant.trim().to_lowercase();
    let email = req.email.trim().to_string();
    let ip = client_ip(&headers);
    let ip_key = format!("register-ip:{ip}");
    let account_key = format!("register:{slug}:{}", email.to_lowercase());
    // Same per-IP budget as creating a portal (SIGNUP_IP_LIMIT, 10 an hour by default).
    state.limiter.check(&ip_key, state.cfg.signup_ip_limit, WINDOW_1_HOUR)?;
    state.limiter.check(&account_key, 3, WINDOW_1_HOUR)?;

    let tenant: Option<(Uuid, String)> = sqlx::query_as("SELECT id, registration_mode FROM tenants WHERE slug = $1 AND status = 'active'")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await?;
    let Some((tenant_id, mode)) = tenant else {
        return bad_request("портал не найден");
    };
    if mode == "off" {
        return bad_request("регистрация в этом портале закрыта — попросите администратора прислать приглашение");
    }
    let name = req.display_name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > 200 {
        return bad_request("укажите имя (до 200 символов)");
    }
    if email.parse::<lettre::Address>().is_err() {
        return bad_request("неверный адрес почты");
    }
    validate_password(&req.password)?;
    let comment: String = req.comment.trim().chars().take(1000).collect();
    state.limiter.hit(&ip_key, WINDOW_1_HOUR);
    state.limiter.hit(&account_key, WINDOW_1_HOUR);
    let mail_ok = mail::available(&state, tenant_id).await;

    if !req.website.trim().is_empty() {
        tracing::info!(%ip, "registration honeypot filled, ignored");
        return Ok(next_step(mail_ok));
    }

    // An existing account: tell its owner by mail, answer as usual.
    let existing: Option<(String, bool)> =
        sqlx::query_as("SELECT display_name, invite_pending FROM users WHERE tenant_id = $1 AND email = $2::citext")
            .bind(tenant_id)
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    if let Some((existing_name, pending)) = existing {
        if mail_ok {
            let portal = mail::portal_name(&state, tenant_id).await?;
            let login = format!("{}/login?portal={slug}", state.cfg.public_url);
            let m = mail::render(
                &email,
                &portal,
                "Вы уже зарегистрированы",
                &[
                    format!("Здравствуйте, {existing_name}!"),
                    if pending {
                        format!("Для этого адреса в портале {} уже есть приглашение. Найдите письмо с приглашением или попросите администратора отправить его ещё раз.", mail::quoted(&portal))
                    } else {
                        format!("С этим адресом уже есть учётная запись в портале {}. Войдите в портал; если не помните пароль, нажмите «Забыли пароль?» на странице входа.", mail::quoted(&portal))
                    },
                ],
                Some(("Войти", &login)),
                "Если вы не пытались зарегистрироваться, просто проигнорируйте это письмо.",
            );
            mail::enqueue(&state, tenant_id, "register_exists", &m).await?;
        }
        return Ok(next_step(mail_ok));
    }

    let hash = hash_password(req.password).await?;
    let (token, token_h) = new_token();
    let status = if mail_ok { "unconfirmed" } else { "pending" };
    // Clean up stale unconfirmed applications, then replace an open one for this address.
    sqlx::query("DELETE FROM registration_requests WHERE status = 'unconfirmed' AND confirm_expires_at < now() - interval '1 day'")
        .execute(&state.db)
        .await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM registration_requests WHERE tenant_id = $1 AND email = $2::citext AND status IN ('unconfirmed', 'pending')")
        .bind(tenant_id)
        .bind(&email)
        .execute(&mut *tx)
        .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO registration_requests (tenant_id, email, display_name, password_hash, comment, status, confirm_token_hash,
                                            confirm_expires_at, ip)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now() + make_interval(hours => $8), $9) RETURNING id",
    )
    .bind(tenant_id)
    .bind(&email)
    .bind(&name)
    .bind(&hash)
    .bind(&comment)
    .bind(status)
    .bind(mail_ok.then_some(token_h))
    .bind(CONFIRM_HOURS as i32)
    .bind(&ip)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    state
        .audit(tenant_id, None, "registration.applied", Some("registration_request"), Some(id), json!({ "email": email, "emailed": mail_ok }))
        .await;

    if mail_ok {
        let portal = mail::portal_name(&state, tenant_id).await?;
        let url = format!("{}/register/confirm/{token}", state.cfg.public_url);
        let m = mail::render(
            &email,
            &portal,
            "Подтвердите адрес почты",
            &[
                format!("Здравствуйте, {name}!"),
                format!("Вы подали заявку на регистрацию в портале {}. Подтвердите, что это ваш адрес, — нажмите кнопку ниже.", mail::quoted(&portal)),
            ],
            Some(("Подтвердить адрес", &url)),
            &format!("Ссылка действует {CONFIRM_HOURS} часов. Если вы не регистрировались, просто проигнорируйте это письмо."),
        );
        mail::enqueue(&state, tenant_id, "register_confirm", &m).await?;
    } else {
        notify_admins(&state, tenant_id, id, &name, &email).await;
    }
    Ok(next_step(mail_ok))
}

#[derive(Deserialize)]
pub struct ConfirmInput {
    pub token: String,
}

pub async fn confirm(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<ConfirmInput>) -> ApiResult<Json<Value>> {
    let key = format!("register-confirm-ip:{}", client_ip(&headers));
    state.limiter.check(&key, 30, WINDOW_1_HOUR)?;
    state.limiter.hit(&key, WINDOW_1_HOUR);
    let row: Option<(Uuid, Uuid, String, String, String, String, String)> = sqlx::query_as(
        "SELECT r.id, r.tenant_id, r.email::text, r.display_name, t.slug::text, t.registration_mode,
                COALESCE(NULLIF(b.display_name, ''), t.name)
           FROM registration_requests r
           JOIN tenants t ON t.id = r.tenant_id
           LEFT JOIN tenant_branding b ON b.tenant_id = t.id
          WHERE r.confirm_token_hash = $1 AND r.status = 'unconfirmed' AND r.confirm_expires_at > now() AND t.status = 'active'",
    )
    .bind(token_hash(req.token.trim()))
    .fetch_optional(&state.db)
    .await?;
    let Some((id, tenant_id, email, name, slug, mode, portal)) = row else {
        return bad_request("ссылка недействительна или устарела");
    };
    let s = load_settings(&state, tenant_id).await?;
    let auto = mode == "domain" && s.registration_domains.contains(&email_domain(&email));
    let n = sqlx::query(
        "UPDATE registration_requests SET email_confirmed = true, confirm_token_hash = NULL, status = 'pending'
          WHERE id = $1 AND status = 'unconfirmed'",
    )
    .bind(id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if n == 0 {
        return bad_request("ссылка недействительна или устарела");
    }
    if mode == "off" {
        // Closed after the application: it waits for an administrator like any other.
        notify_admins(&state, tenant_id, id, &name, &email).await;
        return Ok(Json(json!({ "status": "pending", "tenant_slug": slug, "tenant_name": portal })));
    }
    if auto {
        let user_id = create_user_from(&state, tenant_id, id, None, &s.registration_role, &s.registration_group_ids).await?;
        state
            .audit(tenant_id, Some(user_id), "registration.joined", Some("user"), Some(user_id), json!({ "by": "domain" }))
            .await;
        return Ok(Json(json!({ "status": "active", "tenant_slug": slug, "tenant_name": portal, "email": email })));
    }
    notify_admins(&state, tenant_id, id, &name, &email).await;
    Ok(Json(json!({ "status": "pending", "tenant_slug": slug, "tenant_name": portal })))
}

async fn notify_admins(state: &AppState, tenant_id: Uuid, request_id: Uuid, name: &str, email: &str) {
    let admins: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND role IN ('owner', 'admin') AND is_active")
        .bind(tenant_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    notify::notify(
        state,
        tenant_id,
        None,
        &admins,
        Note {
            kind: "registration_request",
            title: format!("Заявка на регистрацию: {name}"),
            body: email.to_string(),
            link: "/admin/users?tab=requests".into(),
            entity_type: Some("registration_request"),
            entity_id: Some(request_id),
            dedupe_key: Some(request_id.to_string()),
        },
    )
    .await;
}

/// Turn a request into an account (in one transaction with the status change).
async fn create_user_from(
    state: &AppState,
    tenant_id: Uuid,
    request_id: Uuid,
    decided_by: Option<Uuid>,
    role: &str,
    group_ids: &[Uuid],
) -> ApiResult<Uuid> {
    let mut tx = state.db.begin().await?;
    let (email, name, hash): (String, String, String) = sqlx::query_as(
        "SELECT email::text, display_name, password_hash FROM registration_requests
          WHERE id = $1 AND tenant_id = $2 AND status = 'pending' FOR UPDATE",
    )
    .bind(request_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::BadRequest("заявка уже рассмотрена".into()))?;
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, email, display_name, password_hash, role) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(&email)
    .bind(&name)
    .bind(&hash)
    .bind(role)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("пользователь с этим адресом уже есть в портале".into()),
        other => other,
    })?;
    sqlx::query(
        "INSERT INTO group_members (group_id, user_id)
         SELECT g.id, $2 FROM groups g WHERE g.tenant_id = $3 AND g.id = ANY($1) ON CONFLICT DO NOTHING",
    )
    .bind(group_ids)
    .bind(user_id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE registration_requests SET status = 'approved', decided_by = $2, decided_at = now(), user_id = $3, password_hash = ''
          WHERE id = $1",
    )
    .bind(request_id)
    .bind(decided_by)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(user_id)
}

// ---------------------------------------------------------------------------
// Admin: requests
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListQuery {
    /// `pending` (default) or `all` (also decided in the last 90 days and unconfirmed).
    pub status: Option<String>,
}

pub async fn list_requests(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let all = q.status.as_deref() == Some("all");
    let rows: Vec<(Uuid, String, String, String, String, bool, chrono::DateTime<chrono::Utc>, Option<String>, Option<chrono::DateTime<chrono::Utc>>, String)> =
        sqlx::query_as(
            "SELECT r.id, r.email::text, r.display_name, r.comment, r.status, r.email_confirmed, r.created_at,
                    d.display_name, r.decided_at, r.reject_reason
               FROM registration_requests r LEFT JOIN users d ON d.id = r.decided_by
              WHERE r.tenant_id = $1
                AND (r.status = 'pending' OR ($2 AND (r.status = 'unconfirmed' OR r.decided_at > now() - interval '90 days')))
              ORDER BY (r.status = 'pending') DESC, r.created_at DESC
              LIMIT 500",
        )
        .bind(user.tenant_id)
        .bind(all)
        .fetch_all(&state.db)
        .await?;
    let s = load_settings(&state, user.tenant_id).await?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, email, name, comment, status, confirmed, created, by, at, reason)| {
            json!({ "id": id, "email": email, "display_name": name, "comment": comment, "status": status,
                    "email_confirmed": confirmed, "created_at": created, "decided_by_name": by, "decided_at": at,
                    "reject_reason": reason, "domain_match": s.registration_domains.contains(&email_domain(&email)) })
        })
        .collect();
    Ok(Json(json!({ "requests": items, "default_role": s.registration_role, "default_group_ids": s.registration_group_ids })))
}

#[derive(Deserialize)]
pub struct ApproveInput {
    pub role: Option<String>,
    pub group_ids: Option<Vec<Uuid>>,
}

pub async fn approve(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ApproveInput>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let s = load_settings(&state, user.tenant_id).await?;
    let role = req.role.unwrap_or(s.registration_role);
    if !matches!(role.as_str(), "member" | "guest") {
        return bad_request("role must be member or guest");
    }
    let groups = check_groups(&state, user.tenant_id, &req.group_ids.unwrap_or(s.registration_group_ids)).await?;
    let user_id = create_user_from(&state, user.tenant_id, id, Some(user.user_id), &role, &groups).await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "registration.approved", Some("user"), Some(user_id), json!({ "role": role, "request": id }))
        .await;
    let (email, name): (String, String) = sqlx::query_as("SELECT email::text, display_name FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    let mut emailed = false;
    if mail::available(&state, user.tenant_id).await {
        let portal = mail::portal_name(&state, user.tenant_id).await?;
        let slug: String = sqlx::query_scalar("SELECT slug::text FROM tenants WHERE id = $1").bind(user.tenant_id).fetch_one(&state.db).await?;
        let login = format!("{}/login?portal={slug}", state.cfg.public_url);
        let m = mail::render(
            &email,
            &portal,
            "Заявка одобрена",
            &[
                format!("Здравствуйте, {name}!"),
                format!("Администратор одобрил вашу заявку: теперь вы можете войти в портал {} с адресом {email} и паролем, который указали при регистрации.", mail::quoted(&portal)),
            ],
            Some(("Войти в портал", &login)),
            "",
        );
        emailed = mail::enqueue(&state, user.tenant_id, "register_approved", &m).await.is_ok();
    }
    Ok(Json(json!({ "user_id": user_id, "emailed": emailed })))
}

#[derive(Deserialize)]
pub struct RejectInput {
    #[serde(default)]
    pub reason: String,
    /// Tell the applicant by mail (default yes).
    #[serde(default = "yes")]
    pub notify: bool,
}

fn yes() -> bool {
    true
}

pub async fn reject(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<RejectInput>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let reason: String = req.reason.trim().chars().take(1000).collect();
    let row: Option<(String, String)> = sqlx::query_as(
        "UPDATE registration_requests
            SET status = 'rejected', decided_by = $3, decided_at = now(), reject_reason = $4, password_hash = '', confirm_token_hash = NULL
          WHERE id = $1 AND tenant_id = $2 AND status IN ('pending', 'unconfirmed')
          RETURNING email::text, display_name",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(&reason)
    .fetch_optional(&state.db)
    .await?;
    let Some((email, name)) = row else {
        return bad_request("заявка уже рассмотрена");
    };
    state
        .audit(user.tenant_id, Some(user.user_id), "registration.rejected", Some("registration_request"), Some(id), json!({ "email": email }))
        .await;
    let mut emailed = false;
    if req.notify && mail::available(&state, user.tenant_id).await {
        let portal = mail::portal_name(&state, user.tenant_id).await?;
        let mut paragraphs = vec![
            format!("Здравствуйте, {name}!"),
            format!("К сожалению, заявка на регистрацию в портале {} отклонена.", mail::quoted(&portal)),
        ];
        if !reason.is_empty() {
            paragraphs.push(format!("Комментарий администратора: {reason}"));
        }
        let m = mail::render(&email, &portal, "Заявка на регистрацию", &paragraphs, None, "");
        emailed = mail::enqueue(&state, user.tenant_id, "register_rejected", &m).await.is_ok();
    }
    Ok(Json(json!({ "ok": true, "emailed": emailed })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_normalized() {
        assert_eq!(normalize_domain(" @Partner-Garant.RU ").as_deref(), Some("partner-garant.ru"));
        assert_eq!(normalize_domain("ivan@mail.example.com").as_deref(), Some("mail.example.com"));
        assert_eq!(normalize_domain("пг.рф").as_deref(), Some("пг.рф"));
        assert!(normalize_domain("localhost").is_none());
        assert!(normalize_domain("a..ru").is_none());
        assert!(normalize_domain("a b.ru").is_none());
        assert_eq!(email_domain("Ivan@Partner-Garant.ru"), "partner-garant.ru");
    }
}
