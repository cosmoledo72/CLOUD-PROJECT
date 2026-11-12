//! Public links (F10 + C4): a file, a folder or a knowledge base page opened
//! without an account by an unguessable link, optionally with a password and
//! an expiry date. Visitors may view (files in ONLYOFFICE, read-only) and
//! download (folders also as one ZIP). Employees manage links from the
//! «Доступ» dialog; administrators see every working link and can switch
//! public links off for the whole portal.
//!
//! A password-protected link is unlocked with `POST …/unlock`, which returns
//! an access key `k` derived from the link token and the password hash: it is
//! passed with every further request (also in download URLs) and stops
//! working when the password changes.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use base64::Engine;
use chrono::{DateTime, NaiveDate, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    acl::{self, Access},
    auth::{hash_password, verify_password, CurrentUser},
    error::{bad_request, ApiError, ApiResult},
    file_ops::{build_zip, folder_entries},
    files::{file_response, read_version},
    ratelimit::{client_ip, WINDOW_15_MIN},
    state::AppState,
};

/// Default validity of a new link.
pub const DEFAULT_DAYS: i64 = 30;

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct Link {
    pub id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub token: String,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub has_password: bool,
    pub expires_at: Option<DateTime<Utc>>,
    /// Last day the link works, in the portal's time zone.
    pub expires_on: Option<NaiveDate>,
    pub created_by: Option<Uuid>,
    pub created_by_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub opens: i32,
    pub downloads: i32,
    pub last_opened_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub tenant_id: Uuid,
}

const LINK_SELECT: &str = "SELECT l.id, l.resource_type, l.resource_id, l.token, l.password_hash, (l.password_hash IS NOT NULL) AS has_password,
        l.expires_at, ((l.expires_at AT TIME ZONE t.timezone) - interval '1 second')::date AS expires_on,
        l.created_by, u.display_name AS created_by_name, l.created_at, l.opens, l.downloads, l.last_opened_at, l.tenant_id
   FROM public_links l JOIN tenants t ON t.id = l.tenant_id LEFT JOIN users u ON u.id = l.created_by";

fn new_token() -> String {
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Access key of an unlocked password-protected link.
fn access_key(state: &AppState, link: &Link) -> String {
    let mut h = Sha256::new();
    h.update(state.cfg.app_secret.as_bytes());
    h.update(b":");
    h.update(link.token.as_bytes());
    h.update(b":");
    h.update(link.password_hash.as_deref().unwrap_or("").as_bytes());
    hex::encode(&h.finalize()[..16])
}

// ---------------------------------------------------------------------------
// Managing links (employees)
// ---------------------------------------------------------------------------

async fn links_enabled(state: &AppState, tenant: Uuid) -> ApiResult<bool> {
    Ok(sqlx::query_scalar("SELECT public_links_enabled FROM tenants WHERE id = $1").bind(tenant).fetch_one(&state.db).await?)
}

/// Name of the resource, and whether this user may manage its link.
async fn resource(state: &AppState, user: &CurrentUser, rtype: &str, id: Uuid) -> ApiResult<(String, bool)> {
    match rtype {
        "file" => {
            let (access, _) = acl::file_access(&state.db, user, id).await?;
            access.require(Access::Read)?;
            let name: String = sqlx::query_scalar("SELECT name FROM files WHERE id = $1").bind(id).fetch_one(&state.db).await?;
            Ok((name, access >= Access::Full && !user.is_guest()))
        }
        "folder" => {
            let access = acl::folder_access(&state.db, user, id).await?;
            access.require(Access::Read)?;
            let name: String = sqlx::query_scalar("SELECT name FROM folders WHERE id = $1").bind(id).fetch_one(&state.db).await?;
            Ok((crate::files::display_folder_name(&name), access >= Access::Full && !user.is_guest()))
        }
        "kb_page" => {
            if user.is_guest() || !user.module_on("kb") {
                return Err(ApiError::NotFound);
            }
            let title: String = sqlx::query_scalar("SELECT title FROM kb_pages WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(user.tenant_id)
                .fetch_optional(&state.db)
                .await?
                .ok_or(ApiError::NotFound)?;
            Ok((title, true))
        }
        _ => bad_request("unknown resource type"),
    }
}

async fn active_link(state: &AppState, tenant: Uuid, rtype: &str, id: Uuid) -> ApiResult<Option<Link>> {
    Ok(sqlx::query_as::<_, Link>(&format!(
        "{LINK_SELECT} WHERE l.tenant_id = $1 AND l.resource_type = $2 AND l.resource_id = $3 AND l.revoked_at IS NULL"
    ))
    .bind(tenant)
    .bind(rtype)
    .bind(id)
    .fetch_optional(&state.db)
    .await?)
}

#[derive(Deserialize)]
pub struct LinkQuery {
    pub resource_type: String,
    pub resource_id: Uuid,
}

/// `GET /links?resource_type&resource_id`: the working link (or null).
pub async fn get(State(state): State<AppState>, user: CurrentUser, Query(q): Query<LinkQuery>) -> ApiResult<Json<Value>> {
    let (_, can_manage) = resource(&state, &user, &q.resource_type, q.resource_id).await?;
    let link = active_link(&state, user.tenant_id, &q.resource_type, q.resource_id).await?;
    Ok(Json(json!({
        "link": if can_manage { link.as_ref().map(|l| json!(l)) } else { None },
        "exists": link.is_some(),
        "can_manage": can_manage,
        "enabled": links_enabled(&state, user.tenant_id).await?,
        "default_days": DEFAULT_DAYS,
    })))
}

#[derive(Deserialize)]
pub struct SaveLink {
    pub resource_type: String,
    pub resource_id: Uuid,
    /// Last day the link works; `null` with `no_expiry` for a link without end.
    #[serde(default)]
    pub expires_on: Option<NaiveDate>,
    #[serde(default)]
    pub no_expiry: bool,
    /// New password (empty or absent keeps the current one).
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub remove_password: bool,
}

/// `PUT /links`: create the link or change its expiry and password.
pub async fn save(State(state): State<AppState>, user: CurrentUser, Json(req): Json<SaveLink>) -> ApiResult<Json<Value>> {
    let (name, can_manage) = resource(&state, &user, &req.resource_type, req.resource_id).await?;
    if !can_manage {
        return Err(ApiError::Forbidden);
    }
    if !links_enabled(&state, user.tenant_id).await? {
        return Err(ApiError::Forbidden);
    }
    let today = crate::crm::tasks::portal_today(&state, user.tenant_id).await?;
    if let Some(d) = req.expires_on {
        if d < today {
            return bad_request("expiry date is in the past");
        }
        if d > today + chrono::Duration::days(3660) {
            return bad_request("expiry date is too far");
        }
    }
    let password = match req.password.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => {
            if p.chars().count() < 4 || p.chars().count() > 128 {
                return bad_request("password must be 4 to 128 characters");
            }
            Some(hash_password(p.to_string()).await?)
        }
        _ => None,
    };
    let existing = active_link(&state, user.tenant_id, &req.resource_type, req.resource_id).await?;
    // Expiry: an explicit date, «no expiry», or (new link) 30 days.
    let expires_on: Option<NaiveDate> = if req.no_expiry {
        None
    } else if let Some(d) = req.expires_on {
        Some(d)
    } else if let Some(l) = &existing {
        l.expires_on
    } else {
        Some(today + chrono::Duration::days(DEFAULT_DAYS))
    };
    let id = match &existing {
        None => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO public_links (tenant_id, resource_type, resource_id, token, password_hash, expires_at, created_by)
                 SELECT $1, $2, $3, $4, $5, CASE WHEN $6::date IS NULL THEN NULL ELSE (($6::date + 1)::timestamp AT TIME ZONE t.timezone) END, $7
                   FROM tenants t WHERE t.id = $1
                 RETURNING id",
            )
            .bind(user.tenant_id)
            .bind(&req.resource_type)
            .bind(req.resource_id)
            .bind(new_token())
            .bind(&password)
            .bind(expires_on)
            .bind(user.user_id)
            .fetch_one(&state.db)
            .await?;
            state
                .audit(user.tenant_id, Some(user.user_id), "link.created", Some(link_entity(&req.resource_type)), Some(req.resource_id),
                       json!({ "name": name, "expires_on": expires_on, "password": password.is_some() }))
                .await;
            id
        }
        Some(l) => {
            sqlx::query(
                "UPDATE public_links l SET
                    expires_at = CASE WHEN $2::date IS NULL THEN NULL ELSE (($2::date + 1)::timestamp AT TIME ZONE t.timezone) END,
                    password_hash = CASE WHEN $4 THEN NULL ELSE COALESCE($3, l.password_hash) END,
                    updated_at = now()
                   FROM tenants t WHERE t.id = l.tenant_id AND l.id = $1",
            )
            .bind(l.id)
            .bind(expires_on)
            .bind(&password)
            .bind(req.remove_password && password.is_none())
            .execute(&state.db)
            .await?;
            state
                .audit(user.tenant_id, Some(user.user_id), "link.updated", Some(link_entity(&req.resource_type)), Some(req.resource_id),
                       json!({ "name": name, "expires_on": expires_on, "password_changed": password.is_some(), "password_removed": req.remove_password }))
                .await;
            l.id
        }
    };
    let link = sqlx::query_as::<_, Link>(&format!("{LINK_SELECT} WHERE l.id = $1")).bind(id).fetch_one(&state.db).await?;
    Ok(Json(json!({ "link": link })))
}

fn link_entity(rtype: &str) -> &'static str {
    match rtype {
        "file" => "file",
        "folder" => "folder",
        _ => "kb_page",
    }
}

/// `DELETE /links/{id}`: the link stops working at once.
pub async fn revoke(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let link = sqlx::query_as::<_, Link>(&format!("{LINK_SELECT} WHERE l.id = $1 AND l.tenant_id = $2 AND l.revoked_at IS NULL"))
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    // Administrators may revoke any link (e.g. from the list of all links).
    if !user.is_admin() {
        let (_, can_manage) = resource(&state, &user, &link.resource_type, link.resource_id).await?;
        if !can_manage {
            return Err(ApiError::Forbidden);
        }
    }
    sqlx::query("UPDATE public_links SET revoked_at = now(), revoked_by = $2 WHERE id = $1")
        .bind(id)
        .bind(user.user_id)
        .execute(&state.db)
        .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "link.revoked", Some(link_entity(&link.resource_type)), Some(link.resource_id),
               json!({ "opens": link.opens, "downloads": link.downloads }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /admin/links`: every working link of the portal.
pub async fn admin_list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let links = sqlx::query_as::<_, Link>(&format!(
        "{LINK_SELECT} WHERE l.tenant_id = $1 AND l.revoked_at IS NULL ORDER BY l.created_at DESC"
    ))
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::new();
    for l in links {
        // Name even of a deleted item (marked as missing: the link no longer opens).
        let row: Option<(String, bool)> = sqlx::query_as(match l.resource_type.as_str() {
            "file" => "SELECT name, deleted_at IS NOT NULL FROM files WHERE id = $1",
            "folder" => "SELECT name, deleted_at IS NOT NULL FROM folders WHERE id = $1",
            _ => "SELECT title, false FROM kb_pages WHERE id = $1",
        })
        .bind(l.resource_id)
        .fetch_optional(&state.db)
        .await?;
        let (name, missing) = match row {
            Some((n, gone)) => (Some(n), gone),
            None => (None, true),
        };
        let expired = l.expires_at.is_some_and(|e| e <= Utc::now());
        let mut v = json!(l);
        v["name"] = json!(name.as_deref().map(crate::files::display_folder_name));
        v["missing"] = json!(missing);
        v["expired"] = json!(expired);
        out.push(v);
    }
    Ok(Json(json!({ "links": out, "enabled": links_enabled(&state, user.tenant_id).await? })))
}

#[derive(Deserialize)]
pub struct Toggle {
    pub enabled: bool,
}

/// `PUT /admin/links/settings {enabled}`: allow or forbid public links.
pub async fn admin_toggle(State(state): State<AppState>, user: CurrentUser, Json(req): Json<Toggle>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    sqlx::query("UPDATE tenants SET public_links_enabled = $2 WHERE id = $1")
        .bind(user.tenant_id)
        .bind(req.enabled)
        .execute(&state.db)
        .await?;
    state.audit(user.tenant_id, Some(user.user_id), "links.settings", None, None, json!({ "enabled": req.enabled })).await;
    Ok(Json(json!({ "enabled": req.enabled })))
}

// ---------------------------------------------------------------------------
// Public side (no account)
// ---------------------------------------------------------------------------

/// A link that works right now: not revoked, not expired, portal active and
/// links allowed, resource not deleted.
async fn live_link(state: &AppState, token: &str) -> ApiResult<Link> {
    if token.len() > 64 {
        return Err(ApiError::NotFound);
    }
    let link = sqlx::query_as::<_, Link>(&format!(
        "{LINK_SELECT} WHERE l.token = $1 AND l.revoked_at IS NULL AND (l.expires_at IS NULL OR l.expires_at > now())
            AND t.status = 'active' AND t.public_links_enabled"
    ))
    .bind(token)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let alive: bool = sqlx::query_scalar(match link.resource_type.as_str() {
        "file" => "SELECT EXISTS (SELECT 1 FROM files WHERE id = $1 AND deleted_at IS NULL)",
        "folder" => "SELECT EXISTS (SELECT 1 FROM folders WHERE id = $1 AND deleted_at IS NULL)",
        _ => "SELECT EXISTS (SELECT 1 FROM kb_pages WHERE id = $1)",
    })
    .bind(link.resource_id)
    .fetch_one(&state.db)
    .await?;
    if !alive {
        return Err(ApiError::NotFound);
    }
    Ok(link)
}

#[derive(Deserialize, Default)]
pub struct KeyQuery {
    #[serde(default)]
    pub k: Option<String>,
    /// Subfolder of a shared folder.
    #[serde(default)]
    pub folder: Option<Uuid>,
}

fn unlocked(state: &AppState, link: &Link, k: Option<&str>) -> bool {
    link.password_hash.is_none() || k.is_some_and(|k| k == access_key(state, link))
}

async fn require_unlocked(state: &AppState, token: &str, k: Option<&str>) -> ApiResult<Link> {
    let link = live_link(state, token).await?;
    if !unlocked(state, &link, k) {
        return Err(ApiError::Unauthorized);
    }
    Ok(link)
}

/// Is `folder` the shared folder or below it?
async fn folder_in_scope(state: &AppState, root: Uuid, folder: Uuid) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE up AS (SELECT id, parent_id, 0 AS depth FROM folders WHERE id = $1 AND deleted_at IS NULL
                               UNION ALL SELECT f.id, f.parent_id, u.depth + 1 FROM folders f JOIN up u ON f.id = u.parent_id
                                WHERE u.depth < 64 AND f.deleted_at IS NULL)
         SELECT EXISTS (SELECT 1 FROM up WHERE id = $2)",
    )
    .bind(folder)
    .bind(root)
    .fetch_one(&state.db)
    .await?)
}

/// Is `file` the shared file or inside the shared folder?
async fn file_in_scope(state: &AppState, link: &Link, file: Uuid) -> ApiResult<bool> {
    match link.resource_type.as_str() {
        "file" => Ok(file == link.resource_id),
        "folder" => {
            let folder: Option<Uuid> = sqlx::query_scalar("SELECT folder_id FROM files WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL")
                .bind(file)
                .bind(link.tenant_id)
                .fetch_optional(&state.db)
                .await?;
            match folder {
                Some(f) => folder_in_scope(state, link.resource_id, f).await,
                None => Ok(false),
            }
        }
        _ => Ok(false),
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct PubFile {
    id: Uuid,
    name: String,
    extension: String,
    size_bytes: i64,
    updated_at: DateTime<Utc>,
}

/// `GET /public/s/{token}?k&folder`: what the link shows.
pub async fn open(State(state): State<AppState>, Path(token): Path<String>, Query(q): Query<KeyQuery>) -> ApiResult<Json<Value>> {
    let link = live_link(&state, &token).await?;
    let brand = crate::branding::public_by_tenant(&state, link.tenant_id).await?;
    let mut out = json!({
        "type": link.resource_type,
        "brand": { "name": brand["display_name"], "logo_url": brand["logo_url"], "accent": brand["accent_color"] },
        "shared_by": link.created_by_name,
        "expires_on": link.expires_on,
        "locked": false,
    });
    if !unlocked(&state, &link, q.k.as_deref()) {
        out["locked"] = json!(true);
        return Ok(Json(out));
    }
    match link.resource_type.as_str() {
        "file" => {
            let f = sqlx::query_as::<_, PubFile>("SELECT id, name, extension, size_bytes, updated_at FROM files WHERE id = $1")
                .bind(link.resource_id)
                .fetch_one(&state.db)
                .await?;
            out["name"] = json!(f.name);
            out["file"] = json!(f);
        }
        "folder" => {
            let at = q.folder.unwrap_or(link.resource_id);
            if !folder_in_scope(&state, link.resource_id, at).await? {
                return Err(ApiError::NotFound);
            }
            let root_name: String = sqlx::query_scalar("SELECT name FROM folders WHERE id = $1").bind(link.resource_id).fetch_one(&state.db).await?;
            // Path from the shared folder down to the open one.
            let crumbs: Vec<(Uuid, String)> = sqlx::query_as(
                "WITH RECURSIVE up AS (SELECT id, parent_id, name, 0 AS depth FROM folders WHERE id = $1
                                       UNION ALL SELECT f.id, f.parent_id, f.name, u.depth + 1 FROM folders f JOIN up u ON f.id = u.parent_id
                                        WHERE u.id <> $2 AND u.depth < 64)
                 SELECT id, name FROM up ORDER BY depth DESC",
            )
            .bind(at)
            .bind(link.resource_id)
            .fetch_all(&state.db)
            .await?;
            let folders: Vec<(Uuid, String, i64)> = sqlx::query_as(
                "SELECT d.id, d.name,
                        (SELECT count(*) FROM folders c WHERE c.parent_id = d.id AND c.deleted_at IS NULL)
                        + (SELECT count(*) FROM files f WHERE f.folder_id = d.id AND f.deleted_at IS NULL)
                   FROM folders d WHERE d.parent_id = $1 AND d.deleted_at IS NULL ORDER BY lower(d.name)",
            )
            .bind(at)
            .fetch_all(&state.db)
            .await?;
            let files = sqlx::query_as::<_, PubFile>(
                "SELECT id, name, extension, size_bytes, updated_at FROM files WHERE folder_id = $1 AND deleted_at IS NULL ORDER BY lower(name)",
            )
            .bind(at)
            .fetch_all(&state.db)
            .await?;
            let (count, size): (i64, i64) = sqlx::query_as(
                "WITH RECURSIVE tree AS (SELECT id FROM folders WHERE id = $1
                                         UNION ALL SELECT c.id FROM folders c JOIN tree t ON c.parent_id = t.id WHERE c.deleted_at IS NULL)
                 SELECT count(*), COALESCE(sum(size_bytes), 0)::bigint FROM files WHERE folder_id IN (SELECT id FROM tree) AND deleted_at IS NULL",
            )
            .bind(link.resource_id)
            .fetch_one(&state.db)
            .await?;
            out["name"] = json!(crate::files::display_folder_name(&root_name));
            out["folder"] = json!({
                "id": at,
                "crumbs": crumbs.iter().enumerate().map(|(i, (id, n))| json!({ "id": id, "name": if i == 0 { crate::files::display_folder_name(n) } else { n.clone() } })).collect::<Vec<_>>(),
                "folders": folders.iter().map(|(id, n, items)| json!({ "id": id, "name": n, "items": items })).collect::<Vec<_>>(),
                "files": files,
                "total_files": count,
                "total_size": size,
            });
        }
        _ => {
            let (title, icon, body, updated_at): (String, String, String, DateTime<Utc>) =
                sqlx::query_as("SELECT title, icon, body, updated_at FROM kb_pages WHERE id = $1")
                    .bind(link.resource_id)
                    .fetch_one(&state.db)
                    .await?;
            out["name"] = json!(title);
            out["page"] = json!({ "title": title, "icon": icon, "body": body, "updated_at": updated_at });
        }
    }
    // Count opens of the link itself (not of subfolders); the first one
    // tells the person who shared it.
    if q.folder.is_none() {
        let first: bool = sqlx::query_scalar(
            "UPDATE public_links SET opens = opens + 1, last_opened_at = now() WHERE id = $1 RETURNING opens = 1",
        )
        .bind(link.id)
        .fetch_one(&state.db)
        .await?;
        if first {
            let name = out["name"].as_str().unwrap_or_default().to_string();
            state
                .audit(link.tenant_id, None, "link.opened", Some(link_entity(&link.resource_type)), Some(link.resource_id), json!({ "name": name }))
                .await;
            if let Some(owner) = link.created_by {
                let place = match link.resource_type.as_str() {
                    "file" => format!("/editor/{}", link.resource_id),
                    "folder" => format!("/files/{}", link.resource_id),
                    _ => format!("/kb?p={}", link.resource_id),
                };
                crate::notify::notify(
                    &state,
                    link.tenant_id,
                    None,
                    &[owner],
                    crate::notify::Note {
                        kind: "public_link_opened",
                        title: format!("Вашу ссылку открыли: «{name}»"),
                        body: "Ссылку для внешних открыли впервые".into(),
                        link: place,
                        entity_type: Some(link_entity(&link.resource_type)),
                        entity_id: Some(link.resource_id),
                        dedupe_key: Some(format!("link-open-{}", link.id)),
                    },
                )
                .await;
            }
        }
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct Unlock {
    pub password: String,
}

/// `POST /public/s/{token}/unlock {password}`: the access key for further requests.
pub async fn unlock(State(state): State<AppState>, headers: HeaderMap, Path(token): Path<String>, Json(req): Json<Unlock>) -> ApiResult<Json<Value>> {
    let link = live_link(&state, &token).await?;
    let Some(hash) = link.password_hash.clone() else {
        return Ok(Json(json!({ "k": null })));
    };
    let key = format!("link-unlock:{}:{}", client_ip(&headers), link.id);
    state.limiter.check(&key, 10, WINDOW_15_MIN)?;
    if !verify_password(req.password, hash).await {
        state.limiter.hit(&key, WINDOW_15_MIN);
        return Err(ApiError::Unauthorized);
    }
    Ok(Json(json!({ "k": access_key(&state, &link) })))
}

async fn count_download(state: &AppState, link: &Link) -> ApiResult<()> {
    sqlx::query("UPDATE public_links SET downloads = downloads + 1 WHERE id = $1").bind(link.id).execute(&state.db).await?;
    Ok(())
}

/// `GET /public/s/{token}/download/{file}?k`: one file.
pub async fn download(State(state): State<AppState>, Path((token, file)): Path<(String, Uuid)>, Query(q): Query<KeyQuery>) -> ApiResult<Response> {
    let link = require_unlocked(&state, &token, q.k.as_deref()).await?;
    if !file_in_scope(&state, &link, file).await? {
        return Err(ApiError::NotFound);
    }
    let (name, data) = read_version(&state, link.tenant_id, file, None).await?;
    count_download(&state, &link).await?;
    Ok(file_response(&name, data, false))
}

/// `GET /public/s/{token}/zip?k&folder`: the shared folder (or a subfolder) as one archive.
pub async fn zip(State(state): State<AppState>, Path(token): Path<String>, Query(q): Query<KeyQuery>) -> ApiResult<Response> {
    let link = require_unlocked(&state, &token, q.k.as_deref()).await?;
    if link.resource_type != "folder" {
        return Err(ApiError::NotFound);
    }
    let at = q.folder.unwrap_or(link.resource_id);
    if !folder_in_scope(&state, link.resource_id, at).await? {
        return Err(ApiError::NotFound);
    }
    let (dirs, entries) = folder_entries(&state, link.tenant_id, at).await?;
    let data = build_zip(&state, link.tenant_id, &dirs, &entries).await?;
    count_download(&state, &link).await?;
    let name = format!("{}.zip", crate::files::display_folder_name(&dirs[0]));
    Ok(file_response(&name, data.into(), false))
}

/// `GET /public/s/{token}/view/{file}?k`: read-only ONLYOFFICE viewer config.
pub async fn view(State(state): State<AppState>, Path((token, file)): Path<(String, Uuid)>, Query(q): Query<KeyQuery>) -> ApiResult<Json<Value>> {
    let link = require_unlocked(&state, &token, q.k.as_deref()).await?;
    if !file_in_scope(&state, &link, file).await? {
        return Err(ApiError::NotFound);
    }
    Ok(Json(crate::onlyoffice::public_viewer_config(&state, link.tenant_id, file).await?))
}

#[cfg(test)]
mod tests {
    use super::new_token;

    #[test]
    fn tokens_are_long_and_url_safe() {
        let a = new_token();
        assert_eq!(a.len(), 22);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a, new_token());
    }
}
