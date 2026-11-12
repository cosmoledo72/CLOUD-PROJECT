//! Export of a project's Gantt chart (C2): PDF for printing (A3/A4 landscape,
//! several sheets with a repeated time scale and a branded stamp), XLSX (a
//! chart of shaded cells and a task list) and MS Project XML (MSPDI).
//!
//! The rows are the project's task tree in the Gantt order; the client may
//! pass the ids it currently shows (filters, collapsed groups) and ask to
//! leave out done tasks. Names for the stamp are remembered per project.

mod mspdi;
mod pdf;
mod xlsx;

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    calendar::WorkCalendar,
    error::{bad_request, ApiError, ApiResult},
    projects::{project_role, ProjectRole, TaskRow, TASK_SELECT},
    scheduling,
    state::AppState,
    storage::BlobStore,
};

/// Names in the stamp.
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Stamp {
    #[serde(default)]
    pub developer: String,
    #[serde(default)]
    pub checker: String,
    #[serde(default)]
    pub gip: String,
}

impl Stamp {
    fn clean(mut self) -> Self {
        for s in [&mut self.developer, &mut self.checker, &mut self.gip] {
            *s = s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
        }
        self
    }
}

/// One row of the exported chart (a task in tree order).
#[derive(Clone, Debug)]
pub struct Row {
    pub id: Uuid,
    pub number: i64,
    pub title: String,
    pub description: String,
    pub depth: usize,
    /// «1.2.3» — position in the exported tree.
    pub wbs: String,
    pub parent: Option<Uuid>,
    pub is_parent: bool,
    pub milestone: bool,
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub progress: u8,
    pub done: bool,
    pub assignee: Option<String>,
    pub critical: bool,
    pub base: Option<(NaiveDate, NaiveDate)>,
}

#[derive(Clone, Debug)]
pub struct Link {
    pub task: Uuid,
    pub pred: Uuid,
    /// FS | SS | FF | SF
    pub kind: String,
    pub lag: i32,
}

/// Everything the writers need.
pub struct Data {
    pub project_name: String,
    pub code: String,
    pub portal_name: String,
    pub accent: (u8, u8, u8),
    /// Logo bytes and MIME type (PNG, JPEG or SVG are drawn in the PDF stamp).
    pub logo: Option<(Vec<u8>, String)>,
    pub rows: Vec<Row>,
    pub links: Vec<Link>,
    /// `work` (working days) or `calendar`.
    pub day_mode: String,
    /// Working days of the portal (for shading and the MS Project calendar).
    pub workdays: WorkCalendar,
    pub special: HashMap<NaiveDate, bool>,
    pub today: NaiveDate,
    pub stamp: Stamp,
    pub show_critical: bool,
    pub show_baseline: bool,
    pub exported_by: String,
}

impl Data {
    pub fn is_work(&self, d: NaiveDate) -> bool {
        self.workdays.is_work(d)
    }

    /// Duration of a row: working days in work mode, otherwise calendar days.
    pub fn duration(&self, r: &Row) -> Option<i64> {
        let (s, e) = (r.start?, r.end?);
        if r.milestone {
            return Some(0);
        }
        Some(if self.day_mode == "work" { (self.workdays.floor(e) - self.workdays.ceil(s) + 1).max(0) } else { (e - s).num_days() + 1 })
    }

    pub fn unit(&self) -> &'static str {
        if self.day_mode == "work" { "раб. дн." } else { "дн." }
    }

    /// First and last dates over bars, milestones, baselines and today.
    pub fn span(&self) -> (NaiveDate, NaiveDate) {
        let mut lo: Option<NaiveDate> = None;
        let mut hi: Option<NaiveDate> = None;
        let mut put = |d: NaiveDate| {
            lo = Some(lo.map_or(d, |x| x.min(d)));
            hi = Some(hi.map_or(d, |x| x.max(d)));
        };
        for r in &self.rows {
            r.start.into_iter().chain(r.end).for_each(&mut put);
            if self.show_baseline {
                if let Some((a, b)) = r.base {
                    put(a);
                    put(b);
                }
            }
        }
        match (lo, hi) {
            (Some(a), Some(b)) => (a, b),
            _ => (self.today, self.today + chrono::Duration::days(30)),
        }
    }

    pub fn title_line(&self) -> String {
        if self.code.is_empty() { self.project_name.clone() } else { format!("{} — {}", self.code, self.project_name) }
    }
}

/// «12ОН+2» — a predecessor in Russian MS Project notation (by row number).
pub fn link_label(row_no: usize, kind: &str, lag: i32) -> String {
    let k = match kind {
        "SS" => "НН",
        "FF" => "ОО",
        "SF" => "НО",
        _ => "ОН",
    };
    let lag = match lag {
        0 => String::new(),
        l if l > 0 => format!("+{l}"),
        l => format!("{l}"),
    };
    format!("{row_no}{k}{lag}")
}

pub fn ru_date(d: NaiveDate) -> String {
    d.format("%d.%m.%Y").to_string()
}

pub const MONTHS: [&str; 12] = ["Январь", "Февраль", "Март", "Апрель", "Май", "Июнь", "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь"];
pub const MONTHS_SHORT: [&str; 12] = ["янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"];

pub fn month_name(d: NaiveDate) -> &'static str {
    MONTHS[d.month0() as usize]
}

// ---------------------------------------------------------------------------
// Settings (stamp names)
// ---------------------------------------------------------------------------

async fn suggested_stamp(state: &AppState, user: &CurrentUser, project_id: Uuid) -> ApiResult<Stamp> {
    let saved: Value = sqlx::query_scalar("SELECT export_stamp FROM projects WHERE id = $1").bind(project_id).fetch_one(&state.db).await?;
    let mut stamp: Stamp = serde_json::from_value(saved).unwrap_or_default();
    if stamp.developer.is_empty() {
        stamp.developer = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1").bind(user.user_id).fetch_one(&state.db).await?;
    }
    if stamp.gip.is_empty() {
        // A «ГИП» user requisite, else the first project manager.
        let gip: Option<String> = sqlx::query_scalar(
            "SELECT u.display_name FROM project_field_defs d
               JOIN project_field_values v ON v.field_id = d.id AND v.project_id = $1
               JOIN users u ON u.id::text = v.value #>> '{}'
              WHERE d.tenant_id = $2 AND d.field_type = 'user' AND NOT d.is_archived
                AND (d.key = 'chief_engineer' OR d.label ILIKE '%ГИП%')
              ORDER BY (d.key = 'chief_engineer') DESC, d.sort_order LIMIT 1",
        )
        .bind(project_id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?;
        stamp.gip = match gip {
            Some(g) => g,
            None => sqlx::query_scalar(
                "SELECT u.display_name FROM project_members pm JOIN users u ON u.id = pm.user_id
                  WHERE pm.project_id = $1 AND pm.role = 'manager' ORDER BY lower(u.display_name) LIMIT 1",
            )
            .bind(project_id)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or_default(),
        };
    }
    Ok(stamp)
}

/// `GET /api/projects/{id}/gantt/export` — suggested stamp names for the export dialog.
pub async fn settings(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    project_role(&state, &user, id).await?;
    let stamp = suggested_stamp(&state, &user, id).await?;
    Ok(Json(json!({ "stamp": stamp })))
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExportRequest {
    /// pdf | xlsx | xml
    pub format: String,
    /// a3 | a4 (PDF)
    #[serde(default = "default_paper")]
    pub paper: String,
    /// auto | day | week | month (PDF, XLSX)
    #[serde(default = "default_scale")]
    pub scale: String,
    /// Only these tasks (what the chart shows now); `None` — all.
    #[serde(default)]
    pub task_ids: Option<Vec<Uuid>>,
    #[serde(default)]
    pub hide_done: bool,
    #[serde(default = "yes")]
    pub critical: bool,
    #[serde(default)]
    pub baseline: bool,
    #[serde(default)]
    pub stamp: Option<Stamp>,
}

fn default_paper() -> String {
    "a3".into()
}
fn default_scale() -> String {
    "auto".into()
}
fn yes() -> bool {
    true
}

/// `POST /api/projects/{id}/gantt/export` — the chart as a PDF, XLSX or MS Project XML file.
pub async fn export(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<ExportRequest>) -> ApiResult<Response> {
    let role = project_role(&state, &user, id).await?;
    if !matches!(req.format.as_str(), "pdf" | "xlsx" | "xml") {
        return bad_request("format must be pdf, xlsx or xml");
    }
    if !matches!(req.paper.as_str(), "a3" | "a4") {
        return bad_request("paper must be a3 or a4");
    }
    if !matches!(req.scale.as_str(), "auto" | "day" | "week" | "month") {
        return bad_request("scale must be auto, day, week or month");
    }
    let stamp = match req.stamp.clone() {
        Some(s) => {
            let s = s.clean();
            if role >= ProjectRole::Member {
                sqlx::query("UPDATE projects SET export_stamp = $2 WHERE id = $1")
                    .bind(id)
                    .bind(serde_json::to_value(&s).unwrap_or(Value::Null))
                    .execute(&state.db)
                    .await?;
            }
            s
        }
        None => suggested_stamp(&state, &user, id).await?,
    };
    let data = collect(&state, &user, id, &req, stamp).await?;
    if data.rows.is_empty() {
        return bad_request("Нет задач для выгрузки");
    }
    let fmt = req.format.clone();
    let paper = req.paper.clone();
    let scale = req.scale.clone();
    let file_base = {
        let n = if data.code.is_empty() { data.project_name.clone() } else { data.code.clone() };
        let n: String = n.chars().filter(|c| !"\\/:*?\"<>|\n\r\t".contains(*c)).take(80).collect();
        format!("График {} {}", n.trim(), data.today.format("%d.%m.%Y"))
    };
    let tenant = user.tenant_id;
    let bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        Ok(match fmt.as_str() {
            "pdf" => pdf::write(&data, &paper, &scale)?,
            "xlsx" => xlsx::write(&data, &scale)?,
            _ => mspdi::write(&data).into_bytes(),
        })
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    let (ext, mime) = match req.format.as_str() {
        "pdf" => ("pdf", "application/pdf"),
        "xlsx" => ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        _ => ("xml", "application/xml"),
    };
    state
        .audit(tenant, Some(user.user_id), "project.gantt_exported", Some("project"), Some(id), json!({ "format": req.format, "paper": req.paper }))
        .await;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
            (header::CONTENT_DISPOSITION, crate::files::content_disposition(&format!("{file_base}.{ext}"), false)),
        ],
        bytes,
    )
        .into_response())
}

fn hex_rgb(s: &str) -> (u8, u8, u8) {
    let h = s.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    if h.len() == 6 { (p(0), p(2), p(4)) } else { (0x1f, 0x4e, 0x79) }
}

async fn collect(state: &AppState, user: &CurrentUser, id: Uuid, req: &ExportRequest, stamp: Stamp) -> ApiResult<Data> {
    let (project_name, day_mode, fields): (String, String, Value) = sqlx::query_as(
        "SELECT p.name, p.day_mode,
                (SELECT COALESCE(jsonb_object_agg(d.key, v.value), '{}'::jsonb)
                   FROM project_field_values v JOIN project_field_defs d ON d.id = v.field_id WHERE v.project_id = p.id)
           FROM projects p WHERE p.id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    let code = fields.get("code").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let (portal_name, accent, logo_key, logo_mime): (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT COALESCE(NULLIF(b.display_name, ''), t.name), COALESCE(b.accent_color, '#1f4e79'), b.logo_key, b.logo_mime
           FROM tenants t LEFT JOIN tenant_branding b ON b.tenant_id = t.id WHERE t.id = $1",
    )
    .bind(user.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let logo = match (logo_key, logo_mime) {
        (Some(k), Some(m)) if matches!(m.as_str(), "image/png" | "image/jpeg" | "image/svg+xml") => {
            state.store.get(&k).await.ok().map(|b| (b.to_vec(), m))
        }
        _ => None,
    };
    let exported_by: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1").bind(user.user_id).fetch_one(&state.db).await?;

    let tasks = sqlx::query_as::<_, TaskRow>(&format!("{TASK_SELECT} WHERE t.project_id = $1 ORDER BY t.sort_order, t.created_at"))
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let links: Vec<(Uuid, Uuid, String, i32)> = sqlx::query_as(
        "SELECT d.task_id, d.depends_on_id, d.kind, d.lag_days FROM task_dependencies d JOIN tasks t ON t.id = d.task_id WHERE t.project_id = $1",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let (nodes, graph) = crate::projects::load_graph(state, id).await?;
    let cal = crate::calendar::for_project(state, id).await?;
    let critical: HashSet<Uuid> = if req.critical { scheduling::critical_path(&cal, &nodes, &graph).critical.into_iter().collect() } else { HashSet::new() };
    let baseline: HashMap<Uuid, (NaiveDate, NaiveDate)> = if req.baseline {
        let rows: Vec<(Uuid, Option<NaiveDate>, Option<NaiveDate>)> =
            sqlx::query_as("SELECT task_id, start_date, due_date FROM task_baselines WHERE project_id = $1").bind(id).fetch_all(&state.db).await?;
        rows.into_iter()
            .filter_map(|(t, s, e)| match (s.or(e), e.or(s)) {
                (Some(a), Some(b)) => Some((t, (a, b))),
                _ => None,
            })
            .collect()
    } else {
        HashMap::new()
    };
    let special = crate::calendar::special_days(state, user.tenant_id).await?;
    let workdays = WorkCalendar::from_special(&special);
    let today = crate::crm::tasks::portal_today(state, user.tenant_id).await?;

    // --- Tree order, filters --------------------------------------------------
    let wanted: Option<HashSet<Uuid>> = req.task_ids.as_ref().map(|v| v.iter().copied().collect());
    let keep = |t: &TaskRow| wanted.as_ref().map_or(true, |w| w.contains(&t.id)) && !(req.hide_done && t.status == "done");
    let by_id: HashMap<Uuid, &TaskRow> = tasks.iter().map(|t| (t.id, t)).collect();
    let mut children: HashMap<Option<Uuid>, Vec<&TaskRow>> = HashMap::new();
    for t in &tasks {
        let parent = t.parent_id.filter(|p| by_id.contains_key(p));
        children.entry(parent).or_default().push(t);
    }
    let mut rows: Vec<Row> = Vec::new();
    // Depth-first; a hidden task hides its subtree.
    fn walk<'a>(
        parent: Option<Uuid>,
        depth: usize,
        prefix: &str,
        children: &HashMap<Option<Uuid>, Vec<&'a TaskRow>>,
        keep: &dyn Fn(&TaskRow) -> bool,
        out: &mut Vec<(&'a TaskRow, usize, String, Option<Uuid>)>,
    ) {
        let mut n = 0;
        for t in children.get(&parent).map(|v| v.as_slice()).unwrap_or(&[]) {
            if !keep(t) {
                continue;
            }
            n += 1;
            let wbs = if prefix.is_empty() { n.to_string() } else { format!("{prefix}.{n}") };
            out.push((t, depth, wbs.clone(), parent));
            walk(Some(t.id), depth + 1, &wbs, children, keep, out);
        }
    }
    let mut order = Vec::new();
    walk(None, 0, "", &children, &keep, &mut order);
    let in_rows: HashSet<Uuid> = order.iter().map(|(t, ..)| t.id).collect();
    let has_child: HashSet<Uuid> = order.iter().filter_map(|(_, _, _, p)| *p).collect();
    for (t, depth, wbs, parent) in &order {
        let base = baseline.get(&t.id).copied();
        let progress = if t.status == "done" { 100 } else { t.progress.clamp(0, 100) as u8 };
        rows.push(Row {
            id: t.id,
            number: t.number,
            title: t.title.clone(),
            description: t.description.clone(),
            depth: *depth,
            wbs: wbs.clone(),
            parent: *parent,
            is_parent: has_child.contains(&t.id),
            milestone: t.is_milestone,
            start: t.start_date.or(t.due_date),
            end: t.due_date.or(t.start_date),
            progress,
            done: t.status == "done",
            assignee: t.assignee_name.clone(),
            critical: critical.contains(&t.id),
            base,
        });
    }
    // Parents without own dates span their children.
    for i in (0..rows.len()).rev() {
        if rows[i].is_parent && (rows[i].start.is_none() || rows[i].end.is_none()) {
            let id = rows[i].id;
            let kids: Vec<(Option<NaiveDate>, Option<NaiveDate>)> = rows.iter().filter(|r| r.parent == Some(id)).map(|r| (r.start, r.end)).collect();
            let s = kids.iter().filter_map(|k| k.0).min();
            let e = kids.iter().filter_map(|k| k.1).max();
            if rows[i].start.is_none() {
                rows[i].start = s;
            }
            if rows[i].end.is_none() {
                rows[i].end = e;
            }
        }
    }
    let links = links
        .into_iter()
        .filter(|(t, p, ..)| in_rows.contains(t) && in_rows.contains(p))
        .map(|(task, pred, kind, lag)| Link { task, pred, kind, lag })
        .collect();
    Ok(Data {
        project_name,
        code,
        portal_name,
        accent: hex_rgb(&accent),
        logo,
        rows,
        links,
        day_mode,
        workdays,
        special,
        today,
        stamp,
        show_critical: req.critical,
        show_baseline: req.baseline,
        exported_by,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_labels() {
        assert_eq!(link_label(12, "FS", 0), "12ОН");
        assert_eq!(link_label(3, "SS", 2), "3НН+2");
        assert_eq!(link_label(4, "FF", -1), "4ОО-1");
        assert_eq!(link_label(5, "SF", 0), "5НО");
    }

    #[test]
    fn colors() {
        assert_eq!(hex_rgb("#1f4e79"), (0x1f, 0x4e, 0x79));
        assert_eq!(hex_rgb("bad"), (0x1f, 0x4e, 0x79));
    }
}
