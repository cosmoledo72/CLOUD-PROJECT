//! Knowledge base: a tree of Markdown pages shared by the portal's staff,
//! with full-text search (Russian stemming).

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
};

const MAX_BODY: usize = 200_000;

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
    updated_at: DateTime<Utc>,
}

pub async fn tree(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let rows = sqlx::query_as::<_, TreeRow>(
        "SELECT id, parent_id, title, icon, sort_order, updated_at FROM kb_pages WHERE tenant_id = $1 ORDER BY sort_order, lower(title)",
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
    body: String,
    created_by: Option<Uuid>,
    created_by_name: Option<String>,
    updated_by_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn load(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<PageRow> {
    sqlx::query_as::<_, PageRow>(
        "SELECT p.id, p.parent_id, p.title, p.icon, p.body, p.created_by, c.display_name AS created_by_name,
                u.display_name AS updated_by_name, p.created_at, p.updated_at
           FROM kb_pages p LEFT JOIN users c ON c.id = p.created_by LEFT JOIN users u ON u.id = p.updated_by
          WHERE p.id = $1 AND p.tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)
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

#[derive(Deserialize)]
pub struct NewPage {
    pub parent_id: Option<Uuid>,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub icon: String,
}

pub async fn create(State(state): State<AppState>, user: CurrentUser, Json(req): Json<NewPage>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let title = clean_title(&req.title)?;
    let icon = clean_icon(&req.icon)?;
    if req.body.len() > MAX_BODY {
        return bad_request("страница слишком большая");
    }
    if let Some(p) = req.parent_id {
        load(&state, &user, p).await?;
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO kb_pages (tenant_id, parent_id, title, icon, body, sort_order, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5,
                 COALESCE((SELECT max(sort_order) + 1 FROM kb_pages WHERE tenant_id = $1 AND parent_id IS NOT DISTINCT FROM $2), 0),
                 $6, $6)
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(req.parent_id)
    .bind(&title)
    .bind(&icon)
    .bind(&req.body)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "kb.page_created", Some("kb_page"), Some(id), json!({ "title": title })).await;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct UpdatePage {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub icon: String,
    /// `updated_at` the editor started from: protects against overwriting a
    /// colleague's simultaneous changes.
    pub base_updated_at: Option<DateTime<Utc>>,
}

pub async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<UpdatePage>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let current = load(&state, &user, id).await?;
    if let Some(base) = req.base_updated_at {
        // Compare at millisecond precision (JSON round trips may lose micros).
        if (current.updated_at - base).num_milliseconds().abs() > 1 {
            return Err(ApiError::Conflict(format!(
                "страницу уже изменил(а) {} — обновите страницу и внесите правки заново",
                current.updated_by_name.unwrap_or_else(|| "другой пользователь".into())
            )));
        }
    }
    let title = clean_title(&req.title)?;
    let icon = clean_icon(&req.icon)?;
    if req.body.len() > MAX_BODY {
        return bad_request("страница слишком большая");
    }
    sqlx::query("UPDATE kb_pages SET title = $2, body = $3, icon = $4, updated_by = $5, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(&title)
        .bind(&req.body)
        .bind(&icon)
        .bind(user.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "page": load(&state, &user, id).await? })))
}

#[derive(Deserialize)]
pub struct MovePage {
    pub parent_id: Option<Uuid>,
    pub sort_order: Option<i32>,
}

pub async fn move_page(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<MovePage>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    load(&state, &user, id).await?;
    if let Some(p) = req.parent_id {
        load(&state, &user, p).await?;
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

pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let page = load(&state, &user, id).await?;
    if !(user.is_admin() || page.created_by == Some(user.user_id)) {
        return Err(ApiError::Forbidden);
    }
    let n: i64 = sqlx::query_scalar(
        "WITH RECURSIVE down AS (SELECT id FROM kb_pages WHERE id = $1 UNION ALL SELECT p.id FROM kb_pages p JOIN down ON p.parent_id = down.id)
         SELECT count(*) FROM down",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    sqlx::query("DELETE FROM kb_pages WHERE id = $1").bind(id).execute(&state.db).await?;
    state.audit(user.tenant_id, Some(user.user_id), "kb.page_deleted", Some("kb_page"), Some(id), json!({ "title": page.title, "pages": n })).await;
    Ok(Json(json!({ "deleted": n })))
}

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
          WHERE tenant_id = $1 AND (search @@ websearch_to_tsquery('russian', $2) OR title ILIKE '%' || $3 || '%')
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
