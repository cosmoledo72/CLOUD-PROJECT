//! Unified API error type. Every handler returns `ApiResult<T>`.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("access denied")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("payload too large: {0}")]
    TooLarge(String),
    /// Retry after the given number of seconds.
    #[error("too many attempts, retry in {0} s")]
    TooManyRequests(u64),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => ApiError::NotFound,
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
                ApiError::Conflict("an item with this name already exists".into())
            }
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23514") => {
                ApiError::BadRequest(format!("constraint violated: {}", db.message()))
            }
            _ => ApiError::Internal(e.into()),
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::Internal(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::TooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            ApiError::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "too_many_requests"),
            ApiError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        // Never leak internal error details to clients.
        let message = match &self {
            ApiError::Internal(_) => "internal server error".to_string(),
            other => other.to_string(),
        };
        let mut resp = (status, Json(json!({ "error": code, "message": message }))).into_response();
        if let ApiError::TooManyRequests(secs) = &self {
            if let Ok(v) = secs.to_string().parse() {
                resp.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
            }
        }
        resp
    }
}

/// Shorthand for `Err(ApiError::BadRequest(..))`.
pub fn bad_request<T>(msg: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::BadRequest(msg.into()))
}
