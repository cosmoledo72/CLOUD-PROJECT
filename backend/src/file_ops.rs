//! Batch file operations for the file list: storage usage, copying files and
//! folders (with «(копия)» names on conflict) and downloading a selection as
//! one ZIP archive.

use std::io::Write;

use axum::{extract::State, response::Response, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    acl::{self, Access},
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    files::{create_file, file_response, read_version},
    state::AppState,
};

/// Largest archive built in memory.
const ZIP_LIMIT: i64 = 1024 * 1024 * 1024;
/// Most items copied or zipped in one request (folders count their contents).
const MAX_ITEMS: usize = 5000;

/// `GET /storage/usage`: portal storage used (all versions) and the quota.
pub async fn usage(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let (used, quota): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE((SELECT sum(fv.size_bytes) FROM file_versions fv JOIN files f ON f.id = fv.file_id
                           WHERE f.tenant_id = $1), 0)::bigint, quota_bytes
           FROM tenants WHERE id = $1",
    )
    .bind(user.tenant_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({ "used": used, "quota": quota })))
}

#[derive(Deserialize)]
pub struct Selection {
    #[serde(default)]
    pub files: Vec<Uuid>,
    #[serde(default)]
    pub folders: Vec<Uuid>,
}

#[derive(Deserialize)]
pub struct CopyRequest {
    #[serde(flatten)]
    pub items: Selection,
    pub target: Uuid,
}

/// «Отчёт.docx» → «Отчёт (копия).docx», then «Отчёт (копия 2).docx», …
pub fn copy_name(name: &str, n: u32) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    if n <= 1 { format!("{stem} (копия){ext}") } else { format!("{stem} (копия {n}){ext}") }
}

/// «Акт.pdf» → «Акт (2).pdf» for repeated names inside one archive.
fn numbered(path: &str, n: u32) -> String {
    let (dir, name) = path.rsplit_once('/').map(|(d, f)| (format!("{d}/"), f)).unwrap_or((String::new(), path));
    match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => format!("{dir}{s} ({n}).{e}"),
        _ => format!("{dir}{name} ({n})"),
    }
}

async fn free_name(state: &AppState, tenant: Uuid, folder: Uuid, name: &str, is_folder: bool) -> ApiResult<String> {
    let sql = if is_folder {
        "SELECT EXISTS (SELECT 1 FROM folders WHERE tenant_id = $1 AND parent_id = $2 AND lower(name) = lower($3) AND deleted_at IS NULL)"
    } else {
        "SELECT EXISTS (SELECT 1 FROM files WHERE tenant_id = $1 AND folder_id = $2 AND lower(name) = lower($3) AND deleted_at IS NULL)"
    };
    let mut candidate = name.to_string();
    for n in 0..1000 {
        if n > 0 {
            candidate = copy_name(name, n);
        }
        let taken: bool = sqlx::query_scalar(sql).bind(tenant).bind(folder).bind(&candidate).fetch_one(&state.db).await?;
        if !taken {
            return Ok(candidate);
        }
    }
    Err(ApiError::Conflict("name".into()))
}

/// `POST /files/copy {files, folders, target}`: copies the current versions
/// into the target folder; folders are copied with everything inside.
pub async fn copy(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CopyRequest>) -> ApiResult<Json<Value>> {
    if req.items.files.is_empty() && req.items.folders.is_empty() {
        return bad_request("nothing selected");
    }
    acl::folder_access(&state.db, &user, req.target).await?.require(Access::Edit)?;
    for f in &req.items.files {
        acl::file_access(&state.db, &user, *f).await?.0.require(Access::Read)?;
    }
    for f in &req.items.folders {
        acl::folder_access(&state.db, &user, *f).await?.require(Access::Read)?;
        // A folder cannot be copied into itself or below itself.
        let inside: bool = sqlx::query_scalar(
            "WITH RECURSIVE up AS (SELECT id, parent_id FROM folders WHERE id = $1
                                   UNION ALL SELECT p.id, p.parent_id FROM folders p JOIN up ON p.id = up.parent_id)
             SELECT EXISTS (SELECT 1 FROM up WHERE id = $2)",
        )
        .bind(req.target)
        .bind(f)
        .fetch_one(&state.db)
        .await?;
        if inside {
            return bad_request("cannot copy a folder into itself");
        }
    }
    let mut budget = MAX_ITEMS;
    let mut created = Vec::new();
    for f in &req.items.files {
        created.push(copy_file(&state, &user, *f, req.target, &mut budget).await?);
    }
    for f in &req.items.folders {
        created.push(Box::pin(copy_folder(&state, &user, *f, req.target, &mut budget)).await?);
    }
    state
        .audit(user.tenant_id, Some(user.user_id), "file.copied", Some("folder"), Some(req.target),
               json!({ "files": req.items.files, "folders": req.items.folders }))
        .await;
    Ok(Json(json!({ "ids": created })))
}

async fn copy_file(state: &AppState, user: &CurrentUser, id: Uuid, target: Uuid, budget: &mut usize) -> ApiResult<Uuid> {
    if *budget == 0 {
        return bad_request("too many items");
    }
    *budget -= 1;
    let (name, data) = read_version(state, user.tenant_id, id, None).await?;
    let name = free_name(state, user.tenant_id, target, &name, false).await?;
    create_file(state, user, target, &name, data).await
}

async fn copy_folder(state: &AppState, user: &CurrentUser, id: Uuid, target: Uuid, budget: &mut usize) -> ApiResult<Uuid> {
    if *budget == 0 {
        return bad_request("too many items");
    }
    *budget -= 1;
    let name: String = sqlx::query_scalar("SELECT name FROM folders WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let name = free_name(state, user.tenant_id, target, &name, true).await?;
    let new_id: Uuid = sqlx::query_scalar("INSERT INTO folders (tenant_id, parent_id, name, owner_id) VALUES ($1, $2, $3, $4) RETURNING id")
        .bind(user.tenant_id)
        .bind(target)
        .bind(&name)
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?;
    let files: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM files WHERE folder_id = $1 AND deleted_at IS NULL ORDER BY lower(name)")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    for f in files {
        copy_file(state, user, f, new_id, budget).await?;
    }
    let subs: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM folders WHERE parent_id = $1 AND deleted_at IS NULL ORDER BY lower(name)")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    for s in subs {
        Box::pin(copy_folder(state, user, s, new_id, budget)).await?;
    }
    Ok(new_id)
}

/// `POST /files/zip {files, folders}`: the selection as one archive; folders
/// keep their structure. Named after the single folder or «Файлы.zip».
pub async fn zip(State(state): State<AppState>, user: CurrentUser, Json(req): Json<Selection>) -> ApiResult<Response> {
    if req.files.is_empty() && req.folders.is_empty() {
        return bad_request("nothing selected");
    }
    // (path inside the archive, file id)
    let mut entries: Vec<(String, Uuid)> = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    for f in &req.files {
        acl::file_access(&state.db, &user, *f).await?.0.require(Access::Read)?;
        let name: String = sqlx::query_scalar("SELECT name FROM files WHERE id = $1")
            .bind(f)
            .fetch_one(&state.db)
            .await?;
        entries.push((name, *f));
    }
    let mut archive_name = "Файлы.zip".to_string();
    for f in &req.folders {
        acl::folder_access(&state.db, &user, *f).await?.require(Access::Read)?;
        // Every file below the folder with its path relative to the folder's parent.
        let rows: Vec<(Option<Uuid>, String)> = sqlx::query_as(
            "WITH RECURSIVE tree AS (
                SELECT id, name::text AS path FROM folders WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
                UNION ALL
                SELECT c.id, t.path || '/' || c.name FROM folders c JOIN tree t ON c.parent_id = t.id WHERE c.deleted_at IS NULL
             )
             SELECT NULL::uuid, path FROM tree
             UNION ALL
             SELECT f.id, t.path || '/' || f.name FROM files f JOIN tree t ON f.folder_id = t.id WHERE f.deleted_at IS NULL",
        )
        .bind(f)
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?;
        if rows.is_empty() {
            return Err(ApiError::NotFound);
        }
        if req.folders.len() == 1 && req.files.is_empty() {
            archive_name = format!("{}.zip", rows[0].1);
        }
        for (id, path) in rows {
            match id {
                Some(id) => entries.push((path, id)),
                None => dirs.push(path),
            }
        }
    }
    if entries.len() > MAX_ITEMS {
        return bad_request("too many files");
    }
    let ids: Vec<Uuid> = entries.iter().map(|e| e.1).collect();
    let total: i64 = sqlx::query_scalar("SELECT COALESCE(sum(size_bytes), 0)::bigint FROM files WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_one(&state.db)
        .await?;
    if total > ZIP_LIMIT {
        return Err(ApiError::TooLarge("selection is larger than 1 GB".into()));
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut seen = std::collections::HashSet::new();
        for d in &dirs {
            z.add_directory(format!("{d}/"), opts).map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
        }
        for (path, id) in &entries {
            let (_, data) = read_version(&state, user.tenant_id, *id, None).await?;
            // Same names in the selection (from different folders) get a suffix.
            let mut p = path.clone();
            let mut n = 1;
            while !seen.insert(p.to_lowercase()) {
                n += 1;
                p = numbered(path, n);
            }
            // Already compressed formats are stored as is.
            let ext = crate::files::extension_of(path);
            let o = if matches!(ext.as_str(), "zip" | "docx" | "xlsx" | "pptx" | "jpg" | "jpeg" | "png" | "pdf" | "7z" | "rar" | "mp4") { stored } else { opts };
            z.start_file(p, o).map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
            z.write_all(&data).map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
        }
        z.finish().map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
    }
    state
        .audit(user.tenant_id, Some(user.user_id), "file.zip_downloaded", None, None,
               json!({ "files": req.files.len(), "folders": req.folders.len(), "entries": entries.len() }))
        .await;
    Ok(file_response(&archive_name, buf.into_inner().into(), false))
}

#[cfg(test)]
mod tests {
    use super::{copy_name, numbered};

    #[test]
    fn copy_names() {
        assert_eq!(copy_name("Отчёт.docx", 1), "Отчёт (копия).docx");
        assert_eq!(copy_name("Отчёт.docx", 3), "Отчёт (копия 3).docx");
        assert_eq!(copy_name("Папка", 1), "Папка (копия)");
        assert_eq!(copy_name(".env", 2), ".env (копия 2)");
        assert_eq!(numbered("Папка/Акт.pdf", 2), "Папка/Акт (2).pdf");
        assert_eq!(numbered("Акт", 3), "Акт (3)");
    }
}
