//! Knowledge base: a tree of pages shared by the portal's staff, written in a
//! visual editor (sanitized HTML in `content`, plain text in `body` for
//! full-text search). Page history, images and files inside pages, pinned
//! pages, archive instead of deletion, who is looking at a page now.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    files::{content_disposition, extension_of},
    kb_html,
    state::AppState,
    storage::BlobStore,
};

const MAX_CONTENT: usize = 1_000_000;
/// History kept per page.
const MAX_VERSIONS: i64 = 100;
/// Files and images uploaded into a page.
const MAX_FILE: usize = 50 * 1024 * 1024;

fn require_staff(user: &CurrentUser) -> ApiResult<()> {
    // Guests (external users) do not see the internal knowledge base.
    if user.role == "guest" {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

#[derive(Serialize, sqlx::FromRow)]
struct TreeRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    title: String,
    icon: String,
    sort_order: i32,
    pinned: bool,
    updated_at: DateTime<Utc>,
}

pub async fn tree(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let rows = sqlx::query_as::<_, TreeRow>(
        "SELECT id, parent_id, title, icon, sort_order, pinned, updated_at FROM kb_pages
          WHERE tenant_id = $1 AND archived_at IS NULL ORDER BY sort_order, lower(title)",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "pages": rows })))
}

#[derive(Serialize, sqlx::FromRow)]
struct PageRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    title: String,
    icon: String,
    content: Option<String>,
    version: i32,
    pinned: bool,
    archived_at: Option<DateTime<Utc>>,
    created_by: Option<Uuid>,
    created_by_name: Option<String>,
    updated_by: Option<Uuid>,
    updated_by_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn load(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<PageRow> {
    sqlx::query_as::<_, PageRow>(
        "SELECT p.id, p.parent_id, p.title, p.icon, p.content, p.version, p.pinned, p.archived_at,
                p.created_by, c.display_name AS created_by_name, p.updated_by, u.display_name AS updated_by_name, p.created_at, p.updated_at
           FROM kb_pages p LEFT JOIN users c ON c.id = p.created_by LEFT JOIN users u ON u.id = p.updated_by
          WHERE p.id = $1 AND p.tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)
}

async fn load_live(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<PageRow> {
    let p = load(state, user, id).await?;
    if p.archived_at.is_some() {
        return Err(ApiError::NotFound);
    }
    Ok(p)
}

pub async fn get(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let page = load(&state, &user, id).await?;
    let crumbs: Vec<(Uuid, String)> = sqlx::query_as(
        "WITH RECURSIVE up AS (
            SELECT id, parent_id, title, 0 AS depth FROM kb_pages WHERE id = $1
            UNION ALL
            SELECT p.id, p.parent_id, p.title, up.depth + 1 FROM kb_pages p JOIN up ON p.id = up.parent_id
         ) SELECT id, title FROM up WHERE id <> $1 ORDER BY depth DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let can_delete = user.is_admin() || page.created_by == Some(user.user_id);
    Ok(Json(json!({
        "page": page,
        "breadcrumbs": crumbs.into_iter().map(|(id, title)| json!({ "id": id, "title": title })).collect::<Vec<_>>(),
        "can_delete": can_delete,
        "viewers": presence(id, &user, false),
    })))
}

fn clean_title(t: &str) -> ApiResult<String> {
    let t = t.trim();
    if t.is_empty() || t.chars().count() > 200 {
        return bad_request("название страницы — от 1 до 200 символов");
    }
    Ok(t.to_string())
}

fn clean_icon(i: &str) -> ApiResult<String> {
    let i = i.trim();
    if i.chars().count() > 4 {
        return bad_request("значок — один эмодзи");
    }
    Ok(i.to_string())
}

/// Page text from the request: HTML from the editor, or Markdown (older
/// clients and imports) converted to HTML.
fn page_html(content: Option<&str>, body: Option<&str>) -> ApiResult<String> {
    let html = match (content, body) {
        (Some(c), _) => kb_html::sanitize(c),
        (None, Some(b)) => kb_html::from_markdown(b),
        (None, None) => String::new(),
    };
    if html.len() > MAX_CONTENT {
        return bad_request("страница слишком большая");
    }
    Ok(html)
}

async fn save_version(state: &AppState, id: Uuid, version: i32, title: &str, icon: &str, content: &str, author: Uuid) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO kb_page_versions (page_id, version, title, icon, content, author_id) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (page_id, version) DO NOTHING",
    )
    .bind(id)
    .bind(version)
    .bind(title)
    .bind(icon)
    .bind(content)
    .bind(author)
    .execute(&state.db)
    .await?;
    sqlx::query("DELETE FROM kb_page_versions WHERE page_id = $1 AND version <= $2 - $3")
        .bind(id)
        .bind(version)
        .bind(MAX_VERSIONS as i32)
        .execute(&state.db)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct NewPage {
    pub parent_id: Option<Uuid>,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    /// Markdown (kept for older clients and imports).
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub icon: String,
}

pub async fn create(State(state): State<AppState>, user: CurrentUser, Json(req): Json<NewPage>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let title = clean_title(&req.title)?;
    let icon = clean_icon(&req.icon)?;
    let html = page_html(req.content.as_deref(), req.body.as_deref())?;
    if let Some(p) = req.parent_id {
        load_live(&state, &user, p).await?;
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO kb_pages (tenant_id, parent_id, title, icon, content, body, sort_order, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6,
                 COALESCE((SELECT max(sort_order) + 1 FROM kb_pages WHERE tenant_id = $1 AND parent_id IS NOT DISTINCT FROM $2), 0),
                 $7, $7)
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(req.parent_id)
    .bind(&title)
    .bind(&icon)
    .bind(&html)
    .bind(kb_html::to_text(&html))
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    save_version(&state, id, 1, &title, &icon, &html, user.user_id).await?;
    state.audit(user.tenant_id, Some(user.user_id), "kb.page_created", Some("kb_page"), Some(id), json!({ "title": title })).await;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct UpdatePage {
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub icon: String,
    /// Version the editor started from: protects against overwriting a
    /// colleague's simultaneous changes (409 unless `force`).
    #[serde(default)]
    pub base_version: Option<i32>,
    /// Older clients: `updated_at` the editor started from.
    #[serde(default)]
    pub base_updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub force: bool,
}

pub async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<UpdatePage>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let current = load_live(&state, &user, id).await?;
    let stale = match (req.base_version, req.base_updated_at) {
        (Some(v), _) => v != current.version,
        // Compare at millisecond precision (JSON round trips may lose micros).
        (None, Some(base)) => (current.updated_at - base).num_milliseconds().abs() > 1,
        _ => false,
    };
    if stale && !req.force {
        return Err(ApiError::Conflict(format!(
            "страницу уже изменил(а) {} — обновите страницу и внесите правки заново",
            current.updated_by_name.unwrap_or_else(|| "другой пользователь".into())
        )));
    }
    let title = clean_title(&req.title)?;
    let icon = clean_icon(&req.icon)?;
    let html = page_html(req.content.as_deref(), req.body.as_deref())?;
    if current.content.as_deref() == Some(html.as_str()) && current.title == title && current.icon == icon {
        // Nothing changed: no new version.
        return Ok(Json(json!({ "page": current })));
    }
    let version: i32 = sqlx::query_scalar(
        "UPDATE kb_pages SET title = $2, content = $3, body = $4, icon = $5, updated_by = $6, updated_at = now(), version = version + 1
          WHERE id = $1 RETURNING version",
    )
    .bind(id)
    .bind(&title)
    .bind(&html)
    .bind(kb_html::to_text(&html))
    .bind(&icon)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    save_version(&state, id, version, &title, &icon, &html, user.user_id).await?;
    Ok(Json(json!({ "page": load(&state, &user, id).await? })))
}

#[derive(Deserialize)]
pub struct MovePage {
    pub parent_id: Option<Uuid>,
    pub sort_order: Option<i32>,
}

pub async fn move_page(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<MovePage>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    load_live(&state, &user, id).await?;
    if let Some(p) = req.parent_id {
        load_live(&state, &user, p).await?;
        // A page cannot move under itself or its own descendants.
        let cycle: bool = sqlx::query_scalar(
            "WITH RECURSIVE up AS (SELECT id, parent_id FROM kb_pages WHERE id = $1
                                   UNION ALL SELECT p.id, p.parent_id FROM kb_pages p JOIN up ON p.id = up.parent_id)
             SELECT EXISTS (SELECT 1 FROM up WHERE id = $2)",
        )
        .bind(p)
        .bind(id)
        .fetch_one(&state.db)
        .await?;
        if cycle {
            return bad_request("нельзя переместить страницу внутрь неё самой");
        }
    }
    sqlx::query("UPDATE kb_pages SET parent_id = $2, sort_order = COALESCE($3, sort_order) WHERE id = $1")
        .bind(id)
        .bind(req.parent_id)
        .bind(req.sort_order)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct Pin {
    pub pinned: bool,
}

/// `PATCH /kb/pages/{id}/pin`: pinned pages are listed first for everyone.
pub async fn pin(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<Pin>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    load_live(&state, &user, id).await?;
    sqlx::query("UPDATE kb_pages SET pinned = $2 WHERE id = $1").bind(id).bind(req.pinned).execute(&state.db).await?;
    Ok(Json(json!({ "pinned": req.pinned })))
}

// ---------------------------------------------------------------------------
// Archive
// ---------------------------------------------------------------------------

fn can_remove(user: &CurrentUser, page: &PageRow) -> ApiResult<()> {
    if user.is_admin() || page.created_by == Some(user.user_id) {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

/// `DELETE /kb/pages/{id}`: moves the page with its subpages to the archive;
/// `?permanent=true` on an archived page deletes it for good.
#[derive(Deserialize)]
pub struct DeleteQuery {
    #[serde(default)]
    pub permanent: bool,
}

pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Query(q): Query<DeleteQuery>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let page = load(&state, &user, id).await?;
    can_remove(&user, &page)?;
    if q.permanent {
        if page.archived_at.is_none() {
            return bad_request("сначала переместите страницу в архив");
        }
        let keys: Vec<String> = sqlx::query_scalar(
            "WITH RECURSIVE down AS (SELECT id FROM kb_pages WHERE id = $1 UNION ALL SELECT p.id FROM kb_pages p JOIN down ON p.parent_id = down.id)
             SELECT storage_key FROM kb_files WHERE page_id IN (SELECT id FROM down)",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await?;
        sqlx::query("DELETE FROM kb_pages WHERE id = $1").bind(id).execute(&state.db).await?;
        for k in keys {
            if let Err(e) = state.store.delete(&k).await {
                tracing::warn!(error = ?e, "failed to delete a knowledge base file");
            }
        }
        state.audit(user.tenant_id, Some(user.user_id), "kb.page_deleted", Some("kb_page"), Some(id), json!({ "title": page.title })).await;
        return Ok(Json(json!({ "deleted": true })));
    }
    if page.archived_at.is_some() {
        return Ok(Json(json!({ "archived": 0 })));
    }
    // The page and its live subpages share one archive time, so «restore»
    // brings back exactly what was archived together.
    let n: i64 = sqlx::query_scalar(
        "WITH RECURSIVE down AS (SELECT id FROM kb_pages WHERE id = $1
                                 UNION ALL SELECT p.id FROM kb_pages p JOIN down ON p.parent_id = down.id WHERE p.archived_at IS NULL),
              upd AS (UPDATE kb_pages SET archived_at = now(), archived_by = $2, pinned = false WHERE id IN (SELECT id FROM down) RETURNING 1)
         SELECT count(*) FROM upd",
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "kb.page_archived", Some("kb_page"), Some(id), json!({ "title": page.title, "pages": n })).await;
    Ok(Json(json!({ "archived": n })))
}

/// `POST /kb/pages/{id}/restore`: back from the archive (with the subpages
/// archived together with it); a page whose parent is archived returns to the top level.
pub async fn restore(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let page = load(&state, &user, id).await?;
    let Some(at) = page.archived_at else {
        return Ok(Json(json!({ "restored": 0 })));
    };
    let n: i64 = sqlx::query_scalar(
        "WITH RECURSIVE down AS (SELECT id FROM kb_pages WHERE id = $1
                                 UNION ALL SELECT p.id FROM kb_pages p JOIN down ON p.parent_id = down.id WHERE p.archived_at = $2),
              upd AS (UPDATE kb_pages SET archived_at = NULL, archived_by = NULL WHERE id IN (SELECT id FROM down) RETURNING 1)
         SELECT count(*) FROM upd",
    )
    .bind(id)
    .bind(at)
    .fetch_one(&state.db)
    .await?;
    sqlx::query(
        "UPDATE kb_pages SET parent_id = NULL WHERE id = $1
            AND EXISTS (SELECT 1 FROM kb_pages p WHERE p.id = kb_pages.parent_id AND p.archived_at IS NOT NULL)",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "kb.page_restored", Some("kb_page"), Some(id), json!({ "title": page.title, "pages": n })).await;
    Ok(Json(json!({ "restored": n })))
}

/// `GET /kb/archive`: archived pages (the ones archived directly, not their subpages).
pub async fn archive(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let rows: Vec<(Uuid, String, String, DateTime<Utc>, Option<String>, Option<Uuid>, i64)> = sqlx::query_as(
        "SELECT p.id, p.title, p.icon, p.archived_at, u.display_name, p.created_by,
                (SELECT count(*) FROM kb_pages c WHERE c.parent_id = p.id AND c.archived_at = p.archived_at)
           FROM kb_pages p LEFT JOIN users u ON u.id = p.archived_by
          WHERE p.tenant_id = $1 AND p.archived_at IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM kb_pages par WHERE par.id = p.parent_id AND par.archived_at = p.archived_at)
          ORDER BY p.archived_at DESC",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "pages": rows.into_iter().map(|(id, title, icon, at, by, creator, children)| json!({
        "id": id, "title": title, "icon": icon, "archived_at": at, "archived_by_name": by, "subpages": children,
        "can_delete": user.is_admin() || creator == Some(user.user_id),
    })).collect::<Vec<_>>() })))
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

pub async fn versions(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    load(&state, &user, id).await?;
    let rows: Vec<(i32, String, Option<String>, DateTime<Utc>, i32)> = sqlx::query_as(
        "SELECT v.version, v.title, u.display_name, v.created_at, length(v.content)
           FROM kb_page_versions v LEFT JOIN users u ON u.id = v.author_id WHERE v.page_id = $1 ORDER BY v.version DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "versions": rows.into_iter().map(|(v, t, a, at, len)| json!({
        "version": v, "title": t, "author_name": a, "created_at": at, "length": len
    })).collect::<Vec<_>>() })))
}

pub async fn version(State(state): State<AppState>, user: CurrentUser, Path((id, v)): Path<(Uuid, i32)>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    load(&state, &user, id).await?;
    let (title, icon, content, author, at): (String, String, String, Option<String>, DateTime<Utc>) = sqlx::query_as(
        "SELECT v.title, v.icon, v.content, u.display_name, v.created_at FROM kb_page_versions v LEFT JOIN users u ON u.id = v.author_id
          WHERE v.page_id = $1 AND v.version = $2",
    )
    .bind(id)
    .bind(v)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(json!({ "version": v, "title": title, "icon": icon, "content": content, "author_name": author, "created_at": at })))
}

/// `POST /kb/pages/{id}/versions/{v}/restore`: the old text becomes the newest version.
pub async fn restore_version(State(state): State<AppState>, user: CurrentUser, Path((id, v)): Path<(Uuid, i32)>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    load_live(&state, &user, id).await?;
    let (title, icon, content): (String, String, String) =
        sqlx::query_as("SELECT title, icon, content FROM kb_page_versions WHERE page_id = $1 AND version = $2")
            .bind(id)
            .bind(v)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let version: i32 = sqlx::query_scalar(
        "UPDATE kb_pages SET title = $2, icon = $3, content = $4, body = $5, updated_by = $6, updated_at = now(), version = version + 1
          WHERE id = $1 RETURNING version",
    )
    .bind(id)
    .bind(&title)
    .bind(&icon)
    .bind(&content)
    .bind(kb_html::to_text(&content))
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    save_version(&state, id, version, &title, &icon, &content, user.user_id).await?;
    state.audit(user.tenant_id, Some(user.user_id), "kb.version_restored", Some("kb_page"), Some(id), json!({ "title": title, "from": v })).await;
    Ok(Json(json!({ "page": load(&state, &user, id).await? })))
}

// ---------------------------------------------------------------------------
// Files and images inside pages
// ---------------------------------------------------------------------------

/// Images shown inline; everything else is downloaded.
pub fn inline_type(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// `POST /kb/pages/{id}/files` (multipart, one file): an image or a file for the page.
pub async fn upload(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, mut multipart: Multipart) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    load_live(&state, &user, id).await?;
    let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? else {
        return bad_request("no file");
    };
    let name = crate::files::validate_name(field.file_name().unwrap_or("файл"))?;
    let data: Bytes = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if data.is_empty() || data.len() > MAX_FILE {
        return Err(ApiError::TooLarge("file is larger than 50 MB".into()));
    }
    crate::files::check_quota(&state, user.tenant_id, data.len() as i64).await?;
    let fid = Uuid::new_v4();
    let key = format!("{}/kb/{}", user.tenant_id, fid.simple());
    let size = data.len() as i64;
    state.store.put(&key, data).await?;
    let ext = extension_of(&name);
    let created_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO kb_files (id, tenant_id, page_id, name, extension, size_bytes, storage_key, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING created_at",
    )
    .bind(fid)
    .bind(user.tenant_id)
    .bind(id)
    .bind(&name)
    .bind(&ext)
    .bind(size)
    .bind(&key)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({
        "id": fid, "name": name, "extension": ext, "size_bytes": size, "url": format!("/api/kb/files/{fid}"),
        "image": inline_type(&ext).is_some(), "author_name": user.display_name, "created_at": created_at,
    })))
}

#[derive(Deserialize, Default)]
pub struct FileQuery {
    #[serde(default)]
    pub download: Option<String>,
}

pub fn kb_file_response(name: &str, ext: &str, data: Bytes, download: bool) -> Response {
    let inline = inline_type(ext).filter(|_| !download);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, inline.unwrap_or("application/octet-stream"))
        .header(header::CONTENT_DISPOSITION, content_disposition(name, inline.is_some()))
        .header(header::CONTENT_LENGTH, data.len())
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(data))
        .expect("valid response")
}

/// `GET /kb/files/{id}`: images inline, other files as a download.
pub async fn file(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Query(q): Query<FileQuery>) -> ApiResult<Response> {
    require_staff(&user)?;
    let (name, ext, key): (String, String, String) =
        sqlx::query_as("SELECT name, extension, storage_key FROM kb_files WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let data = state.store.get(&key).await?;
    Ok(kb_file_response(&name, &ext, data, q.download.is_some()))
}

// ---------------------------------------------------------------------------
// Who is looking at a page now
// ---------------------------------------------------------------------------

/// page → user → (last heartbeat, editing, name)
type Presence = HashMap<Uuid, HashMap<Uuid, (Instant, bool, String)>>;
const PRESENCE_TTL: Duration = Duration::from_secs(45);

fn presence_map() -> &'static Mutex<Presence> {
    static P: OnceLock<Mutex<Presence>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a heartbeat (when `touch`) and list the others on the page.
fn presence(page: Uuid, user: &CurrentUser, touch: bool) -> Vec<Value> {
    presence_with(page, user, touch, false)
}

fn presence_with(page: Uuid, user: &CurrentUser, touch: bool, editing: bool) -> Vec<Value> {
    let mut map = presence_map().lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    map.retain(|_, users| {
        users.retain(|_, (t, _, _)| now.duration_since(*t) < PRESENCE_TTL);
        !users.is_empty()
    });
    if touch {
        map.entry(page).or_default().insert(user.user_id, (now, editing, user.display_name.clone()));
    }
    map.get(&page)
        .map(|users| {
            let mut v: Vec<_> = users
                .iter()
                .filter(|(id, _)| **id != user.user_id)
                .map(|(id, (_, editing, name))| json!({ "id": id, "name": name, "editing": editing }))
                .collect();
            v.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            v
        })
        .unwrap_or_default()
}

#[derive(Deserialize)]
pub struct Heartbeat {
    #[serde(default)]
    pub editing: bool,
    /// Leaving the page.
    #[serde(default)]
    pub leave: bool,
}

/// `POST /kb/pages/{id}/presence {editing, leave}`: heartbeat every ~20 s;
/// returns the others looking at the page and whether they are editing.
pub async fn heartbeat(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<Heartbeat>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let version: i32 = sqlx::query_scalar("SELECT version FROM kb_pages WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if req.leave {
        let mut map = presence_map().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(users) = map.get_mut(&id) {
            users.remove(&user.user_id);
        }
        return Ok(Json(json!({ "viewers": [] })));
    }
    Ok(Json(json!({ "viewers": presence_with(id, &user, true, req.editing), "version": version })))
}

// ---------------------------------------------------------------------------
// Search and conversion
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct Hit {
    id: Uuid,
    title: String,
    icon: String,
    snippet: String,
    updated_at: DateTime<Utc>,
}

pub async fn search(State(state): State<AppState>, user: CurrentUser, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let text = q.q.trim();
    if text.is_empty() {
        return Ok(Json(json!({ "results": [] })));
    }
    // Stemmed full-text match, plus a substring match on titles for short or partial words.
    let rows = sqlx::query_as::<_, Hit>(
        "SELECT id, title, icon,
                ts_headline('russian', body, websearch_to_tsquery('russian', $2),
                            'StartSel=«, StopSel=», MaxWords=24, MinWords=8, MaxFragments=1') AS snippet,
                updated_at
           FROM kb_pages
          WHERE tenant_id = $1 AND archived_at IS NULL AND (search @@ websearch_to_tsquery('russian', $2) OR title ILIKE '%' || $3 || '%')
          ORDER BY ts_rank(search, websearch_to_tsquery('russian', $2)) DESC, updated_at DESC
          LIMIT 30",
    )
    .bind(user.tenant_id)
    .bind(text)
    .bind(text.replace('%', "\\%").replace('_', "\\_"))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "results": rows })))
}

/// Convert pages written in Markdown (before the visual editor) to HTML;
/// runs at start, idempotent. The Markdown text becomes version 1 of the history.
pub async fn convert_markdown(state: &AppState) -> anyhow::Result<usize> {
    let rows: Vec<(Uuid, String, String, String, Option<Uuid>, i32)> = sqlx::query_as(
        "SELECT id, title, icon, body, COALESCE(updated_by, created_by), version FROM kb_pages WHERE content IS NULL",
    )
    .fetch_all(&state.db)
    .await?;
    let n = rows.len();
    for (id, title, icon, body, author, version) in rows {
        let html = kb_html::from_markdown(&body);
        sqlx::query("UPDATE kb_pages SET content = $2, body = $3 WHERE id = $1 AND content IS NULL")
            .bind(id)
            .bind(&html)
            .bind(kb_html::to_text(&html))
            .execute(&state.db)
            .await?;
        sqlx::query(
            "INSERT INTO kb_page_versions (page_id, version, title, icon, content, author_id) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(version)
        .bind(&title)
        .bind(&icon)
        .bind(&html)
        .bind(author)
        .execute(&state.db)
        .await?;
    }
    if n > 0 {
        tracing::info!(pages = n, "knowledge base pages converted from Markdown");
    }
    Ok(n)
}
