//! Outgoing mail: per-portal SMTP settings (with a system-wide fallback),
//! a persistent outbox processed by a background worker with retries, and the
//! admin API for settings, test messages and the delivery log.

use std::{collections::HashMap, net::IpAddr, time::Duration};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use lettre::{
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    ratelimit::WINDOW_1_HOUR,
    state::AppState,
};

/// Attempts before a message is marked as failed.
const MAX_ATTEMPTS: i32 = 6;
/// Delay before each retry, in minutes (index = attempts made so far - 1).
const BACKOFF_MIN: [i64; 5] = [1, 5, 15, 60, 180];
/// Sent and failed messages are kept for the delivery log this many days.
const RETENTION_DAYS: i64 = 90;

// ---------------------------------------------------------------------------
// Transport settings
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Smtp {
    pub host: String,
    pub port: u16,
    pub security: String,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub from_name: String,
    pub reply_to: String,
    /// The portal's own server (subject to the private-address check).
    pub tenant_owned: bool,
}

fn system_smtp(state: &AppState) -> Option<Smtp> {
    state.cfg.smtp.as_ref().map(|s| Smtp {
        host: s.host.clone(),
        port: s.port,
        security: s.security.clone(),
        username: s.username.clone(),
        password: s.password.clone(),
        from_email: s.from_email.clone(),
        from_name: s.from_name.clone(),
        reply_to: String::new(),
        tenant_owned: false,
    })
}

#[derive(sqlx::FromRow)]
struct SettingsRow {
    enabled: bool,
    host: String,
    port: i32,
    security: String,
    username: String,
    password_enc: Option<Vec<u8>>,
    from_email: String,
    from_name: String,
    reply_to: String,
}

async fn settings_row(state: &AppState, tenant_id: Uuid) -> ApiResult<Option<SettingsRow>> {
    Ok(sqlx::query_as::<_, SettingsRow>(
        "SELECT enabled, host, port, security, username, password_enc, from_email, from_name, reply_to
           FROM tenant_mail_settings WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?)
}

impl SettingsRow {
    fn to_smtp(&self, state: &AppState) -> Smtp {
        Smtp {
            host: self.host.clone(),
            port: self.port as u16,
            security: self.security.clone(),
            username: self.username.clone(),
            password: self.password_enc.as_deref().and_then(|c| decrypt(state, c)).unwrap_or_default(),
            from_email: self.from_email.clone(),
            from_name: self.from_name.clone(),
            reply_to: self.reply_to.clone(),
            tenant_owned: true,
        }
    }
}

/// The server mail of this portal goes through: its own when enabled,
/// otherwise the system-wide one. `None` means mail cannot be delivered.
pub async fn effective_smtp(state: &AppState, tenant_id: Uuid) -> ApiResult<Option<Smtp>> {
    match settings_row(state, tenant_id).await? {
        Some(r) if r.enabled => Ok(Some(r.to_smtp(state))),
        _ => Ok(system_smtp(state)),
    }
}

/// Whether mail of this portal can be delivered at all.
pub async fn available(state: &AppState, tenant_id: Uuid) -> bool {
    matches!(effective_smtp(state, tenant_id).await, Ok(Some(_)))
}

// ---------------------------------------------------------------------------
// Password encryption at rest (AES-256-GCM, key derived from APP_SECRET)
// ---------------------------------------------------------------------------

fn cipher(state: &AppState) -> Aes256Gcm {
    let key = Sha256::digest(format!("workhub-mail-password:{}", state.cfg.app_secret).as_bytes());
    Aes256Gcm::new_from_slice(&key).expect("32-byte key")
}

fn encrypt(state: &AppState, plain: &str) -> ApiResult<Vec<u8>> {
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let data = cipher(state)
        .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("encryption failed")))?;
    Ok([nonce.as_slice(), &data].concat())
}

fn decrypt(state: &AppState, blob: &[u8]) -> Option<String> {
    if blob.len() < 13 {
        return None;
    }
    let (nonce, data) = blob.split_at(12);
    let plain = cipher(state).decrypt(Nonce::from_slice(nonce), data).ok()?;
    String::from_utf8(plain).ok()
}

// ---------------------------------------------------------------------------
// Sending
// ---------------------------------------------------------------------------

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast()
                // 100.64.0.0/10 (carrier-grade NAT)
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified()
                // fc00::/7 unique local, fe80::/10 link local
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || v6.to_ipv4_mapped().is_some_and(|v4| is_private(IpAddr::V4(v4)))
        }
    }
}

/// Refuse portal SMTP servers inside the private network (SSRF protection).
async fn check_host(state: &AppState, smtp: &Smtp) -> Result<(), String> {
    if !smtp.tenant_owned || state.cfg.smtp_allow_private {
        return Ok(());
    }
    let addrs = tokio::net::lookup_host((smtp.host.as_str(), smtp.port))
        .await
        .map_err(|e| format!("не удалось найти сервер {}: {e}", smtp.host))?;
    for a in addrs {
        if is_private(a.ip()) {
            return Err(format!("адрес {} находится во внутренней сети; такие серверы запрещены", a.ip()));
        }
    }
    Ok(())
}

/// One rendered message.
#[derive(Clone, Debug)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// Deliver a message right away. The error text is shown to administrators.
pub async fn send_now(state: &AppState, smtp: &Smtp, mail: &Mail) -> Result<(), String> {
    check_host(state, smtp).await?;
    let from_addr = smtp.from_email.parse().map_err(|e| format!("адрес отправителя: {e}"))?;
    let from = Mailbox::new((!smtp.from_name.trim().is_empty()).then(|| smtp.from_name.trim().to_string()), from_addr);
    let to: Mailbox = mail.to.parse().map_err(|e| format!("адрес получателя: {e}"))?;
    let mut builder = Message::builder().from(from).to(to).subject(&mail.subject);
    if !smtp.reply_to.trim().is_empty() {
        builder = builder.reply_to(smtp.reply_to.trim().parse().map_err(|e| format!("адрес для ответа: {e}"))?);
    }
    let msg = builder
        .multipart(MultiPart::alternative_plain_html(mail.text.clone(), mail.html.clone()))
        .map_err(|e| format!("сборка письма: {e}"))?;

    let transport = match smtp.security.as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host).map_err(|e| e.to_string())?,
        "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host).map_err(|e| e.to_string())?,
        _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host),
    };
    let mut transport = transport.port(smtp.port).timeout(Some(Duration::from_secs(20)));
    if !smtp.username.is_empty() {
        transport = transport.credentials(Credentials::new(smtp.username.clone(), smtp.password.clone()));
    }
    transport.build().send(msg).await.map(|_| ()).map_err(|e| {
        let mut s = e.to_string();
        if let Some(src) = std::error::Error::source(&e) {
            s = format!("{s}: {src}");
        }
        s
    })
}

/// Put a message into the outbox and wake the worker.
pub async fn enqueue(state: &AppState, tenant_id: Uuid, kind: &str, mail: &Mail) -> ApiResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mail_outbox (tenant_id, kind, to_email, subject, body_text, body_html)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(&mail.to)
    .bind(&mail.subject)
    .bind(&mail.text)
    .bind(&mail.html)
    .fetch_one(&state.db)
    .await?;
    state.mail_wakeup.notify_one();
    Ok(id)
}

// ---------------------------------------------------------------------------
// Background worker
// ---------------------------------------------------------------------------

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        let mut rounds: u64 = 0;
        loop {
            if let Err(e) = process_due(&state).await {
                tracing::warn!(error = ?e, "mail worker round failed");
            }
            rounds += 1;
            if rounds % 240 == 1 {
                let _ = sqlx::query(
                    "DELETE FROM mail_outbox WHERE status <> 'queued' AND created_at < now() - make_interval(days => $1)",
                )
                .bind(RETENTION_DAYS as i32)
                .execute(&state.db)
                .await;
                let _ = sqlx::query("DELETE FROM user_tokens WHERE expires_at < now() - interval '30 days'").execute(&state.db).await;
            }
            tokio::select! {
                _ = state.mail_wakeup.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(15)) => {}
            }
        }
    });
}

#[derive(sqlx::FromRow)]
struct DueRow {
    id: Uuid,
    tenant_id: Uuid,
    to_email: String,
    subject: String,
    body_text: String,
    body_html: String,
    attempts: i32,
}

async fn process_due(state: &AppState) -> anyhow::Result<()> {
    // Lease a batch so that several server instances never send twice.
    let due = sqlx::query_as::<_, DueRow>(
        "UPDATE mail_outbox SET next_attempt_at = now() + interval '5 minutes'
          WHERE id IN (SELECT id FROM mail_outbox
                        WHERE status = 'queued' AND next_attempt_at <= now()
                        ORDER BY next_attempt_at LIMIT 20 FOR UPDATE SKIP LOCKED)
         RETURNING id, tenant_id, to_email, subject, body_text, body_html, attempts",
    )
    .fetch_all(&state.db)
    .await?;
    let mut transports: HashMap<Uuid, Option<Smtp>> = HashMap::new();
    for m in due {
        if let std::collections::hash_map::Entry::Vacant(e) = transports.entry(m.tenant_id) {
            e.insert(effective_smtp(state, m.tenant_id).await.ok().flatten());
        }
        let Some(smtp) = transports[&m.tenant_id].clone() else {
            // Nothing to send through yet: keep the message and look again later.
            sqlx::query(
                "UPDATE mail_outbox SET last_error = $2, next_attempt_at = now() + interval '10 minutes' WHERE id = $1",
            )
            .bind(m.id)
            .bind("почтовый сервер не настроен")
            .execute(&state.db)
            .await?;
            continue;
        };
        let mail = Mail { to: m.to_email, subject: m.subject, text: m.body_text, html: m.body_html };
        match send_now(state, &smtp, &mail).await {
            Ok(()) => {
                sqlx::query(
                    "UPDATE mail_outbox SET status = 'sent', sent_at = now(), attempts = attempts + 1, last_error = NULL WHERE id = $1",
                )
                .bind(m.id)
                .execute(&state.db)
                .await?;
            }
            Err(err) => {
                let attempts = m.attempts + 1;
                let failed = attempts >= MAX_ATTEMPTS;
                let delay = BACKOFF_MIN[(attempts as usize - 1).min(BACKOFF_MIN.len() - 1)];
                tracing::warn!(id = %m.id, attempts, error = %err, "mail delivery failed");
                sqlx::query(
                    "UPDATE mail_outbox SET attempts = $2, last_error = $3,
                            status = CASE WHEN $4 THEN 'failed' ELSE 'queued' END,
                            next_attempt_at = now() + make_interval(mins => $5)
                      WHERE id = $1",
                )
                .bind(m.id)
                .bind(attempts)
                .bind(err.chars().take(1000).collect::<String>())
                .bind(failed)
                .bind(delay as i32)
                .execute(&state.db)
                .await?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Message templates
// ---------------------------------------------------------------------------

/// Name in «quotes» unless it already carries its own (ООО «Ромашка»).
pub fn quoted(name: &str) -> String {
    if name.contains('«') || name.contains('"') {
        name.to_string()
    } else {
        format!("«{name}»")
    }
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Portal name as users know it (branding name or the portal name).
pub async fn portal_name(state: &AppState, tenant_id: Uuid) -> ApiResult<String> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(b.display_name, ''), t.name) FROM tenants t
           LEFT JOIN tenant_branding b ON b.tenant_id = t.id WHERE t.id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&state.db)
    .await?)
}

/// Render a simple message: paragraphs, an optional button and a footnote.
pub fn render(to: &str, portal: &str, subject: &str, paragraphs: &[String], button: Option<(&str, &str)>, note: &str) -> Mail {
    let mut text = String::new();
    let mut html = String::from(
        "<!doctype html><html><body style=\"margin:0;background:#f3f4f6;font-family:Segoe UI,Arial,sans-serif;color:#1f2937\">\
         <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\"><tr><td align=\"center\" style=\"padding:24px 12px\">\
         <table role=\"presentation\" width=\"100%\" style=\"max-width:560px;background:#ffffff;border:1px solid #e5e7eb;border-radius:8px\">\
         <tr><td style=\"padding:24px 28px\">",
    );
    html.push_str(&format!(
        "<div style=\"font-size:13px;color:#6b7280;margin-bottom:12px\">{}</div><h1 style=\"font-size:20px;margin:0 0 16px\">{}</h1>",
        escape(portal),
        escape(subject)
    ));
    for p in paragraphs {
        text.push_str(p);
        text.push_str("\n\n");
        html.push_str(&format!("<p style=\"font-size:15px;line-height:1.5;margin:0 0 14px\">{}</p>", escape(p)));
    }
    if let Some((label, url)) = button {
        text.push_str(&format!("{label}: {url}\n\n"));
        html.push_str(&format!(
            "<p style=\"margin:20px 0\"><a href=\"{u}\" style=\"display:inline-block;background:#1f4e79;color:#ffffff;text-decoration:none;\
             padding:10px 18px;border-radius:6px;font-size:15px\">{l}</a></p>\
             <p style=\"font-size:12px;color:#6b7280;word-break:break-all\">Если кнопка не работает, откройте ссылку: {u}</p>",
            u = escape(url),
            l = escape(label)
        ));
    }
    if !note.is_empty() {
        text.push_str(note);
        text.push('\n');
        html.push_str(&format!("<p style=\"font-size:12px;color:#6b7280;margin:16px 0 0\">{}</p>", escape(note)));
    }
    text.push_str(&format!("\n— {portal}\n"));
    html.push_str("</td></tr></table></td></tr></table></body></html>");
    Mail { to: to.to_string(), subject: subject.to_string(), text, html }
}

/// One entry of a list message (notification digest).
pub struct ListItem {
    pub title: String,
    pub body: String,
    pub url: Option<String>,
}

/// Render a message with a list of linked entries.
pub fn render_list(to: &str, portal: &str, subject: &str, intro: &str, items: &[ListItem], button: (&str, &str), note: &str) -> Mail {
    let mut text = format!("{intro}\n\n");
    let mut rows = String::new();
    for i in items {
        text.push_str(&format!("• {}\n", i.title));
        if !i.body.is_empty() {
            text.push_str(&format!("  {}\n", i.body));
        }
        if let Some(u) = &i.url {
            text.push_str(&format!("  {u}\n"));
        }
        text.push('\n');
        let title = match &i.url {
            Some(u) => format!("<a href=\"{}\" style=\"color:#1f4e79;text-decoration:none;font-weight:600\">{}</a>", escape(u), escape(&i.title)),
            None => format!("<b>{}</b>", escape(&i.title)),
        };
        rows.push_str(&format!(
            "<tr><td style=\"padding:10px 0;border-top:1px solid #e5e7eb;font-size:14px;line-height:1.45\">{title}{}</td></tr>",
            if i.body.is_empty() { String::new() } else { format!("<div style=\"color:#4b5563\">{}</div>", escape(&i.body)) }
        ));
    }
    let mut m = render(to, portal, subject, &[intro.to_string()], None, "");
    // Insert the list before the closing of the content cell, then the button and note.
    let tail = "</td></tr></table></td></tr></table></body></html>";
    let html_body = m.html.trim_end_matches(tail).to_string();
    m.html = format!(
        "{html_body}<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\">{rows}</table>\
         <p style=\"margin:20px 0\"><a href=\"{u}\" style=\"display:inline-block;background:#1f4e79;color:#ffffff;text-decoration:none;\
         padding:10px 18px;border-radius:6px;font-size:15px\">{l}</a></p>\
         <p style=\"font-size:12px;color:#6b7280;margin:16px 0 0\">{n}</p>{tail}",
        u = escape(button.1),
        l = escape(button.0),
        n = escape(note)
    );
    text.push_str(&format!("{}: {}\n\n{note}\n\n— {portal}\n", button.0, button.1));
    m.text = text;
    m
}

// ---------------------------------------------------------------------------
// Admin API
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct OutboxRow {
    id: Uuid,
    kind: String,
    to_email: String,
    subject: String,
    status: String,
    attempts: i32,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    sent_at: Option<DateTime<Utc>>,
    next_attempt_at: DateTime<Utc>,
}

pub async fn get_settings(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let row = settings_row(&state, user.tenant_id).await?;
    let settings = match &row {
        Some(r) => json!({
            "enabled": r.enabled, "host": r.host, "port": r.port, "security": r.security, "username": r.username,
            "has_password": r.password_enc.is_some(), "from_email": r.from_email, "from_name": r.from_name, "reply_to": r.reply_to,
        }),
        None => json!({
            "enabled": false, "host": "", "port": 587, "security": "starttls", "username": "",
            "has_password": false, "from_email": "", "from_name": "", "reply_to": "",
        }),
    };
    let outbox = sqlx::query_as::<_, OutboxRow>(
        "SELECT id, kind, to_email, subject, status, attempts, last_error, created_at, sent_at, next_attempt_at
           FROM mail_outbox WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let system = state.cfg.smtp.as_ref().map(|s| json!({ "host": s.host, "from_email": s.from_email }));
    Ok(Json(json!({
        "settings": settings,
        "system": system,
        "available": available(&state, user.tenant_id).await,
        "public_url": state.cfg.public_url,
        "outbox": outbox,
    })))
}

#[derive(Deserialize)]
pub struct SettingsInput {
    pub enabled: bool,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: i32,
    #[serde(default = "default_security")]
    pub security: String,
    #[serde(default)]
    pub username: String,
    /// `None` keeps the stored password, an empty string clears it.
    pub password: Option<String>,
    #[serde(default)]
    pub from_email: String,
    #[serde(default)]
    pub from_name: String,
    #[serde(default)]
    pub reply_to: String,
}

fn default_port() -> i32 {
    587
}
fn default_security() -> String {
    "starttls".into()
}

fn valid_email(s: &str) -> bool {
    let s = s.trim();
    s.parse::<lettre::Address>().is_ok()
}

impl SettingsInput {
    fn validate(&self) -> ApiResult<()> {
        if !matches!(self.security.as_str(), "none" | "starttls" | "tls") {
            return bad_request("security must be none, starttls or tls");
        }
        if !(1..=65535).contains(&self.port) {
            return bad_request("укажите порт от 1 до 65535");
        }
        if self.enabled {
            if self.host.trim().is_empty() {
                return bad_request("укажите адрес почтового сервера");
            }
            if !valid_email(&self.from_email) {
                return bad_request("укажите корректный адрес отправителя");
            }
        }
        if !self.reply_to.trim().is_empty() && !valid_email(&self.reply_to) {
            return bad_request("некорректный адрес для ответа");
        }
        if self.host.chars().any(|c| c.is_whitespace() || c == '/') {
            return bad_request("адрес сервера указывается без протокола и пробелов, например smtp.yandex.ru");
        }
        Ok(())
    }

    fn to_smtp(&self, stored_password: &str) -> Smtp {
        Smtp {
            host: self.host.trim().into(),
            port: self.port as u16,
            security: self.security.clone(),
            username: self.username.trim().into(),
            password: self.password.clone().unwrap_or_else(|| stored_password.to_string()),
            from_email: self.from_email.trim().into(),
            from_name: self.from_name.trim().into(),
            reply_to: self.reply_to.trim().into(),
            tenant_owned: true,
        }
    }
}

pub async fn save_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<SettingsInput>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    req.validate()?;
    let password_enc = match &req.password {
        Some(p) if !p.is_empty() => Some(Some(encrypt(&state, p)?)),
        Some(_) => Some(None),
        None => None,
    };
    sqlx::query(
        "INSERT INTO tenant_mail_settings
                (tenant_id, enabled, host, port, security, username, password_enc, from_email, from_name, reply_to)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (tenant_id) DO UPDATE SET
                enabled = EXCLUDED.enabled, host = EXCLUDED.host, port = EXCLUDED.port, security = EXCLUDED.security,
                username = EXCLUDED.username,
                password_enc = CASE WHEN $11 THEN EXCLUDED.password_enc ELSE tenant_mail_settings.password_enc END,
                from_email = EXCLUDED.from_email, from_name = EXCLUDED.from_name, reply_to = EXCLUDED.reply_to,
                updated_at = now()",
    )
    .bind(user.tenant_id)
    .bind(req.enabled)
    .bind(req.host.trim())
    .bind(req.port)
    .bind(&req.security)
    .bind(req.username.trim())
    .bind(password_enc.clone().flatten())
    .bind(req.from_email.trim())
    .bind(req.from_name.trim())
    .bind(req.reply_to.trim())
    .bind(password_enc.is_some())
    .execute(&state.db)
    .await?;
    // Mail waiting for a transport goes out right away.
    sqlx::query("UPDATE mail_outbox SET next_attempt_at = now() WHERE tenant_id = $1 AND status = 'queued'")
        .bind(user.tenant_id)
        .execute(&state.db)
        .await?;
    state.mail_wakeup.notify_one();
    state
        .audit(user.tenant_id, Some(user.user_id), "mail.settings_updated", None, None, json!({ "enabled": req.enabled, "host": req.host.trim() }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct TestInput {
    pub to: String,
    /// Unsaved settings from the form; the saved/effective ones when absent.
    pub settings: Option<SettingsInput>,
}

/// Send a test message synchronously and report the SMTP error, if any.
pub async fn send_test(State(state): State<AppState>, user: CurrentUser, Json(req): Json<TestInput>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    state.limiter.check(&format!("mail-test:{}", user.tenant_id), 20, WINDOW_1_HOUR)?;
    state.limiter.hit(&format!("mail-test:{}", user.tenant_id), WINDOW_1_HOUR);
    if !valid_email(&req.to) {
        return bad_request("укажите корректный адрес получателя");
    }
    let smtp = match &req.settings {
        Some(s) if s.enabled => {
            s.validate()?;
            let stored = settings_row(&state, user.tenant_id)
                .await?
                .and_then(|r| r.password_enc)
                .and_then(|c| decrypt(&state, &c))
                .unwrap_or_default();
            Some(s.to_smtp(&stored))
        }
        Some(_) => system_smtp(&state),
        None => effective_smtp(&state, user.tenant_id).await?,
    };
    let Some(smtp) = smtp else {
        return Ok(Json(json!({ "ok": false, "error": "почтовый сервер не настроен" })));
    };
    let portal = portal_name(&state, user.tenant_id).await?;
    let mail = render(
        req.to.trim(),
        &portal,
        "Проверка почты",
        &[format!(
            "Это тестовое письмо портала {}. Если вы его получили, отправка почты настроена верно. Сервер: {}:{}.",
            quoted(&portal), smtp.host, smtp.port
        )],
        None,
        "",
    );
    let res = send_now(&state, &smtp, &mail).await;
    state
        .audit(user.tenant_id, Some(user.user_id), "mail.test", None, None, json!({ "to": req.to.trim(), "ok": res.is_ok() }))
        .await;
    Ok(Json(match res {
        Ok(()) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e }),
    }))
}

pub async fn retry(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let n = sqlx::query(
        "UPDATE mail_outbox SET status = 'queued', attempts = 0, next_attempt_at = now(), last_error = NULL
          WHERE id = $1 AND tenant_id = $2 AND status <> 'sent'",
    )
    .bind(id)
    .bind(user.tenant_id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    state.mail_wakeup.notify_one();
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_addresses() {
        for a in ["127.0.0.1", "10.1.2.3", "192.168.0.10", "172.20.0.5", "169.254.1.1", "100.64.0.1", "::1", "fd00::1", "fe80::1", "::ffff:10.0.0.1"] {
            assert!(is_private(a.parse().unwrap()), "{a}");
        }
        for a in ["77.88.21.158", "8.8.8.8", "2a02:6b8::1"] {
            assert!(!is_private(a.parse().unwrap()), "{a}");
        }
    }

    #[test]
    fn quoting() {
        assert_eq!(quoted("Бюро"), "«Бюро»");
        assert_eq!(quoted("ООО «Ромашка»"), "ООО «Ромашка»");
    }

    #[test]
    fn render_escapes_html() {
        let m = render("a@b.ru", "ООО <Тест>", "Тема", &["x < y & z".into()], Some(("Открыть", "https://e.x/?a=1&b=2")), "");
        assert!(m.html.contains("ООО &lt;Тест&gt;"));
        assert!(m.html.contains("x &lt; y &amp; z"));
        assert!(m.html.contains("a=1&amp;b=2"));
        assert!(m.text.contains("https://e.x/?a=1&b=2"));
    }
}
