//! Chats: a channel per project (members of the project), group channels
//! and direct messages. New messages reach open browsers over the
//! notification stream; unread counts follow each member's read position.

use std::sync::Arc;

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
    state::{AppState, UserEvent},
};

const MAX_BODY: usize = 10_000;

fn require_staff(user: &CurrentUser) -> ApiResult<()> {
    if user.role == "guest" {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

/// Make sure every project the user belongs to has its channel.
async fn ensure_project_channels(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO chat_channels (tenant_id, kind, project_id, created_by)
         SELECT p.tenant_id, 'project', p.id, p.owner_id FROM projects p
           JOIN project_members pm ON pm.project_id = p.id AND pm.user_id = $1
          WHERE p.tenant_id = $2 AND NOT EXISTS (SELECT 1 FROM chat_channels c WHERE c.project_id = p.id)
         ON CONFLICT DO NOTHING",
    )
    .bind(user.user_id)
    .bind(user.tenant_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// SQL condition: user `$1` may use channel alias `c`.
const ACCESS: &str = "(
    (c.kind = 'project' AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = c.project_id AND pm.user_id = $1))
    OR (c.kind <> 'project' AND EXISTS (SELECT 1 FROM chat_members m WHERE m.channel_id = c.id AND m.user_id = $1)))";

#[derive(Serialize, sqlx::FromRow)]
pub struct ChannelRow {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub project_id: Option<Uuid>,
    pub other_user_id: Option<Uuid>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_body: Option<String>,
    pub last_author: Option<String>,
    pub unread: i64,
    pub created_by: Option<Uuid>,
    /// Badge of the project (project channels).
    pub badge_color: Option<String>,
    pub badge_icon: Option<String>,
}

fn channel_select(extra: &str) -> String {
    format!(
        "SELECT c.id, c.kind,
                CASE c.kind WHEN 'project' THEN p.name
                            WHEN 'direct' THEN COALESCE(ou.display_name, 'Удалённый пользователь')
                            ELSE c.name END AS name,
                c.project_id, ou.id AS other_user_id, c.last_message_at,
                lm.body AS last_body, la.display_name AS last_author,
                (SELECT count(*) FROM chat_messages x
                  WHERE x.channel_id = c.id AND x.deleted_at IS NULL AND x.author_id IS DISTINCT FROM $1
                    AND x.created_at > COALESCE(me.last_read_at, '-infinity')) AS unread,
                c.created_by, p.badge_color, p.badge_icon
           FROM chat_channels c
           LEFT JOIN projects p ON p.id = c.project_id
           LEFT JOIN chat_members me ON me.channel_id = c.id AND me.user_id = $1
           LEFT JOIN LATERAL (SELECT m2.user_id FROM chat_members m2 WHERE c.kind = 'direct' AND m2.channel_id = c.id AND m2.user_id <> $1 LIMIT 1) o ON true
           LEFT JOIN users ou ON ou.id = o.user_id
           LEFT JOIN LATERAL (SELECT body, author_id FROM chat_messages y WHERE y.channel_id = c.id AND y.deleted_at IS NULL
                               ORDER BY created_at DESC LIMIT 1) lm ON true
           LEFT JOIN users la ON la.id = lm.author_id
          WHERE c.tenant_id = $2 AND {ACCESS} {extra}"
    )
}

pub async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    ensure_project_channels(&state, &user).await?;
    let rows = sqlx::query_as::<_, ChannelRow>(&(channel_select("") + " ORDER BY COALESCE(c.last_message_at, c.created_at) DESC"))
        .bind(user.user_id)
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({ "channels": rows })))
}

async fn unread_total(state: &AppState, user: &CurrentUser) -> ApiResult<i64> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT COALESCE(sum(unread), 0)::bigint FROM ({}) t",
        channel_select("")
    ))
    .bind(user.user_id)
    .bind(user.tenant_id)
    .fetch_one(&state.db)
    .await?)
}

pub async fn unread(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    if user.role == "guest" {
        return Ok(Json(json!({ "total": 0 })));
    }
    Ok(Json(json!({ "total": unread_total(&state, &user).await? })))
}

async fn channel(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<ChannelRow> {
    require_staff(user)?;
    sqlx::query_as::<_, ChannelRow>(&channel_select("AND c.id = $3"))
        .bind(user.user_id)
        .bind(user.tenant_id)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Everyone who should receive live events of a channel.
async fn audience(state: &AppState, ch: &ChannelRow) -> ApiResult<Vec<Uuid>> {
    Ok(match ch.project_id {
        Some(p) => sqlx::query_scalar("SELECT user_id FROM project_members WHERE project_id = $1").bind(p).fetch_all(&state.db).await?,
        None => sqlx::query_scalar("SELECT user_id FROM chat_members WHERE channel_id = $1").bind(ch.id).fetch_all(&state.db).await?,
    })
}

fn broadcast(state: &AppState, users: &[Uuid], data: Value) {
    let data = Arc::new(data);
    for u in users {
        let _ = state.events.send(UserEvent { user_id: *u, data: data.clone() });
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct MemberRow {
    id: Uuid,
    display_name: String,
    position: String,
}

pub async fn get(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let ch = channel(&state, &user, id).await?;
    let members = match ch.project_id {
        Some(p) => sqlx::query_as::<_, MemberRow>(
            "SELECT u.id, u.display_name, u.position FROM project_members pm JOIN users u ON u.id = pm.user_id
              WHERE pm.project_id = $1 AND u.is_active ORDER BY lower(u.display_name)",
        )
        .bind(p)
        .fetch_all(&state.db)
        .await?,
        None => sqlx::query_as::<_, MemberRow>(
            "SELECT u.id, u.display_name, u.position FROM chat_members m JOIN users u ON u.id = m.user_id
              WHERE m.channel_id = $1 AND u.is_active ORDER BY lower(u.display_name)",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await?,
    };
    let can_manage = ch.kind == "group" && (ch.created_by == Some(user.user_id) || user.is_admin());
    Ok(Json(json!({ "channel": ch, "members": members, "can_manage": can_manage })))
}

async fn check_users(state: &AppState, user: &CurrentUser, ids: &[Uuid]) -> ApiResult<()> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE id = ANY($1) AND tenant_id = $2 AND is_active AND role <> 'guest'")
        .bind(ids)
        .bind(user.tenant_id)
        .fetch_one(&state.db)
        .await?;
    if n as usize != ids.len() {
        return bad_request("участниками чата могут быть только активные сотрудники портала");
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct DirectInput {
    pub user_id: Uuid,
}

/// Open (create if needed) the direct chat with a colleague.
pub async fn direct(State(state): State<AppState>, user: CurrentUser, Json(req): Json<DirectInput>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    if req.user_id == user.user_id {
        return bad_request("нельзя начать чат с самим собой");
    }
    check_users(&state, &user, &[req.user_id]).await?;
    let (a, b) = if user.user_id < req.user_id { (user.user_id, req.user_id) } else { (req.user_id, user.user_id) };
    let key = format!("{a}:{b}");
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO chat_channels (tenant_id, kind, direct_key, created_by) VALUES ($1, 'direct', $2, $3)
         ON CONFLICT (tenant_id, direct_key) WHERE direct_key IS NOT NULL DO UPDATE SET direct_key = EXCLUDED.direct_key
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&key)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO chat_members (channel_id, user_id) VALUES ($1, $2), ($1, $3) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(a)
        .bind(b)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct GroupInput {
    pub name: String,
    #[serde(default)]
    pub member_ids: Vec<Uuid>,
}

fn clean_name(n: &str) -> ApiResult<String> {
    let n = n.trim();
    if n.is_empty() || n.chars().count() > 100 {
        return bad_request("название чата — от 1 до 100 символов");
    }
    Ok(n.to_string())
}

pub async fn create_group(State(state): State<AppState>, user: CurrentUser, Json(req): Json<GroupInput>) -> ApiResult<Json<Value>> {
    require_staff(&user)?;
    let name = clean_name(&req.name)?;
    let mut ids = req.member_ids.clone();
    ids.push(user.user_id);
    ids.sort();
    ids.dedup();
    check_users(&state, &user, &ids).await?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO chat_channels (tenant_id, kind, name, created_by) VALUES ($1, 'group', $2, $3) RETURNING id")
        .bind(user.tenant_id)
        .bind(&name)
        .bind(user.user_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO chat_members (channel_id, user_id) SELECT $1, unnest($2::uuid[])")
        .bind(id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn update_group(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<GroupInput>) -> ApiResult<Json<Value>> {
    let ch = channel(&state, &user, id).await?;
    if ch.kind != "group" || !(ch.created_by == Some(user.user_id) || user.is_admin()) {
        return Err(ApiError::Forbidden);
    }
    let name = clean_name(&req.name)?;
    let mut ids = req.member_ids.clone();
    ids.push(user.user_id);
    ids.sort();
    ids.dedup();
    check_users(&state, &user, &ids).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE chat_channels SET name = $2 WHERE id = $1").bind(id).bind(&name).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM chat_members WHERE channel_id = $1 AND NOT (user_id = ANY($2))").bind(id).bind(&ids).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chat_members (channel_id, user_id) SELECT $1, unnest($2::uuid[]) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Leave a group chat (the last member removes it).
pub async fn leave(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let ch = channel(&state, &user, id).await?;
    if ch.kind != "group" {
        return bad_request("покинуть можно только групповой чат");
    }
    sqlx::query("DELETE FROM chat_members WHERE channel_id = $1 AND user_id = $2").bind(id).bind(user.user_id).execute(&state.db).await?;
    sqlx::query("DELETE FROM chat_channels WHERE id = $1 AND NOT EXISTS (SELECT 1 FROM chat_members WHERE channel_id = $1)")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub author_id: Option<Uuid>,
    pub author_name: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

const MESSAGE_SELECT: &str = "SELECT m.id, m.channel_id, m.author_id, u.display_name AS author_name, m.body, m.created_at, m.edited_at
                                FROM chat_messages m LEFT JOIN users u ON u.id = m.author_id";

#[derive(Deserialize)]
pub struct MessagesQuery {
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

pub async fn messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<MessagesQuery>,
) -> ApiResult<Json<Value>> {
    channel(&state, &user, id).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let mut rows = sqlx::query_as::<_, MessageRow>(&format!(
        "{MESSAGE_SELECT} WHERE m.channel_id = $1 AND m.deleted_at IS NULL AND ($2::timestamptz IS NULL OR m.created_at < $2)
          ORDER BY m.created_at DESC LIMIT $3"
    ))
    .bind(id)
    .bind(q.before)
    .bind(limit + 1)
    .fetch_all(&state.db)
    .await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    rows.reverse();
    Ok(Json(json!({ "messages": rows, "has_more": has_more })))
}

#[derive(Deserialize)]
pub struct MessageInput {
    pub body: String,
}

fn clean_body(b: &str) -> ApiResult<String> {
    let b = b.trim();
    if b.is_empty() {
        return bad_request("пустое сообщение");
    }
    if b.chars().count() > MAX_BODY {
        return bad_request("сообщение длиннее 10 000 символов");
    }
    Ok(b.to_string())
}

pub async fn post(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<MessageInput>) -> ApiResult<Json<Value>> {
    let ch = channel(&state, &user, id).await?;
    let body = clean_body(&req.body)?;
    state.limiter.check(&format!("chat:{}", user.user_id), 60, std::time::Duration::from_secs(60))?;
    state.limiter.hit(&format!("chat:{}", user.user_id), std::time::Duration::from_secs(60));
    let mid: Uuid = sqlx::query_scalar("INSERT INTO chat_messages (channel_id, author_id, body) VALUES ($1, $2, $3) RETURNING id")
        .bind(id)
        .bind(user.user_id)
        .bind(&body)
        .fetch_one(&state.db)
        .await?;
    sqlx::query("UPDATE chat_channels SET last_message_at = now() WHERE id = $1").bind(id).execute(&state.db).await?;
    // Writing a message means the author has read the channel.
    mark_read(&state, user.user_id, id).await?;
    let msg = sqlx::query_as::<_, MessageRow>(&format!("{MESSAGE_SELECT} WHERE m.id = $1")).bind(mid).fetch_one(&state.db).await?;
    let to = audience(&state, &ch).await?;
    broadcast(&state, &to, json!({ "type": "chat_message", "channel_id": id, "message": msg }));
    Ok(Json(json!({ "message": msg })))
}

async fn own_message(state: &AppState, user: &CurrentUser, mid: Uuid) -> ApiResult<(Uuid, Option<Uuid>)> {
    let row: (Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT m.channel_id, m.author_id FROM chat_messages m JOIN chat_channels c ON c.id = m.channel_id
          WHERE m.id = $1 AND c.tenant_id = $2 AND m.deleted_at IS NULL",
    )
    .bind(mid)
    .bind(user.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    channel(state, user, row.0).await?;
    Ok(row)
}

pub async fn edit(State(state): State<AppState>, user: CurrentUser, Path(mid): Path<Uuid>, Json(req): Json<MessageInput>) -> ApiResult<Json<Value>> {
    let (cid, author) = own_message(&state, &user, mid).await?;
    if author != Some(user.user_id) {
        return Err(ApiError::Forbidden);
    }
    let body = clean_body(&req.body)?;
    sqlx::query("UPDATE chat_messages SET body = $2, edited_at = now() WHERE id = $1").bind(mid).bind(&body).execute(&state.db).await?;
    let msg = sqlx::query_as::<_, MessageRow>(&format!("{MESSAGE_SELECT} WHERE m.id = $1")).bind(mid).fetch_one(&state.db).await?;
    let ch = channel(&state, &user, cid).await?;
    broadcast(&state, &audience(&state, &ch).await?, json!({ "type": "chat_edit", "channel_id": cid, "message": msg }));
    Ok(Json(json!({ "message": msg })))
}

pub async fn delete_message(State(state): State<AppState>, user: CurrentUser, Path(mid): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (cid, author) = own_message(&state, &user, mid).await?;
    if author != Some(user.user_id) && !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("UPDATE chat_messages SET deleted_at = now() WHERE id = $1").bind(mid).execute(&state.db).await?;
    let ch = channel(&state, &user, cid).await?;
    broadcast(&state, &audience(&state, &ch).await?, json!({ "type": "chat_delete", "channel_id": cid, "id": mid }));
    Ok(Json(json!({ "ok": true })))
}

async fn mark_read(state: &AppState, user_id: Uuid, channel_id: Uuid) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO chat_members (channel_id, user_id, last_read_at) VALUES ($1, $2, now())
         ON CONFLICT (channel_id, user_id) DO UPDATE SET last_read_at = now()",
    )
    .bind(channel_id)
    .bind(user_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

pub async fn read(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let ch = channel(&state, &user, id).await?;
    // Project chats keep only the read position in chat_members; access still follows the project.
    let _ = ch;
    mark_read(&state, user.user_id, id).await?;
    let total = unread_total(&state, &user).await?;
    broadcast(&state, &[user.user_id], json!({ "type": "chat_read", "channel_id": id, "unread_total": total }));
    Ok(Json(json!({ "unread_total": total })))
}
