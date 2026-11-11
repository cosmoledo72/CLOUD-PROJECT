//! One-time links sent by email: invitations of new users and password reset.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
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
        let details = invite_details(state, user_id).await?;
        let inviter = details.invited_by_name.clone().unwrap_or_else(|| admin.display_name.clone());
        let mut paragraphs = vec![
            format!("Здравствуйте, {name}!"),
            format!("{inviter} приглашает вас в рабочее пространство {}: документы, проекты и задачи команды.", mail::quoted(&portal)),
        ];
        if !details.message.is_empty() {
            paragraphs.push(format!("Сообщение от {inviter}: «{}»", details.message));
        }
        if !details.projects.is_empty() {
            let list: Vec<String> = details.projects.iter().map(|p| mail::quoted(&p.name)).collect();
            paragraphs.push(format!(
                "{} {}.",
                if list.len() == 1 { "Вас сразу добавят в проект" } else { "Вас сразу добавят в проекты" },
                list.join(", ")
            ));
        }
        paragraphs.push("Чтобы присоединиться, задайте пароль по ссылке ниже. Логином будет этот адрес электронной почты.".into());
        let m = mail::render(
            &email,
            &portal,
            &format!("Приглашение в портал {}", mail::quoted(&portal)),
            &paragraphs,
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

#[derive(Serialize, Clone)]
pub struct InvitedProject {
    pub id: Uuid,
    pub name: String,
    pub role: String,
}

pub struct InviteDetails {
    pub invited_by_name: Option<String>,
    pub message: String,
    pub projects: Vec<InvitedProject>,
    pub groups: Vec<String>,
}

/// Who invited the user, the personal message, and where the user is going.
pub async fn invite_details(state: &AppState, user_id: Uuid) -> ApiResult<InviteDetails> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT u.display_name, i.message FROM invitations i LEFT JOIN users u ON u.id = i.invited_by WHERE i.user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;
    let (invited_by_name, message) = row.unwrap_or((None, String::new()));
    let projects: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT p.id, p.name, pm.role FROM project_members pm JOIN projects p ON p.id = pm.project_id
          WHERE pm.user_id = $1 ORDER BY p.name",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let groups: Vec<String> =
        sqlx::query_scalar("SELECT g.name FROM group_members gm JOIN groups g ON g.id = gm.group_id WHERE gm.user_id = $1 ORDER BY g.name")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;
    Ok(InviteDetails {
        invited_by_name,
        message,
        projects: projects.into_iter().map(|(id, name, role)| InvitedProject { id, name, role }).collect(),
        groups,
    })
}

// ---------------------------------------------------------------------------
// Invitations: several addresses at once, with groups, projects and a message
// ---------------------------------------------------------------------------

/// One invitee parsed from the free-form list.
#[derive(Debug, PartialEq)]
pub struct Invitee {
    pub email: String,
    pub name: Option<String>,
}

/// Parse «Иван Петров <ivan@x.ru>», «ivan@x.ru Иван Петров» or «ivan@x.ru»,
/// one per line or separated by commas / semicolons.
pub fn parse_invitees(text: &str) -> Vec<Result<Invitee, String>> {
    text.split(['\n', ',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (email, name) = if let (Some(a), Some(b)) = (item.find('<'), item.rfind('>')) {
                (item[a + 1..b].trim().to_string(), format!("{} {}", &item[..a], &item[b + 1..]))
            } else {
                let mut email = String::new();
                let mut rest = Vec::new();
                for w in item.split_whitespace() {
                    if email.is_empty() && w.contains('@') {
                        email = w.trim_matches(|c: char| c == '"' || c == '\'' || c == '(' || c == ')').to_string();
                    } else {
                        rest.push(w);
                    }
                }
                (email, rest.join(" "))
            };
            let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            let name = name.trim_matches(|c: char| c == '"' || c == '\'').trim().to_string();
            if email.parse::<lettre::Address>().is_err() {
                return Err(item.to_string());
            }
            Ok(Invitee { email, name: (!name.is_empty()).then_some(name) })
        })
        .collect()
}

/// «ivan.petrov@x.ru» → «Ivan Petrov» — a placeholder the invitee can change.
pub fn name_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email);
    let words: Vec<String> = local
        .split(['.', '_', '-', '+'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect();
    if words.is_empty() { local.to_string() } else { words.join(" ") }
}

#[derive(Deserialize)]
pub struct ProjectInvite {
    pub project_id: Uuid,
    #[serde(default = "member_role")]
    pub role: String,
}

fn member_role() -> String {
    "member".into()
}

#[derive(Deserialize)]
pub struct InviteRequest {
    /// Free-form list of addresses (see `parse_invitees`).
    pub emails: String,
    #[serde(default = "member_role")]
    pub role: String,
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
    #[serde(default)]
    pub projects: Vec<ProjectInvite>,
    #[serde(default)]
    pub message: String,
}

pub async fn invite_many(State(state): State<AppState>, user: CurrentUser, Json(req): Json<InviteRequest>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    if !matches!(req.role.as_str(), "admin" | "member" | "guest") {
        return bad_request("role must be admin, member or guest");
    }
    if req.role == "admin" && user.role != "owner" {
        return Err(ApiError::Forbidden);
    }
    let message: String = req.message.trim().chars().take(1000).collect();
    let groups = crate::registration::check_groups(&state, user.tenant_id, &req.group_ids).await?;
    let mut projects: Vec<(Uuid, String)> = Vec::new();
    for p in &req.projects {
        if !matches!(p.role.as_str(), "manager" | "member" | "viewer") {
            return bad_request("project role must be manager, member or viewer");
        }
        if !projects.iter().any(|(id, _)| *id == p.project_id) {
            projects.push((p.project_id, p.role.clone()));
        }
    }
    let project_ids: Vec<Uuid> = projects.iter().map(|(id, _)| *id).collect();
    let found: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE tenant_id = $1 AND id = ANY($2)")
        .bind(user.tenant_id)
        .bind(&project_ids)
        .fetch_one(&state.db)
        .await?;
    if found as usize != project_ids.len() {
        return bad_request("проект не найден");
    }
    let parsed = parse_invitees(&req.emails);
    if parsed.is_empty() {
        return bad_request("укажите хотя бы один адрес почты");
    }
    if parsed.len() > 100 {
        return bad_request("не более 100 адресов за раз");
    }

    let mut results = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for item in parsed {
        let inv = match item {
            Ok(i) => i,
            Err(raw) => {
                results.push(json!({ "input": raw, "status": "invalid" }));
                continue;
            }
        };
        let key = inv.email.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let existing: Option<(Uuid, bool)> = sqlx::query_as("SELECT id, invite_pending FROM users WHERE tenant_id = $1 AND email = $2::citext")
            .bind(user.tenant_id)
            .bind(&inv.email)
            .fetch_optional(&state.db)
            .await?;
        if let Some((id, pending)) = existing {
            results.push(json!({ "email": inv.email, "status": if pending { "already_invited" } else { "exists" }, "id": id }));
            continue;
        }
        let name: String = inv.name.clone().unwrap_or_else(|| name_from_email(&inv.email)).chars().take(200).collect();
        let mut tx = state.db.begin().await?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (tenant_id, email, display_name, password_hash, role, invite_pending)
             VALUES ($1, $2, $3, $4, $5, true) RETURNING id",
        )
        .bind(user.tenant_id)
        .bind(&inv.email)
        .bind(&name)
        .bind(NO_PASSWORD)
        .bind(&req.role)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO invitations (user_id, tenant_id, invited_by, message) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(&message)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO group_members (group_id, user_id) SELECT unnest($1::uuid[]), $2 ON CONFLICT DO NOTHING")
            .bind(&groups)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for (pid, role) in &projects {
            sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
                .bind(pid)
                .bind(id)
                .bind(role)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        state
            .audit(user.tenant_id, Some(user.user_id), "user.created", Some("user"), Some(id),
                   json!({ "role": req.role, "invited": true, "groups": groups.len(), "projects": projects.len() }))
            .await;
        let (url, emailed) = send_invite(&state, &user, id).await?;
        results.push(json!({ "email": inv.email, "display_name": name, "status": "invited", "id": id, "invite_url": url, "emailed": emailed }));
    }
    Ok(Json(json!({ "results": results })))
}

/// Pending invitations of the portal.
pub async fn list_invitations(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(Uuid, String, String, String, Option<String>, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>, bool)> =
        sqlx::query_as(
            "SELECT u.id, u.email::text, u.display_name, u.role, ib.display_name, COALESCE(i.message, ''),
                    COALESCE(i.created_at, u.created_at), k.created_at, k.expires_at, u.is_active
               FROM users u
               LEFT JOIN invitations i ON i.user_id = u.id
               LEFT JOIN users ib ON ib.id = i.invited_by
               LEFT JOIN LATERAL (SELECT created_at, expires_at FROM user_tokens
                                   WHERE user_id = u.id AND kind = 'invite' AND used_at IS NULL
                                   ORDER BY created_at DESC LIMIT 1) k ON true
              WHERE u.tenant_id = $1 AND u.invite_pending
              ORDER BY COALESCE(i.created_at, u.created_at) DESC",
        )
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?;
    let now = chrono::Utc::now();
    let mut items = Vec::new();
    for (id, email, name, role, by, message, created, sent, expires, active) in rows {
        let d = invite_details(&state, id).await?;
        items.push(json!({
            "id": id, "email": email, "display_name": name, "role": role, "invited_by_name": by, "message": message,
            "created_at": created, "sent_at": sent, "expires_at": expires, "is_active": active,
            "expired": expires.is_none_or(|e| e <= now),
            "projects": d.projects, "groups": d.groups,
        }));
    }
    Ok(Json(json!({ "invitations": items })))
}

/// Withdraw an invitation: the not-yet-joined account is removed (or, if
/// something already refers to it, blocked) and the link stops working.
pub async fn revoke_invitation(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let email: String = sqlx::query_scalar("SELECT email::text FROM users WHERE id = $1 AND tenant_id = $2 AND invite_pending")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::BadRequest("приглашение уже принято или отозвано".into()))?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SAVEPOINT revoke").execute(&mut *tx).await?;
    let removed = sqlx::query("DELETE FROM users WHERE id = $1").bind(id).execute(&mut *tx).await.is_ok();
    if !removed {
        sqlx::query("ROLLBACK TO SAVEPOINT revoke").execute(&mut *tx).await?;
        sqlx::query("UPDATE users SET is_active = false WHERE id = $1").bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM user_tokens WHERE user_id = $1 AND used_at IS NULL").bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "user.invite_revoked", Some("user"), if removed { None } else { Some(id) }, json!({ "email": email, "removed": removed }))
        .await;
    Ok(Json(json!({ "ok": true, "removed": removed })))
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
    let d = invite_details(&state, t.user_id).await?;
    Ok(Json(json!({
        "email": t.email, "display_name": t.display_name, "tenant_slug": t.tenant_slug, "tenant_name": t.tenant_name,
        "invited_by_name": d.invited_by_name, "message": d.message,
        "projects": d.projects.iter().map(|p| &p.name).collect::<Vec<_>>(), "groups": d.groups,
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
          WHERE t.slug = $1 AND u.email = $2::citext AND u.is_active AND NOT u.invite_pending AND t.status = 'active'",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitees_are_parsed() {
        let r = parse_invitees("Иван Петров <ivan@x.ru>, anna@x.ru Анна Смирнова; bob@x.ru\nnot-an-address\n\n");
        assert_eq!(r[0], Ok(Invitee { email: "ivan@x.ru".into(), name: Some("Иван Петров".into()) }));
        assert_eq!(r[1], Ok(Invitee { email: "anna@x.ru".into(), name: Some("Анна Смирнова".into()) }));
        assert_eq!(r[2], Ok(Invitee { email: "bob@x.ru".into(), name: None }));
        assert_eq!(r[3], Err("not-an-address".into()));
        assert_eq!(r.len(), 4);
        assert_eq!(parse_invitees("\"Пётр\" <p@x.ru>")[0], Ok(Invitee { email: "p@x.ru".into(), name: Some("Пётр".into()) }));
    }

    #[test]
    fn names_from_addresses() {
        assert_eq!(name_from_email("ivan.petrov@x.ru"), "Ivan Petrov");
        assert_eq!(name_from_email("a_b-c+tag@x.ru"), "A B C Tag");
    }
}
