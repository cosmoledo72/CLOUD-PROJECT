//! Runtime configuration loaded from environment variables.

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    /// Postgres connection string.
    pub database_url: String,
    /// Address the HTTP server binds to, e.g. `0.0.0.0:8080`.
    pub bind_addr: String,
    /// Base URL of this API as the ONLYOFFICE Document Server sees it
    /// (used for document download and save callbacks), e.g. `http://backend:8080`.
    pub internal_url: String,
    /// Base URL of the Document Server as the browser sees it, e.g. `http://localhost:8081`.
    pub docs_public_url: String,
    /// Base URL of the Document Server as this backend sees it, e.g. `http://onlyoffice`.
    /// Used to rewrite download links that arrive in save callbacks.
    pub docs_internal_url: String,
    /// Shared JWT secret configured in the Document Server (`JWT_SECRET`).
    pub docs_jwt_secret: String,
    /// Secret used to sign short-lived URLs handed to the Document Server.
    pub app_secret: String,
    /// Directory of the local blob store.
    pub storage_dir: String,
    /// Mark session cookies `Secure` (enable behind HTTPS).
    pub cookie_secure: bool,
    /// Session lifetime in hours.
    pub session_ttl_hours: i64,
    /// Lifetime of a session with «Запомнить меня» (REMEMBER_TTL_HOURS, default 720 = 30 days).
    pub remember_ttl_hours: i64,
    /// Allow public self-service tenant registration.
    pub signup_enabled: bool,
    /// Allowed browser origin for CORS (the SvelteKit app), if served separately.
    pub cors_origin: Option<String>,
    /// Maximum upload size in bytes.
    pub max_upload_bytes: usize,
    /// Days after which trashed items are purged automatically.
    pub trash_retention_days: i64,
    /// Failed logins per account (portal + email) per 15 minutes.
    pub login_account_limit: u32,
    /// Failed logins per client IP per 15 minutes.
    pub login_ip_limit: u32,
    /// Portal sign-ups per client IP per hour.
    pub signup_ip_limit: u32,
    /// Base URL of the web app as users open it; used in links sent by email.
    pub public_url: String,
    /// System-wide SMTP server used by portals without their own settings.
    pub smtp: Option<SmtpEnv>,
    /// Allow portal SMTP servers on loopback/private addresses (development only:
    /// otherwise portal admins could probe the internal network).
    pub smtp_allow_private: bool,
    /// Unread notifications are emailed (as one digest) after this delay, so
    /// that what the user already saw in the app does not arrive by mail.
    pub notify_email_delay_secs: i64,
    /// How often due-date reminders are checked.
    pub due_scan_secs: u64,
    /// Local hour (portal time zone) from which due-date reminders are sent.
    pub due_reminder_hour: i32,
    /// Folder of the backup service (`backup.sh`), mounted into the backend.
    pub backup_dir: String,
    /// Slug of the portal whose administrators see «Резервные копии»
    /// (a backup holds every portal of the server). Empty = the section is off.
    pub backup_portal: Option<String>,
    /// Nightly backup hour and retention, as the backup service uses them (shown in the section).
    pub backup_hour: u32,
    pub backup_keep_days: u32,
}

/// SMTP server configured through the environment (`SMTP_*`).
#[derive(Clone, Debug)]
pub struct SmtpEnv {
    pub host: String,
    pub port: u16,
    /// none | starttls | tls
    pub security: String,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub from_name: String,
}

fn var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("environment variable {name} is required"))
}

/// Value of an optional variable; unset and empty both mean the default
/// (Docker Compose passes `${X:-}` as an empty string).
fn var_or(name: &str, default: &str) -> String {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let cfg = Self {
            database_url: var("DATABASE_URL")?,
            bind_addr: var_or("BIND_ADDR", "0.0.0.0:8080"),
            internal_url: var_or("INTERNAL_URL", "http://localhost:8080").trim_end_matches('/').into(),
            docs_public_url: var_or("DOCS_PUBLIC_URL", "http://localhost:8081").trim_end_matches('/').into(),
            docs_internal_url: var_or("DOCS_INTERNAL_URL", "http://localhost:8081").trim_end_matches('/').into(),
            docs_jwt_secret: var("DOCS_JWT_SECRET")?,
            app_secret: var("APP_SECRET")?,
            storage_dir: var_or("STORAGE_DIR", "./data/blobs"),
            cookie_secure: var_or("COOKIE_SECURE", "false") == "true",
            session_ttl_hours: var_or("SESSION_TTL_HOURS", "168").parse().context("SESSION_TTL_HOURS")?,
            remember_ttl_hours: var_or("REMEMBER_TTL_HOURS", "720").parse().context("REMEMBER_TTL_HOURS")?,
            signup_enabled: var_or("SIGNUP_ENABLED", "true") == "true",
            cors_origin: std::env::var("CORS_ORIGIN").ok().filter(|s| !s.is_empty()),
            max_upload_bytes: var_or("MAX_UPLOAD_MB", "512").parse::<usize>().context("MAX_UPLOAD_MB")? * 1024 * 1024,
            trash_retention_days: var_or("TRASH_RETENTION_DAYS", "30").parse().context("TRASH_RETENTION_DAYS")?,
            login_account_limit: var_or("LOGIN_ACCOUNT_LIMIT", "8").parse().context("LOGIN_ACCOUNT_LIMIT")?,
            login_ip_limit: var_or("LOGIN_IP_LIMIT", "30").parse().context("LOGIN_IP_LIMIT")?,
            signup_ip_limit: var_or("SIGNUP_IP_LIMIT", "10").parse().context("SIGNUP_IP_LIMIT")?,
            public_url: var_or("PUBLIC_URL", "http://localhost:8080").trim_end_matches('/').into(),
            smtp: smtp_from_env()?,
            smtp_allow_private: var_or("SMTP_ALLOW_PRIVATE", "false") == "true",
            notify_email_delay_secs: var_or("NOTIFY_EMAIL_DELAY_SECS", "180").parse().context("NOTIFY_EMAIL_DELAY_SECS")?,
            due_scan_secs: var_or("DUE_SCAN_SECS", "60").parse::<u64>().context("DUE_SCAN_SECS")?.max(1),
            due_reminder_hour: var_or("DUE_REMINDER_HOUR", "8").parse().context("DUE_REMINDER_HOUR")?,
            backup_dir: var_or("BACKUP_DIR", "/backups"),
            backup_portal: std::env::var("BACKUP_PORTAL").ok().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()),
            backup_hour: var_or("BACKUP_HOUR", "3").trim_start_matches('0').parse().unwrap_or(0).min(23),
            backup_keep_days: var_or("BACKUP_KEEP_DAYS", "14").parse().context("BACKUP_KEEP_DAYS")?,
        };
        if cfg.app_secret.len() < 32 || cfg.docs_jwt_secret.len() < 16 {
            anyhow::bail!("APP_SECRET must be >= 32 chars and DOCS_JWT_SECRET >= 16 chars");
        }
        Ok(cfg)
    }
}

fn smtp_from_env() -> Result<Option<SmtpEnv>> {
    let host = var_or("SMTP_HOST", "");
    if host.trim().is_empty() {
        return Ok(None);
    }
    let security = var_or("SMTP_SECURITY", "starttls");
    if !matches!(security.as_str(), "none" | "starttls" | "tls") {
        anyhow::bail!("SMTP_SECURITY must be none, starttls or tls");
    }
    let default_port = match security.as_str() {
        "tls" => "465",
        "none" => "25",
        _ => "587",
    };
    let from_email = var_or("SMTP_FROM", "");
    if !from_email.contains('@') {
        anyhow::bail!("SMTP_FROM (sender address) is required when SMTP_HOST is set");
    }
    Ok(Some(SmtpEnv {
        host: host.trim().into(),
        port: var_or("SMTP_PORT", default_port).parse().context("SMTP_PORT")?,
        security,
        username: var_or("SMTP_USERNAME", ""),
        password: var_or("SMTP_PASSWORD", ""),
        from_email,
        from_name: var_or("SMTP_FROM_NAME", "WorkHub"),
    }))
}
