//! Finance (module `finance`): payments — income and expenses, planned (the
//! contract payment schedule) and actual — with categories, links to a
//! project, a counterparty and the invoice they pay; analytics, receivables
//! and the money of each project.
//!
//! Access: administrators and users with `finance_role` view/edit see the
//! whole section; `edit` records any payment. A project manager sees the
//! money of their own project and records its expenses.

use std::collections::BTreeMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    money,
    projects::{project_role, ProjectRole},
    state::AppState,
};

const DEFAULT_CATEGORIES: &[(&str, &str)] = &[
    ("income", "Оплата по договору"),
    ("income", "Прочие поступления"),
    ("expense", "Зарплата"),
    ("expense", "Субподряд"),
    ("expense", "Лаборатория"),
    ("expense", "Командировки"),
    ("expense", "Офис"),
    ("expense", "Налоги"),
    ("expense", "Прочее"),
];

fn require_module(user: &CurrentUser) -> ApiResult<()> {
    if user.module_on("finance") && !user.is_guest() { Ok(()) } else { Err(ApiError::Forbidden) }
}
fn require_view(user: &CurrentUser) -> ApiResult<()> {
    if user.finance_view() { Ok(()) } else { Err(ApiError::Forbidden) }
}

/// Is the user a manager of the project (administrators are)?
async fn manages(state: &AppState, user: &CurrentUser, project_id: Uuid) -> ApiResult<bool> {
    match project_role(state, user, project_id).await {
        Ok(r) => Ok(r == ProjectRole::Manager),
        Err(ApiError::NotFound) => Ok(false),
        Err(e) => Err(e),
    }
}

/// May the user create or keep a payment with these properties?
async fn may_write(state: &AppState, user: &CurrentUser, kind: &str, project_id: Option<Uuid>) -> ApiResult<bool> {
    if user.finance_edit() {
        return Ok(true);
    }
    match project_id {
        Some(p) if kind == "expense" && user.module_on("finance") && !user.is_guest() => manages(state, user, p).await,
        _ => Ok(false),
    }
}

async fn invoice_due_days(state: &AppState, tenant_id: Uuid) -> ApiResult<i32> {
    Ok(sqlx::query_scalar("SELECT invoice_due_days FROM tenant_requisites WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or(10))
}

// ---------------------------------------------------------------------------
// Settings and categories
// ---------------------------------------------------------------------------

pub async fn get_settings(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    Ok(Json(json!({
        "invoice_due_days": invoice_due_days(&state, user.tenant_id).await?,
        "vat_rates": money::VAT_RATES,
        "can_view": user.finance_view(),
        "can_edit": user.finance_edit(),
    })))
}

#[derive(Deserialize)]
pub struct SettingsInput {
    pub invoice_due_days: i32,
}

pub async fn save_settings(State(state): State<AppState>, user: CurrentUser, Json(req): Json<SettingsInput>) -> ApiResult<Json<Value>> {
    if !user.finance_edit() {
        return Err(ApiError::Forbidden);
    }
    if !(0..=365).contains(&req.invoice_due_days) {
        return bad_request("срок оплаты счёта — от 0 до 365 дней");
    }
    sqlx::query(
        "INSERT INTO tenant_requisites (tenant_id, invoice_due_days) VALUES ($1, $2)
         ON CONFLICT (tenant_id) DO UPDATE SET invoice_due_days = EXCLUDED.invoice_due_days",
    )
    .bind(user.tenant_id)
    .bind(req.invoice_due_days)
    .execute(&state.db)
    .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "finance.settings", Some("tenant"), Some(user.tenant_id), json!({ "invoice_due_days": req.invoice_due_days }))
        .await;
    get_settings(State(state), user).await
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Category {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub sort: i32,
    pub is_archived: bool,
}

/// Categories of the portal; the default set is created on first use.
async fn categories(state: &AppState, tenant_id: Uuid) -> ApiResult<Vec<Category>> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM finance_categories WHERE tenant_id = $1").bind(tenant_id).fetch_one(&state.db).await?;
    if n == 0 {
        for (i, (kind, name)) in DEFAULT_CATEGORIES.iter().enumerate() {
            sqlx::query("INSERT INTO finance_categories (tenant_id, kind, name, sort) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
                .bind(tenant_id)
                .bind(kind)
                .bind(name)
                .bind(i as i32)
                .execute(&state.db)
                .await?;
        }
    }
    Ok(sqlx::query_as("SELECT id, kind, name, sort, is_archived FROM finance_categories WHERE tenant_id = $1 ORDER BY kind, sort, lower(name)")
        .bind(tenant_id)
        .fetch_all(&state.db)
        .await?)
}

pub async fn list_categories(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    Ok(Json(json!({ "categories": categories(&state, user.tenant_id).await? })))
}

#[derive(Deserialize)]
pub struct CategoryInput {
    pub kind: Option<String>,
    pub name: Option<String>,
    pub is_archived: Option<bool>,
    pub sort: Option<i32>,
}

fn category_name(n: &str) -> ApiResult<String> {
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() || n.chars().count() > 100 {
        return bad_request("название статьи — от 1 до 100 символов");
    }
    Ok(n)
}

fn conflict_name(e: sqlx::Error) -> ApiError {
    match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("такая статья уже есть".into()),
        other => other,
    }
}

pub async fn create_category(State(state): State<AppState>, user: CurrentUser, Json(req): Json<CategoryInput>) -> ApiResult<Json<Value>> {
    if !user.finance_edit() {
        return Err(ApiError::Forbidden);
    }
    let kind = req.kind.unwrap_or_default();
    if !matches!(kind.as_str(), "income" | "expense") {
        return bad_request("kind must be income or expense");
    }
    let name = category_name(req.name.as_deref().unwrap_or(""))?;
    categories(&state, user.tenant_id).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO finance_categories (tenant_id, kind, name, sort)
         VALUES ($1, $2, $3, (SELECT COALESCE(max(sort), 0) + 1 FROM finance_categories WHERE tenant_id = $1 AND kind = $2)) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&kind)
    .bind(&name)
    .fetch_one(&state.db)
    .await
    .map_err(conflict_name)?;
    Ok(Json(json!({ "id": id })))
}

pub async fn update_category(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<CategoryInput>) -> ApiResult<Json<Value>> {
    if !user.finance_edit() {
        return Err(ApiError::Forbidden);
    }
    let name = req.name.as_deref().map(category_name).transpose()?;
    let n = sqlx::query(
        "UPDATE finance_categories SET name = COALESCE($3, name), is_archived = COALESCE($4, is_archived), sort = COALESCE($5, sort)
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(name)
    .bind(req.is_archived)
    .bind(req.sort)
    .execute(&state.db)
    .await
    .map_err(conflict_name)?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Payments
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct PaymentRow {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub pay_date: NaiveDate,
    pub amount: f64,
    pub vat_rate: String,
    pub vat_amount: f64,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub company_id: Option<Uuid>,
    pub company_name: String,
    pub invoice_id: Option<Uuid>,
    pub invoice_number: Option<String>,
    pub description: String,
    pub created_by: Option<Uuid>,
    pub created_by_name: Option<String>,
}

const PAYMENT_SELECT: &str = "SELECT p.id, p.kind, p.status, p.pay_date, p.amount::float8 AS amount, p.vat_rate, p.vat_amount::float8 AS vat_amount,
        p.category_id, c.name AS category_name, p.project_id, pr.name AS project_name, p.company_id,
        COALESCE(co.name, p.company_name) AS company_name, p.invoice_id, d.number AS invoice_number, p.description,
        p.created_by, u.display_name AS created_by_name
   FROM payments p
   LEFT JOIN finance_categories c ON c.id = p.category_id
   LEFT JOIN projects pr ON pr.id = p.project_id
   LEFT JOIN crm_companies co ON co.id = p.company_id
   LEFT JOIN crm_documents d ON d.id = p.invoice_id
   LEFT JOIN users u ON u.id = p.created_by";

#[derive(Deserialize)]
pub struct PaymentQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub category_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub company_id: Option<Uuid>,
    pub q: Option<String>,
}

async fn fetch_payments(state: &AppState, tenant_id: Uuid, q: &PaymentQuery) -> ApiResult<Vec<PaymentRow>> {
    let sql = format!(
        "{PAYMENT_SELECT}
          WHERE p.tenant_id = $1 AND ($2::date IS NULL OR p.pay_date >= $2) AND ($3::date IS NULL OR p.pay_date <= $3)
            AND ($4::text IS NULL OR p.kind = $4) AND ($5::text IS NULL OR p.status = $5)
            AND ($6::uuid IS NULL OR p.category_id = $6) AND ($7::uuid IS NULL OR p.project_id = $7)
            AND ($8::uuid IS NULL OR p.company_id = $8)
            AND ($9::text IS NULL OR p.description ILIKE '%' || $9 || '%' OR COALESCE(co.name, p.company_name) ILIKE '%' || $9 || '%'
                 OR pr.name ILIKE '%' || $9 || '%' OR d.number ILIKE '%' || $9 || '%')
          ORDER BY p.pay_date DESC, p.created_at DESC
          LIMIT 5000"
    );
    Ok(sqlx::query_as::<_, PaymentRow>(&sql)
        .bind(tenant_id)
        .bind(q.from)
        .bind(q.to)
        .bind(q.kind.as_deref().filter(|s| !s.is_empty()))
        .bind(q.status.as_deref().filter(|s| !s.is_empty()))
        .bind(q.category_id)
        .bind(q.project_id)
        .bind(q.company_id)
        .bind(q.q.as_deref().map(|s| s.trim().replace('%', "\\%").replace('_', "\\_")).filter(|s| !s.is_empty()))
        .fetch_all(&state.db)
        .await?)
}

#[derive(Serialize, Default, Clone, Copy)]
pub struct Totals {
    pub income: f64,
    pub expense: f64,
    pub income_plan: f64,
    pub expense_plan: f64,
}

fn totals(rows: &[PaymentRow]) -> Totals {
    let mut t = Totals::default();
    for r in rows {
        match (r.kind.as_str(), r.status.as_str()) {
            ("income", "fact") => t.income += r.amount,
            ("expense", "fact") => t.expense += r.amount,
            ("income", _) => t.income_plan += r.amount,
            _ => t.expense_plan += r.amount,
        }
    }
    for v in [&mut t.income, &mut t.expense, &mut t.income_plan, &mut t.expense_plan] {
        *v = money::round2(*v);
    }
    t
}

pub async fn list_payments(State(state): State<AppState>, user: CurrentUser, Query(q): Query<PaymentQuery>) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    if !user.finance_view() {
        // A project manager sees the payments of their own project.
        match q.project_id {
            Some(p) if manages(&state, &user, p).await? => {}
            _ => return Err(ApiError::Forbidden),
        }
    }
    let rows = fetch_payments(&state, user.tenant_id, &q).await?;
    Ok(Json(json!({ "totals": totals(&rows), "payments": rows })))
}

#[derive(Deserialize)]
pub struct PaymentInput {
    pub kind: String,
    #[serde(default = "fact")]
    pub status: String,
    pub pay_date: NaiveDate,
    pub amount: f64,
    #[serde(default = "no_vat")]
    pub vat_rate: String,
    pub category_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub company_id: Option<Uuid>,
    #[serde(default)]
    pub company_name: String,
    pub invoice_id: Option<Uuid>,
    #[serde(default)]
    pub description: String,
}

fn fact() -> String {
    "fact".into()
}
fn no_vat() -> String {
    "none".into()
}

/// Checked and completed payment values.
struct Clean {
    kind: String,
    status: String,
    pay_date: NaiveDate,
    amount: f64,
    vat_rate: String,
    vat_amount: f64,
    category_id: Option<Uuid>,
    project_id: Option<Uuid>,
    company_id: Option<Uuid>,
    company_name: String,
    invoice_id: Option<Uuid>,
    description: String,
}

async fn clean(state: &AppState, user: &CurrentUser, r: PaymentInput) -> ApiResult<Clean> {
    if !matches!(r.kind.as_str(), "income" | "expense") {
        return bad_request("kind must be income or expense");
    }
    if !matches!(r.status.as_str(), "plan" | "fact") {
        return bad_request("status must be plan or fact");
    }
    let amount = money::round2(r.amount);
    if !(amount > 0.0 && amount < 1e13) || !amount.is_finite() {
        return bad_request("сумма должна быть больше нуля");
    }
    if !money::valid_vat(&r.vat_rate) {
        return bad_request("unknown VAT rate");
    }
    let year = r.pay_date.year();
    if !(2000..=2100).contains(&year) {
        return bad_request("неверная дата платежа");
    }
    let t = user.tenant_id;
    if let Some(c) = r.category_id {
        let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM finance_categories WHERE id = $1 AND tenant_id = $2")
            .bind(c)
            .bind(t)
            .fetch_optional(&state.db)
            .await?;
        if kind.as_deref() != Some(r.kind.as_str()) {
            return bad_request("статья не найдена");
        }
    }
    let mut project_id = r.project_id;
    if let Some(p) = project_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM projects WHERE id = $1 AND tenant_id = $2)")
            .bind(p)
            .bind(t)
            .fetch_one(&state.db)
            .await?;
        if !ok {
            return bad_request("проект не найден");
        }
    }
    let mut company_id = r.company_id;
    let mut company_name: String = r.company_name.trim().chars().take(500).collect();
    if let Some(inv) = r.invoice_id {
        if r.kind != "income" {
            return bad_request("счёт оплачивается поступлением");
        }
        let row: Option<(Option<Uuid>, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT company_id, company_name, deal_id FROM crm_documents WHERE id = $1 AND tenant_id = $2 AND kind = 'invoice'",
        )
        .bind(inv)
        .bind(t)
        .fetch_optional(&state.db)
        .await?;
        let Some((inv_company, inv_company_name, deal)) = row else {
            return bad_request("счёт не найден");
        };
        if company_id.is_none() {
            company_id = inv_company;
            company_name = inv_company_name;
        }
        if project_id.is_none() {
            if let Some(d) = deal {
                project_id = sqlx::query_scalar("SELECT id FROM projects WHERE tenant_id = $1 AND deal_id = $2 ORDER BY created_at LIMIT 1")
                    .bind(t)
                    .bind(d)
                    .fetch_optional(&state.db)
                    .await?;
            }
        }
    }
    if let Some(c) = company_id {
        let name: Option<String> = sqlx::query_scalar("SELECT name FROM crm_companies WHERE id = $1 AND tenant_id = $2")
            .bind(c)
            .bind(t)
            .fetch_optional(&state.db)
            .await?;
        company_name = name.ok_or_else(|| ApiError::BadRequest("контрагент не найден".into()))?;
    }
    Ok(Clean {
        vat_amount: money::vat_included(amount, &r.vat_rate),
        kind: r.kind,
        status: r.status,
        pay_date: r.pay_date,
        amount,
        vat_rate: r.vat_rate,
        category_id: r.category_id,
        project_id,
        company_id,
        company_name,
        invoice_id: r.invoice_id,
        description: r.description.trim().chars().take(2000).collect(),
    })
}

async fn load_payment(state: &AppState, tenant_id: Uuid, id: Uuid) -> ApiResult<PaymentRow> {
    sqlx::query_as::<_, PaymentRow>(&format!("{PAYMENT_SELECT} WHERE p.id = $1 AND p.tenant_id = $2"))
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn create_payment(State(state): State<AppState>, user: CurrentUser, Json(req): Json<PaymentInput>) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    let c = clean(&state, &user, req).await?;
    if !may_write(&state, &user, &c.kind, c.project_id).await? {
        return Err(ApiError::Forbidden);
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (tenant_id, kind, status, pay_date, amount, vat_rate, vat_amount, category_id, project_id, company_id,
                               company_name, invoice_id, description, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&c.kind)
    .bind(&c.status)
    .bind(c.pay_date)
    .bind(c.amount)
    .bind(&c.vat_rate)
    .bind(c.vat_amount)
    .bind(c.category_id)
    .bind(c.project_id)
    .bind(c.company_id)
    .bind(&c.company_name)
    .bind(c.invoice_id)
    .bind(&c.description)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "finance.payment_created", Some("payment"), Some(id),
               json!({ "kind": c.kind, "status": c.status, "amount": c.amount, "project": c.project_id }))
        .await;
    Ok(Json(json!({ "payment": load_payment(&state, user.tenant_id, id).await? })))
}

pub async fn update_payment(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<PaymentInput>) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    let old = load_payment(&state, user.tenant_id, id).await?;
    if !may_write(&state, &user, &old.kind, old.project_id).await? {
        return Err(ApiError::Forbidden);
    }
    let c = clean(&state, &user, req).await?;
    if !may_write(&state, &user, &c.kind, c.project_id).await? {
        return Err(ApiError::Forbidden);
    }
    sqlx::query(
        "UPDATE payments SET kind = $3, status = $4, pay_date = $5, amount = $6, vat_rate = $7, vat_amount = $8, category_id = $9,
                project_id = $10, company_id = $11, company_name = $12, invoice_id = $13, description = $14, updated_at = now()
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(&c.kind)
    .bind(&c.status)
    .bind(c.pay_date)
    .bind(c.amount)
    .bind(&c.vat_rate)
    .bind(c.vat_amount)
    .bind(c.category_id)
    .bind(c.project_id)
    .bind(c.company_id)
    .bind(&c.company_name)
    .bind(c.invoice_id)
    .bind(&c.description)
    .execute(&state.db)
    .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "finance.payment_updated", Some("payment"), Some(id),
               json!({ "amount": [old.amount, c.amount], "status": [old.status, c.status], "date": [old.pay_date, c.pay_date] }))
        .await;
    Ok(Json(json!({ "payment": load_payment(&state, user.tenant_id, id).await? })))
}

pub async fn delete_payment(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    let old = load_payment(&state, user.tenant_id, id).await?;
    if !may_write(&state, &user, &old.kind, old.project_id).await? {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("DELETE FROM payments WHERE id = $1").bind(id).execute(&state.db).await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "finance.payment_deleted", Some("payment"), Some(id),
               json!({ "kind": old.kind, "amount": old.amount, "date": old.pay_date, "project": old.project_name }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Analytics
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// `month` (default) or `week`.
    pub period: Option<String>,
}

/// Start of the bucket a date falls into.
pub fn bucket_start(d: NaiveDate, week: bool) -> NaiveDate {
    if week {
        d - Duration::days(d.weekday().num_days_from_monday() as i64)
    } else {
        d.with_day(1).unwrap_or(d)
    }
}

/// All bucket starts covering [from, to].
pub fn buckets(from: NaiveDate, to: NaiveDate, week: bool) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut d = bucket_start(from, week);
    while d <= to && out.len() < 400 {
        out.push(d);
        d = if week {
            d + Duration::days(7)
        } else if d.month() == 12 {
            NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap_or(d)
        } else {
            NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap_or(d)
        };
    }
    out
}

pub async fn summary(State(state): State<AppState>, user: CurrentUser, Query(q): Query<SummaryQuery>) -> ApiResult<Json<Value>> {
    require_view(&user)?;
    if q.to < q.from || (q.to - q.from).num_days() > 366 * 5 {
        return bad_request("неверный период");
    }
    let week = q.period.as_deref() == Some("week");
    let pq = PaymentQuery { from: Some(q.from), to: Some(q.to), kind: None, status: None, category_id: None, project_id: None, company_id: None, q: None };
    let rows = fetch_payments(&state, user.tenant_id, &pq).await?;
    let t = totals(&rows);

    // Series of actual money by period.
    let mut series: BTreeMap<NaiveDate, (f64, f64)> = buckets(q.from, q.to, week).into_iter().map(|d| (d, (0.0, 0.0))).collect();
    // By category and by project.
    let mut by_cat: BTreeMap<(String, String), f64> = BTreeMap::new();
    let mut by_project: BTreeMap<Uuid, (String, f64, f64)> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.status == "fact") {
        let e = series.entry(bucket_start(r.pay_date, week)).or_default();
        if r.kind == "income" { e.0 += r.amount } else { e.1 += r.amount }
        *by_cat.entry((r.kind.clone(), r.category_name.clone().unwrap_or_else(|| "Без статьи".into()))).or_default() += r.amount;
        if let (Some(pid), Some(name)) = (r.project_id, &r.project_name) {
            let p = by_project.entry(pid).or_insert((name.clone(), 0.0, 0.0));
            if r.kind == "income" { p.1 += r.amount } else { p.2 += r.amount }
        }
    }
    let series: Vec<Value> = series
        .into_iter()
        .map(|(d, (i, e))| json!({ "start": d, "income": money::round2(i), "expense": money::round2(e) }))
        .collect();
    let mut cats: Vec<Value> = by_cat
        .into_iter()
        .map(|((kind, name), sum)| json!({ "kind": kind, "name": name, "amount": money::round2(sum) }))
        .collect();
    cats.sort_by(|a, b| b["amount"].as_f64().partial_cmp(&a["amount"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    let mut projects: Vec<Value> = by_project
        .into_iter()
        .map(|(id, (name, i, e))| json!({ "id": id, "name": name, "income": money::round2(i), "expense": money::round2(e), "profit": money::round2(i - e) }))
        .collect();
    projects.sort_by(|a, b| b["profit"].as_f64().partial_cmp(&a["profit"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));

    // Planned income: overdue (before today) and expected in the next 30 days, regardless of the period.
    let today = Utc::now().date_naive();
    let (overdue, upcoming): (f64, f64) = sqlx::query_as(
        "SELECT COALESCE(sum(amount) FILTER (WHERE pay_date < $2), 0)::float8,
                COALESCE(sum(amount) FILTER (WHERE pay_date >= $2 AND pay_date <= $2 + 30), 0)::float8
           FROM payments WHERE tenant_id = $1 AND kind = 'income' AND status = 'plan'",
    )
    .bind(user.tenant_id)
    .bind(today)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({
        "totals": { "income": t.income, "expense": t.expense, "profit": money::round2(t.income - t.expense),
                    "income_plan": t.income_plan, "expense_plan": t.expense_plan },
        "series": series, "categories": cats, "projects": projects,
        "plan": { "overdue": money::round2(overdue), "next_30_days": money::round2(upcoming) },
    })))
}

// ---------------------------------------------------------------------------
// Receivables: unpaid invoices and overdue planned income
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct InvoiceDebt {
    pub id: Uuid,
    pub number: String,
    pub doc_date: NaiveDate,
    pub company_id: Option<Uuid>,
    pub company_name: String,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub subject: String,
    pub amount: f64,
    pub paid: f64,
    pub due_date: NaiveDate,
}

/// Payment state of an invoice: paid | partial | overdue | issued.
pub fn invoice_state(amount: f64, paid: f64, due: NaiveDate, today: NaiveDate) -> &'static str {
    if paid + 0.005 >= amount {
        "paid"
    } else if due < today {
        "overdue"
    } else if paid > 0.0 {
        "partial"
    } else {
        "issued"
    }
}

/// Invoices (not cancelled) with what was paid; `$1` tenant, `$2` due days.
const INVOICE_SELECT: &str = "SELECT d.id, d.number, d.doc_date, d.company_id, COALESCE(co.name, d.company_name) AS company_name,
        pr.id AS project_id, pr.name AS project_name, d.subject, d.amount::float8 AS amount,
        COALESCE((SELECT sum(p.amount) FROM payments p WHERE p.invoice_id = d.id AND p.status = 'fact'), 0)::float8 AS paid,
        (d.doc_date + $2::int) AS due_date
   FROM crm_documents d
   LEFT JOIN crm_companies co ON co.id = d.company_id
   LEFT JOIN LATERAL (SELECT id, name FROM projects WHERE tenant_id = d.tenant_id AND deal_id = d.deal_id AND d.deal_id IS NOT NULL
                       ORDER BY created_at LIMIT 1) pr ON true
  WHERE d.tenant_id = $1 AND d.kind = 'invoice' AND d.status <> 'cancelled'";

fn invoice_json(i: &InvoiceDebt, today: NaiveDate) -> Value {
    let st = invoice_state(i.amount, i.paid, i.due_date, today);
    json!({
        "id": i.id, "number": i.number, "doc_date": i.doc_date, "company_id": i.company_id, "company_name": i.company_name,
        "project_id": i.project_id, "project_name": i.project_name, "subject": i.subject, "amount": i.amount, "paid": money::round2(i.paid),
        "debt": money::round2((i.amount - i.paid).max(0.0)), "due_date": i.due_date, "state": st,
        "days_overdue": if st == "overdue" { (today - i.due_date).num_days() } else { 0 },
    })
}

pub async fn receivables(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_view(&user)?;
    let today = Utc::now().date_naive();
    let days = invoice_due_days(&state, user.tenant_id).await?;
    let invoices: Vec<InvoiceDebt> = if user.module_on("crm") {
        sqlx::query_as(&format!("{INVOICE_SELECT} ORDER BY d.doc_date"))
            .bind(user.tenant_id)
            .bind(days)
            .fetch_all(&state.db)
            .await?
    } else {
        Vec::new()
    };
    let open: Vec<&InvoiceDebt> = invoices.iter().filter(|i| i.paid + 0.005 < i.amount).collect();
    // By counterparty.
    let mut by_company: BTreeMap<String, (Option<Uuid>, f64, f64, usize)> = BTreeMap::new();
    for i in &open {
        let debt = i.amount - i.paid;
        let e = by_company.entry(i.company_name.clone()).or_insert((i.company_id, 0.0, 0.0, 0));
        e.1 += debt;
        if i.due_date < today {
            e.2 += debt;
        }
        e.3 += 1;
    }
    let mut companies: Vec<Value> = by_company
        .into_iter()
        .map(|(name, (id, debt, overdue, n))| json!({ "company_id": id, "company_name": name, "debt": money::round2(debt), "overdue": money::round2(overdue), "invoices": n }))
        .collect();
    companies.sort_by(|a, b| b["debt"].as_f64().partial_cmp(&a["debt"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    let plan = fetch_payments(
        &state,
        user.tenant_id,
        &PaymentQuery { from: None, to: Some(today + Duration::days(30)), kind: Some("income".into()), status: Some("plan".into()),
                        category_id: None, project_id: None, company_id: None, q: None },
    )
    .await?;
    let debt: f64 = open.iter().map(|i| i.amount - i.paid).sum();
    let overdue: f64 = open.iter().filter(|i| i.due_date < today).map(|i| i.amount - i.paid).sum();
    let plan_overdue: f64 = plan.iter().filter(|p| p.pay_date < today).map(|p| p.amount).sum();
    Ok(Json(json!({
        "invoice_due_days": days,
        "totals": { "debt": money::round2(debt), "overdue": money::round2(overdue), "plan_overdue": money::round2(plan_overdue) },
        "invoices": open.iter().map(|i| invoice_json(i, today)).collect::<Vec<_>>(),
        "companies": companies,
        "plan": plan.iter().rev().collect::<Vec<_>>(),
        "crm": user.module_on("crm"),
    })))
}

/// Unpaid invoices for the "pays invoice" picker of the payment form.
pub async fn open_invoices(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    if !user.finance_edit() || !user.module_on("crm") {
        return Ok(Json(json!({ "invoices": [] })));
    }
    let today = Utc::now().date_naive();
    let days = invoice_due_days(&state, user.tenant_id).await?;
    let rows: Vec<InvoiceDebt> = sqlx::query_as(&format!("{INVOICE_SELECT} ORDER BY d.doc_date DESC LIMIT 500"))
        .bind(user.tenant_id)
        .bind(days)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({ "invoices": rows.iter().map(|i| invoice_json(i, today)).collect::<Vec<_>>() })))
}

// ---------------------------------------------------------------------------
// Money of a project
// ---------------------------------------------------------------------------

/// The requisite that holds the contract amount: key `contract_amount`, else
/// the first number field whose label looks like money.
async fn budget_field(state: &AppState, tenant_id: Uuid) -> ApiResult<Option<(Uuid, String)>> {
    let defs: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, key, label FROM project_field_defs WHERE tenant_id = $1 AND field_type = 'number' AND NOT is_archived ORDER BY sort_order, created_at",
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;
    let looks = |l: &str| {
        let l = l.to_lowercase();
        ["руб", "₽", "стоимост", "сумм", "цена"].iter().any(|w| l.contains(w))
    };
    Ok(defs
        .iter()
        .find(|(_, k, _)| k == "contract_amount")
        .or_else(|| defs.iter().find(|(_, _, l)| looks(l)))
        .map(|(id, _, l)| (*id, l.clone())))
}

/// Budget, paid and spent for projects (`$1` tenant, `$2` budget field or NULL, `$3` project ids or NULL for all).
const MONEY_SQL: &str = "SELECT p.id,
        COALESCE(
          (SELECT CASE WHEN jsonb_typeof(v.value) = 'number' THEN (v.value #>> '{}')::float8 END
             FROM project_field_values v WHERE v.project_id = p.id AND v.field_id = $2),
          (SELECT NULLIF(dl.amount, 0)::float8 FROM crm_deals dl WHERE dl.id = p.deal_id)) AS budget,
        CASE WHEN EXISTS (SELECT 1 FROM project_field_values v WHERE v.project_id = p.id AND v.field_id = $2 AND jsonb_typeof(v.value) = 'number')
             THEN 'field' WHEN p.deal_id IS NOT NULL THEN 'deal' END AS budget_source,
        COALESCE(sum(pay.amount) FILTER (WHERE pay.kind = 'income' AND pay.status = 'fact'), 0)::float8 AS paid,
        COALESCE(sum(pay.amount) FILTER (WHERE pay.kind = 'income' AND pay.status = 'plan'), 0)::float8 AS income_plan,
        COALESCE(sum(pay.amount) FILTER (WHERE pay.kind = 'expense' AND pay.status = 'fact'), 0)::float8 AS spent,
        COALESCE(sum(pay.amount) FILTER (WHERE pay.kind = 'expense' AND pay.status = 'plan'), 0)::float8 AS expense_plan
   FROM projects p LEFT JOIN payments pay ON pay.project_id = p.id
  WHERE p.tenant_id = $1 AND ($3::uuid[] IS NULL OR p.id = ANY($3))
  GROUP BY p.id";

#[derive(sqlx::FromRow, Serialize)]
pub struct ProjectMoney {
    pub id: Uuid,
    pub budget: Option<f64>,
    pub budget_source: Option<String>,
    pub paid: f64,
    pub income_plan: f64,
    pub spent: f64,
    pub expense_plan: f64,
}

async fn project_money(state: &AppState, tenant_id: Uuid, ids: Option<Vec<Uuid>>) -> ApiResult<Vec<ProjectMoney>> {
    let field = budget_field(state, tenant_id).await?.map(|(id, _)| id);
    Ok(sqlx::query_as::<_, ProjectMoney>(MONEY_SQL)
        .bind(tenant_id)
        .bind(field)
        .bind(ids)
        .fetch_all(&state.db)
        .await?)
}

/// Money columns of the project list: all projects for finance users, the
/// managed ones for project managers, nothing for others.
pub async fn projects_money(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    let rows = if user.finance_view() {
        project_money(&state, user.tenant_id, None).await?
    } else {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT pm.project_id FROM project_members pm JOIN projects p ON p.id = pm.project_id
              WHERE pm.user_id = $1 AND pm.role = 'manager' AND p.tenant_id = $2",
        )
        .bind(user.user_id)
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?;
        if ids.is_empty() { Vec::new() } else { project_money(&state, user.tenant_id, Some(ids)).await? }
    };
    let list: Vec<Value> = rows
        .iter()
        .map(|m| json!({ "project_id": m.id, "budget": m.budget, "paid": money::round2(m.paid), "spent": money::round2(m.spent),
                         "due": m.budget.map(|b| money::round2((b - m.paid).max(0.0))) }))
        .collect();
    Ok(Json(json!({ "projects": list })))
}

pub async fn project_finance(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    require_module(&user)?;
    let is_manager = manages(&state, &user, id).await?;
    if !user.finance_view() && !is_manager {
        return Err(ApiError::Forbidden);
    }
    let m = project_money(&state, user.tenant_id, Some(vec![id])).await?.into_iter().next().ok_or(ApiError::NotFound)?;
    let field_label = budget_field(&state, user.tenant_id).await?.map(|(_, l)| l);
    let pq = PaymentQuery { from: None, to: None, kind: None, status: None, category_id: None, project_id: Some(id), company_id: None, q: None };
    let payments = fetch_payments(&state, user.tenant_id, &pq).await?;
    let today = Utc::now().date_naive();
    let invoices: Vec<Value> = if user.module_on("crm") {
        let days = invoice_due_days(&state, user.tenant_id).await?;
        let rows: Vec<InvoiceDebt> = sqlx::query_as(&format!("{INVOICE_SELECT} AND pr.id = $3 ORDER BY d.doc_date"))
            .bind(user.tenant_id)
            .bind(days)
            .bind(id)
            .fetch_all(&state.db)
            .await?;
        rows.iter().map(|i| invoice_json(i, today)).collect()
    } else {
        Vec::new()
    };
    let plan_overdue: f64 = payments.iter().filter(|p| p.kind == "income" && p.status == "plan" && p.pay_date < today).map(|p| p.amount).sum();
    Ok(Json(json!({
        "budget": m.budget,
        "budget_source": m.budget_source,
        "budget_label": if m.budget_source.as_deref() == Some("field") { field_label } else if m.budget_source.is_some() { Some("Сумма сделки".to_string()) } else { None },
        "paid": money::round2(m.paid),
        "due": m.budget.map(|b| money::round2((b - m.paid).max(0.0))),
        "income_plan": money::round2(m.income_plan),
        "plan_overdue": money::round2(plan_overdue),
        "spent": money::round2(m.spent),
        "expense_plan": money::round2(m.expense_plan),
        "profit": m.budget.map(|b| money::round2(b - m.spent)),
        "cash": money::round2(m.paid - m.spent),
        "payments": payments,
        "invoices": invoices,
        "can_edit": user.finance_edit(),
        "can_add_expense": user.finance_edit() || is_manager,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn buckets_cover_the_period() {
        assert_eq!(buckets(d(2026, 11, 15), d(2027, 1, 3), false), vec![d(2026, 11, 1), d(2026, 12, 1), d(2027, 1, 1)]);
        // 11.11.2026 is a Wednesday: weeks start on Mondays.
        assert_eq!(buckets(d(2026, 11, 11), d(2026, 11, 23), true), vec![d(2026, 11, 9), d(2026, 11, 16), d(2026, 11, 23)]);
        assert_eq!(bucket_start(d(2026, 11, 15), true), d(2026, 11, 9));
    }

    #[test]
    fn invoice_states() {
        let today = d(2026, 11, 11);
        assert_eq!(invoice_state(100.0, 100.0, d(2026, 11, 1), today), "paid");
        assert_eq!(invoice_state(100.0, 99.999, d(2026, 11, 1), today), "paid");
        assert_eq!(invoice_state(100.0, 40.0, d(2026, 11, 1), today), "overdue");
        assert_eq!(invoice_state(100.0, 40.0, d(2026, 11, 20), today), "partial");
        assert_eq!(invoice_state(100.0, 0.0, d(2026, 11, 11), today), "issued");
    }
}
