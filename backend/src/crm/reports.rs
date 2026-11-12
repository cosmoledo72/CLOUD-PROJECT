//! CRM reports (B11): key figures, funnel and conversion, won and lost by
//! month, sales managers, reasons for lost deals; drill-down to the deals
//! behind a number and an Excel export.
//!
//! Scope: CRM managers see every deal (optionally one responsible person);
//! other CRM users see only deals they are responsible for. Money received and
//! owed by clients is shown only to those who see the finance section.
//!
//! Periods are dates in the portal's time zone: new deals by creation date,
//! won and lost deals by closing date. The funnel follows the deals created in
//! the period: a deal counts on every open stage up to the furthest one it
//! reached (`crm_deal_stage_log`); a won deal counts on all of them.

use std::collections::{BTreeMap, HashMap};

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    money::round2,
    state::AppState,
};

#[derive(Deserialize, Clone)]
pub struct ReportQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Managers only: one responsible person; `none` — deals without one.
    pub responsible_id: Option<String>,
}

#[derive(sqlx::FromRow, Clone)]
struct DealRow {
    id: Uuid,
    title: String,
    company_name: Option<String>,
    responsible_id: Option<Uuid>,
    responsible_name: Option<String>,
    amount: f64,
    created_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    created_d: NaiveDate,
    closed_d: Option<NaiveDate>,
    stage_id: Uuid,
    stage_name: String,
    stage_kind: String,
    probability: i16,
    lost_reason: String,
}

#[derive(sqlx::FromRow, Clone)]
struct StageRow {
    id: Uuid,
    name: String,
    kind: String,
    color: String,
}

struct Data {
    deals: Vec<DealRow>,
    stages: Vec<StageRow>,
    /// Deal → stages it entered.
    reached: HashMap<Uuid, Vec<Uuid>>,
    /// The user sees money received and owed (finance section).
    money: bool,
    received: HashMap<Option<Uuid>, f64>,
    debt: HashMap<Option<Uuid>, f64>,
    prev_from: NaiveDate,
    prev_to: NaiveDate,
}

/// Scope condition for deal alias `d`: `$1` tenant, `$2` user, `$3` manager flag,
/// `$4` filter set, `$5` responsible (NULL = without one).
const SCOPE: &str = "d.tenant_id = $1
     AND (($3 AND (NOT $4 OR d.responsible_id IS NOT DISTINCT FROM $5)) OR (NOT $3 AND d.responsible_id = $2))";

fn filter(user: &CurrentUser, q: &ReportQuery) -> ApiResult<(bool, Option<Uuid>)> {
    match q.responsible_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok((false, None)),
        Some(_) if !user.crm_manager() => Ok((false, None)),
        Some("none") => Ok((true, None)),
        Some(s) => Uuid::parse_str(s).map(|u| (true, Some(u))).map_err(|_| ApiError::BadRequest("bad responsible_id".into())),
    }
}

async fn load(state: &AppState, user: &CurrentUser, q: &ReportQuery) -> ApiResult<Data> {
    user.require_crm()?;
    if q.to < q.from {
        return bad_request("the period ends before it starts");
    }
    if (q.to - q.from).num_days() > 366 * 20 {
        return bad_request("the period is too long");
    }
    let (filtered, who) = filter(user, q)?;
    let manager = user.crm_manager();
    let deals: Vec<DealRow> = sqlx::query_as(&format!(
        "SELECT d.id, d.title, c.name AS company_name, d.responsible_id, u.display_name AS responsible_name,
                d.amount::float8 AS amount, d.created_at, d.closed_at,
                (d.created_at AT TIME ZONE t.timezone)::date AS created_d,
                (d.closed_at AT TIME ZONE t.timezone)::date AS closed_d,
                d.stage_id, s.name AS stage_name, s.kind AS stage_kind, s.probability, d.lost_reason
           FROM crm_deals d
           JOIN tenants t ON t.id = d.tenant_id
           JOIN crm_stages s ON s.id = d.stage_id
           LEFT JOIN crm_companies c ON c.id = d.company_id
           LEFT JOIN users u ON u.id = d.responsible_id
          WHERE {SCOPE}
          ORDER BY d.created_at
          LIMIT 50000"
    ))
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(manager)
    .bind(filtered)
    .bind(who)
    .fetch_all(&state.db)
    .await?;
    let stages: Vec<StageRow> = sqlx::query_as("SELECT id, name, kind, color FROM crm_stages WHERE tenant_id = $1 ORDER BY sort_order")
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?;
    let ids: Vec<Uuid> = deals.iter().map(|d| d.id).collect();
    let log: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT deal_id, stage_id FROM crm_deal_stage_log WHERE deal_id = ANY($1)")
        .bind(&ids)
        .fetch_all(&state.db)
        .await?;
    let mut reached: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (d, s) in log {
        reached.entry(d).or_default().push(s);
    }
    for d in &deals {
        let r = reached.entry(d.id).or_default();
        if !r.contains(&d.stage_id) {
            r.push(d.stage_id);
        }
    }

    let money = user.finance_view();
    let (mut received, mut debt) = (HashMap::new(), HashMap::new());
    if money {
        let rows: Vec<(Option<Uuid>, f64)> = sqlx::query_as(&format!(
            "SELECT d.responsible_id, sum(p.amount)::float8
               FROM payments p
               LEFT JOIN crm_documents doc ON doc.id = p.invoice_id
               LEFT JOIN projects pr ON pr.id = p.project_id
               JOIN crm_deals d ON d.id = COALESCE(doc.deal_id, pr.deal_id)
              WHERE p.tenant_id = $1 AND p.kind = 'income' AND p.status = 'fact'
                AND p.pay_date BETWEEN $6 AND $7 AND {SCOPE}
              GROUP BY 1"
        ))
        .bind(user.tenant_id)
        .bind(user.user_id)
        .bind(manager)
        .bind(filtered)
        .bind(who)
        .bind(q.from)
        .bind(q.to)
        .fetch_all(&state.db)
        .await?;
        received.extend(rows);
        let rows: Vec<(Option<Uuid>, f64)> = sqlx::query_as(&format!(
            "SELECT d.responsible_id, sum(GREATEST(doc.amount
                        - COALESCE((SELECT sum(p.amount) FROM payments p WHERE p.invoice_id = doc.id AND p.status = 'fact'), 0), 0))::float8
               FROM crm_documents doc
               JOIN crm_deals d ON d.id = doc.deal_id
              WHERE doc.kind = 'invoice' AND doc.status <> 'cancelled' AND {SCOPE}
              GROUP BY 1"
        ))
        .bind(user.tenant_id)
        .bind(user.user_id)
        .bind(manager)
        .bind(filtered)
        .bind(who)
        .fetch_all(&state.db)
        .await?;
        debt.extend(rows);
    }
    let days = (q.to - q.from).num_days() + 1;
    let prev_to = q.from - Duration::days(1);
    let prev_from = prev_to - Duration::days(days - 1);
    Ok(Data { deals, stages, reached, money, received, debt, prev_from, prev_to })
}

fn within(d: Option<NaiveDate>, from: NaiveDate, to: NaiveDate) -> bool {
    d.is_some_and(|d| d >= from && d <= to)
}

struct Figures {
    new: usize,
    new_amount: f64,
    won: usize,
    won_amount: f64,
    lost: usize,
    lost_amount: f64,
    cycle_days: Option<f64>,
}

fn figures(deals: &[DealRow], from: NaiveDate, to: NaiveDate) -> Figures {
    let new: Vec<&DealRow> = deals.iter().filter(|d| d.created_d >= from && d.created_d <= to).collect();
    let won: Vec<&DealRow> = deals.iter().filter(|d| d.stage_kind == "won" && within(d.closed_d, from, to)).collect();
    let lost: Vec<&DealRow> = deals.iter().filter(|d| d.stage_kind == "lost" && within(d.closed_d, from, to)).collect();
    let cycles: Vec<f64> = won.iter().filter_map(|d| d.closed_at.map(|c| (c - d.created_at).num_seconds() as f64 / 86400.0)).collect();
    Figures {
        new: new.len(),
        new_amount: new.iter().map(|d| d.amount).sum(),
        won: won.len(),
        won_amount: won.iter().map(|d| d.amount).sum(),
        lost: lost.len(),
        lost_amount: lost.iter().map(|d| d.amount).sum(),
        cycle_days: (!cycles.is_empty()).then(|| cycles.iter().sum::<f64>() / cycles.len() as f64),
    }
}

fn win_rate(won: usize, lost: usize) -> Option<f64> {
    (won + lost > 0).then(|| (won as f64 * 1000.0 / (won + lost) as f64).round() / 10.0)
}

/// Index of the furthest open stage a deal reached; won deals reach past all.
fn furthest(data: &Data, open: &[&StageRow], d: &DealRow) -> Option<usize> {
    if d.stage_kind == "won" {
        return Some(open.len());
    }
    let r = data.reached.get(&d.id)?;
    open.iter().enumerate().filter(|(_, s)| r.contains(&s.id)).map(|(i, _)| i).max()
}

fn reason_key(r: &str) -> String {
    let t = r.trim();
    if t.is_empty() { String::new() } else { t.to_lowercase() }
}

/// Months shown in the chart: those of the period, at least six (ending with
/// the period's last month), at most 24 (the last ones).
fn months(from: NaiveDate, to: NaiveDate) -> Vec<(i32, u32)> {
    let idx = |d: NaiveDate| d.year() * 12 + d.month0() as i32;
    let (mut a, b) = (idx(from), idx(to));
    a = a.min(b - 5).max(b - 23);
    (a..=b).map(|i| (i.div_euclid(12), i.rem_euclid(12) as u32 + 1)).collect()
}

fn report(data: &Data, user: &CurrentUser, q: &ReportQuery) -> Value {
    let cur = figures(&data.deals, q.from, q.to);
    let prev = figures(&data.deals, data.prev_from, data.prev_to);
    let open_deals: Vec<&DealRow> = data.deals.iter().filter(|d| d.stage_kind == "open").collect();
    let weighted: f64 = open_deals.iter().map(|d| d.amount * d.probability as f64 / 100.0).sum();

    // Funnel of the deals created in the period.
    let open: Vec<&StageRow> = data.stages.iter().filter(|s| s.kind == "open").collect();
    let cohort: Vec<&DealRow> = data.deals.iter().filter(|d| d.created_d >= q.from && d.created_d <= q.to).collect();
    let mut steps: Vec<(usize, f64)> = vec![(0, 0.0); open.len() + 1];
    for d in &cohort {
        if let Some(f) = furthest(data, &open, d) {
            for s in steps.iter_mut().take(f.min(open.len()) + 1) {
                s.0 += 1;
                s.1 += d.amount;
            }
        }
    }
    let won_stage = data.stages.iter().find(|s| s.kind == "won");
    let mut funnel: Vec<Value> = open
        .iter()
        .enumerate()
        .map(|(i, s)| json!({ "stage_id": s.id, "name": s.name, "color": s.color, "kind": "open", "count": steps[i].0, "amount": round2(steps[i].1) }))
        .collect();
    funnel.push(json!({
        "stage_id": won_stage.map(|s| s.id), "name": won_stage.map(|s| s.name.as_str()).unwrap_or("Выиграна"),
        "color": won_stage.map(|s| s.color.as_str()).unwrap_or("#16a34a"), "kind": "won",
        "count": steps[open.len()].0, "amount": round2(steps[open.len()].1),
    }));

    // Won and lost by month.
    let by_month: Vec<Value> = months(q.from, q.to)
        .into_iter()
        .map(|(y, m)| {
            let inm = |d: &&DealRow| d.closed_d.is_some_and(|c| c.year() == y && c.month() == m);
            let w: Vec<&DealRow> = data.deals.iter().filter(|d| d.stage_kind == "won").filter(inm).collect();
            let l: Vec<&DealRow> = data.deals.iter().filter(|d| d.stage_kind == "lost").filter(inm).collect();
            json!({
                "month": format!("{y:04}-{m:02}"),
                "won": w.len(), "won_amount": round2(w.iter().map(|d| d.amount).sum()),
                "lost": l.len(), "lost_amount": round2(l.iter().map(|d| d.amount).sum()),
            })
        })
        .collect();

    // Managers.
    struct M {
        name: String,
        new: usize,
        won: usize,
        won_amount: f64,
        lost: usize,
        open: usize,
    }
    let mut mgr: BTreeMap<Option<Uuid>, M> = BTreeMap::new();
    let entry = |mgr: &mut BTreeMap<Option<Uuid>, M>, d: &DealRow| {
        mgr.entry(d.responsible_id)
            .or_insert_with(|| M { name: d.responsible_name.clone().unwrap_or_else(|| "Без ответственного".into()), new: 0, won: 0, won_amount: 0.0, lost: 0, open: 0 });
    };
    for d in &data.deals {
        let is_new = d.created_d >= q.from && d.created_d <= q.to;
        let closed = within(d.closed_d, q.from, q.to);
        if !(is_new || closed || d.stage_kind == "open") {
            continue;
        }
        entry(&mut mgr, d);
        let m = mgr.get_mut(&d.responsible_id).expect("inserted");
        if is_new {
            m.new += 1;
        }
        match d.stage_kind.as_str() {
            "won" if closed => {
                m.won += 1;
                m.won_amount += d.amount;
            }
            "lost" if closed => m.lost += 1,
            "open" => m.open += 1,
            _ => {}
        }
    }
    if data.money {
        // People with money but no deals in the period still show up.
        for k in data.received.keys().chain(data.debt.keys()) {
            if !mgr.contains_key(k) {
                let name = data.deals.iter().find(|d| d.responsible_id == *k).and_then(|d| d.responsible_name.clone());
                mgr.insert(*k, M { name: name.unwrap_or_else(|| "Без ответственного".into()), new: 0, won: 0, won_amount: 0.0, lost: 0, open: 0 });
            }
        }
    }
    let mut managers: Vec<Value> = mgr
        .iter()
        .map(|(id, m)| {
            let mut v = json!({
                "user_id": id, "name": m.name, "new": m.new, "won": m.won, "won_amount": round2(m.won_amount),
                "lost": m.lost, "open": m.open, "win_rate": win_rate(m.won, m.lost),
            });
            if data.money {
                v["received"] = json!(round2(data.received.get(id).copied().unwrap_or(0.0)));
                v["debt"] = json!(round2(data.debt.get(id).copied().unwrap_or(0.0)));
            }
            v
        })
        .collect();
    managers.sort_by(|a, b| {
        b["won_amount"].as_f64().partial_cmp(&a["won_amount"].as_f64()).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b["new"].as_u64().cmp(&a["new"].as_u64()))
    });
    let mut totals = json!({
        "new": cur.new, "won": cur.won, "won_amount": round2(cur.won_amount), "lost": cur.lost,
        "open": open_deals.len(), "win_rate": win_rate(cur.won, cur.lost),
    });
    if data.money {
        totals["received"] = json!(round2(data.received.values().sum()));
        totals["debt"] = json!(round2(data.debt.values().sum()));
    }

    // Reasons for lost deals closed in the period.
    let mut reasons: BTreeMap<String, (String, usize, f64)> = BTreeMap::new();
    for d in data.deals.iter().filter(|d| d.stage_kind == "lost" && within(d.closed_d, q.from, q.to)) {
        let e = reasons.entry(reason_key(&d.lost_reason)).or_insert_with(|| (d.lost_reason.trim().to_string(), 0, 0.0));
        e.1 += 1;
        e.2 += d.amount;
    }
    let mut reasons: Vec<Value> = reasons
        .into_iter()
        .map(|(k, (text, n, sum))| json!({ "key": k, "reason": if k.is_empty() { "Без причины".to_string() } else { text }, "count": n, "amount": round2(sum), "empty": k.is_empty() }))
        .collect();
    reasons.sort_by(|a, b| a["empty"].as_bool().cmp(&b["empty"].as_bool()).then_with(|| b["count"].as_u64().cmp(&a["count"].as_u64())));

    json!({
        "from": q.from, "to": q.to, "prev_from": data.prev_from, "prev_to": data.prev_to,
        "can_filter": user.crm_manager(), "money": data.money,
        "kpi": {
            "new": cur.new, "new_amount": round2(cur.new_amount), "new_prev": prev.new,
            "won": cur.won, "won_amount": round2(cur.won_amount), "won_prev": prev.won, "won_amount_prev": round2(prev.won_amount),
            "avg_check": (cur.won > 0).then(|| round2(cur.won_amount / cur.won as f64)),
            "lost": cur.lost, "lost_amount": round2(cur.lost_amount),
            "win_rate": win_rate(cur.won, cur.lost), "win_rate_prev": win_rate(prev.won, prev.lost),
            "cycle_days": cur.cycle_days.map(|c| c.round()), "cycle_days_prev": prev.cycle_days.map(|c| c.round()),
            "open": open_deals.len(), "open_amount": round2(open_deals.iter().map(|d| d.amount).sum()), "weighted": round2(weighted),
        },
        "funnel": funnel,
        "months": by_month,
        "managers": managers,
        "totals": totals,
        "reasons": reasons,
    })
}

/// `GET /api/crm/reports?from&to&responsible_id`
pub async fn summary(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ReportQuery>) -> ApiResult<Json<Value>> {
    let data = load(&state, &user, &q).await?;
    Ok(Json(report(&data, &user, &q)))
}

#[derive(Deserialize)]
pub struct DealsQuery {
    #[serde(flatten)]
    pub period: ReportQuery,
    /// new | won | lost | open | funnel | reason
    pub set: String,
    /// `funnel`: the stage (an open stage or the won one).
    pub stage_id: Option<Uuid>,
    /// `reason`: the reason key (lowercase; empty — without a reason).
    pub reason: Option<String>,
    /// Narrow to one responsible person (`none` — without one).
    pub manager: Option<String>,
}

fn select<'a>(data: &'a Data, q: &DealsQuery) -> ApiResult<Vec<&'a DealRow>> {
    let p = &q.period;
    let manager = match q.manager.as_deref() {
        None | Some("") => None,
        Some("none") => Some(None),
        Some(s) => Some(Some(Uuid::parse_str(s).map_err(|_| ApiError::BadRequest("bad manager".into()))?)),
    };
    let is_new = |d: &DealRow| d.created_d >= p.from && d.created_d <= p.to;
    let open: Vec<&StageRow> = data.stages.iter().filter(|s| s.kind == "open").collect();
    let mut out: Vec<&DealRow> = match q.set.as_str() {
        "new" => data.deals.iter().filter(|d| is_new(d)).collect(),
        "won" | "lost" => data.deals.iter().filter(|d| d.stage_kind == q.set && within(d.closed_d, p.from, p.to)).collect(),
        "open" => data.deals.iter().filter(|d| d.stage_kind == "open").collect(),
        "funnel" => {
            let step = match q.stage_id {
                Some(id) => match open.iter().position(|s| s.id == id) {
                    Some(i) => i,
                    None if data.stages.iter().any(|s| s.id == id && s.kind == "won") => open.len(),
                    None => return bad_request("unknown stage"),
                },
                None => return bad_request("stage_id is required"),
            };
            data.deals.iter().filter(|d| is_new(d) && furthest(data, &open, d).is_some_and(|f| f >= step)).collect()
        }
        "reason" => {
            let key = q.reason.as_deref().map(reason_key).unwrap_or_default();
            data.deals
                .iter()
                .filter(|d| d.stage_kind == "lost" && within(d.closed_d, p.from, p.to) && reason_key(&d.lost_reason) == key)
                .collect()
        }
        _ => return bad_request("unknown set"),
    };
    if let Some(m) = manager {
        out.retain(|d| d.responsible_id == m);
    }
    out.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

fn deal_json(d: &DealRow) -> Value {
    json!({
        "id": d.id, "title": d.title, "company_name": d.company_name, "responsible_name": d.responsible_name,
        "amount": d.amount, "stage_name": d.stage_name, "stage_kind": d.stage_kind,
        "created": d.created_d, "closed": d.closed_d, "lost_reason": d.lost_reason,
    })
}

/// `GET /api/crm/reports/deals?…&set=…` — the deals behind a number.
pub async fn deals(State(state): State<AppState>, user: CurrentUser, Query(q): Query<DealsQuery>) -> ApiResult<Json<Value>> {
    let data = load(&state, &user, &q.period).await?;
    let list = select(&data, &q)?;
    let total: f64 = list.iter().map(|d| d.amount).sum();
    Ok(Json(json!({ "deals": list.iter().take(500).map(|d| deal_json(d)).collect::<Vec<_>>(), "count": list.len(), "amount": round2(total) })))
}

/// `GET /api/crm/lost-reasons` — reasons used before, most frequent first
/// (suggestions when a deal is lost).
pub async fn lost_reasons(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT min(btrim(lost_reason)) FROM crm_deals
          WHERE tenant_id = $1 AND btrim(lost_reason) <> ''
          GROUP BY lower(btrim(lost_reason)) ORDER BY count(*) DESC, 1 LIMIT 12",
    )
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "reasons": rows.into_iter().map(|r| r.0).collect::<Vec<_>>() })))
}

/// `GET /api/crm/reports.xlsx` — the report as an Excel workbook.
pub async fn xlsx(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ReportQuery>) -> ApiResult<Response> {
    let data = load(&state, &user, &q).await?;
    let r = report(&data, &user, &q);
    let period: Vec<DealRow> = data
        .deals
        .iter()
        .filter(|d| (d.created_d >= q.from && d.created_d <= q.to) || within(d.closed_d, q.from, q.to))
        .cloned()
        .collect();
    let (from, to) = (q.from, q.to);
    let bytes = tokio::task::spawn_blocking(move || write_xlsx(&r, &period, from, to))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
    let name = format!("Отчёт CRM {} — {}.xlsx", q.from.format("%d.%m.%Y"), q.to.format("%d.%m.%Y"));
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")),
            (header::CONTENT_DISPOSITION, crate::files::content_disposition(&name, false)),
        ],
        bytes,
    )
        .into_response())
}

const MONTHS: [&str; 12] = ["январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь"];

fn write_xlsx(r: &Value, deals: &[DealRow], from: NaiveDate, to: NaiveDate) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    use rust_xlsxwriter::{Format, FormatAlign, Workbook};
    let mut wb = Workbook::new();
    let head = Format::new().set_bold().set_background_color(0xE8EDF3).set_text_wrap().set_align(FormatAlign::Top);
    let money_f = Format::new().set_num_format("# ##0.00 ₽");
    let pct_f = Format::new().set_num_format("0.0\" %\"");
    let date_f = Format::new().set_num_format("dd.mm.yyyy");
    let bold = Format::new().set_bold();
    let xdate = |d: NaiveDate| rust_xlsxwriter::ExcelDateTime::from_ymd(d.year_ce().1 as u16, d.month() as u8, d.day() as u8);
    let f = |v: &Value| v.as_f64().unwrap_or(0.0);
    let k = &r["kpi"];

    // Summary.
    let ws = wb.add_worksheet().set_name("Сводка")?;
    ws.set_column_width(0, 40)?;
    ws.set_column_width(1, 20)?;
    ws.write_with_format(0, 0, format!("Отчёт CRM за {} — {}", from.format("%d.%m.%Y"), to.format("%d.%m.%Y")), &bold)?;
    let mut row = 2;
    let mut line = |ws: &mut rust_xlsxwriter::Worksheet, name: &str, v: &Value, fmt: Option<&Format>| -> Result<(), rust_xlsxwriter::XlsxError> {
        ws.write(row, 0, name)?;
        if !v.is_null() {
            match fmt {
                Some(fm) => ws.write_number_with_format(row, 1, f(v), fm)?,
                None => ws.write_number(row, 1, f(v))?,
            };
        }
        row += 1;
        Ok(())
    };
    line(ws, "Новых сделок", &k["new"], None)?;
    line(ws, "Сумма новых сделок", &k["new_amount"], Some(&money_f))?;
    line(ws, "Выиграно сделок", &k["won"], None)?;
    line(ws, "Сумма выигранных", &k["won_amount"], Some(&money_f))?;
    line(ws, "Средний чек", &k["avg_check"], Some(&money_f))?;
    line(ws, "Проиграно сделок", &k["lost"], None)?;
    line(ws, "Конверсия в договор (выиграно из закрытых)", &k["win_rate"], Some(&pct_f))?;
    line(ws, "Средний цикл выигранной сделки, дней", &k["cycle_days"], None)?;
    line(ws, "Открытых сделок сейчас", &k["open"], None)?;
    line(ws, "Сумма открытых", &k["open_amount"], Some(&money_f))?;
    line(ws, "В работе с учётом вероятности", &k["weighted"], Some(&money_f))?;

    // Funnel.
    let ws = wb.add_worksheet().set_name("Воронка")?;
    for (c, (h, w)) in [("Этап", 30.0), ("Сделок дошло", 14.0), ("От предыдущего", 14.0), ("От первого этапа", 14.0), ("Сумма", 18.0)].iter().enumerate() {
        ws.write_with_format(0, c as u16, *h, &head)?;
        ws.set_column_width(c as u16, *w)?;
    }
    let funnel = r["funnel"].as_array().cloned().unwrap_or_default();
    let first = funnel.first().map(|s| f(&s["count"])).unwrap_or(0.0);
    for (i, s) in funnel.iter().enumerate() {
        let row = i as u32 + 1;
        let n = f(&s["count"]);
        ws.write(row, 0, s["name"].as_str().unwrap_or(""))?;
        ws.write_number(row, 1, n)?;
        if i > 0 {
            let p = f(&funnel[i - 1]["count"]);
            if p > 0.0 {
                ws.write_number_with_format(row, 2, (n * 1000.0 / p).round() / 10.0, &pct_f)?;
            }
        }
        if first > 0.0 {
            ws.write_number_with_format(row, 3, (n * 1000.0 / first).round() / 10.0, &pct_f)?;
        }
        ws.write_number_with_format(row, 4, f(&s["amount"]), &money_f)?;
    }

    // Months.
    let ws = wb.add_worksheet().set_name("По месяцам")?;
    for (c, (h, w)) in [("Месяц", 18.0), ("Выиграно", 12.0), ("Сумма выигранных", 18.0), ("Проиграно", 12.0), ("Сумма проигранных", 18.0)].iter().enumerate() {
        ws.write_with_format(0, c as u16, *h, &head)?;
        ws.set_column_width(c as u16, *w)?;
    }
    for (i, m) in r["months"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let row = i as u32 + 1;
        let key = m["month"].as_str().unwrap_or("0000-01");
        let mi: usize = key[5..].parse().unwrap_or(1);
        ws.write(row, 0, format!("{} {}", MONTHS[mi - 1], &key[..4]))?;
        ws.write_number(row, 1, f(&m["won"]))?;
        ws.write_number_with_format(row, 2, f(&m["won_amount"]), &money_f)?;
        ws.write_number(row, 3, f(&m["lost"]))?;
        ws.write_number_with_format(row, 4, f(&m["lost_amount"]), &money_f)?;
    }

    // Managers.
    let ws = wb.add_worksheet().set_name("Менеджеры")?;
    let money = r["money"].as_bool().unwrap_or(false);
    let mut cols = vec![("Менеджер", 28.0), ("Новых", 9.0), ("Выиграно", 10.0), ("Сумма выигранных", 18.0), ("Проиграно", 10.0), ("Конверсия", 11.0), ("Открытых", 10.0)];
    if money {
        cols.push(("Поступило", 18.0));
        cols.push(("Долг клиентов", 18.0));
    }
    for (c, (h, w)) in cols.iter().enumerate() {
        ws.write_with_format(0, c as u16, *h, &head)?;
        ws.set_column_width(c as u16, *w)?;
    }
    let mut list = r["managers"].as_array().cloned().unwrap_or_default();
    let mut t = r["totals"].clone();
    t["name"] = json!("Итого");
    list.push(t);
    let n = list.len();
    for (i, m) in list.iter().enumerate() {
        let row = i as u32 + 1;
        let total = i + 1 == n;
        let money_cell = if total { Format::new().set_bold().set_num_format("# ##0.00 ₽") } else { money_f.clone() };
        if total {
            ws.write_with_format(row, 0, m["name"].as_str().unwrap_or(""), &bold)?;
        } else {
            ws.write(row, 0, m["name"].as_str().unwrap_or(""))?;
        }
        ws.write_number(row, 1, f(&m["new"]))?;
        ws.write_number(row, 2, f(&m["won"]))?;
        ws.write_number_with_format(row, 3, f(&m["won_amount"]), &money_cell)?;
        ws.write_number(row, 4, f(&m["lost"]))?;
        if !m["win_rate"].is_null() {
            ws.write_number_with_format(row, 5, f(&m["win_rate"]), &pct_f)?;
        }
        ws.write_number(row, 6, f(&m["open"]))?;
        if money {
            ws.write_number_with_format(row, 7, f(&m["received"]), &money_cell)?;
            ws.write_number_with_format(row, 8, f(&m["debt"]), &money_cell)?;
        }
    }

    // Reasons.
    let ws = wb.add_worksheet().set_name("Причины отказов")?;
    for (c, (h, w)) in [("Причина", 44.0), ("Сделок", 10.0), ("Сумма", 18.0)].iter().enumerate() {
        ws.write_with_format(0, c as u16, *h, &head)?;
        ws.set_column_width(c as u16, *w)?;
    }
    for (i, x) in r["reasons"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let row = i as u32 + 1;
        ws.write(row, 0, x["reason"].as_str().unwrap_or(""))?;
        ws.write_number(row, 1, f(&x["count"]))?;
        ws.write_number_with_format(row, 2, f(&x["amount"]), &money_f)?;
    }

    // Deals of the period.
    let ws = wb.add_worksheet().set_name("Сделки")?;
    for (c, (h, w)) in [("Сделка", 40.0), ("Контрагент", 30.0), ("Ответственный", 24.0), ("Этап", 22.0), ("Сумма", 16.0), ("Создана", 12.0), ("Закрыта", 12.0), ("Причина проигрыша", 30.0)]
        .iter()
        .enumerate()
    {
        ws.write_with_format(0, c as u16, *h, &head)?;
        ws.set_column_width(c as u16, *w)?;
    }
    for (i, d) in deals.iter().enumerate() {
        let row = i as u32 + 1;
        ws.write(row, 0, &d.title)?;
        ws.write(row, 1, d.company_name.as_deref().unwrap_or(""))?;
        ws.write(row, 2, d.responsible_name.as_deref().unwrap_or(""))?;
        ws.write(row, 3, &d.stage_name)?;
        ws.write_number_with_format(row, 4, d.amount, &money_f)?;
        ws.write_datetime_with_format(row, 5, xdate(d.created_d)?, &date_f)?;
        if let Some(c) = d.closed_d {
            ws.write_datetime_with_format(row, 6, xdate(c)?, &date_f)?;
        }
        ws.write(row, 7, &d.lost_reason)?;
    }
    ws.autofilter(0, 0, deals.len() as u32, 7)?;
    wb.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_months() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        // A quarter is widened to six months ending with its last month.
        let m = months(d(2026, 10, 1), d(2026, 12, 31));
        assert_eq!(m.first(), Some(&(2026, 7)));
        assert_eq!(m.last(), Some(&(2026, 12)));
        assert_eq!(m.len(), 6);
        // Across the new year.
        assert_eq!(months(d(2027, 1, 1), d(2027, 1, 31)).first(), Some(&(2026, 8)));
        // A long period keeps the last 24 months.
        let m = months(d(2020, 1, 1), d(2026, 12, 31));
        assert_eq!((m.len(), m[0]), (24, (2025, 1)));
        assert_eq!(win_rate(9, 15), Some(37.5));
        assert_eq!(win_rate(0, 0), None);
        assert_eq!(reason_key("  Цена "), "цена");
    }
}
