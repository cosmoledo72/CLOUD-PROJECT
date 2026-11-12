//! ONLYOFFICE Docs integration.
//!
//! Flow:
//! 1. Browser asks `GET /api/files/{id}/editor` → we return a signed editor
//!    config (JWT with the Document Server secret) containing:
//!    - `document.url`  → our `/api/oo/download/{id}?t=…` (short-lived app token)
//!    - `editorConfig.callbackUrl` → our `/api/oo/callback/{id}?t=…`
//! 2. The Document Server downloads the file from `document.url`.
//! 3. On save (status 2 = all editors closed, 6 = force-save) the Document
//!    Server POSTs to the callback with a link to the edited file; we fetch it
//!    and store it as a new version.
//!
//! The document `key` identifies a co-editing session. All users opening the
//! same file share `files.edit_key`; it is cleared after a final save
//! (status 2) or a close without changes (status 4), so the next session
//! starts from the newly stored version.

use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    acl::{self, Access},
    auth::{random_key, CurrentUser},
    error::{ApiError, ApiResult},
    files::{add_version, file_response, read_version},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Format tables
// ---------------------------------------------------------------------------

fn document_type(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "doc" | "docx" | "docm" | "dot" | "dotx" | "dotm" | "odt" | "ott" | "rtf" | "txt" | "html" | "htm" | "mht"
        | "epub" | "fb2" | "xml" | "djvu" | "xps" | "oxps" => "word",
        "xls" | "xlsx" | "xlsm" | "xlsb" | "xlt" | "xltx" | "xltm" | "ods" | "ots" | "csv" => "cell",
        "ppt" | "pptx" | "pptm" | "pps" | "ppsx" | "pot" | "potx" | "odp" | "otp" => "slide",
        "pdf" => "pdf",
        _ => return None,
    })
}

/// Formats edited natively (saved back in the same format).
fn is_editable(ext: &str) -> bool {
    matches!(ext, "docx" | "docm" | "dotx" | "xlsx" | "xlsm" | "xltx" | "pptx" | "pptm" | "potx" | "odt" | "ods" | "odp" | "csv" | "txt" | "rtf")
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

/// Claims of URLs we hand to the Document Server.
#[derive(Serialize, Deserialize)]
struct AppClaims {
    /// "dl" (download) or "cb" (callback).
    sub: String,
    fid: Uuid,
    tid: Uuid,
    #[serde(default)]
    v: Option<i32>,
    exp: i64,
}

const URL_TOKEN_HOURS: i64 = 24;

fn sign_app(state: &AppState, sub: &str, fid: Uuid, tid: Uuid, v: Option<i32>) -> ApiResult<String> {
    let claims = AppClaims { sub: sub.into(), fid, tid, v, exp: Utc::now().timestamp() + URL_TOKEN_HOURS * 3600 };
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(state.cfg.app_secret.as_bytes()))
        .map_err(|e| ApiError::Internal(e.into()))
}

fn verify_app(state: &AppState, token: &str, sub: &str, fid: Uuid) -> ApiResult<AppClaims> {
    let data = decode::<AppClaims>(token, &DecodingKey::from_secret(state.cfg.app_secret.as_bytes()), &Validation::new(Algorithm::HS256))
        .map_err(|_| ApiError::Unauthorized)?;
    if data.claims.sub != sub || data.claims.fid != fid {
        return Err(ApiError::Unauthorized);
    }
    Ok(data.claims)
}

fn sign_docs(state: &AppState, payload: &Value) -> ApiResult<String> {
    encode(&Header::new(Algorithm::HS256), payload, &EncodingKey::from_secret(state.cfg.docs_jwt_secret.as_bytes()))
        .map_err(|e| ApiError::Internal(e.into()))
}

fn verify_docs(state: &AppState, token: &str) -> ApiResult<Value> {
    let mut v = Validation::new(Algorithm::HS256);
    v.required_spec_claims = HashSet::new();
    v.leeway = 60;
    decode::<Value>(token, &DecodingKey::from_secret(state.cfg.docs_jwt_secret.as_bytes()), &v)
        .map(|d| d.claims)
        .map_err(|_| ApiError::Unauthorized)
}

#[derive(Deserialize)]
pub struct TokenQuery {
    pub t: String,
}

// ---------------------------------------------------------------------------
// Editor config
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct EditorQuery {
    /// "edit" (default) or "view".
    pub mode: Option<String>,
}

pub async fn editor_config(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<EditorQuery>,
) -> ApiResult<Json<Value>> {
    let (access, _) = acl::file_access(&state.db, &user, id).await?;
    access.require(Access::Read)?;
    let (name, ext, version): (String, String, i32) =
        sqlx::query_as("SELECT name, extension, current_version FROM files WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .fetch_one(&state.db)
            .await?;
    let doc_type = document_type(&ext).ok_or_else(|| ApiError::BadRequest(format!("format .{ext} cannot be opened in the editor")))?;

    let can_edit = access >= Access::Edit && is_editable(&ext);
    let edit_mode = can_edit && q.mode.as_deref() != Some("view");

    let key = if edit_mode {
        // Join the running co-editing session or start a new one.
        sqlx::query_scalar::<_, String>("UPDATE files SET edit_key = COALESCE(edit_key, $2) WHERE id = $1 RETURNING edit_key")
            .bind(id)
            .bind(random_key())
            .fetch_one(&state.db)
            .await?
    } else {
        format!("{}-v{version}", id.simple())
    };

    let base = &state.cfg.internal_url;
    let dl = sign_app(&state, "dl", id, user.tenant_id, None)?;
    let mut config = json!({
        "documentType": doc_type,
        "document": {
            "fileType": ext,
            "key": key,
            "title": name,
            "url": format!("{base}/api/oo/download/{id}?t={dl}"),
            "permissions": {
                "edit": edit_mode,
                "comment": access >= Access::Comment,
                "review": edit_mode,
                "download": true,
                "print": true,
            },
        },
        "editorConfig": {
            "mode": if edit_mode { "edit" } else { "view" },
            "lang": "ru",
            "region": "ru-RU",
            "user": { "id": user.user_id.to_string(), "name": user.display_name },
            "customization": { "forcesave": true, "autosave": true },
        },
    });
    if edit_mode {
        let cb = sign_app(&state, "cb", id, user.tenant_id, None)?;
        config["editorConfig"]["callbackUrl"] = json!(format!("{base}/api/oo/callback/{id}?t={cb}"));
    }
    let token = sign_docs(&state, &config)?;
    config["token"] = json!(token);

    Ok(Json(json!({
        "docs_url": state.cfg.docs_public_url,
        "can_edit": can_edit,
        "config": config,
    })))
}

/// Read-only viewer config for someone without an account (public links).
pub async fn public_viewer_config(state: &AppState, tenant_id: Uuid, file_id: Uuid) -> ApiResult<Value> {
    let (name, ext, version): (String, String, i32) =
        sqlx::query_as("SELECT name, extension, current_version FROM files WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL")
            .bind(file_id)
            .bind(tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let doc_type = document_type(&ext).ok_or_else(|| ApiError::BadRequest(format!("format .{ext} cannot be viewed")))?;
    let base = &state.cfg.internal_url;
    let dl = sign_app(state, "dl", file_id, tenant_id, None)?;
    let mut config = json!({
        "documentType": doc_type,
        "document": {
            "fileType": ext,
            "key": format!("{}-v{version}", file_id.simple()),
            "title": name,
            "url": format!("{base}/api/oo/download/{file_id}?t={dl}"),
            "permissions": { "edit": false, "comment": false, "review": false, "download": true, "print": true, "chat": false },
        },
        "editorConfig": {
            "mode": "view",
            "lang": "ru",
            "region": "ru-RU",
            "user": { "id": format!("guest-{}", random_key()), "name": "Гость" },
            "customization": { "anonymous": { "request": false }, "chat": false, "comments": false },
        },
    });
    let token = sign_docs(state, &config)?;
    config["token"] = json!(token);
    Ok(json!({ "docs_url": state.cfg.docs_public_url, "can_edit": false, "config": config }))
}

// ---------------------------------------------------------------------------
// Endpoints called by the Document Server
// ---------------------------------------------------------------------------

pub async fn ds_download(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<TokenQuery>,
) -> ApiResult<Response> {
    let claims = verify_app(&state, &q.t, "dl", id)?;
    let (name, data) = read_version(&state, claims.tid, id, claims.v).await?;
    Ok(file_response(&name, data, false))
}

/// Extract the trusted callback body: either the `token` field of the body or
/// the `Authorization: Bearer` header (`{ "payload": { ... } }`).
fn trusted_callback_body(state: &AppState, headers: &HeaderMap, body: &Value) -> ApiResult<Value> {
    if let Some(t) = body.get("token").and_then(Value::as_str) {
        return verify_docs(state, t);
    }
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    let claims = verify_docs(state, auth)?;
    claims.get("payload").cloned().ok_or(ApiError::Unauthorized)
}

/// Rewrite a Document Server link from its public origin to the one reachable
/// from this backend (e.g. inside a Docker network).
fn internal_docs_url(state: &AppState, url: &str) -> String {
    match url.strip_prefix(&state.cfg.docs_public_url) {
        Some(rest) if state.cfg.docs_public_url != state.cfg.docs_internal_url => format!("{}{rest}", state.cfg.docs_internal_url),
        _ => url.to_string(),
    }
}

async fn save_from_docs(state: &AppState, tenant_id: Uuid, file_id: Uuid, body: &Value) -> ApiResult<i32> {
    let url = body.get("url").and_then(Value::as_str).ok_or_else(|| ApiError::BadRequest("missing url".into()))?;
    let url = internal_docs_url(state, url);
    let resp = state.http.get(&url).send().await.map_err(|e| ApiError::Internal(e.into()))?;
    if !resp.status().is_success() {
        return Err(ApiError::Internal(anyhow::anyhow!("document server returned {}", resp.status())));
    }
    let data = resp.bytes().await.map_err(|e| ApiError::Internal(e.into()))?;
    // The first user in the list is treated as the author of the version.
    let author = body
        .get("users")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok());
    add_version(state, tenant_id, file_id, author, data).await
}

pub async fn ds_callback(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<TokenQuery>,
    headers: HeaderMap,
    Json(raw): Json<Value>,
) -> Json<Value> {
    let result: ApiResult<()> = async {
        let claims = verify_app(&state, &q.t, "cb", id)?;
        let body = trusted_callback_body(&state, &headers, &raw)?;
        let status = body.get("status").and_then(Value::as_i64).unwrap_or(0);
        let key = body.get("key").and_then(Value::as_str).unwrap_or_default();

        let current_key: Option<String> = sqlx::query_scalar("SELECT edit_key FROM files WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(claims.tid)
            .fetch_optional(&state.db)
            .await?
            .flatten();
        if current_key.as_deref() != Some(key) {
            tracing::warn!(%id, key, "callback for a stale editing session ignored");
            return Ok(());
        }

        match status {
            1 => {} // document is being edited
            2 => {
                let v = save_from_docs(&state, claims.tid, id, &body).await?;
                sqlx::query("UPDATE files SET edit_key = NULL WHERE id = $1 AND edit_key = $2").bind(id).bind(key).execute(&state.db).await?;
                state.audit(claims.tid, None, "file.saved", Some("file"), Some(id), json!({ "version": v })).await;
            }
            6 => {
                let v = save_from_docs(&state, claims.tid, id, &body).await?;
                state.audit(claims.tid, None, "file.forcesaved", Some("file"), Some(id), json!({ "version": v })).await;
            }
            4 => {
                sqlx::query("UPDATE files SET edit_key = NULL WHERE id = $1 AND edit_key = $2").bind(id).bind(key).execute(&state.db).await?;
            }
            3 | 7 => tracing::error!(%id, status, "document server reported a save error"),
            other => tracing::debug!(%id, other, "unhandled callback status"),
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => Json(json!({ "error": 0 })),
        Err(e) => {
            tracing::error!(%id, error = %e, "onlyoffice callback failed");
            // Non-zero makes the Document Server retry the callback later.
            Json(json!({ "error": 1 }))
        }
    }
}
