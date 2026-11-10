//! Trash: soft-deleted files and folders can be restored or purged.
//! Items are purged automatically after `TRASH_RETENTION_DAYS`.
//!
//! Visibility: tenant admins see the whole portal trash; other users see the
//! items they deleted themselves. Only top-level deleted items are listed
//! (contents of a deleted folder travel with it).

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
    storage::BlobStore,
};

/// Folders reachable from live roots through live folders.
const LIVE_FOLDERS: &str = "WITH RECURSIVE live AS (
        SELECT id FROM folders WHERE tenant_id = $1 AND parent_id IS NULL AND deleted_at IS NULL
        UNION ALL
        SELECT f.id FROM folders f JOIN live l ON f.parent_id = l.id WHERE f.deleted_at IS NULL
    )";

#[derive(Serialize, sqlx::FromRow)]
struct TrashItem {
    item_type: String,
    id: Uuid,
    name: String,
    kind: String,
    size_bytes: i64,
    deleted_at: DateTime<Utc>,
    deleted_by: Option<Uuid>,
    deleted_by_name: Option<String>,
    location: Option<String>,
}

async fn visible_items(state: &AppState, user: &CurrentUser) -> ApiResult<Vec<TrashItem>> {
    Ok(sqlx::query_as::<_, TrashItem>(&format!(
        "{LIVE_FOLDERS}
         SELECT 'folder' AS item_type, f.id, f.name, f.kind, 0::bigint AS size_bytes, f.deleted_at, f.deleted_by,
                u.display_name AS deleted_by_name, p.name AS location
           FROM folders f
           LEFT JOIN folders p ON p.id = f.parent_id
           LEFT JOIN users u ON u.id = f.deleted_by
          WHERE f.tenant_id = $1 AND f.deleted_at IS NOT NULL
            AND (f.parent_id IS NULL OR f.parent_id IN (SELECT id FROM live))
            AND ($2 OR f.deleted_by = $3)
         UNION ALL
         SELECT 'file', fi.id, fi.name, fi.extension,
                COALESCE((SELECT sum(size_bytes) FROM file_versions v WHERE v.file_id = fi.id), 0)::bigint,
                fi.deleted_at, fi.deleted_by, u.display_name, p.name
           FROM files fi
           JOIN folders p ON p.id = fi.folder_id
           LEFT JOIN users u ON u.id = fi.deleted_by
          WHERE fi.tenant_id = $1 AND fi.deleted_at IS NOT NULL
            AND fi.folder_id IN (SELECT id FROM live)
            AND ($2 OR fi.deleted_by = $3)
          ORDER BY deleted_at DESC"
    ))
    .bind(user.tenant_id)
    .bind(user.is_admin())
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?)
}

pub async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let items = visible_items(&state, &user).await?;
    let days = state.cfg.trash_retention_days;
    let items: Vec<Value> = items
        .into_iter()
        .map(|i| {
            let purge_at = i.deleted_at + chrono::Duration::days(days);
            // Project roots lose their project on deletion; they can only be purged.
            let restorable = !(i.item_type == "folder" && i.kind == "project");
            let mut v = serde_json::to_value(&i).unwrap_or_default();
            v["purge_at"] = json!(purge_at);
            v["restorable"] = json!(restorable);
            v
        })
        .collect();
    Ok(Json(json!({ "items": items, "retention_days": days })))
}

/// Load a top-level trash item the user may act on. Returns (deleted_by, parent folder, name, kind).
async fn load_item(state: &AppState, user: &CurrentUser, item_type: &str, id: Uuid) -> ApiResult<(Option<Uuid>, Option<Uuid>, String, String)> {
    let row: Option<(Option<Uuid>, Option<Uuid>, String, String)> = match item_type {
        "folder" => sqlx::query_as(
            "SELECT deleted_by, parent_id, name, kind FROM folders WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
        )
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?,
        "file" => sqlx::query_as(
            "SELECT deleted_by, folder_id, name, extension FROM files WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
        )
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?,
        _ => return bad_request("type must be 'folder' or 'file'"),
    };
    let row = row.ok_or(ApiError::NotFound)?;
    if !user.is_admin() && row.0 != Some(user.user_id) {
        return Err(ApiError::NotFound);
    }
    Ok(row)
}

pub async fn restore(State(state): State<AppState>, user: CurrentUser, Path((item_type, id)): Path<(String, Uuid)>) -> ApiResult<Json<Value>> {
    let (_, parent, name, kind) = load_item(&state, &user, &item_type, id).await?;
    if item_type == "folder" && kind == "project" {
        return bad_request("the folder of a deleted project cannot be restored");
    }
    let parent = parent.ok_or_else(|| ApiError::BadRequest("root folders cannot be restored".into()))?;
    let parent_live: bool = sqlx::query_scalar(&format!("{LIVE_FOLDERS} SELECT EXISTS (SELECT 1 FROM live WHERE id = $2)"))
        .bind(user.tenant_id)
        .bind(parent)
        .fetch_one(&state.db)
        .await?;
    if !parent_live {
        return bad_request("the original folder was deleted; restore it first");
    }

    // Avoid name clashes with items created after the deletion.
    let (table, parent_col) = if item_type == "folder" { ("folders", "parent_id") } else { ("files", "folder_id") };
    let (stem, ext) = match (item_type.as_str(), name.rsplit_once('.')) {
        ("file", Some((s, e))) => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    let mut new_name = name.clone();
    for i in 1..100 {
        if i > 1 {
            new_name = format!("{stem} (восстановлено{}){ext}", if i > 2 { format!(" {}", i - 1) } else { String::new() });
        }
        let taken: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS (SELECT 1 FROM {table} WHERE {parent_col} = $1 AND lower(name) = lower($2) AND deleted_at IS NULL)"
        ))
        .bind(parent)
        .bind(&new_name)
        .fetch_one(&state.db)
        .await?;
        if !taken {
            break;
        }
    }
    sqlx::query(&format!("UPDATE {table} SET deleted_at = NULL, deleted_by = NULL, name = $2 WHERE id = $1"))
        .bind(id)
        .bind(&new_name)
        .execute(&state.db)
        .await?;
    state.audit(user.tenant_id, Some(user.user_id), "trash.restored", Some(&item_type), Some(id), json!({ "name": new_name })).await;
    Ok(Json(json!({ "ok": true, "name": new_name, "parent_id": parent })))
}

/// Permanently delete an item (and, for folders, its whole subtree) including blobs.
pub async fn purge_item(state: &AppState, tenant_id: Uuid, item_type: &str, id: Uuid) -> ApiResult<u64> {
    let mut tx = state.db.begin().await?;
    let (keys, resource_ids): (Vec<String>, Vec<Uuid>) = if item_type == "folder" {
        let sub = "WITH RECURSIVE sub AS (
                SELECT id FROM folders WHERE id = $1 AND tenant_id = $2
                UNION ALL SELECT f.id FROM folders f JOIN sub s ON f.parent_id = s.id
            )";
        let keys: Vec<String> = sqlx::query_scalar(&format!(
            "{sub} SELECT fv.storage_key FROM file_versions fv JOIN files fi ON fi.id = fv.file_id WHERE fi.folder_id IN (SELECT id FROM sub)"
        ))
        .bind(id)
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await?;
        let ids: Vec<Uuid> = sqlx::query_scalar(&format!(
            "{sub} SELECT id FROM sub UNION ALL SELECT fi.id FROM files fi WHERE fi.folder_id IN (SELECT id FROM sub)"
        ))
        .bind(id)
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM folders WHERE id = $1 AND tenant_id = $2").bind(id).bind(tenant_id).execute(&mut *tx).await?;
        (keys, ids)
    } else {
        let keys: Vec<String> = sqlx::query_scalar(
            "SELECT fv.storage_key FROM file_versions fv JOIN files fi ON fi.id = fv.file_id WHERE fi.id = $1 AND fi.tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM files WHERE id = $1 AND tenant_id = $2").bind(id).bind(tenant_id).execute(&mut *tx).await?;
        (keys, vec![id])
    };
    // Shares have no foreign key to their resource.
    sqlx::query("DELETE FROM shares WHERE tenant_id = $1 AND resource_id = ANY($2)")
        .bind(tenant_id)
        .bind(&resource_ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    // Blobs are removed after the commit; a failure leaves an orphan blob, never a dangling row.
    for k in &keys {
        if let Err(e) = state.store.delete(k).await {
            tracing::warn!(key = %k, error = ?e, "failed to delete blob");
        }
    }
    Ok(keys.len() as u64)
}

pub async fn purge(State(state): State<AppState>, user: CurrentUser, Path((item_type, id)): Path<(String, Uuid)>) -> ApiResult<Json<Value>> {
    let (_, _, name, _) = load_item(&state, &user, &item_type, id).await?;
    purge_item(&state, user.tenant_id, &item_type, id).await?;
    state.audit(user.tenant_id, Some(user.user_id), "trash.purged", None, Some(id), json!({ "name": name, "type": item_type })).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn empty(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let items = visible_items(&state, &user).await?;
    let n = items.len();
    for i in items {
        purge_item(&state, user.tenant_id, &i.item_type, i.id).await?;
    }
    state.audit(user.tenant_id, Some(user.user_id), "trash.emptied", None, None, json!({ "count": n })).await;
    Ok(Json(json!({ "purged": n })))
}

/// Background job: purge items older than the retention period, hourly.
pub fn spawn_auto_purge(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = auto_purge_once(&state).await {
                tracing::error!(error = %e, "trash auto-purge failed");
            }
        }
    });
}

pub async fn auto_purge_once(state: &AppState) -> ApiResult<usize> {
    let days = state.cfg.trash_retention_days;
    // Folders first: their contents go with them.
    let folders: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT f.id, f.tenant_id FROM folders f
          WHERE f.deleted_at < now() - make_interval(days => $1::int)
            AND (f.parent_id IS NULL OR NOT EXISTS (
                 SELECT 1 FROM folders p WHERE p.id = f.parent_id AND p.deleted_at IS NOT NULL))",
    )
    .bind(days as i32)
    .fetch_all(&state.db)
    .await?;
    for (id, tenant) in &folders {
        purge_item(state, *tenant, "folder", *id).await?;
    }
    let files: Vec<(Uuid, Uuid)> =
        sqlx::query_as("SELECT id, tenant_id FROM files WHERE deleted_at < now() - make_interval(days => $1::int)")
            .bind(days as i32)
            .fetch_all(&state.db)
            .await?;
    for (id, tenant) in &files {
        purge_item(state, *tenant, "file", *id).await?;
    }
    let total = folders.len() + files.len();
    if total > 0 {
        tracing::info!(total, "trash auto-purge removed items");
    }
    Ok(total)
}
