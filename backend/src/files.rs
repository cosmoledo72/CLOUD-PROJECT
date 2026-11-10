//! Document management: folders, files, versions and shares.

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    Json,
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    acl::{self, Access},
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
    storage::BlobStore,
};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct FolderRow {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub kind: String,
    pub name: String,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct FileRow {
    pub id: Uuid,
    pub folder_id: Uuid,
    pub name: String,
    pub extension: String,
    pub owner_id: Uuid,
    pub owner_name: String,
    pub current_version: i32,
    pub size_bytes: i64,
    pub is_editing: bool,
    pub updated_at: DateTime<Utc>,
}

const FILE_SELECT: &str = "SELECT f.id, f.folder_id, f.name, f.extension, f.owner_id, u.display_name AS owner_name,
        f.current_version, f.size_bytes, (f.edit_key IS NOT NULL) AS is_editing, f.updated_at
   FROM files f JOIN users u ON u.id = f.owner_id";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn validate_name(name: &str) -> ApiResult<String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 255 || n == "." || n == ".." || n.contains(['/', '\\', '\0']) {
        return bad_request("invalid name");
    }
    Ok(n.to_string())
}

pub fn extension_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

/// Create the tenant-wide "Common documents" root if it does not exist yet.
pub async fn ensure_common_root(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, owner_id: Uuid) -> ApiResult<Uuid> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM folders WHERE tenant_id = $1 AND kind = 'common' AND parent_id IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(id);
    }
    Ok(sqlx::query_scalar(
        "INSERT INTO folders (tenant_id, kind, name, owner_id) VALUES ($1, 'common', 'Common documents', $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(owner_id)
    .fetch_one(&mut **tx)
    .await?)
}

async fn ensure_my_root(state: &AppState, user: &CurrentUser) -> ApiResult<Uuid> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM folders WHERE tenant_id = $1 AND kind = 'my' AND owner_id = $2 AND parent_id IS NULL",
    )
    .bind(user.tenant_id)
    .bind(user.user_id)
    .fetch_optional(&state.db)
    .await?
    {
        return Ok(id);
    }
    Ok(sqlx::query_scalar(
        "INSERT INTO folders (tenant_id, kind, name, owner_id) VALUES ($1, 'my', 'My documents', $2) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?)
}

async fn check_quota(state: &AppState, tenant_id: Uuid, extra: i64) -> ApiResult<()> {
    let (used, quota): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE((SELECT sum(fv.size_bytes) FROM file_versions fv JOIN files f ON f.id = fv.file_id
                           WHERE f.tenant_id = $1), 0)::bigint, quota_bytes
           FROM tenants WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&state.db)
    .await?;
    if used + extra > quota {
        return Err(ApiError::TooLarge("storage quota exceeded".into()));
    }
    Ok(())
}

/// Store a blob and register it as the next version of an existing file.
pub async fn add_version(state: &AppState, tenant_id: Uuid, file_id: Uuid, author: Option<Uuid>, data: Bytes) -> ApiResult<i32> {
    check_quota(state, tenant_id, data.len() as i64).await?;
    let sha = Sha256::digest(&data).to_vec();
    let size = data.len() as i64;
    let mut tx = state.db.begin().await?;
    // Lock the file row so concurrent saves get sequential version numbers.
    let current: i32 = sqlx::query_scalar("SELECT current_version FROM files WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(file_id)
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let version = current + 1;
    let key = format!("{tenant_id}/{file_id}/{version}-{}", Uuid::new_v4().simple());
    state.store.put(&key, data).await?;
    sqlx::query(
        "INSERT INTO file_versions (file_id, version, storage_key, size_bytes, sha256, author_id) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(file_id)
    .bind(version)
    .bind(&key)
    .bind(size)
    .bind(&sha)
    .bind(author)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE files SET current_version = $2, size_bytes = $3, updated_at = now() WHERE id = $1")
        .bind(file_id)
        .bind(version)
        .bind(size)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(version)
}

/// Create a new file (version 1) in a folder.
pub async fn create_file(state: &AppState, user: &CurrentUser, folder_id: Uuid, name: &str, data: Bytes) -> ApiResult<Uuid> {
    check_quota(state, user.tenant_id, data.len() as i64).await?;
    let file_id = Uuid::new_v4();
    let key = format!("{}/{file_id}/1-{}", user.tenant_id, Uuid::new_v4().simple());
    let sha = Sha256::digest(&data).to_vec();
    let size = data.len() as i64;
    state.store.put(&key, data).await?;

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO files (id, tenant_id, folder_id, name, extension, owner_id, current_version, size_bytes)
         VALUES ($1, $2, $3, $4, $5, $6, 1, $7)",
    )
    .bind(file_id)
    .bind(user.tenant_id)
    .bind(folder_id)
    .bind(name)
    .bind(extension_of(name))
    .bind(user.user_id)
    .bind(size)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from)?;
    sqlx::query(
        "INSERT INTO file_versions (file_id, version, storage_key, size_bytes, sha256, author_id) VALUES ($1, 1, $2, $3, $4, $5)",
    )
    .bind(file_id)
    .bind(&key)
    .bind(size)
    .bind(&sha)
    .bind(user.user_id)
    .execute(&mut *tx)
    .await?;
    if let Err(e) = tx.commit().await {
        let _ = state.store.delete(&key).await;
        return Err(e.into());
    }
    state.audit(user.tenant_id, Some(user.user_id), "file.created", Some("file"), Some(file_id), json!({ "name": name })).await;
    Ok(file_id)
}

fn content_disposition(name: &str, inline: bool) -> HeaderValue {
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let kind = if inline { "inline" } else { "attachment" };
    HeaderValue::from_str(&format!("{kind}; filename*=UTF-8''{encoded}")).unwrap_or(HeaderValue::from_static("attachment"))
}

/// Load the blob of a given (or current) file version.
pub async fn read_version(state: &AppState, tenant_id: Uuid, file_id: Uuid, version: Option<i32>) -> ApiResult<(String, Bytes)> {
    let (name, key): (String, String) = sqlx::query_as(
        "SELECT f.name, fv.storage_key FROM files f
           JOIN file_versions fv ON fv.file_id = f.id AND fv.version = COALESCE($3, f.current_version)
          WHERE f.id = $1 AND f.tenant_id = $2 AND f.deleted_at IS NULL",
    )
    .bind(file_id)
    .bind(tenant_id)
    .bind(version)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok((name, state.store.get(&key).await?))
}

pub fn file_response(name: &str, data: Bytes, inline: bool) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, content_disposition(name, inline))
        .header(header::CONTENT_LENGTH, data.len())
        .body(Body::from(data))
        .expect("valid response")
}

// ---------------------------------------------------------------------------
// Folder endpoints
// ---------------------------------------------------------------------------

pub async fn roots(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let my = if user.is_guest() { None } else { Some(ensure_my_root(&state, &user).await?) };
    let common: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM folders WHERE tenant_id = $1 AND kind = 'common' AND parent_id IS NULL",
    )
    .bind(user.tenant_id)
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(json!({ "my": my, "common": common })))
}

#[derive(Serialize, sqlx::FromRow)]
struct Crumb {
    id: Uuid,
    name: String,
}

pub async fn get_folder(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let access = acl::folder_access(&state.db, &user, id).await?;
    access.require(Access::Read)?;
    let folder = sqlx::query_as::<_, FolderRow>(
        "SELECT id, parent_id, kind, name, owner_id, created_at FROM folders WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let breadcrumbs = sqlx::query_as::<_, Crumb>(
        "WITH RECURSIVE chain AS (
             SELECT id, parent_id, name, 0 AS depth FROM folders WHERE id = $1
             UNION ALL
             SELECT f.id, f.parent_id, f.name, c.depth + 1 FROM folders f JOIN chain c ON f.id = c.parent_id WHERE c.depth < 64
         ) SELECT id, name FROM chain ORDER BY depth DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let folders = sqlx::query_as::<_, FolderRow>(
        "SELECT id, parent_id, kind, name, owner_id, created_at FROM folders
          WHERE tenant_id = $1 AND parent_id = $2 AND deleted_at IS NULL ORDER BY lower(name)",
    )
    .bind(user.tenant_id)
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let files = sqlx::query_as::<_, FileRow>(&format!(
        "{FILE_SELECT} WHERE f.tenant_id = $1 AND f.folder_id = $2 AND f.deleted_at IS NULL ORDER BY lower(f.name)"
    ))
    .bind(user.tenant_id)
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "folder": folder, "access": access, "breadcrumbs": breadcrumbs, "folders": folders, "files": files })))
}

#[derive(Deserialize)]
pub struct CreateFolder {
    pub parent_id: Uuid,
    pub name: String,
}

pub async fn create_folder(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CreateFolder>) -> ApiResult<Json<Value>> {
    acl::folder_access(&state.db, &user, req.parent_id).await?.require(Access::Edit)?;
    let name = validate_name(&req.name)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO folders (tenant_id, parent_id, name, owner_id) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(req.parent_id)
    .bind(&name)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "folder.created", Some("folder"), Some(id), json!({ "name": name })).await;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub name: Option<String>,
    /// Move the folder under another folder (drag and drop in the browser).
    pub parent_id: Option<Uuid>,
}

pub async fn rename_folder(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RenameRequest>,
) -> ApiResult<Json<Value>> {
    acl::folder_access(&state.db, &user, id).await?.require(Access::Full)?;
    let is_root: bool = sqlx::query_scalar("SELECT parent_id IS NULL FROM folders WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if is_root {
        return bad_request("root folders cannot be renamed or moved");
    }
    if let Some(target) = req.parent_id {
        acl::folder_access(&state.db, &user, target).await?.require(Access::Edit)?;
        // A folder cannot go into itself or into one of its own subfolders.
        let cycle: bool = sqlx::query_scalar(
            "WITH RECURSIVE up AS (SELECT id, parent_id FROM folders WHERE id = $1
                                   UNION ALL SELECT f.id, f.parent_id FROM folders f JOIN up ON f.id = up.parent_id)
             SELECT EXISTS (SELECT 1 FROM up WHERE id = $2)",
        )
        .bind(target)
        .bind(id)
        .fetch_one(&state.db)
        .await?;
        if cycle {
            return bad_request("a folder cannot be moved into itself");
        }
        sqlx::query("UPDATE folders SET parent_id = $3 WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .bind(target)
            .execute(&state.db)
            .await?;
        state.audit(user.tenant_id, Some(user.user_id), "folder.moved", Some("folder"), Some(id), json!({ "to": target })).await;
    }
    if let Some(name) = req.name {
        let name = validate_name(&name)?;
        sqlx::query("UPDATE folders SET name = $3 WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .bind(&name)
            .execute(&state.db)
            .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_folder(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    acl::folder_access(&state.db, &user, id).await?.require(Access::Full)?;
    // Soft delete; a background job purges blobs after the retention period (SPEC, phase 2).
    let n = sqlx::query("UPDATE folders SET deleted_at = now(), deleted_by = $3 WHERE id = $1 AND tenant_id = $2 AND parent_id IS NOT NULL")
        .bind(id)
        .bind(user.tenant_id)
        .bind(user.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return bad_request("root folders cannot be deleted");
    }
    state.audit(user.tenant_id, Some(user.user_id), "folder.deleted", Some("folder"), Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// File endpoints
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UploadQuery {
    /// When a file with the same name exists, store the upload as its new version.
    #[serde(default)]
    pub overwrite: bool,
}

pub async fn upload(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(folder_id): Path<Uuid>,
    Query(q): Query<UploadQuery>,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    acl::folder_access(&state.db, &user, folder_id).await?.require(Access::Edit)?;
    let mut created = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        let Some(file_name) = field.file_name().map(str::to_string) else { continue };
        let name = validate_name(&file_name)?;
        let data = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM files WHERE folder_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL",
        )
        .bind(folder_id)
        .bind(&name)
        .fetch_optional(&state.db)
        .await?;
        let id = match existing {
            Some(id) if q.overwrite => {
                acl::file_access(&state.db, &user, id).await?.0.require(Access::Edit)?;
                add_version(&state, user.tenant_id, id, Some(user.user_id), data).await?;
                id
            }
            Some(_) => return Err(ApiError::Conflict(format!("file '{name}' already exists"))),
            None => create_file(&state, &user, folder_id, &name, data).await?,
        };
        created.push(id);
    }
    Ok(Json(json!({ "ids": created })))
}

#[derive(Deserialize)]
pub struct NewDocRequest {
    pub kind: String,
    pub name: String,
}

static TEMPLATE_DOCX: &[u8] = include_bytes!("../assets/templates/new.docx");
static TEMPLATE_XLSX: &[u8] = include_bytes!("../assets/templates/new.xlsx");
static TEMPLATE_PPTX: &[u8] = include_bytes!("../assets/templates/new.pptx");

/// Create a blank document from a bundled template.
pub async fn new_document(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(folder_id): Path<Uuid>,
    Json(req): Json<NewDocRequest>,
) -> ApiResult<Json<Value>> {
    acl::folder_access(&state.db, &user, folder_id).await?.require(Access::Edit)?;
    let template: &'static [u8] = match req.kind.as_str() {
        "docx" => TEMPLATE_DOCX,
        "xlsx" => TEMPLATE_XLSX,
        "pptx" => TEMPLATE_PPTX,
        _ => return bad_request("kind must be docx, xlsx or pptx"),
    };
    let mut name = validate_name(&req.name)?;
    if extension_of(&name) != req.kind {
        name = format!("{name}.{}", req.kind);
    }
    let id = create_file(&state, &user, folder_id, &name, Bytes::from_static(template)).await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn get_file(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (access, _) = acl::file_access(&state.db, &user, id).await?;
    access.require(Access::Read)?;
    let file = sqlx::query_as::<_, FileRow>(&format!("{FILE_SELECT} WHERE f.id = $1 AND f.tenant_id = $2"))
        .bind(id)
        .bind(user.tenant_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "file": file, "access": access })))
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    pub version: Option<i32>,
    #[serde(default)]
    pub inline: bool,
}

pub async fn download(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<DownloadQuery>,
) -> ApiResult<Response> {
    acl::file_access(&state.db, &user, id).await?.0.require(Access::Read)?;
    let (name, data) = read_version(&state, user.tenant_id, id, q.version).await?;
    Ok(file_response(&name, data, q.inline))
}

#[derive(Deserialize)]
pub struct UpdateFile {
    pub name: Option<String>,
    pub folder_id: Option<Uuid>,
}

pub async fn update_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateFile>,
) -> ApiResult<Json<Value>> {
    acl::file_access(&state.db, &user, id).await?.0.require(Access::Full)?;
    if let Some(target) = req.folder_id {
        acl::folder_access(&state.db, &user, target).await?.require(Access::Edit)?;
        sqlx::query("UPDATE files SET folder_id = $3 WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .bind(target)
            .execute(&state.db)
            .await?;
        state.audit(user.tenant_id, Some(user.user_id), "file.moved", Some("file"), Some(id), json!({ "to": target })).await;
    }
    if let Some(name) = req.name {
        let name = validate_name(&name)?;
        sqlx::query("UPDATE files SET name = $3, extension = $4, updated_at = now() WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .bind(&name)
            .bind(extension_of(&name))
            .execute(&state.db)
            .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_file(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    acl::file_access(&state.db, &user, id).await?.0.require(Access::Full)?;
    sqlx::query("UPDATE files SET deleted_at = now(), deleted_by = $3 WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .bind(user.user_id)
        .execute(&state.db)
        .await?;
    state.audit(user.tenant_id, Some(user.user_id), "file.deleted", Some("file"), Some(id), json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize, sqlx::FromRow)]
struct VersionRow {
    version: i32,
    size_bytes: i64,
    author_name: Option<String>,
    created_at: DateTime<Utc>,
}

pub async fn versions(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    acl::file_access(&state.db, &user, id).await?.0.require(Access::Read)?;
    let rows = sqlx::query_as::<_, VersionRow>(
        "SELECT fv.version, fv.size_bytes, u.display_name AS author_name, fv.created_at
           FROM file_versions fv LEFT JOIN users u ON u.id = fv.author_id
          WHERE fv.file_id = $1 ORDER BY fv.version DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "versions": rows })))
}

/// Restore an old version by copying it as a new current version.
pub async fn restore_version(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, version)): Path<(Uuid, i32)>,
) -> ApiResult<Json<Value>> {
    acl::file_access(&state.db, &user, id).await?.0.require(Access::Edit)?;
    let editing: bool = sqlx::query_scalar("SELECT edit_key IS NOT NULL FROM files WHERE id = $1").bind(id).fetch_one(&state.db).await?;
    if editing {
        return Err(ApiError::Conflict("the document is open for editing".into()));
    }
    let (_, data) = read_version(&state, user.tenant_id, id, Some(version)).await?;
    let new_version = add_version(&state, user.tenant_id, id, Some(user.user_id), data).await?;
    Ok(Json(json!({ "version": new_version })))
}

// ---------------------------------------------------------------------------
// Shares
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ShareTarget {
    pub resource_type: String,
    pub resource_id: Uuid,
}

async fn resource_access(state: &AppState, user: &CurrentUser, t: &ShareTarget) -> ApiResult<Access> {
    match t.resource_type.as_str() {
        "folder" => acl::folder_access(&state.db, user, t.resource_id).await,
        "file" => Ok(acl::file_access(&state.db, user, t.resource_id).await?.0),
        _ => bad_request("resource_type must be 'folder' or 'file'"),
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct ShareRow {
    id: Uuid,
    user_id: Option<Uuid>,
    group_id: Option<Uuid>,
    principal_name: String,
    access: i16,
}

pub async fn list_shares(State(state): State<AppState>, user: CurrentUser, Query(t): Query<ShareTarget>) -> ApiResult<Json<Value>> {
    resource_access(&state, &user, &t).await?.require(Access::Read)?;
    let rows = sqlx::query_as::<_, ShareRow>(
        "SELECT s.id, s.user_id, s.group_id,
                COALESCE(u.display_name, g.name, 'Everyone') AS principal_name, s.access
           FROM shares s LEFT JOIN users u ON u.id = s.user_id LEFT JOIN groups g ON g.id = s.group_id
          WHERE s.tenant_id = $1 AND s.resource_type = $2 AND s.resource_id = $3",
    )
    .bind(user.tenant_id)
    .bind(&t.resource_type)
    .bind(t.resource_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "shares": rows })))
}

#[derive(Deserialize)]
pub struct CreateShare {
    pub resource_type: String,
    pub resource_id: Uuid,
    pub user_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub access: i16,
}

pub async fn create_share(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CreateShare>) -> ApiResult<Json<Value>> {
    let target = ShareTarget { resource_type: req.resource_type.clone(), resource_id: req.resource_id };
    resource_access(&state, &user, &target).await?.require(Access::Full)?;
    if !(1..=4).contains(&req.access) {
        return bad_request("access must be 1..4");
    }
    // Principals must belong to the same tenant.
    if let Some(uid) = req.user_id {
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(uid)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::BadRequest("unknown user".into()))?;
    }
    if let Some(gid) = req.group_id {
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM groups WHERE id = $1 AND tenant_id = $2")
            .bind(gid)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::BadRequest("unknown group".into()))?;
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO shares (tenant_id, resource_type, resource_id, user_id, group_id, access, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&req.resource_type)
    .bind(req.resource_id)
    .bind(req.user_id)
    .bind(req.group_id)
    .bind(req.access)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "share.created", Some(&req.resource_type), Some(req.resource_id), json!({ "access": req.access }))
        .await;
    notify_share(&state, &user, &req).await;
    Ok(Json(json!({ "id": id })))
}

/// Tell the people who just got access. Shares with everyone are not announced.
async fn notify_share(state: &AppState, user: &CurrentUser, req: &CreateShare) {
    let recipients: Vec<Uuid> = if let Some(u) = req.user_id {
        vec![u]
    } else if let Some(g) = req.group_id {
        sqlx::query_scalar("SELECT user_id FROM group_members WHERE group_id = $1")
            .bind(g)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
    } else {
        return;
    };
    let table = if req.resource_type == "folder" { "folders" } else { "files" };
    let name: String = sqlx::query_scalar(&format!("SELECT name FROM {table} WHERE id = $1"))
        .bind(req.resource_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or_default();
    let access = match req.access {
        1 => "просмотр",
        2 => "комментирование",
        3 => "редактирование",
        _ => "полный доступ",
    };
    let (what, link) = if req.resource_type == "folder" {
        ("папке", format!("/files/{}", req.resource_id))
    } else {
        ("документу", "/shared".to_string())
    };
    crate::notify::notify(
        state,
        user.tenant_id,
        Some(user.user_id),
        &recipients,
        crate::notify::Note {
            kind: "file_shared",
            title: format!("{} открыл(а) вам доступ к {what} «{name}»", user.display_name),
            body: format!("Права: {access}"),
            link,
            entity_type: Some(if req.resource_type == "folder" { "folder" } else { "file" }),
            entity_id: Some(req.resource_id),
            ..Default::default()
        },
    )
    .await;
}

pub async fn delete_share(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (rt, rid): (String, Uuid) = sqlx::query_as("SELECT resource_type, resource_id FROM shares WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    resource_access(&state, &user, &ShareTarget { resource_type: rt, resource_id: rid }).await?.require(Access::Full)?;
    sqlx::query("DELETE FROM shares WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Items shared directly with the current user or their groups.
pub async fn shared_with_me(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let folders = sqlx::query_as::<_, FolderRow>(
        "SELECT DISTINCT f.id, f.parent_id, f.kind, f.name, f.owner_id, f.created_at
           FROM shares s JOIN folders f ON f.id = s.resource_id AND s.resource_type = 'folder'
          WHERE s.tenant_id = $1 AND f.deleted_at IS NULL AND f.owner_id <> $2
            AND (s.user_id = $2 OR s.group_id IN (SELECT group_id FROM group_members WHERE user_id = $2))",
    )
    .bind(user.tenant_id)
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;
    let files = sqlx::query_as::<_, FileRow>(&format!(
        "{FILE_SELECT} WHERE f.tenant_id = $1 AND f.deleted_at IS NULL AND f.owner_id <> $2
            AND f.id IN (SELECT s.resource_id FROM shares s WHERE s.resource_type = 'file'
                          AND (s.user_id = $2 OR s.group_id IN (SELECT group_id FROM group_members WHERE user_id = $2)))"
    ))
    .bind(user.tenant_id)
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "folders": folders, "files": files })))
}
