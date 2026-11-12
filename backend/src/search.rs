//! Portal-wide search (F8 + C5): projects, tasks, files (names and text inside
//! documents), knowledge base, CRM companies, contacts and deals, comments on
//! tasks, chat messages and people.
//!
//! Matching: Russian stemmed full-text with word prefixes («обслед» finds
//! «обследования»), substrings of names and codes, trigram similarity of
//! names for typos, task numbers («#234»). Every group is filtered by the
//! same rules as its section: project membership, folder access (own,
//! common, project and CRM folders, shares), CRM visibility, chat membership,
//! switched-off modules.

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{auth::CurrentUser, error::ApiResult, state::AppState};

/// Groups in display order.
pub const GROUPS: [(&str, &str); 10] = [
    ("projects", "Проекты"),
    ("tasks", "Задачи"),
    ("files", "Файлы"),
    ("kb", "База знаний"),
    ("companies", "Контрагенты"),
    ("contacts", "Контакты"),
    ("deals", "Сделки"),
    ("comments", "Комментарии к задачам"),
    ("messages", "Сообщения в чатах"),
    ("people", "Сотрудники"),
];

/// Highlight markers in snippets (the client turns them into <mark>).
const HL: &str = "StartSel=⟦, StopSel=⟧, MaxWords=22, MinWords=8, MaxFragments=2, FragmentDelimiter=\" … \"";

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    /// all (a few hits per group) or one group name.
    #[serde(default = "all")]
    pub r#type: String,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub limit: Option<i64>,
    /// Only within this project (tasks, files, comments, project chat).
    #[serde(default)]
    pub project: Option<Uuid>,
    /// Changed or written since this date.
    #[serde(default)]
    pub since: Option<NaiveDate>,
}

fn all() -> String {
    "all".into()
}

/// Parsed query.
struct Q {
    /// `to_tsquery` text: «word:* & word:*»; a never-matching word when empty.
    ts: String,
    has_words: bool,
    /// Lower-cased query for trigram similarity.
    low: String,
    /// `%…%` pattern for substrings (LIKE special characters escaped).
    like: String,
    /// Task number from «#234» or «234».
    number: i64,
    /// Trigram matching only for queries of 4+ characters (shorter ones are noise).
    fuzzy: bool,
    project: Option<Uuid>,
    since: Option<NaiveDate>,
}

/// Groups that can be narrowed to a project or a date.
const SCOPED: [&str; 4] = ["tasks", "files", "comments", "messages"];

fn parse(text: &str) -> Q {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .map(|w| w.to_lowercase())
        .collect();
    let ts = if words.is_empty() { "qqqqzzzz".to_string() } else { words.iter().map(|w| format!("{w}:*")).collect::<Vec<_>>().join(" & ") };
    let low = text.trim().to_lowercase();
    let esc = low.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let number = low.trim_start_matches('#').parse::<i64>().unwrap_or(-1);
    Q { has_words: !words.is_empty(), ts, fuzzy: low.chars().count() >= 4, like: format!("%{esc}%"), low, number, project: None, since: None }
}

/// The visible part of a stored snippet: highlights kept as markers.
/// Leading text far before the first match is cut so the match stays visible
/// in two clamped lines.
fn snip(s: Option<String>) -> Option<String> {
    let x = s?.split_whitespace().collect::<Vec<_>>().join(" ");
    let at = x.find('⟦')?;
    let lead = x[..at].chars().count();
    if lead <= 60 {
        return Some(x);
    }
    let tail: String = x[..at].chars().skip(lead - 40).collect();
    let tail = tail.split_once(' ').map(|(_, t)| t.to_string()).unwrap_or(tail);
    Some(format!("…{tail}{}", &x[at..]))
}

/// `GET /api/search?q&type&offset&limit`.
pub async fn search(State(state): State<AppState>, user: CurrentUser, Query(p): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    let text: String = p.q.trim().chars().take(200).collect();
    if text.chars().count() < 2 {
        return Ok(Json(json!({ "q": text, "groups": [] })));
    }
    let mut q = parse(&text);
    q.project = p.project;
    q.since = p.since;
    let scoped = q.project.is_some() || q.since.is_some();
    let one = p.r#type != "all";
    let limit = if one { p.limit.unwrap_or(30).clamp(1, 100) } else { p.limit.unwrap_or(5).clamp(1, 20) };
    let offset = if one { p.offset.max(0) } else { 0 };
    let mut groups = Vec::new();
    for (key, label) in GROUPS {
        if one && p.r#type != key {
            continue;
        }
        if !allowed(&user, key) || (scoped && !SCOPED.contains(&key)) {
            continue;
        }
        let mut items = run(&state, &user, key, &q, limit + 1, offset).await?;
        let more = items.len() as i64 > limit;
        items.truncate(limit as usize);
        if !items.is_empty() || one {
            groups.push(json!({ "type": key, "label": label, "items": items, "more": more }));
        }
    }
    Ok(Json(json!({ "q": text, "groups": groups })))
}

fn allowed(user: &CurrentUser, key: &str) -> bool {
    let staff = !user.is_guest();
    match key {
        "projects" | "tasks" | "comments" => user.module_on("projects"),
        "files" => user.module_on("storage") || user.module_on("projects"),
        "kb" => staff && user.module_on("kb"),
        "companies" | "contacts" | "deals" => user.crm_enabled(),
        "messages" => user.module_on("chats"),
        "people" => staff && user.module_on("team"),
        _ => false,
    }
}

async fn run(state: &AppState, user: &CurrentUser, key: &str, q: &Q, limit: i64, offset: i64) -> ApiResult<Vec<Value>> {
    let db = &state.db;
    let admin = user.is_admin();
    Ok(match key {
        "projects" => {
            let rows: Vec<(Uuid, String, String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(&format!(
                "SELECT p.id, p.name, p.status,
                        (SELECT v.value #>> '{{}}' FROM project_field_values v JOIN project_field_defs d ON d.id = v.field_id
                          WHERE v.project_id = p.id AND d.key = 'code') AS code,
                        (SELECT c.name FROM crm_companies c WHERE c.id = p.company_id) AS company,
                        CASE WHEN $3 AND to_tsvector('russian', p.description) @@ to_tsquery('russian', $4)
                             THEN ts_headline('russian', p.description, to_tsquery('russian', $4), '{HL}') END
                   FROM projects p
                  WHERE p.tenant_id = $1
                    AND ($5 OR EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.user_id = $2))
                    AND (($3 AND p.search @@ to_tsquery('russian', $4)) OR lower(p.name) LIKE $6 OR ($7 AND $8 <% lower(p.name))
                         OR EXISTS (SELECT 1 FROM project_field_values v WHERE v.project_id = p.id AND lower(v.value #>> '{{}}') LIKE $6))
                  ORDER BY (lower(p.name) LIKE $6) DESC, ts_rank(p.search, to_tsquery('russian', $4)) DESC, p.created_at DESC
                  LIMIT $9 OFFSET $10"
            ))
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(q.has_words)
            .bind(&q.ts)
            .bind(admin)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(limit)
            .bind(offset)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, name, status, code, company, sn)| {
                    let mut sub = Vec::new();
                    if let Some(c) = code.filter(|c| !c.is_empty()) {
                        sub.push(c);
                    }
                    if let Some(c) = company {
                        sub.push(format!("Заказчик: {c}"));
                    }
                    if status != "active" {
                        sub.push(if status == "paused" { "приостановлен" } else { "закрыт" }.to_string());
                    }
                    json!({ "type": "projects", "id": id, "title": name, "subtitle": sub.join(" · "), "snippet": snip(sn), "url": format!("/projects/{id}") })
                })
                .collect()
        }
        "tasks" => {
            let rows: Vec<(Uuid, i64, String, String, Uuid, String, Option<NaiveDate>, Option<String>, Option<String>)> = sqlx::query_as(&format!(
                "SELECT t.id, t.number, t.title, t.status, t.project_id, p.name, t.due_date, a.display_name,
                        CASE WHEN $3 AND to_tsvector('russian', t.description) @@ to_tsquery('russian', $4)
                             THEN ts_headline('russian', t.description, to_tsquery('russian', $4), '{HL}') END
                   FROM tasks t JOIN projects p ON p.id = t.project_id LEFT JOIN users a ON a.id = t.assignee_id
                  WHERE t.tenant_id = $1
                    AND ($5 OR EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.user_id = $2))
                    AND (($3 AND t.search @@ to_tsquery('russian', $4)) OR lower(t.title) LIKE $6 OR ($7 AND $8 <% lower(t.title)) OR t.number = $9)
                    AND ($12::uuid IS NULL OR t.project_id = $12) AND ($13::date IS NULL OR t.updated_at >= $13)
                  ORDER BY (t.number = $9) DESC, (t.status = 'done'), (lower(t.title) LIKE $6) DESC,
                           ts_rank(t.search, to_tsquery('russian', $4)) DESC, t.updated_at DESC
                  LIMIT $10 OFFSET $11"
            ))
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(q.has_words)
            .bind(&q.ts)
            .bind(admin)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(q.number)
            .bind(limit)
            .bind(offset)
            .bind(q.project)
            .bind(q.since)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, number, title, status, pid, pname, due, who, sn)| {
                    let mut sub = vec![pname];
                    if let Some(w) = who {
                        sub.push(w);
                    }
                    if let Some(d) = due {
                        sub.push(format!("до {}", d.format("%d.%m.%Y")));
                    }
                    json!({ "type": "tasks", "id": id, "number": number, "title": title, "status": status, "subtitle": sub.join(" · "),
                            "snippet": snip(sn), "url": format!("/projects/{pid}?task={id}") })
                })
                .collect()
        }
        "files" => {
            let storage = user.module_on("storage");
            let crm = user.crm_enabled();
            let rows: Vec<(Uuid, String, String, i64, DateTime<Utc>, String, Option<String>)> = sqlx::query_as(&format!(
                "WITH RECURSIVE seeds AS (
                    SELECT f.id FROM folders f
                     WHERE f.tenant_id = $1 AND f.deleted_at IS NULL AND f.parent_id IS NULL
                       AND ((f.kind = 'project' AND ($5 OR EXISTS (SELECT 1 FROM projects p JOIN project_members pm ON pm.project_id = p.id
                                                                   WHERE p.folder_id = f.id AND pm.user_id = $2)))
                         OR (f.kind = 'crm' AND $10 AND ($5 OR EXISTS (SELECT 1 FROM crm_companies c WHERE c.folder_id = f.id
                                                         AND crm_company_visible(c.visibility, c.responsible_id, c.group_id, $2, $11))))
                         OR (f.kind = 'common' AND $9))
                    UNION
                    SELECT f.id FROM folders f WHERE f.tenant_id = $1 AND f.deleted_at IS NULL AND f.owner_id = $2 AND ($9 OR f.kind <> 'my')
                    UNION
                    SELECT s.resource_id FROM shares s
                     WHERE $9 AND s.tenant_id = $1 AND s.resource_type = 'folder'
                       AND (s.user_id = $2 OR s.group_id IN (SELECT group_id FROM group_members WHERE user_id = $2)
                            OR (s.user_id IS NULL AND s.group_id IS NULL))
                 ), tree AS (
                    SELECT id FROM seeds
                    UNION
                    SELECT f.id FROM folders f JOIN tree t ON f.parent_id = t.id WHERE f.deleted_at IS NULL
                 )
                 SELECT f.id, f.name, f.extension, f.size_bytes, f.updated_at, d.name,
                        CASE WHEN $3 AND x.search @@ to_tsquery('russian', $4)
                             THEN ts_headline('russian', x.content, to_tsquery('russian', $4), '{HL}') END
                   FROM files f JOIN folders d ON d.id = f.folder_id
                   LEFT JOIN file_texts x ON x.file_id = f.id
                  WHERE f.tenant_id = $1 AND f.deleted_at IS NULL
                    AND (f.folder_id IN (SELECT id FROM tree)
                         OR ($9 AND f.owner_id = $2)
                         OR ($9 AND EXISTS (SELECT 1 FROM shares s WHERE s.tenant_id = $1 AND s.resource_type = 'file' AND s.resource_id = f.id
                                             AND (s.user_id = $2 OR s.group_id IN (SELECT group_id FROM group_members WHERE user_id = $2)
                                                  OR (s.user_id IS NULL AND s.group_id IS NULL)))))
                    AND (($3 AND (f.search @@ to_tsquery('russian', $4) OR x.search @@ to_tsquery('russian', $4)))
                         OR lower(f.name) LIKE $6 OR ($7 AND $8 <% lower(f.name)))
                    AND ($14::uuid IS NULL OR f.folder_id IN (
                          WITH RECURSIVE pt AS (SELECT folder_id AS id FROM projects WHERE id = $14 AND tenant_id = $1
                                                UNION SELECT c.id FROM folders c JOIN pt ON c.parent_id = pt.id)
                          SELECT id FROM pt))
                    AND ($15::date IS NULL OR f.updated_at >= $15)
                  ORDER BY (lower(f.name) LIKE $6) DESC, ($3 AND f.search @@ to_tsquery('russian', $4)) DESC,
                           ts_rank(x.search, to_tsquery('russian', $4)) DESC, f.updated_at DESC
                  LIMIT $12 OFFSET $13"
            ))
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(q.has_words)
            .bind(&q.ts)
            .bind(admin)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(storage)
            .bind(crm)
            .bind(user.crm_manager())
            .bind(limit)
            .bind(offset)
            .bind(q.project)
            .bind(q.since)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, name, ext, size, updated, folder, sn)| {
                    json!({ "type": "files", "id": id, "title": name, "ext": ext, "size": size, "updated_at": updated,
                            "subtitle": folder, "snippet": snip(sn), "url": format!("/editor/{id}") })
                })
                .collect()
        }
        "kb" => {
            let rows: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(&format!(
                "SELECT k.id, k.title, k.icon,
                        CASE WHEN $2 AND to_tsvector('russian', k.body) @@ to_tsquery('russian', $3)
                             THEN ts_headline('russian', k.body, to_tsquery('russian', $3), '{HL}') END
                   FROM kb_pages k
                  WHERE k.tenant_id = $1 AND (($2 AND k.search @@ to_tsquery('russian', $3)) OR lower(k.title) LIKE $4 OR ($5 AND $6 <% lower(k.title)))
                  ORDER BY (lower(k.title) LIKE $4) DESC, ts_rank(k.search, to_tsquery('russian', $3)) DESC, k.updated_at DESC
                  LIMIT $7 OFFSET $8"
            ))
            .bind(user.tenant_id)
            .bind(q.has_words)
            .bind(&q.ts)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(limit)
            .bind(offset)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, title, icon, sn)| json!({ "type": "kb", "id": id, "title": title, "icon": icon, "subtitle": "", "snippet": snip(sn), "url": format!("/kb?p={id}") }))
                .collect()
        }
        "companies" => {
            let rows: Vec<(Uuid, String, Option<String>, String)> = sqlx::query_as(
                "SELECT c.id, c.name, c.inn, c.full_name FROM crm_companies c
                  WHERE c.tenant_id = $1 AND crm_company_visible(c.visibility, c.responsible_id, c.group_id, $2, $3)
                    AND (($4 AND c.search @@ to_tsquery('russian', $5)) OR lower(c.name || ' ' || c.full_name) LIKE $6
                         OR ($7 AND $8 <% lower(c.name || ' ' || c.full_name)) OR c.inn LIKE $6)
                  ORDER BY (lower(c.name) LIKE $6) DESC, ts_rank(c.search, to_tsquery('russian', $5)) DESC, lower(c.name)
                  LIMIT $9 OFFSET $10",
            )
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(user.crm_manager())
            .bind(q.has_words)
            .bind(&q.ts)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(limit)
            .bind(offset)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, name, inn, full)| {
                    let mut sub = Vec::new();
                    if !full.is_empty() && full != name {
                        sub.push(full);
                    }
                    if let Some(i) = inn {
                        sub.push(format!("ИНН {i}"));
                    }
                    json!({ "type": "companies", "id": id, "title": name, "subtitle": sub.join(" · "), "snippet": null, "url": format!("/crm/companies/{id}") })
                })
                .collect()
        }
        "contacts" => {
            let rows: Vec<(Uuid, String, String, String, String, String, String, Option<Uuid>, Option<String>)> = sqlx::query_as(
                "SELECT ct.id, ct.last_name, ct.first_name, ct.middle_name, ct.position, ct.phone, ct.email, ct.company_id, c.name
                   FROM crm_contacts ct LEFT JOIN crm_companies c ON c.id = ct.company_id
                  WHERE ct.tenant_id = $1 AND (ct.company_id IS NULL OR crm_company_visible(c.visibility, c.responsible_id, c.group_id, $2, $3))
                    AND (($4 AND ct.search @@ to_tsquery('russian', $5))
                         OR lower(ct.last_name || ' ' || ct.first_name || ' ' || ct.middle_name) LIKE $6
                         OR ($7 AND $8 <% lower(ct.last_name || ' ' || ct.first_name || ' ' || ct.middle_name))
                         OR lower(ct.email) LIKE $6 OR regexp_replace(ct.phone, '\\D', '', 'g') LIKE regexp_replace($6, '[^0-9%]', '', 'g'))
                  ORDER BY lower(ct.last_name), lower(ct.first_name)
                  LIMIT $9 OFFSET $10",
            )
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(user.crm_manager())
            .bind(q.has_words)
            .bind(&q.ts)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(limit)
            .bind(offset)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, last, first, middle, pos, phone, email, company_id, company)| {
                    let name = [last, first, middle].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
                    let sub = [Some(pos), company, Some(phone), Some(email)].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                    let url = match company_id {
                        Some(c) => format!("/crm/companies/{c}"),
                        None => format!("/crm/contacts?q={}", urlencode(&name)),
                    };
                    json!({ "type": "contacts", "id": id, "title": name, "subtitle": sub, "snippet": null, "url": url })
                })
                .collect()
        }
        "deals" => {
            let rows: Vec<(Uuid, String, Option<String>, Option<f64>, Option<String>)> = sqlx::query_as(
                "SELECT d.id, d.title, c.name, d.amount::float8, s.name
                   FROM crm_deals d LEFT JOIN crm_companies c ON c.id = d.company_id LEFT JOIN crm_stages s ON s.id = d.stage_id
                  WHERE d.tenant_id = $1
                    AND (d.company_id IS NULL OR d.responsible_id = $2 OR crm_company_visible(c.visibility, c.responsible_id, c.group_id, $2, $3))
                    AND (($4 AND d.search @@ to_tsquery('russian', $5)) OR lower(d.title) LIKE $6 OR ($7 AND $8 <% lower(d.title))
                         OR lower(COALESCE(c.name, '')) LIKE $6)
                  ORDER BY (lower(d.title) LIKE $6) DESC, ts_rank(d.search, to_tsquery('russian', $5)) DESC, d.created_at DESC
                  LIMIT $9 OFFSET $10",
            )
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(user.crm_manager())
            .bind(q.has_words)
            .bind(&q.ts)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(limit)
            .bind(offset)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, title, company, amount, stage)| {
                    let mut sub = Vec::new();
                    if let Some(c) = company {
                        sub.push(c);
                    }
                    if let Some(s) = stage {
                        sub.push(s);
                    }
                    if let Some(a) = amount.filter(|a| *a > 0.0) {
                        sub.push(format!("{} ₽", crate::money::format_rub(a)));
                    }
                    json!({ "type": "deals", "id": id, "title": title, "subtitle": sub.join(" · "), "snippet": null, "url": format!("/crm/deals/{id}") })
                })
                .collect()
        }
        "comments" => {
            if !q.has_words {
                return Ok(Vec::new());
            }
            let rows: Vec<(Uuid, Uuid, i64, String, Uuid, String, Option<String>, DateTime<Utc>, Option<String>)> = sqlx::query_as(&format!(
                "SELECT c.id, t.id, t.number, t.title, p.id, p.name, a.display_name, c.created_at,
                        ts_headline('russian', c.body, to_tsquery('russian', $3), '{HL}')
                   FROM task_comments c JOIN tasks t ON t.id = c.task_id JOIN projects p ON p.id = t.project_id
                   LEFT JOIN users a ON a.id = c.author_id
                  WHERE c.tenant_id = $1 AND c.search @@ to_tsquery('russian', $3)
                    AND ($4 OR EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.user_id = $2))
                    AND ($7::uuid IS NULL OR p.id = $7) AND ($8::date IS NULL OR c.created_at >= $8)
                  ORDER BY ts_rank(c.search, to_tsquery('russian', $3)) DESC, c.created_at DESC
                  LIMIT $5 OFFSET $6"
            ))
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(&q.ts)
            .bind(admin)
            .bind(limit)
            .bind(offset)
            .bind(q.project)
            .bind(q.since)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, tid, number, title, pid, pname, who, at, sn)| {
                    json!({ "type": "comments", "id": id, "title": format!("#{number} {title}"),
                            "subtitle": format!("{} · {} · {}", pname, who.unwrap_or_default(), at.format("%d.%m.%Y")),
                            "snippet": snip(sn), "url": format!("/projects/{pid}?task={tid}") })
                })
                .collect()
        }
        "messages" => {
            if !q.has_words {
                return Ok(Vec::new());
            }
            let rows: Vec<(Uuid, Uuid, String, Option<String>, Option<String>, Option<String>, DateTime<Utc>, Option<String>)> = sqlx::query_as(&format!(
                "SELECT m.id, c.id, c.kind, NULLIF(c.name, ''), p.name,
                        (SELECT u.display_name FROM chat_members cm JOIN users u ON u.id = cm.user_id
                          WHERE c.kind = 'direct' AND cm.channel_id = c.id AND cm.user_id <> $2 LIMIT 1),
                        m.created_at,
                        ts_headline('russian', m.body, to_tsquery('russian', $3), '{HL}')
                   FROM chat_messages m JOIN chat_channels c ON c.id = m.channel_id LEFT JOIN projects p ON p.id = c.project_id
                  WHERE c.tenant_id = $1 AND m.deleted_at IS NULL AND m.search @@ to_tsquery('russian', $3)
                    AND ((c.kind = 'project' AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = c.project_id AND pm.user_id = $2))
                         OR (c.kind <> 'project' AND EXISTS (SELECT 1 FROM chat_members cm WHERE cm.channel_id = c.id AND cm.user_id = $2)))
                    AND ($6::uuid IS NULL OR c.project_id = $6) AND ($7::date IS NULL OR m.created_at >= $7)
                  ORDER BY m.created_at DESC
                  LIMIT $4 OFFSET $5"
            ))
            .bind(user.tenant_id)
            .bind(user.user_id)
            .bind(&q.ts)
            .bind(limit)
            .bind(offset)
            .bind(q.project)
            .bind(q.since)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, cid, kind, name, project, other, at, sn)| {
                    let chat = match kind.as_str() {
                        "project" => project.unwrap_or_default(),
                        "direct" => other.unwrap_or_else(|| "Личные сообщения".into()),
                        _ => name.unwrap_or_else(|| "Группа".into()),
                    };
                    json!({ "type": "messages", "id": id, "title": chat, "subtitle": at.format("%d.%m.%Y %H:%M").to_string(),
                            "at": at, "snippet": snip(sn), "url": format!("/chats?c={cid}") })
                })
                .collect()
        }
        "people" => {
            let rows: Vec<(Uuid, String, String, String)> = sqlx::query_as(
                "SELECT u.id, u.display_name, u.position, u.email::text FROM users u
                  WHERE u.tenant_id = $1 AND u.is_active
                    AND (lower(u.display_name) LIKE $2 OR ($3 AND $4 <% lower(u.display_name)) OR lower(u.email::text) LIKE $2 OR lower(u.position) LIKE $2)
                  ORDER BY lower(u.display_name) LIKE $2 DESC, lower(u.display_name)
                  LIMIT $5 OFFSET $6",
            )
            .bind(user.tenant_id)
            .bind(&q.like)
            .bind(q.fuzzy)
            .bind(&q.low)
            .bind(limit)
            .bind(offset)
            .fetch_all(db)
            .await?;
            rows.into_iter()
                .map(|(id, name, pos, email)| {
                    let sub = [pos, email].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                    json!({ "type": "people", "id": id, "title": name, "subtitle": sub, "snippet": null, "url": format!("/team?q={}", urlencode(&name)) })
                })
                .collect()
        }
        _ => Vec::new(),
    })
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_queries() {
        let q = parse("Обследование  фасада!");
        assert_eq!(q.ts, "обследование:* & фасада:*");
        assert!(q.has_words && q.fuzzy);
        assert_eq!(parse("#234").number, 234);
        assert_eq!(parse("50%_x").like, "%50\\%\\_x%");
        assert!(!parse("!!").has_words);
        assert!(!parse("абв").fuzzy);
    }

    #[test]
    fn snippets_start_near_the_match() {
        use super::snip;
        assert_eq!(snip(Some("нет совпадений".into())), None);
        assert_eq!(snip(Some("Короткий  ⟦текст⟧".into())).unwrap(), "Короткий ⟦текст⟧");
        let long = format!("{} слово ⟦коррозия⟧ шпунта", "Протокол испытаний бетона надстройки соответствует ".repeat(3));
        let s = snip(Some(long)).unwrap();
        assert!(s.starts_with('…') && s.ends_with("⟦коррозия⟧ шпунта") && s.chars().count() < 70, "{s}");
    }
}
