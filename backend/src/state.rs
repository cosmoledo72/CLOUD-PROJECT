//! Shared application state handed to every handler.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::{config::Config, ratelimit::RateLimiter, storage::LocalStore};

/// Concrete blob store in use. Swap for an S3-backed store implementing
/// `BlobStore` without touching handlers.
pub type Store = LocalStore;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub store: Arc<Store>,
    pub http: reqwest::Client,
    pub limiter: Arc<RateLimiter>,
    /// Wakes the mail worker when new mail is queued.
    pub mail_wakeup: Arc<tokio::sync::Notify>,
    /// Live events for connected browsers (notification stream).
    pub events: tokio::sync::broadcast::Sender<UserEvent>,
}

/// An event addressed to one user, delivered over the notification stream.
#[derive(Clone, Debug)]
pub struct UserEvent {
    pub user_id: Uuid,
    pub data: Arc<serde_json::Value>,
}

impl AppState {
    /// Append an entry to the tenant audit log. Failures are logged, not propagated,
    /// so that auditing never breaks the user-facing operation.
    pub async fn audit(
        &self,
        tenant_id: Uuid,
        user_id: Option<Uuid>,
        action: &str,
        target_type: Option<&str>,
        target_id: Option<Uuid>,
        details: serde_json::Value,
    ) {
        let res = sqlx::query(
            "INSERT INTO audit_log (tenant_id, user_id, action, target_type, target_id, details)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(details)
        .execute(&self.db)
        .await;
        if let Err(e) = res {
            tracing::warn!(error = ?e, action, "failed to write audit log");
        }
    }
}
