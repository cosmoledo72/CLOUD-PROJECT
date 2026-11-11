//! Sales documents: quotes, contracts, invoices and acts generated from
//! templates for a deal, with the organisation's own requisites and
//! automatic numbering. Files go to the company's CRM folder.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use bytes::Bytes;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{deals::load_deal, validate, visible_company};
use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    files::{create_file, validate_name},
    money,
    ooxml::{self, Kind},
    state::AppState,
    storage::BlobStore,
    templates::{format_number, free_name, load_template_scoped, values_for, PlaceholderInfo, DOC_KINDS, MONTHS_GEN},
};

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "quote" => "Коммерческое предложение",
        "contract" => "Договор",
        "invoice" => "Счёт",
        "act" => "Акт",
        _ => "Документ",
    }
}

fn default_pattern(kind: &str) -> &'static str {
    match kind {
        "quote" => "КП-{n}/{yy}",
        "contract" => "{n}-{yy}",
        _ => "{n}",
    }
}

fn date_long(d: NaiveDate) -> String {
    format!("{} {} {} г.", d.day(), MONTHS_GEN[d.month0() as usize], d.year())
}

// ---------------------------------------------------------------------------
// Organisation requisites (admin)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, sqlx::FromRow, Default, Clone)]
pub struct OrgRequisites {
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub inn: String,
    #[serde(default)]
    pub kpp: String,
    #[serde(default)]
    pub ogrn: String,
    #[serde(default)]
    pub legal_address: String,
    #[serde(default)]
    pub postal_address: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub director: String,
    #[serde(default)]
    pub director_title: String,
    #[serde(default)]
    pub accountant: String,
    #[serde(default)]
    pub bank_name: String,
    #[serde(default)]
    pub bik: String,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub corr_account: String,
    #[serde(default)]
    pub vat_rate: String,
}

const ORG_SELECT: &str = "SELECT full_name, inn, kpp, ogrn, legal_address, postal_address, phone, email, website, director,
        director_title, accountant, bank_name, bik, account, corr_account, vat_rate FROM tenant_requisites WHERE tenant_id = $1";

async fn org(state: &AppState, tenant_id: Uuid) -> ApiResult<OrgRequisites> {
    Ok(sqlx::query_as::<_, OrgRequisites>(ORG_SELECT)
        .bind(tenant_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or_else(|| OrgRequisites { director_title: "Генеральный директор".into(), vat_rate: "none".into(), ..Default::default() }))
}

pub async fn get_org(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    Ok(Json(json!({ "requisites": org(&state, user.tenant_id).await?, "vat_rates": money::VAT_RATES })))
}

pub async fn save_org(State(state): State<AppState>, user: CurrentUser, Json(r): Json<OrgRequisites>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let t = |s: &str| s.trim().to_string();
    let digits = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let (inn, kpp, ogrn, bik, account, corr) =
        (digits(&r.inn), digits(&r.kpp), digits(&r.ogrn), digits(&r.bik), digits(&r.account), digits(&r.corr_account));
    if !inn.is_empty() && !validate::inn_valid(&inn) {
        return bad_request("ИНН организации: неверное контрольное число");
    }
    if !kpp.is_empty() && !validate::kpp_valid(&kpp) {
        return bad_request("КПП организации: 9 знаков");
    }
    if !ogrn.is_empty() && !validate::ogrn_valid(&ogrn) {
        return bad_request("ОГРН организации: неверное контрольное число");
    }
    if !bik.is_empty() && !validate::bik_valid(&bik) {
        return bad_request("БИК: 9 цифр");
    }
    if !account.is_empty() && (bik.is_empty() || !validate::account_valid(&bik, &account, false)) {
        return bad_request("Расчётный счёт не соответствует БИК");
    }
    if !corr.is_empty() && (bik.is_empty() || !validate::account_valid(&bik, &corr, true)) {
        return bad_request("Корреспондентский счёт не соответствует БИК");
    }
    let vat = if r.vat_rate.is_empty() { "none".to_string() } else { r.vat_rate.clone() };
    if !money::valid_vat(&vat) {
        return bad_request("unknown VAT rate");
    }
    let title = if r.director_title.trim().is_empty() { "Генеральный директор".to_string() } else { t(&r.director_title) };
    sqlx::query(
        "INSERT INTO tenant_requisites (tenant_id, full_name, inn, kpp, ogrn, legal_address, postal_address, phone, email, website,
                                        director, director_title, accountant, bank_name, bik, account, corr_account, vat_rate, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, now())
         ON CONFLICT (tenant_id) DO UPDATE SET full_name = EXCLUDED.full_name, inn = EXCLUDED.inn, kpp = EXCLUDED.kpp,
             ogrn = EXCLUDED.ogrn, legal_address = EXCLUDED.legal_address, postal_address = EXCLUDED.postal_address,
             phone = EXCLUDED.phone, email = EXCLUDED.email, website = EXCLUDED.website, director = EXCLUDED.director,
             director_title = EXCLUDED.director_title, accountant = EXCLUDED.accountant, bank_name = EXCLUDED.bank_name,
             bik = EXCLUDED.bik, account = EXCLUDED.account, corr_account = EXCLUDED.corr_account, vat_rate = EXCLUDED.vat_rate,
             updated_at = now()",
    )
    .bind(user.tenant_id)
    .bind(t(&r.full_name))
    .bind(&inn)
    .bind(&kpp)
    .bind(&ogrn)
    .bind(t(&r.legal_address))
    .bind(t(&r.postal_address))
    .bind(t(&r.phone))
    .bind(t(&r.email))
    .bind(t(&r.website))
    .bind(t(&r.director))
    .bind(&title)
    .bind(t(&r.accountant))
    .bind(t(&r.bank_name))
    .bind(&bik)
    .bind(&account)
    .bind(&corr)
    .bind(&vat)
    .execute(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "org.requisites_updated", None, None, json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Numbering
// ---------------------------------------------------------------------------

/// Expand a number pattern: `{n}`, `{n:3}` (zero-padded), `{yyyy}`, `{yy}`, `{mm}`.
pub fn format_doc_number(pattern: &str, n: i32, date: NaiveDate) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &rest[start + 1..start + end];
        match token {
            "n" => out.push_str(&n.to_string()),
            "yyyy" => out.push_str(&date.year().to_string()),
            "yy" => out.push_str(&format!("{:02}", date.year() % 100)),
            "mm" => out.push_str(&format!("{:02}", date.month())),
            t if t.starts_with("n:") => {
                let w: usize = t[2..].parse().unwrap_or(1).min(9);
                out.push_str(&format!("{n:0w$}"));
            }
            other => {
                out.push('{');
                out.push_str(other);
                out.push('}');
            }
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

async fn pattern_for(state: &AppState, tenant_id: Uuid, kind: &str) -> ApiResult<String> {
    let p: Option<String> = sqlx::query_scalar("SELECT pattern FROM crm_doc_numbering WHERE tenant_id = $1 AND kind = $2")
        .bind(tenant_id)
        .bind(kind)
        .fetch_optional(&state.db)
        .await?;
    Ok(p.unwrap_or_else(|| default_pattern(kind).to_string()))
}

async fn last_number(state: &AppState, tenant_id: Uuid, kind: &str, year: i32) -> ApiResult<i32> {
    Ok(sqlx::query_scalar("SELECT last_number FROM crm_doc_counters WHERE tenant_id = $1 AND kind = $2 AND year = $3")
        .bind(tenant_id)
        .bind(kind)
        .bind(year)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or(0))
}

#[derive(Deserialize)]
pub struct NextQuery {
    pub kind: String,
    pub date: Option<NaiveDate>,
}

/// The number the next document of a kind would get (without reserving it).
pub async fn next_number(State(state): State<AppState>, user: CurrentUser, Query(q): Query<NextQuery>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    if !DOC_KINDS.contains(&q.kind.as_str()) {
        return bad_request("unknown document kind");
    }
    let date = match q.date { Some(d) => d, None => crate::crm::tasks::portal_today(&state, user.tenant_id).await? };
    let n = last_number(&state, user.tenant_id, &q.kind, date.year()).await? + 1;
    let pattern = pattern_for(&state, user.tenant_id, &q.kind).await?;
    Ok(Json(json!({ "number": format_doc_number(&pattern, n, date) })))
}

pub async fn get_numbering(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let today = crate::crm::tasks::portal_today(&state, user.tenant_id).await?;
    let mut out = vec![];
    for kind in DOC_KINDS {
        let pattern = pattern_for(&state, user.tenant_id, kind).await?;
        let next = last_number(&state, user.tenant_id, kind, today.year()).await? + 1;
        out.push(json!({ "kind": kind, "label": kind_label(kind), "pattern": pattern, "next": next,
                         "example": format_doc_number(&pattern, next, today), "year": today.year() }));
    }
    Ok(Json(json!({ "numbering": out })))
}

#[derive(Deserialize)]
pub struct NumberingInput {
    pub kind: String,
    pub pattern: String,
    /// Next number in the current year (e.g. to continue numbering from accounting).
    pub next: Option<i32>,
}

pub async fn save_numbering(State(state): State<AppState>, user: CurrentUser, Json(req): Json<Vec<NumberingInput>>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let year = crate::crm::tasks::portal_today(&state, user.tenant_id).await?.year();
    let mut tx = state.db.begin().await?;
    for r in &req {
        if !DOC_KINDS.contains(&r.kind.as_str()) {
            return bad_request("unknown document kind");
        }
        let p = r.pattern.trim();
        if p.is_empty() || p.chars().count() > 60 || !p.contains("{n") {
            return bad_request("формат номера должен содержать {n} и быть не длиннее 60 символов");
        }
        sqlx::query(
            "INSERT INTO crm_doc_numbering (tenant_id, kind, pattern) VALUES ($1, $2, $3)
             ON CONFLICT (tenant_id, kind) DO UPDATE SET pattern = EXCLUDED.pattern",
        )
        .bind(user.tenant_id)
        .bind(&r.kind)
        .bind(p)
        .execute(&mut *tx)
        .await?;
        if let Some(next) = r.next {
            if !(1..=999_999).contains(&next) {
                return bad_request("следующий номер — от 1 до 999999");
            }
            sqlx::query(
                "INSERT INTO crm_doc_counters (tenant_id, kind, year, last_number) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (tenant_id, kind, year) DO UPDATE SET last_number = EXCLUDED.last_number",
            )
            .bind(user.tenant_id)
            .bind(&r.kind)
            .bind(year)
            .bind(next - 1)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "crm.numbering_updated", None, None, json!({})).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Company folders
// ---------------------------------------------------------------------------

/// The company's document folder, created on first use. It is a root of
/// kind `crm`: access follows CRM visibility of the company (see `acl`).
pub async fn company_folder(state: &AppState, tenant_id: Uuid, company_id: Uuid) -> ApiResult<Uuid> {
    let (folder, name): (Option<Uuid>, String) = sqlx::query_as("SELECT folder_id, name FROM crm_companies WHERE id = $1 AND tenant_id = $2")
        .bind(company_id)
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await?;
    if let Some(f) = folder {
        let alive: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM folders WHERE id = $1 AND deleted_at IS NULL)")
            .bind(f)
            .fetch_one(&state.db)
            .await?;
        if alive {
            return Ok(f);
        }
    }
    // Owned by the portal owner, so that no individual employee keeps access
    // after the company becomes hidden from them.
    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND role = 'owner' ORDER BY created_at LIMIT 1")
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await?;
    let folder_name = validate_name(&name.replace(['/', '\\'], "-")).unwrap_or_else(|_| "Контрагент".into());
    let id: Uuid = sqlx::query_scalar("INSERT INTO folders (tenant_id, kind, name, owner_id) VALUES ($1, 'crm', $2, $3) RETURNING id")
        .bind(tenant_id)
        .bind(&folder_name)
        .bind(owner)
        .fetch_one(&state.db)
        .await?;
    sqlx::query("UPDATE crm_companies SET folder_id = $2 WHERE id = $1").bind(company_id).bind(id).execute(&state.db).await?;
    Ok(id)
}

/// Open (create if needed) the company's document folder.
pub async fn open_company_folder(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    visible_company(&state, &user, id).await?;
    Ok(Json(json!({ "folder_id": company_folder(&state, user.tenant_id, id).await? })))
}

// ---------------------------------------------------------------------------
// Placeholders
// ---------------------------------------------------------------------------

pub fn sales_catalogue() -> Vec<PlaceholderInfo> {
    let p = |key: &str, label: &str, group: &str| PlaceholderInfo { key: key.into(), label: label.into(), group: group.into() };
    let mut out = vec![
        p("doc.number", "Номер документа", "Документ"),
        p("doc.date", "Дата документа (10.11.2026)", "Документ"),
        p("doc.date_long", "Дата документа прописью", "Документ"),
        p("doc.subject", "Предмет: наименование работ", "Документ"),
        p("doc.amount", "Сумма (1 250 000,00)", "Документ"),
        p("doc.amount_words", "Сумма прописью", "Документ"),
        p("doc.amount_without_vat", "Сумма без НДС", "Документ"),
        p("doc.vat_rate", "Ставка НДС (22% / без НДС)", "Документ"),
        p("doc.vat_amount", "Сумма НДС", "Документ"),
        p("doc.vat_text", "Строка про НДС («В том числе НДС…» / «НДС не облагается»)", "Документ"),
        p("doc.basis", "Основание: «Договор № … от …»", "Документ"),
        p("doc.basis_number", "Номер договора-основания", "Документ"),
        p("doc.basis_date", "Дата договора-основания", "Документ"),
        p("doc.valid_until", "Действует до (дата + 30 дней)", "Документ"),
        p("deal.title", "Название сделки", "Сделка"),
        p("deal.amount", "Сумма сделки", "Сделка"),
        p("deal.expected_close", "Ожидаемая дата закрытия", "Сделка"),
        p("deal.responsible", "Ответственный по сделке", "Сделка"),
        p("deal.responsible_email", "Email ответственного", "Сделка"),
        p("contact.name", "Контактное лицо: ФИО", "Контактное лицо"),
        p("contact.position", "Должность", "Контактное лицо"),
        p("contact.phone", "Телефон", "Контактное лицо"),
        p("contact.email", "Email", "Контактное лицо"),
    ];
    let party = [
        ("name", "Наименование (краткое)"),
        ("full_name", "Полное наименование"),
        ("inn", "ИНН"),
        ("kpp", "КПП"),
        ("ogrn", "ОГРН"),
        ("legal_address", "Юридический адрес"),
        ("postal_address", "Почтовый адрес"),
        ("phone", "Телефон"),
        ("email", "Email"),
        ("director", "Руководитель"),
        ("director_title", "Должность руководителя"),
        ("bank_name", "Банк"),
        ("bik", "БИК"),
        ("account", "Расчётный счёт"),
        ("corr_account", "Корреспондентский счёт"),
    ];
    for (k, l) in party {
        out.push(p(&format!("client.{k}"), l, "Контрагент (заказчик)"));
    }
    for (k, l) in party {
        out.push(p(&format!("org.{k}"), l, "Наша организация"));
    }
    out.push(p("org.website", "Сайт", "Наша организация"));
    out.push(p("org.accountant", "Главный бухгалтер", "Наша организация"));
    out
}

#[derive(sqlx::FromRow)]
struct ClientRow {
    name: String,
    full_name: String,
    inn: Option<String>,
    kpp: Option<String>,
    ogrn: Option<String>,
    legal_address: String,
    postal_address: String,
    phone: String,
    email: String,
    director: String,
    bank_name: String,
    bik: Option<String>,
    account: Option<String>,
    corr_account: Option<String>,
}

struct DocInput<'a> {
    number: &'a str,
    date: NaiveDate,
    subject: &'a str,
    amount: f64,
    vat_rate: &'a str,
    basis: Option<(String, NaiveDate)>,
}

#[allow(clippy::too_many_arguments)]
async fn sales_values(
    state: &AppState,
    user: &CurrentUser,
    deal: &super::deals::DealRow,
    company_id: Uuid,
    doc: &DocInput<'_>,
) -> ApiResult<HashMap<String, String>> {
    // Project requisites and the common placeholders (date, user, portal).
    let mut map = match deal.project_id {
        Some(p) => values_for(state, user, p, doc.date).await.unwrap_or_default(),
        None => HashMap::new(),
    };
    let tenant_name: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(user.tenant_id).fetch_one(&state.db).await?;
    map.insert("portal.name".into(), tenant_name.clone());
    map.insert("date".into(), doc.date.format("%d.%m.%Y").to_string());
    map.insert("date_long".into(), date_long(doc.date));
    map.insert("year".into(), doc.date.year().to_string());
    map.insert("user.name".into(), user.display_name.clone());
    map.insert("user.email".into(), user.email.clone());

    let money_fmt = |v: f64| {
        let s = format_number(v);
        // Amounts in documents always show kopecks.
        match s.split_once(',') {
            Some((i, f)) => format!("{i},{f:0<2}"),
            None => format!("{s},00"),
        }
    };
    let vat = money::vat_included(doc.amount, doc.vat_rate);
    map.insert("doc.number".into(), doc.number.into());
    map.insert("doc.date".into(), doc.date.format("%d.%m.%Y").to_string());
    map.insert("doc.date_long".into(), date_long(doc.date));
    map.insert("doc.subject".into(), doc.subject.into());
    map.insert("doc.amount".into(), money_fmt(doc.amount));
    map.insert("doc.amount_words".into(), money::rubles_in_words(doc.amount));
    map.insert("doc.amount_without_vat".into(), money_fmt(money::round2(doc.amount - vat)));
    map.insert("doc.vat_rate".into(), if doc.vat_rate == "none" { "без НДС".into() } else { format!("{}%", doc.vat_rate) });
    map.insert("doc.vat_amount".into(), if doc.vat_rate == "none" { "—".into() } else { money_fmt(vat) });
    map.insert("doc.vat_text".into(), money::vat_text(doc.vat_rate, vat, money_fmt));
    map.insert("doc.valid_until".into(), (doc.date + Duration::days(30)).format("%d.%m.%Y").to_string());
    match &doc.basis {
        Some((n, d)) => {
            map.insert("doc.basis".into(), format!("Договор № {n} от {}", d.format("%d.%m.%Y")));
            map.insert("doc.basis_number".into(), n.clone());
            map.insert("doc.basis_date".into(), d.format("%d.%m.%Y").to_string());
        }
        None => {
            map.insert("doc.basis".into(), "Основной договор".into());
            map.insert("doc.basis_number".into(), String::new());
            map.insert("doc.basis_date".into(), String::new());
        }
    }

    map.insert("deal.title".into(), deal.title.clone());
    map.insert("deal.amount".into(), money_fmt(deal.amount));
    map.insert("deal.expected_close".into(), deal.expected_close.map(|d| d.format("%d.%m.%Y").to_string()).unwrap_or_default());
    let resp: Option<(String, String)> = match deal.responsible_id {
        Some(r) => sqlx::query_as("SELECT display_name, email::text FROM users WHERE id = $1").bind(r).fetch_optional(&state.db).await?,
        None => None,
    };
    map.insert("deal.responsible".into(), resp.as_ref().map(|r| r.0.clone()).unwrap_or_default());
    map.insert("deal.responsible_email".into(), resp.map(|r| r.1).unwrap_or_default());

    let contact: Option<(String, String, String, String)> = match deal.contact_id {
        Some(c) => sqlx::query_as(
            "SELECT concat_ws(' ', last_name, first_name, middle_name), position, phone, email FROM crm_contacts WHERE id = $1",
        )
        .bind(c)
        .fetch_optional(&state.db)
        .await?,
        None => None,
    };
    let (cn, cp, cph, ce) = contact.unwrap_or_default();
    map.insert("contact.name".into(), cn);
    map.insert("contact.position".into(), cp);
    map.insert("contact.phone".into(), cph);
    map.insert("contact.email".into(), ce);

    let c = sqlx::query_as::<_, ClientRow>(
        "SELECT name, full_name, inn, kpp, ogrn, legal_address, postal_address, phone, email, director, bank_name, bik, account, corr_account
           FROM crm_companies WHERE id = $1",
    )
    .bind(company_id)
    .fetch_one(&state.db)
    .await?;
    let full = if c.full_name.trim().is_empty() { c.name.clone() } else { c.full_name.clone() };
    for (k, v) in [
        ("name", c.name),
        ("full_name", full),
        ("inn", c.inn.unwrap_or_default()),
        ("kpp", c.kpp.unwrap_or_default()),
        ("ogrn", c.ogrn.unwrap_or_default()),
        ("legal_address", c.legal_address),
        ("postal_address", c.postal_address),
        ("phone", c.phone),
        ("email", c.email),
        ("director", c.director),
        ("director_title", "Генеральный директор".into()),
        ("bank_name", c.bank_name),
        ("bik", c.bik.unwrap_or_default()),
        ("account", c.account.unwrap_or_default()),
        ("corr_account", c.corr_account.unwrap_or_default()),
    ] {
        map.insert(format!("client.{k}"), v);
    }

    let o = org(state, user.tenant_id).await?;
    let org_full = if o.full_name.trim().is_empty() { tenant_name.clone() } else { o.full_name.clone() };
    for (k, v) in [
        ("name", tenant_name.clone()),
        ("full_name", org_full),
        ("inn", o.inn),
        ("kpp", o.kpp),
        ("ogrn", o.ogrn),
        ("legal_address", o.legal_address),
        ("postal_address", o.postal_address),
        ("phone", o.phone),
        ("email", o.email),
        ("website", o.website),
        ("director", o.director),
        ("director_title", o.director_title),
        ("accountant", o.accountant),
        ("bank_name", o.bank_name),
        ("bik", o.bik),
        ("account", o.account),
        ("corr_account", o.corr_account),
    ] {
        map.insert(format!("org.{k}"), v);
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct DocRow {
    pub id: Uuid,
    pub kind: String,
    pub number: String,
    pub doc_date: NaiveDate,
    pub company_id: Option<Uuid>,
    pub company_name: String,
    pub deal_id: Option<Uuid>,
    pub deal_title: Option<String>,
    pub basis_id: Option<Uuid>,
    pub basis_number: Option<String>,
    pub subject: String,
    pub amount: f64,
    pub vat_rate: String,
    pub vat_amount: f64,
    pub status: String,
    pub file_id: Option<Uuid>,
    pub file_name: Option<String>,
    pub created_by_name: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Actual income recorded against this invoice (finance).
    pub paid_amount: f64,
}

/// `$1` user, `$2` manager flag, `$3` tenant.
fn doc_select(extra: &str) -> String {
    format!(
        "SELECT d.id, d.kind, d.number, d.doc_date, d.company_id, d.company_name, d.deal_id, dl.title AS deal_title,
                d.basis_id, b.number AS basis_number, d.subject, d.amount::float8 AS amount, d.vat_rate,
                d.vat_amount::float8 AS vat_amount, d.status, d.file_id, f.name AS file_name,
                u.display_name AS created_by_name, d.created_at,
                COALESCE((SELECT sum(p.amount) FROM payments p WHERE p.invoice_id = d.id AND p.status = 'fact'), 0)::float8 AS paid_amount
           FROM crm_documents d
           LEFT JOIN crm_companies c ON c.id = d.company_id
           LEFT JOIN crm_deals dl ON dl.id = d.deal_id
           LEFT JOIN crm_documents b ON b.id = d.basis_id
           LEFT JOIN files f ON f.id = d.file_id AND f.deleted_at IS NULL
           LEFT JOIN users u ON u.id = d.created_by
          WHERE d.tenant_id = $3
            AND (d.company_id IS NULL AND d.created_by = $1 OR {vis})
            {extra}
          ORDER BY d.doc_date DESC, d.created_at DESC",
        vis = super::company_visible_sql("c", 1, 2)
    )
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub deal_id: Option<Uuid>,
    pub company_id: Option<Uuid>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub q: Option<String>,
}

pub async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let sql = doc_select(
        "AND ($4::uuid IS NULL OR d.deal_id = $4) AND ($5::uuid IS NULL OR d.company_id = $5) AND ($6::text IS NULL OR d.kind = $6)
         AND ($7::text IS NULL OR d.status = $7) AND ($8::date IS NULL OR d.doc_date >= $8) AND ($9::date IS NULL OR d.doc_date <= $9)
         AND ($10::text IS NULL OR d.number ILIKE '%' || $10 || '%' OR d.company_name ILIKE '%' || $10 || '%' OR d.subject ILIKE '%' || $10 || '%')",
    ) + " LIMIT 2000";
    let rows = sqlx::query_as::<_, DocRow>(&sql)
    .bind(user.user_id)
    .bind(user.crm_manager())
    .bind(user.tenant_id)
    .bind(q.deal_id)
    .bind(q.company_id)
    .bind(q.kind.filter(|s| !s.is_empty()))
    .bind(q.status.filter(|s| !s.is_empty()))
    .bind(q.from)
    .bind(q.to)
    .bind(q.q.map(|s| s.trim().replace('%', "\\%").replace('_', "\\_")).filter(|s| !s.is_empty()))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "documents": rows })))
}

async fn load_doc(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<DocRow> {
    user.require_crm()?;
    sqlx::query_as::<_, DocRow>(&doc_select("AND d.id = $4"))
        .bind(user.user_id)
        .bind(user.crm_manager())
        .bind(user.tenant_id)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Defaults of the "new document" dialog for a deal.
pub async fn defaults(State(state): State<AppState>, user: CurrentUser, Path(deal_id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let deal = load_deal(&state, &user, deal_id).await?;
    let o = org(&state, user.tenant_id).await?;
    let contracts = sqlx::query_as::<_, (Uuid, String, NaiveDate)>(
        "SELECT id, number, doc_date FROM crm_documents WHERE deal_id = $1 AND kind = 'contract' AND status <> 'cancelled' ORDER BY doc_date DESC",
    )
    .bind(deal_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(id, n, d)| json!({ "id": id, "number": n, "date": d }))
    .collect::<Vec<_>>();
    let org_filled = !o.inn.is_empty() && !o.account.is_empty();
    Ok(Json(json!({
        "subject": deal.title, "amount": deal.amount, "vat_rate": o.vat_rate, "contracts": contracts,
        "has_company": deal.company_id.is_some(), "org_filled": org_filled,
    })))
}

#[derive(Deserialize)]
pub struct NewDocument {
    pub kind: String,
    pub template_id: Uuid,
    pub date: Option<NaiveDate>,
    /// Manual number; the next automatic one when absent.
    pub number: Option<String>,
    pub subject: Option<String>,
    pub amount: Option<f64>,
    pub vat_rate: Option<String>,
    pub basis_id: Option<Uuid>,
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(deal_id): Path<Uuid>,
    Json(req): Json<NewDocument>,
) -> ApiResult<Json<Value>> {
    let deal = load_deal(&state, &user, deal_id).await?;
    if !super::can_edit(&user, deal.responsible_id, deal.created_by) {
        return Err(ApiError::Forbidden);
    }
    let company_id = deal.company_id.ok_or_else(|| ApiError::BadRequest("укажите в сделке контрагента — документ оформляется на него".into()))?;
    if !DOC_KINDS.contains(&req.kind.as_str()) {
        return bad_request("unknown document kind");
    }
    let (tpl_name, ext, key, _) = load_template_scoped(&state, user.tenant_id, req.template_id, "sales").await?;
    let tpl_kind: Option<String> = sqlx::query_scalar("SELECT doc_kind FROM doc_templates WHERE id = $1").bind(req.template_id).fetch_one(&state.db).await?;
    if tpl_kind.as_deref() != Some(req.kind.as_str()) {
        return bad_request("шаблон предназначен для другого вида документа");
    }
    let o = org(&state, user.tenant_id).await?;
    let vat_rate = req.vat_rate.clone().unwrap_or(o.vat_rate.clone());
    if !money::valid_vat(&vat_rate) {
        return bad_request("unknown VAT rate");
    }
    let amount = money::round2(req.amount.unwrap_or(deal.amount));
    if !amount.is_finite() || !(0.0..1e13).contains(&amount) {
        return bad_request("сумма должна быть от 0");
    }
    let subject = req.subject.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&deal.title).to_string();
    if subject.chars().count() > 1000 {
        return bad_request("слишком длинный предмет");
    }
    let date = match req.date { Some(d) => d, None => crate::crm::tasks::portal_today(&state, user.tenant_id).await? };
    let basis = match req.basis_id {
        Some(b) => {
            let row: Option<(String, NaiveDate)> = sqlx::query_as(
                "SELECT number, doc_date FROM crm_documents WHERE id = $1 AND deal_id = $2 AND kind = 'contract' AND status <> 'cancelled'",
            )
            .bind(b)
            .bind(deal_id)
            .fetch_optional(&state.db)
            .await?;
            Some(row.ok_or_else(|| ApiError::BadRequest("основание — договор этой сделки".into()))?)
        }
        None => None,
    };
    let company_name: String = sqlx::query_scalar("SELECT name FROM crm_companies WHERE id = $1").bind(company_id).fetch_one(&state.db).await?;
    let folder_id = company_folder(&state, user.tenant_id, company_id).await?;

    // Reserve the number and record the document in one transaction; the
    // counter row lock serialises concurrent generation.
    let mut tx = state.db.begin().await?;
    let number = match req.number.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => {
            if n.chars().count() > 60 {
                return bad_request("номер — до 60 символов");
            }
            n.to_string()
        }
        None => {
            let n: i32 = sqlx::query_scalar(
                "INSERT INTO crm_doc_counters (tenant_id, kind, year, last_number) VALUES ($1, $2, $3, 1)
                 ON CONFLICT (tenant_id, kind, year) DO UPDATE SET last_number = crm_doc_counters.last_number + 1
                 RETURNING last_number",
            )
            .bind(user.tenant_id)
            .bind(&req.kind)
            .bind(date.year())
            .fetch_one(&mut *tx)
            .await?;
            format_doc_number(&pattern_for(&state, user.tenant_id, &req.kind).await?, n, date)
        }
    };
    let vat_amount = money::vat_included(amount, &vat_rate);
    let doc_id: Uuid = sqlx::query_scalar(
        "INSERT INTO crm_documents (tenant_id, kind, number, doc_date, company_id, company_name, deal_id, basis_id, subject, amount,
                                    vat_rate, vat_amount, template_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&req.kind)
    .bind(&number)
    .bind(date)
    .bind(company_id)
    .bind(&company_name)
    .bind(deal_id)
    .bind(req.basis_id)
    .bind(&subject)
    .bind(amount)
    .bind(&vat_rate)
    .bind(vat_amount)
    .bind(req.template_id)
    .bind(user.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict(format!("{} № {number} за {} год уже есть", kind_label(&req.kind), date.year())),
        other => other,
    })?;

    let doc = DocInput { number: &number, date, subject: &subject, amount, vat_rate: &vat_rate, basis };
    let values = sales_values(&state, &user, &deal, company_id, &doc).await?;
    let kind = Kind::from_extension(&ext).ok_or_else(|| ApiError::Internal(anyhow::anyhow!("bad template extension")))?;
    let data = state.store.get(&key).await?;
    let (rendered, missing) = tokio::task::spawn_blocking(move || ooxml::render(&data, kind, &values))
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .map_err(|e| ApiError::BadRequest(format!("template cannot be processed: {e}")))?;
    let stem = format!("{} № {} от {}", kind_label(&req.kind), number.replace(['/', '\\'], "-"), date.format("%d.%m.%Y"));
    let file_name = validate_name(&free_name(&state, folder_id, &stem, &ext).await?)?;
    let file_id = create_file(&state, &user, folder_id, &file_name, Bytes::from(rendered)).await?;
    sqlx::query("UPDATE crm_documents SET file_id = $2 WHERE id = $1").bind(doc_id).bind(file_id).execute(&mut *tx).await?;
    tx.commit().await?;

    super::system_activity(
        &state,
        &user,
        Some(company_id),
        Some(deal_id),
        format!("Сформирован документ: {} № {number} от {} на {} руб.", kind_label(&req.kind), date.format("%d.%m.%Y"), format_number(amount)),
    )
    .await;
    state
        .audit(user.tenant_id, Some(user.user_id), "crm.document_created", Some("file"), Some(file_id),
               json!({ "kind": req.kind, "number": number, "template": tpl_name }))
        .await;
    Ok(Json(json!({ "document": load_doc(&state, &user, doc_id).await?, "missing": missing })))
}

#[derive(Deserialize)]
pub struct UpdateDocument {
    pub status: String,
}

/// Change the status: draft → sent → signed, or cancelled (frees the number).
pub async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<UpdateDocument>) -> ApiResult<Json<Value>> {
    let d = load_doc(&state, &user, id).await?;
    if !matches!(req.status.as_str(), "draft" | "sent" | "signed" | "cancelled") {
        return bad_request("unknown status");
    }
    let (responsible, created_by): (Option<Uuid>, Option<Uuid>) = match d.deal_id {
        Some(did) => {
            let deal = load_deal(&state, &user, did).await?;
            (deal.responsible_id, deal.created_by)
        }
        None => (None, None),
    };
    let author: Option<Uuid> = sqlx::query_scalar("SELECT created_by FROM crm_documents WHERE id = $1").bind(id).fetch_one(&state.db).await?;
    if !(super::can_edit(&user, responsible, created_by) || author == Some(user.user_id)) {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("UPDATE crm_documents SET status = $2 WHERE id = $1")
        .bind(id)
        .bind(&req.status)
        .execute(&state.db)
        .await
        .map_err(|e| match ApiError::from(e) {
            ApiError::Conflict(_) => ApiError::Conflict("документ с таким номером уже есть — восстановить нельзя".into()),
            other => other,
        })?;
    Ok(Json(json!({ "document": load_doc(&state, &user, id).await? })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert_eq!(format_doc_number("{n}", 7, d), "7");
        assert_eq!(format_doc_number("КП-{n}/{yy}", 12, d), "КП-12/26");
        assert_eq!(format_doc_number("{yyyy}-{mm}-{n:3}", 5, d), "2026-03-005");
        assert_eq!(format_doc_number("СЧ{n:2}{x}", 123, d), "СЧ123{x}");
        assert_eq!(format_doc_number("{n", 1, d), "{n");
    }
}
