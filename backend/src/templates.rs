//! Document templates: admins upload DOCX/XLSX files with `{{placeholders}}`;
//! project members generate filled-in documents into the project folder.

use std::collections::HashMap;

use axum::{
    extract::{Multipart, Path, State},
    response::Response,
    Json,
};
use bytes::Bytes;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    acl::{self, Access},
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    fields::{load_defs, project_values, FieldDef},
    files::{create_file, extension_of, file_response, validate_name},
    ooxml::{self, Kind},
    projects::project_role,
    state::AppState,
    storage::BlobStore,
};

const MAX_TEMPLATE_BYTES: usize = 20 * 1024 * 1024;
static SAMPLE_TITLE: &[u8] = include_bytes!("../assets/templates/sample_title.docx");

#[derive(Serialize, sqlx::FromRow)]
pub struct TemplateRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: String,
    pub extension: String,
    pub size_bytes: i64,
    pub placeholders: Value,
    pub updated_at: DateTime<Utc>,
    /// project | sales
    pub scope: String,
    /// quote | contract | invoice | act (sales templates)
    pub doc_kind: Option<String>,
}

const TEMPLATE_SELECT: &str =
    "SELECT id, name, description, category, extension, size_bytes, placeholders, updated_at, scope, doc_kind FROM doc_templates";

pub const DOC_KINDS: [&str; 4] = ["quote", "contract", "invoice", "act"];

fn check_scope(scope: &str, doc_kind: Option<&str>) -> ApiResult<()> {
    match (scope, doc_kind) {
        ("project", None) => Ok(()),
        ("sales", Some(k)) if DOC_KINDS.contains(&k) => Ok(()),
        ("sales", _) => bad_request("укажите вид документа продаж: quote, contract, invoice или act"),
        _ => bad_request("scope must be project or sales"),
    }
}

pub async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows = sqlx::query_as::<_, TemplateRow>(&format!(
        "{TEMPLATE_SELECT} WHERE tenant_id = $1 ORDER BY category, lower(name)"
    ))
    .bind(user.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "templates": rows })))
}

// ---------------------------------------------------------------------------
// Placeholder catalogue and values
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct PlaceholderInfo {
    pub key: String,
    pub label: String,
    pub group: String,
}

/// All placeholders available in this portal for a template scope: system
/// ones plus one per project requisite (and client/deal/document/organisation
/// ones for sales documents).
pub async fn catalogue(state: &AppState, tenant_id: Uuid, scope: &str) -> ApiResult<Vec<PlaceholderInfo>> {
    let p = |key: &str, label: &str, group: &str| PlaceholderInfo { key: key.into(), label: label.into(), group: group.into() };
    let mut out = if scope == "sales" { crate::crm::documents::sales_catalogue() } else { vec![] };
    out.extend(vec![
        p("project.name", "Название проекта", "Проект"),
        p("project.description", "Описание проекта", "Проект"),
        p("project.status", "Статус проекта", "Проект"),
        p("portal.name", "Название организации", "Организация"),
        p("date", "Текущая дата (10.11.2026)", "Дата и автор"),
        p("date_long", "Текущая дата прописью (10 ноября 2026 г.)", "Дата и автор"),
        p("year", "Текущий год", "Дата и автор"),
        p("user.name", "Кто сформировал документ", "Дата и автор"),
        p("user.email", "Email автора", "Дата и автор"),
        p("user.full_name", "ФИО автора полностью", "Дата и автор"),
        p("user.short_name", "Автор: фамилия и инициалы", "Дата и автор"),
        p("user.position", "Должность автора (для документов)", "Дата и автор"),
        p("user.name_genitive", "ФИО автора в родительном падеже", "Дата и автор"),
        p("user.name_dative", "ФИО автора в дательном падеже", "Дата и автор"),
        p("user.nrs", "Номер автора в НРС", "Дата и автор"),
        p("user.qual_cert", "Квалификационный аттестат автора", "Дата и автор"),
        p("user.power_of_attorney", "Доверенность автора", "Дата и автор"),
        p("user.phone", "Рабочий телефон автора", "Дата и автор"),
    ]);
    for d in load_defs(state, tenant_id, false).await? {
        let group = if scope == "sales" { format!("Проект сделки: {}", d.section) } else { format!("Реквизиты: {}", d.section) };
        out.push(PlaceholderInfo { key: format!("project.{}", d.key), label: d.label, group });
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct ScopeQuery {
    pub scope: Option<String>,
}

pub async fn placeholders(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Query(q): axum::extract::Query<ScopeQuery>,
) -> ApiResult<Json<Value>> {
    let scope = q.scope.as_deref().unwrap_or("project");
    Ok(Json(json!({ "placeholders": catalogue(&state, user.tenant_id, scope).await? })))
}

/// Russian number formatting: `1 250 000,5`.
pub fn format_number(n: f64) -> String {
    let neg = n < 0.0;
    let rounded = (n.abs() * 100.0).round() / 100.0;
    let int = rounded.trunc() as u64;
    let frac = ((rounded - rounded.trunc()) * 100.0).round() as u64;
    let digits = int.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push('\u{00a0}');
        }
        grouped.push(ch);
    }
    let mut s = if neg { format!("-{grouped}") } else { grouped };
    if frac > 0 {
        let f = format!("{frac:02}");
        s.push(',');
        s.push_str(f.trim_end_matches('0'));
    }
    s
}

pub const MONTHS_GEN: [&str; 12] = [
    "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря",
];

pub fn format_value(def: &FieldDef, v: &Value, users: &HashMap<Uuid, String>) -> String {
    match def.field_type.as_str() {
        "number" => v.as_f64().map(format_number).unwrap_or_default(),
        "date" => v
            .as_str()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| d.format("%d.%m.%Y").to_string())
            .unwrap_or_default(),
        "checkbox" => if v.as_bool() == Some(true) { "Да".into() } else { "Нет".into() },
        "user" => v.as_str().and_then(|s| Uuid::parse_str(s).ok()).and_then(|id| users.get(&id).cloned()).unwrap_or_default(),
        "company" => v.get("name").and_then(Value::as_str).or_else(|| v.as_str()).unwrap_or_default().to_string(),
        _ => v.as_str().map(str::to_string).unwrap_or_default(),
    }
}

pub async fn values_for(state: &AppState, user: &CurrentUser, project_id: Uuid, today: NaiveDate) -> ApiResult<HashMap<String, String>> {
    let (name, description, status): (String, String, String) =
        sqlx::query_as("SELECT name, description, status FROM projects WHERE id = $1 AND tenant_id = $2")
            .bind(project_id)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let portal: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(user.tenant_id).fetch_one(&state.db).await?;
    let users: HashMap<Uuid, String> = sqlx::query_as::<_, (Uuid, String)>("SELECT id, display_name FROM users WHERE tenant_id = $1")
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .collect();

    let mut map = HashMap::new();
    map.insert("project.name".into(), name);
    map.insert("project.description".into(), description);
    map.insert(
        "project.status".into(),
        match status.as_str() { "active" => "Активный", "paused" => "Приостановлен", _ => "Закрыт" }.into(),
    );
    map.insert("portal.name".into(), portal);
    map.insert("date".into(), today.format("%d.%m.%Y").to_string());
    map.insert("date_long".into(), format!("{} {} {} г.", today.day(), MONTHS_GEN[today.month0() as usize], today.year()));
    map.insert("year".into(), today.year().to_string());
    map.insert("user.name".into(), user.display_name.clone());
    map.insert("user.email".into(), user.email.clone());
    for (k, v) in crate::profiles::author_values(state, user).await? {
        map.insert(k, v);
    }

    let values = project_values(state, project_id).await?;
    for def in load_defs(state, user.tenant_id, true).await? {
        let text = values.get(&def.key).map(|v| format_value(&def, v, &users)).unwrap_or_default();
        map.insert(format!("project.{}", def.key), text);
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Template management (admin)
// ---------------------------------------------------------------------------

struct Upload {
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    scope: Option<String>,
    doc_kind: Option<String>,
    file_name: Option<String>,
    data: Option<Bytes>,
}

async fn read_upload(mut multipart: Multipart) -> ApiResult<Upload> {
    let mut u = Upload { name: None, description: None, category: None, scope: None, doc_kind: None, file_name: None, data: None };
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        let fname = field.name().unwrap_or_default().to_string();
        if let Some(file_name) = field.file_name().map(str::to_string) {
            u.file_name = Some(file_name);
            u.data = Some(field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?);
            continue;
        }
        let text = field.text().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
        match fname.as_str() {
            "name" => u.name = Some(text),
            "description" => u.description = Some(text),
            "category" => u.category = Some(text),
            "scope" => u.scope = Some(text),
            "doc_kind" => u.doc_kind = Some(text).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    Ok(u)
}

/// Validate the uploaded file and extract its placeholders.
fn inspect(file_name: &str, data: &[u8]) -> ApiResult<(String, Vec<String>)> {
    if data.len() > MAX_TEMPLATE_BYTES {
        return Err(ApiError::TooLarge("template must not exceed 20 MB".into()));
    }
    let ext = extension_of(file_name);
    let kind = Kind::from_extension(&ext).ok_or_else(|| ApiError::BadRequest("templates must be .docx or .xlsx".into()))?;
    let keys = ooxml::scan(data, kind).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok((ext, keys))
}

#[allow(clippy::too_many_arguments)]
async fn insert_template(
    state: &AppState,
    user: &CurrentUser,
    name: &str,
    description: &str,
    category: &str,
    scope: &str,
    doc_kind: Option<&str>,
    file_name: &str,
    data: Bytes,
) -> ApiResult<Uuid> {
    check_scope(scope, doc_kind)?;
    let (ext, keys) = inspect(file_name, &data)?;
    let id = Uuid::new_v4();
    let key = format!("{}/templates/{id}-{}", user.tenant_id, Uuid::new_v4().simple());
    let size = data.len() as i64;
    state.store.put(&key, data).await?;
    let res = sqlx::query(
        "INSERT INTO doc_templates (id, tenant_id, name, description, category, extension, storage_key, size_bytes, placeholders, created_by,
                                    scope, doc_kind)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(name)
    .bind(description)
    .bind(category)
    .bind(&ext)
    .bind(&key)
    .bind(size)
    .bind(json!(keys))
    .bind(user.user_id)
    .bind(scope)
    .bind(doc_kind)
    .execute(&state.db)
    .await;
    if let Err(e) = res {
        let _ = state.store.delete(&key).await;
        return Err(match ApiError::from(e) {
            ApiError::Conflict(_) => ApiError::Conflict(format!("a template named '{name}' already exists")),
            other => other,
        });
    }
    state.audit(user.tenant_id, Some(user.user_id), "template.created", None, Some(id), json!({ "name": name })).await;
    Ok(id)
}

fn clean_meta(name: Option<String>, fallback: &str) -> ApiResult<String> {
    let n = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| fallback.to_string());
    if n.is_empty() || n.chars().count() > 200 {
        return bad_request("template name is required (up to 200 chars)");
    }
    Ok(n)
}

pub async fn create(State(state): State<AppState>, user: CurrentUser, multipart: Multipart) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let u = read_upload(multipart).await?;
    let (file_name, data) = match (u.file_name, u.data) {
        (Some(f), Some(d)) => (f, d),
        _ => return bad_request("no file uploaded"),
    };
    let stem = file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&file_name).to_string();
    let name = clean_meta(u.name, &stem)?;
    let scope = u.scope.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("project").to_string();
    let default_category = if scope == "sales" { "Документы продаж" } else { "Общие" };
    let category = u.category.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| default_category.into());
    let id = insert_template(
        &state,
        &user,
        &name,
        u.description.as_deref().unwrap_or("").trim(),
        &category,
        &scope,
        u.doc_kind.as_deref().map(str::trim),
        &file_name,
        data,
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

/// Install the bundled sample title page as a template.
pub async fn install_sample(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let id = insert_template(
        &state,
        &user,
        "Титульный лист отчёта (пример)",
        "Пример шаблона с подстановкой реквизитов проекта. Отредактируйте под фирменный стиль.",
        "Отчёты",
        "project",
        None,
        "sample_title.docx",
        Bytes::from_static(SAMPLE_TITLE),
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

static SAMPLE_QUOTE: &[u8] = include_bytes!("../assets/templates/sales_quote.docx");
static SAMPLE_CONTRACT: &[u8] = include_bytes!("../assets/templates/sales_contract.docx");
static SAMPLE_INVOICE: &[u8] = include_bytes!("../assets/templates/sales_invoice.docx");
static SAMPLE_ACT: &[u8] = include_bytes!("../assets/templates/sales_act.docx");

/// Install bundled sample sales documents; existing names are skipped.
pub async fn install_sales_samples(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let samples: [(&str, &str, &str, &[u8]); 4] = [
        ("Коммерческое предложение (пример)", "quote", "sales_quote.docx", SAMPLE_QUOTE),
        ("Договор подряда (пример)", "contract", "sales_contract.docx", SAMPLE_CONTRACT),
        ("Счёт на оплату (пример)", "invoice", "sales_invoice.docx", SAMPLE_INVOICE),
        ("Акт выполненных работ (пример)", "act", "sales_act.docx", SAMPLE_ACT),
    ];
    let mut created = vec![];
    for (name, kind, file, data) in samples {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM doc_templates WHERE tenant_id = $1 AND name = $2)")
            .bind(user.tenant_id)
            .bind(name)
            .fetch_one(&state.db)
            .await?;
        if exists {
            continue;
        }
        let id = insert_template(
            &state,
            &user,
            name,
            "Пример шаблона: подставляются реквизиты вашей организации, контрагента и сделки. Отредактируйте под свою форму.",
            "Документы продаж",
            "sales",
            Some(kind),
            file,
            Bytes::from_static(data),
        )
        .await?;
        created.push(id);
    }
    Ok(Json(json!({ "created": created })))
}

#[derive(Deserialize)]
pub struct UpdateMeta {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    pub scope: Option<String>,
    pub doc_kind: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMeta>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let name = clean_meta(Some(req.name), "")?;
    let category = if req.category.trim().is_empty() { "Общие".to_string() } else { req.category.trim().to_string() };
    let current: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT scope, doc_kind FROM doc_templates WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?;
    let (cur_scope, cur_kind) = current.ok_or(ApiError::NotFound)?;
    let scope = req.scope.clone().unwrap_or(cur_scope);
    let doc_kind = if scope == "project" { None } else { req.doc_kind.clone().filter(|s| !s.is_empty()).or(cur_kind) };
    check_scope(&scope, doc_kind.as_deref())?;
    let n = sqlx::query(
        "UPDATE doc_templates SET name = $3, description = $4, category = $5, scope = $6, doc_kind = $7, updated_at = now()
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(&name)
    .bind(req.description.trim())
    .bind(&category)
    .bind(&scope)
    .bind(&doc_kind)
    .execute(&state.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

/// Replace the template file, keeping its name and category.
pub async fn replace_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    multipart: Multipart,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let old_key: String = sqlx::query_scalar("SELECT storage_key FROM doc_templates WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let u = read_upload(multipart).await?;
    let (file_name, data) = match (u.file_name, u.data) {
        (Some(f), Some(d)) => (f, d),
        _ => return bad_request("no file uploaded"),
    };
    let (ext, keys) = inspect(&file_name, &data)?;
    let key = format!("{}/templates/{id}-{}", user.tenant_id, Uuid::new_v4().simple());
    let size = data.len() as i64;
    state.store.put(&key, data).await?;
    sqlx::query(
        "UPDATE doc_templates SET extension = $3, storage_key = $4, size_bytes = $5, placeholders = $6, updated_at = now()
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(&ext)
    .bind(&key)
    .bind(size)
    .bind(json!(keys))
    .execute(&state.db)
    .await?;
    let _ = state.store.delete(&old_key).await;
    state.audit(user.tenant_id, Some(user.user_id), "template.updated", None, Some(id), json!({})).await;
    Ok(Json(json!({ "placeholders": keys })))
}

pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let (key, name): (String, String) =
        sqlx::query_as("DELETE FROM doc_templates WHERE id = $1 AND tenant_id = $2 RETURNING storage_key, name")
            .bind(id)
            .bind(user.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let _ = state.store.delete(&key).await;
    state.audit(user.tenant_id, Some(user.user_id), "template.deleted", None, Some(id), json!({ "name": name })).await;
    Ok(Json(json!({ "ok": true })))
}

/// (name, extension, storage key, placeholders) of a template of the given scope.
pub async fn load_template_scoped(state: &AppState, tenant_id: Uuid, id: Uuid, scope: &str) -> ApiResult<(String, String, String, Vec<String>)> {
    let (name, ext, key, placeholders, actual): (String, String, String, Value, String) = sqlx::query_as(
        "SELECT name, extension, storage_key, placeholders, scope FROM doc_templates WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    if !scope.is_empty() && actual != scope {
        return bad_request(if scope == "project" {
            "это шаблон документа продаж — он формируется из сделки"
        } else {
            "это шаблон проекта — выберите шаблон документа продаж"
        });
    }
    let keys = serde_json::from_value(placeholders).unwrap_or_default();
    Ok((name, ext, key, keys))
}

async fn load_template(state: &AppState, tenant_id: Uuid, id: Uuid) -> ApiResult<(String, String, String, Vec<String>)> {
    load_template_scoped(state, tenant_id, id, "").await
}

pub async fn download(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Response> {
    let (name, ext, key, _) = load_template(&state, user.tenant_id, id).await?;
    let data = state.store.get(&key).await?;
    Ok(file_response(&format!("{name}.{ext}"), data, false))
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CheckQuery {
    /// Client's local date (YYYY-MM-DD) so that `{{date}}` matches the user's day.
    pub today: Option<NaiveDate>,
}

fn today_or(q: Option<NaiveDate>) -> NaiveDate {
    q.unwrap_or_else(|| Utc::now().date_naive())
}

/// Which placeholders of a template would stay empty for a project.
pub async fn check(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((project_id, template_id)): Path<(Uuid, Uuid)>,
    axum::extract::Query(q): axum::extract::Query<CheckQuery>,
) -> ApiResult<Json<Value>> {
    project_role(&state, &user, project_id).await?;
    let (_, _, _, keys) = load_template_scoped(&state, user.tenant_id, template_id, "project").await?;
    let values = values_for(&state, &user, project_id, today_or(q.today)).await?;
    let labels: HashMap<String, String> = catalogue(&state, user.tenant_id, "project").await?.into_iter().map(|p| (p.key, p.label)).collect();
    let missing: Vec<Value> = keys
        .iter()
        .filter(|k| values.get(*k).is_none_or(|v| v.is_empty()))
        .map(|k| json!({ "key": k, "label": labels.get(k).cloned().unwrap_or_else(|| format!("неизвестная метка {{{{{k}}}}}")), "known": labels.contains_key(k) }))
        .collect();
    Ok(Json(json!({ "missing": missing })))
}

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub template_id: Uuid,
    /// File name without extension; defaults to "<template> <project code>".
    pub name: Option<String>,
    /// Target folder; defaults to the project's document folder.
    pub folder_id: Option<Uuid>,
    pub today: Option<NaiveDate>,
}

/// Pick a free file name in a folder: "Name.docx", "Name (2).docx", ...
pub async fn free_name(state: &AppState, folder_id: Uuid, stem: &str, ext: &str) -> ApiResult<String> {
    for i in 1..1000 {
        let candidate = if i == 1 { format!("{stem}.{ext}") } else { format!("{stem} ({i}).{ext}") };
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM files WHERE folder_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL)",
        )
        .bind(folder_id)
        .bind(&candidate)
        .fetch_one(&state.db)
        .await?;
        if !taken {
            return Ok(candidate);
        }
    }
    bad_request("too many files with this name")
}

pub async fn generate(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<Uuid>,
    Json(req): Json<GenerateRequest>,
) -> ApiResult<Json<Value>> {
    project_role(&state, &user, project_id).await?;
    let project_folder: Option<Uuid> = sqlx::query_scalar("SELECT folder_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&state.db)
        .await?;
    let folder_id = req.folder_id.or(project_folder).ok_or_else(|| ApiError::BadRequest("the project has no document folder".into()))?;
    acl::folder_access(&state.db, &user, folder_id).await?.require(Access::Edit)?;

    let (tpl_name, ext, key, _) = load_template_scoped(&state, user.tenant_id, req.template_id, "project").await?;
    let kind = Kind::from_extension(&ext).ok_or_else(|| ApiError::Internal(anyhow::anyhow!("bad template extension")))?;
    let values = values_for(&state, &user, project_id, today_or(req.today)).await?;

    let data = state.store.get(&key).await?;
    let (doc, missing) = tokio::task::spawn_blocking(move || ooxml::render(&data, kind, &values))
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .map_err(|e| ApiError::BadRequest(format!("template cannot be processed: {e}")))?;

    let code = project_values(&state, project_id).await?.get("code").and_then(Value::as_str).map(str::to_string);
    let default_stem = match code {
        Some(c) if !c.is_empty() => format!("{tpl_name} {c}"),
        _ => tpl_name.clone(),
    };
    let stem = req.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or(default_stem);
    let stem = stem.strip_suffix(&format!(".{ext}")).unwrap_or(&stem).to_string();
    let file_name = validate_name(&free_name(&state, folder_id, &stem.replace(['/', '\\'], "-"), &ext).await?)?;

    let file_id = create_file(&state, &user, folder_id, &file_name, Bytes::from(doc)).await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "template.generated", Some("file"), Some(file_id), json!({ "name": file_name, "template": tpl_name }))
        .await;
    Ok(Json(json!({ "file_id": file_id, "name": file_name, "folder_id": folder_id, "missing": missing })))
}

#[cfg(test)]
mod tests {
    use super::format_number;

    #[test]
    fn russian_numbers() {
        assert_eq!(format_number(1250000.5), "1\u{a0}250\u{a0}000,5");
        assert_eq!(format_number(1000.0), "1\u{a0}000");
        assert_eq!(format_number(12.34), "12,34");
        assert_eq!(format_number(-5.0), "-5");
        assert_eq!(format_number(999.0), "999");
    }
}
