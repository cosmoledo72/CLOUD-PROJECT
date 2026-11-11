//! «Администрирование → Резервные копии»: the state of the backup service,
//! the list of backups, «make a backup now» and downloads.
//!
//! Backups are made by the `backup` service (`scripts/backup.sh`), not by the
//! backend; both share the backup folder (`BACKUP_DIR`). They talk through files:
//!
//! | file          | written by | meaning                                                    |
//! |---------------|------------|------------------------------------------------------------|
//! | `STATUS`      | service    | heartbeat: `alive=<unix time>` every minute                 |
//! | `LAST_RESULT` | service    | last attempt: `result=ok|error`, `time`, `name`, `error`    |
//! | `REQUEST`     | backend    | «make a backup now»; the service takes it within a minute  |
//!
//! A backup holds every portal of the server, so the section is open only to
//! administrators of the portal named in `BACKUP_PORTAL`.

use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use bytes::Bytes;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

const PREFIX: &str = "cloud-project-";
/// The service is considered down when its heartbeat is older than this.
const ALIVE_SECS: u64 = 600;
/// A backup newer than this counts as current (a nightly one plus slack).
const FRESH_SECS: u64 = 26 * 3600;
/// A folder without COMPLETE younger than this is still being written.
const RUNNING_SECS: u64 = 3 * 3600;
/// Files of a backup that may be downloaded.
const PARTS: [&str; 2] = ["db.dump", "files.tar.gz"];

/// May this user open the section: an administrator of the backup portal.
pub fn allowed(state: &AppState, tenant_slug: &str, role: &str) -> bool {
    matches!(role, "owner" | "admin") && state.cfg.backup_portal.as_deref() == Some(&tenant_slug.to_lowercase())
}

async fn require(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    if state.cfg.backup_portal.is_none() || !user.is_admin() {
        return Err(ApiError::NotFound);
    }
    let slug: String = sqlx::query_scalar("SELECT slug::text FROM tenants WHERE id = $1").bind(user.tenant_id).fetch_one(&state.db).await?;
    if allowed(state, &slug, "admin") { Ok(()) } else { Err(ApiError::NotFound) }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn mtime_secs(p: &FsPath) -> Option<u64> {
    std::fs::metadata(p).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// `key=value` lines of a small status file.
fn read_kv(p: &FsPath) -> HashMap<String, String> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

/// A backup folder name the service makes: `cloud-project-YYYY-MM-DD_HHMM[-SS]`.
pub fn valid_name(name: &str) -> bool {
    name.strip_prefix(PREFIX).is_some_and(|rest| {
        rest.len() >= 15 && rest.len() <= 18 && rest.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '_')
    })
}

#[derive(Serialize)]
struct BackupItem {
    name: String,
    /// `ready` | `running` | `unfinished`
    state: &'static str,
    /// Local time the service wrote in COMPLETE («2026-11-12 03:00:41 Asia/Sakhalin»), if ready.
    created: Option<String>,
    /// Unix time of the folder (COMPLETE when ready), for sorting and age.
    time: u64,
    db_bytes: Option<u64>,
    files_bytes: Option<u64>,
}

fn scan(dir: &FsPath) -> Vec<BackupItem> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let now = now_secs();
    let mut out: Vec<BackupItem> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !valid_name(&name) {
                return None;
            }
            let path = e.path();
            let complete = path.join("COMPLETE");
            let size = |f: &str| std::fs::metadata(path.join(f)).ok().map(|m| m.len());
            let (state, created, time) = if complete.is_file() {
                ("ready", read_kv(&complete).get("created").cloned(), mtime_secs(&complete).unwrap_or(0))
            } else {
                let t = mtime_secs(&path).unwrap_or(0);
                (if now.saturating_sub(t) < RUNNING_SECS { "running" } else { "unfinished" }, None, t)
            };
            Some(BackupItem { name, state, created, time, db_bytes: size("db.dump"), files_bytes: size("files.tar.gz") })
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

fn status(state: &AppState) -> Value {
    let dir = PathBuf::from(&state.cfg.backup_dir);
    let now = now_secs();
    let items = scan(&dir);
    let heartbeat = read_kv(&dir.join("STATUS")).get("alive").and_then(|v| v.parse::<u64>().ok());
    let alive = heartbeat.is_some_and(|t| now.saturating_sub(t) < ALIVE_SECS);
    let last = read_kv(&dir.join("LAST_RESULT"));
    let last_ok = items.iter().find(|i| i.state == "ready");
    let request = dir.join("REQUEST");
    let requested_at = if request.is_file() { mtime_secs(&request) } else { None };
    let mut warnings: Vec<String> = Vec::new();
    if !dir.is_dir() {
        warnings.push("Папка резервных копий недоступна серверу портала — проверьте подключение папки backups к сервису backend".into());
    } else if !alive {
        warnings.push("Сервис резервного копирования не отвечает больше 10 минут — проверьте, запущен ли он: docker compose ps backup".into());
    }
    match last_ok {
        None => warnings.push("Готовых резервных копий нет".into()),
        Some(b) if now.saturating_sub(b.time) > FRESH_SECS => {
            warnings.push(format!("Последняя готовая копия сделана более {} ч назад", now.saturating_sub(b.time) / 3600))
        }
        _ => {}
    }
    if last.get("result").map(String::as_str) == Some("error") {
        warnings.push(format!("Последняя попытка ({}) завершилась ошибкой: {}", last.get("time").cloned().unwrap_or_default(), last.get("error").cloned().unwrap_or_default()));
    }
    json!({
        "available": dir.is_dir(),
        "alive": alive,
        "heartbeat": heartbeat,
        "now": now,
        "hour": state.cfg.backup_hour,
        "keep_days": state.cfg.backup_keep_days,
        "requested_at": requested_at,
        "running": items.iter().any(|i| i.state == "running"),
        "last_result": last,
        "warnings": warnings,
        "backups": items,
    })
}

/// `GET /api/admin/backups`
pub async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require(&state, &user).await?;
    Ok(Json(status(&state)))
}

/// `POST /api/admin/backups/now` — ask the service for a backup right away.
pub async fn request_now(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require(&state, &user).await?;
    let dir = PathBuf::from(&state.cfg.backup_dir);
    if !dir.is_dir() {
        return Err(ApiError::BadRequest("папка резервных копий недоступна серверу портала".into()));
    }
    let request = dir.join("REQUEST");
    if !request.exists() {
        let body = format!("by={}\nat={}\n", user.email, now_secs());
        tokio::fs::write(&request, body)
            .await
            .map_err(|e| ApiError::BadRequest(format!("не удалось записать заявку в папку копий: {e}")))?;
        state.audit(user.tenant_id, Some(user.user_id), "backup.requested", Some("backup"), None, json!({})).await;
    }
    Ok(Json(status(&state)))
}

/// `GET /api/admin/backups/{name}/{file}` — download `db.dump` or `files.tar.gz`
/// of a ready backup, streamed from disk.
pub async fn download(State(state): State<AppState>, user: CurrentUser, Path((name, part)): Path<(String, String)>) -> ApiResult<Response> {
    require(&state, &user).await?;
    if !valid_name(&name) || !PARTS.contains(&part.as_str()) {
        return Err(ApiError::NotFound);
    }
    let folder = PathBuf::from(&state.cfg.backup_dir).join(&name);
    if !folder.join("COMPLETE").is_file() {
        return Err(ApiError::NotFound);
    }
    let path = folder.join(&part);
    let mut file = tokio::fs::File::open(&path).await.map_err(|_| ApiError::NotFound)?;
    let len = file.metadata().await.map_err(|_| ApiError::NotFound)?.len();
    state
        .audit(user.tenant_id, Some(user.user_id), "backup.downloaded", Some("backup"), None, json!({ "backup": name, "file": part, "bytes": len }))
        .await;

    // Stream in chunks: a files archive can be gigabytes.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            match file.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(Ok(Bytes::copy_from_slice(&buf[..n]))).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    break;
                }
            }
        }
    });
    let body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx));
    let download_name = format!("{name}_{part}");
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, crate::files::content_disposition(&download_name, false))
        .header(header::CONTENT_LENGTH, len)
        .body(body)
        .expect("valid response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names() {
        assert!(valid_name("cloud-project-2026-11-12_0300"));
        assert!(valid_name("cloud-project-2026-11-12_0300-41"));
        assert!(!valid_name("cloud-project-../../etc"));
        assert!(!valid_name("cloud-project-2026-11-12_0300/x"));
        assert!(!valid_name("other-2026-11-12_0300"));
        assert!(!valid_name("cloud-project-"));
    }
}
