//! Notifications: creation with per-user channel preferences, the in-app
//! list and live stream (SSE), email digests of unread notifications and
//! scheduled due-date reminders.

use std::{collections::BTreeMap, convert::Infallible, sync::Arc, time::Duration};

use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiResult},
    mail,
    state::{AppState, UserEvent},
};

/// A kind of notification with its default channels.
pub struct Kind {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub in_app: bool,
    pub email: bool,
}

pub const KINDS: &[Kind] = &[
    Kind { key: "task_assigned", label: "Мне назначена задача", group: "Проекты", in_app: true, email: true },
    Kind { key: "task_due_soon", label: "Срок задачи — сегодня или завтра", group: "Проекты", in_app: true, email: true },
    Kind { key: "task_overdue", label: "Задача просрочена", group: "Проекты", in_app: true, email: true },
    Kind { key: "task_comment", label: "Комментарий к моей задаче", group: "Проекты", in_app: true, email: false },
    Kind { key: "checklist_assigned", label: "Мне поручен пункт чек-листа", group: "Проекты", in_app: true, email: false },
    Kind { key: "task_done", label: "Поставленная мной задача выполнена", group: "Проекты", in_app: true, email: false },
    Kind { key: "project_added", label: "Меня добавили в проект", group: "Проекты", in_app: true, email: false },
    Kind { key: "mention", label: "Меня упомянули", group: "Общее", in_app: true, email: true },
    Kind { key: "file_shared", label: "Мне открыли доступ к документу", group: "Общее", in_app: true, email: false },
    Kind { key: "deal_assigned", label: "Я назначен ответственным по сделке", group: "CRM", in_app: true, email: true },
    Kind { key: "deal_stage", label: "Моя сделка перешла на другой этап", group: "CRM", in_app: true, email: false },
    Kind { key: "company_assigned", label: "Я назначен ответственным за контрагента", group: "CRM", in_app: true, email: false },
    Kind { key: "crm_task_assigned", label: "Мне поручено дело (звонок, встреча…)", group: "CRM", in_app: true, email: true },
    Kind { key: "crm_task_due", label: "Напоминание о деле: в назначенное время или утром в день срока", group: "CRM", in_app: true, email: true },
    Kind { key: "crm_task_overdue", label: "Дело просрочено", group: "CRM", in_app: true, email: false },
    Kind { key: "registration_request", label: "Новая заявка на регистрацию в портале", group: "Администрирование", in_app: true, email: true },
];

fn kind(key: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.key == key)
}

/// What to tell the recipients.
#[derive(Default)]
pub struct Note {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    pub link: String,
    pub entity_type: Option<&'static str>,
    pub entity_id: Option<Uuid>,
    /// Same key for the same user and kind is stored only once.
    pub dedupe_key: Option<String>,
}

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct Item {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub link: String,
    pub actor_id: Option<Uuid>,
    pub actor_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

/// Notify users. The actor never notifies themselves; inactive and invited
/// users are skipped. Failures are logged and never break the caller.
pub async fn notify(state: &AppState, tenant_id: Uuid, actor: Option<Uuid>, recipients: &[Uuid], note: Note) {
    if let Err(e) = notify_inner(state, tenant_id, actor, recipients, &note).await {
        tracing::warn!(error = ?e, kind = note.kind, "failed to create notification");
    }
}

async fn notify_inner(state: &AppState, tenant_id: Uuid, actor: Option<Uuid>, recipients: &[Uuid], note: &Note) -> ApiResult<()> {
    let Some(k) = kind(note.kind) else {
        return Err(anyhow::anyhow!("unknown notification kind {}", note.kind).into());
    };
    // A switched-off module sends nothing.
    if let Some(m) = kind_module(k.key, note.entity_type) {
        let disabled = crate::modules::load_disabled(state, tenant_id).await?;
        if !crate::modules::enabled(&disabled, m) {
            return Ok(());
        }
    }
    let mut ids: Vec<Uuid> = recipients.iter().copied().filter(|u| Some(*u) != actor).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Ok(());
    }
    let targets: Vec<(Uuid, bool, bool)> = sqlx::query_as(
        "SELECT u.id, COALESCE(p.in_app, $4), COALESCE(p.email, $5)
           FROM users u LEFT JOIN notification_prefs p ON p.user_id = u.id AND p.kind = $3
          WHERE u.id = ANY($1) AND u.tenant_id = $2 AND u.is_active AND NOT u.invite_pending",
    )
    .bind(&ids)
    .bind(tenant_id)
    .bind(k.key)
    .bind(k.in_app)
    .bind(k.email)
    .fetch_all(&state.db)
    .await?;
    let actor_name: Option<String> = match actor {
        Some(a) => sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1").bind(a).fetch_optional(&state.db).await?,
        None => None,
    };
    for (user_id, in_app, email) in targets {
        if !in_app && !email {
            continue;
        }
        let row: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
            "INSERT INTO notifications (tenant_id, user_id, kind, title, body, link, actor_id, entity_type, entity_id,
                                        dedupe_key, show_in_app, email_wanted)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (user_id, kind, dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING
             RETURNING id, created_at",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(k.key)
        .bind(note.title.chars().take(300).collect::<String>())
        .bind(note.body.chars().take(1000).collect::<String>())
        .bind(&note.link)
        .bind(actor)
        .bind(note.entity_type)
        .bind(note.entity_id)
        .bind(&note.dedupe_key)
        .bind(in_app)
        .bind(email)
        .fetch_optional(&state.db)
        .await?;
        let Some((id, created_at)) = row else { continue };
        if in_app {
            let item = Item {
                id,
                kind: k.key.into(),
                title: note.title.clone(),
                body: note.body.clone(),
                link: note.link.clone(),
                actor_id: actor,
                actor_name: actor_name.clone(),
                created_at,
                read_at: None,
            };
            // Nobody listening is fine: the item is in the list on next load.
            let _ = state.events.send(UserEvent { user_id, data: Arc::new(json!({ "type": "notification", "item": item })) });
        }
    }
    Ok(())
}

/// Users of the portal who may see a task: its project's members and admins.
pub async fn task_audience(state: &AppState, tenant_id: Uuid, task_id: Uuid, candidates: &[Uuid]) -> ApiResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT u.id FROM users u
          WHERE u.id = ANY($1) AND u.tenant_id = $2
            AND (u.role IN ('owner', 'admin')
                 OR EXISTS (SELECT 1 FROM tasks t JOIN project_members pm ON pm.project_id = t.project_id
                             WHERE t.id = $3 AND pm.user_id = u.id))",
    )
    .bind(candidates)
    .bind(tenant_id)
    .bind(task_id)
    .fetch_all(&state.db)
    .await?)
}

/// CRM users who may see a company (or any CRM user when there is none).
pub async fn crm_audience(state: &AppState, tenant_id: Uuid, company_id: Option<Uuid>, candidates: &[Uuid]) -> ApiResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT u.id FROM users u
          WHERE u.id = ANY($1) AND u.tenant_id = $2 AND u.role <> 'guest' AND u.crm_role <> 'none'
            AND ($3::uuid IS NULL OR EXISTS (
                 SELECT 1 FROM crm_companies c WHERE c.id = $3
                    AND crm_company_visible(c.visibility, c.responsible_id, c.group_id, u.id,
                                            u.role IN ('owner', 'admin') OR u.crm_role = 'manager')))",
    )
    .bind(candidates)
    .bind(tenant_id)
    .bind(company_id)
    .fetch_all(&state.db)
    .await?)
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListQuery {
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    #[serde(default)]
    pub unread: bool,
}

pub async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let items = sqlx::query_as::<_, Item>(
        "SELECT n.id, n.kind, n.title, n.body, n.link, n.actor_id, a.display_name AS actor_name, n.created_at, n.read_at
           FROM notifications n LEFT JOIN users a ON a.id = n.actor_id
          WHERE n.user_id = $1 AND n.show_in_app
            AND ($2::timestamptz IS NULL OR n.created_at < $2)
            AND (NOT $3 OR n.read_at IS NULL)
          ORDER BY n.created_at DESC LIMIT $4",
    )
    .bind(user.user_id)
    .bind(q.before)
    .bind(q.unread)
    .bind(limit + 1)
    .fetch_all(&state.db)
    .await?;
    let more = items.len() as i64 > limit;
    let items: Vec<Item> = items.into_iter().take(limit as usize).collect();
    Ok(Json(json!({ "items": items, "unread": unread_count(&state, user.user_id).await?, "has_more": more })))
}

async fn unread_count(state: &AppState, user_id: Uuid) -> ApiResult<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM notifications WHERE user_id = $1 AND show_in_app AND read_at IS NULL")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?)
}

#[derive(Deserialize)]
pub struct ReadInput {
    #[serde(default)]
    pub ids: Vec<Uuid>,
    #[serde(default)]
    pub all: bool,
}

pub async fn mark_read(State(state): State<AppState>, user: CurrentUser, Json(req): Json<ReadInput>) -> ApiResult<Json<Value>> {
    if !req.all && req.ids.is_empty() {
        return bad_request("ids or all is required");
    }
    sqlx::query(
        "UPDATE notifications SET read_at = now()
          WHERE user_id = $1 AND read_at IS NULL AND ($2 OR id = ANY($3))",
    )
    .bind(user.user_id)
    .bind(req.all)
    .bind(&req.ids)
    .execute(&state.db)
    .await?;
    let unread = unread_count(&state, user.user_id).await?;
    // Other open tabs of the same user update their counters.
    let _ = state.events.send(UserEvent { user_id: user.user_id, data: Arc::new(json!({ "type": "read", "unread": unread })) });
    Ok(Json(json!({ "unread": unread })))
}

pub async fn get_prefs(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, bool, bool)> = sqlx::query_as("SELECT kind, in_app, email FROM notification_prefs WHERE user_id = $1")
        .bind(user.user_id)
        .fetch_all(&state.db)
        .await?;
    let stored: BTreeMap<String, (bool, bool)> = rows.into_iter().map(|(k, a, e)| (k, (a, e))).collect();
    let prefs: Vec<Value> = KINDS
        .iter()
        .filter(|k| crm_visible(&user, k))
        .map(|k| {
            let (in_app, email) = stored.get(k.key).copied().unwrap_or((k.in_app, k.email));
            json!({ "kind": k.key, "label": k.label, "group": k.group, "in_app": in_app, "email": email,
                    "default_in_app": k.in_app, "default_email": k.email })
        })
        .collect();
    Ok(Json(json!({ "prefs": prefs, "email_available": mail::available(&state, user.tenant_id).await, "email": user.email })))
}

fn crm_visible(user: &CurrentUser, k: &Kind) -> bool {
    (k.group != "CRM" || user.crm_enabled()) && (k.group != "Администрирование" || user.is_admin()) && kind_module(k.key, None).is_none_or(|m| user.module_on(m))
}

/// Portal module a notification kind belongs to (`None` — always available).
/// Mentions follow the object they were made in.
fn kind_module(kind: &str, entity_type: Option<&str>) -> Option<&'static str> {
    match kind {
        "mention" => match entity_type {
            Some("task") => Some("projects"),
            Some(_) => Some("crm"),
            None => None,
        },
        "file_shared" => None,
        k if k.starts_with("task_") || k == "checklist_assigned" || k == "project_added" => Some("projects"),
        k if k.starts_with("crm_") || k.starts_with("deal_") || k == "company_assigned" => Some("crm"),
        _ => None,
    }
}

#[derive(Deserialize)]
pub struct PrefInput {
    pub kind: String,
    pub in_app: bool,
    pub email: bool,
}

pub async fn save_prefs(State(state): State<AppState>, user: CurrentUser, Json(req): Json<Vec<PrefInput>>) -> ApiResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    for p in &req {
        let Some(k) = kind(&p.kind) else {
            return bad_request(format!("unknown notification kind {}", p.kind));
        };
        if p.in_app == k.in_app && p.email == k.email {
            sqlx::query("DELETE FROM notification_prefs WHERE user_id = $1 AND kind = $2")
                .bind(user.user_id)
                .bind(k.key)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query(
                "INSERT INTO notification_prefs (user_id, kind, in_app, email) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id, kind) DO UPDATE SET in_app = EXCLUDED.in_app, email = EXCLUDED.email",
            )
            .bind(user.user_id)
            .bind(k.key)
            .bind(p.in_app)
            .bind(p.email)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Server-sent events: new notifications and read-state changes of the user.
pub async fn stream(State(state): State<AppState>, user: CurrentUser) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let me = user.user_id;
    let hello = tokio_stream::once(Ok(Event::default().event("hello").data("{}")));
    let events = BroadcastStream::new(state.events.subscribe()).filter_map(move |ev| match ev {
        Ok(ev) if ev.user_id == me => Some(Ok(Event::default().event("message").data(ev.data.to_string()))),
        // Lagged receivers just tell the client to reload its list.
        Err(_) => Some(Ok(Event::default().event("message").data(r#"{"type":"resync"}"#))),
        _ => None,
    });
    Sse::new(hello.chain(events)).keep_alive(KeepAlive::new().interval(Duration::from_secs(25)).text("ping"))
}

// ---------------------------------------------------------------------------
// Background: email digests and due-date reminders
// ---------------------------------------------------------------------------

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        let tick = Duration::from_secs((state.cfg.notify_email_delay_secs.clamp(1, 30)) as u64);
        let mut last_scan = std::time::Instant::now() - Duration::from_secs(state.cfg.due_scan_secs);
        loop {
            if last_scan.elapsed() >= Duration::from_secs(state.cfg.due_scan_secs) {
                last_scan = std::time::Instant::now();
                if let Err(e) = scan_due(&state).await {
                    tracing::warn!(error = ?e, "due-date scan failed");
                }
                if let Err(e) = crate::crm::tasks::scan_reminders(&state).await {
                    tracing::warn!(error = ?e, "CRM reminder scan failed");
                }
            }
            if let Err(e) = send_digests(&state).await {
                tracing::warn!(error = ?e, "notification digest failed");
            }
            tokio::time::sleep(tick).await;
        }
    });
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    tenant_id: Uuid,
    user_id: Uuid,
    title: String,
    body: String,
    link: String,
}

async fn send_digests(state: &AppState) -> anyhow::Result<()> {
    let rows = sqlx::query_as::<_, PendingRow>(
        "UPDATE notifications SET emailed_at = now()
          WHERE id IN (SELECT id FROM notifications
                        WHERE email_wanted AND emailed_at IS NULL AND read_at IS NULL
                          AND created_at < now() - make_interval(secs => $1)
                          AND created_at > now() - interval '2 days'
                        ORDER BY created_at LIMIT 500 FOR UPDATE SKIP LOCKED)
         RETURNING tenant_id, user_id, title, body, link",
    )
    .bind(state.cfg.notify_email_delay_secs as f64)
    .fetch_all(&state.db)
    .await?;
    let mut by_user: BTreeMap<(Uuid, Uuid), Vec<PendingRow>> = BTreeMap::new();
    for r in rows {
        by_user.entry((r.tenant_id, r.user_id)).or_default().push(r);
    }
    for ((tenant_id, user_id), items) in by_user {
        if !mail::available(state, tenant_id).await {
            continue;
        }
        let Some((email, name)): Option<(String, String)> =
            sqlx::query_as("SELECT email::text, display_name FROM users WHERE id = $1 AND is_active")
                .bind(user_id)
                .fetch_optional(&state.db)
                .await?
        else {
            continue;
        };
        let portal = mail::portal_name(state, tenant_id).await?;
        let subject = if items.len() == 1 {
            items[0].title.clone()
        } else {
            format!("{} новых уведомлений в портале {}", items.len(), mail::quoted(&portal))
        };
        let list: Vec<mail::ListItem> = items
            .iter()
            .map(|i| mail::ListItem {
                title: i.title.clone(),
                body: i.body.clone(),
                url: (!i.link.is_empty()).then(|| format!("{}{}", state.cfg.public_url, i.link)),
            })
            .collect();
        let m = mail::render_list(
            &email,
            &portal,
            &subject,
            &format!("Здравствуйте, {name}! Пока вас не было в портале:"),
            &list,
            ("Открыть портал", &state.cfg.public_url),
            "Выбрать, о чём сообщать по почте, можно в профиле, раздел «Уведомления».",
        );
        mail::enqueue(state, tenant_id, "digest", &m).await?;
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct DueRow {
    id: Uuid,
    tenant_id: Uuid,
    assignee_id: Uuid,
    title: String,
    due_date: NaiveDate,
    project_id: Uuid,
    project_name: String,
    today: NaiveDate,
}

async fn scan_due(state: &AppState) -> anyhow::Result<()> {
    let rows = sqlx::query_as::<_, DueRow>(
        "SELECT * FROM (
            SELECT t.id, t.tenant_id, t.assignee_id, t.title, t.due_date, t.project_id, p.name AS project_name,
                   (now() AT TIME ZONE ten.timezone)::date AS today
              FROM tasks t
              JOIN projects p ON p.id = t.project_id
              JOIN tenants ten ON ten.id = t.tenant_id
             WHERE t.status <> 'done' AND t.assignee_id IS NOT NULL AND t.due_date IS NOT NULL
               AND p.status = 'active' AND ten.status = 'active'
               AND NOT ('projects' = ANY(ten.disabled_modules))
               AND extract(hour FROM now() AT TIME ZONE ten.timezone) >= $1
               AND t.due_date BETWEEN current_date - 9 AND current_date + 2
         ) x WHERE due_date BETWEEN today - 7 AND today + 1",
    )
    .bind(state.cfg.due_reminder_hour)
    .fetch_all(&state.db)
    .await?;
    for r in rows {
        let date = r.due_date.format("%d.%m.%Y");
        let (kind, title, key) = if r.due_date > r.today {
            ("task_due_soon", format!("Завтра срок: {}", r.title), format!("tomorrow:{}:{}", r.id, r.due_date))
        } else if r.due_date == r.today {
            ("task_due_soon", format!("Сегодня срок: {}", r.title), format!("today:{}:{}", r.id, r.due_date))
        } else {
            ("task_overdue", format!("Просрочена задача: {}", r.title), format!("overdue:{}:{}", r.id, r.due_date))
        };
        notify(
            state,
            r.tenant_id,
            None,
            &[r.assignee_id],
            Note {
                kind,
                title,
                body: format!("Проект «{}», срок {date}", r.project_name),
                link: format!("/projects/{}?task={}", r.project_id, r.id),
                entity_type: Some("task"),
                entity_id: Some(r.id),
                dedupe_key: Some(key),
            },
        )
        .await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_unique() {
        let mut keys: Vec<_> = KINDS.iter().map(|k| k.key).collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len());
        assert!(kind("mention").is_some() && kind("nope").is_none());
    }
}
