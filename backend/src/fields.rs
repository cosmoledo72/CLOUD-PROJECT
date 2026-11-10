//! Project requisites: tenant admins define typed fields (project code,
//! client, contract number, design stage, chief engineer, ...); project
//! managers fill them in per project.

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    projects::{project_role, ProjectRole},
    state::AppState,
};

pub const FIELD_TYPES: &[&str] = &["text", "textarea", "number", "date", "select", "user", "checkbox", "company"];

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct FieldDef {
    pub id: Uuid,
    pub key: String,
    pub label: String,
    pub section: String,
    pub field_type: String,
    pub options: Value,
    pub help_text: String,
    pub is_required: bool,
    pub show_in_list: bool,
    pub is_archived: bool,
    pub sort_order: f64,
}

const DEF_SELECT: &str = "SELECT id, key, label, section, field_type, options, help_text, is_required, show_in_list,
        is_archived, sort_order FROM project_field_defs";

pub async fn load_defs(state: &AppState, tenant_id: Uuid, include_archived: bool) -> ApiResult<Vec<FieldDef>> {
    Ok(sqlx::query_as::<_, FieldDef>(&format!(
        "{DEF_SELECT} WHERE tenant_id = $1 AND ($2 OR NOT is_archived) ORDER BY sort_order, created_at"
    ))
    .bind(tenant_id)
    .bind(include_archived)
    .fetch_all(&state.db)
    .await?)
}

/// The requisite that names the project's customer: the first active field
/// of type "company".
pub fn customer_def(defs: &[FieldDef]) -> Option<&FieldDef> {
    defs.iter().find(|d| d.field_type == "company" && !d.is_archived)
}

/// Field definitions visible to any portal user (needed to render project forms).
pub async fn list_defs(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let all = user.is_admin();
    Ok(Json(json!({ "fields": load_defs(&state, user.tenant_id, all).await? })))
}

// ---------------------------------------------------------------------------
// Definition management (admin)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DefInput {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub section: Option<String>,
    pub field_type: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub help_text: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub show_in_list: bool,
    #[serde(default)]
    pub is_archived: bool,
}

struct CleanDef {
    key: String,
    label: String,
    section: String,
    field_type: String,
    options: Value,
    help_text: String,
}

fn clean_def(d: &DefInput) -> ApiResult<CleanDef> {
    let key = d.key.trim().to_lowercase();
    let key_ok = !key.is_empty()
        && key.len() <= 40
        && key.starts_with(|c: char| c.is_ascii_lowercase())
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !key_ok {
        return bad_request("key: latin lowercase letters, digits and '_', starting with a letter, up to 40 chars");
    }
    let label = d.label.trim().to_string();
    if label.is_empty() || label.chars().count() > 120 {
        return bad_request("label is required (up to 120 chars)");
    }
    if !FIELD_TYPES.contains(&d.field_type.as_str()) {
        return bad_request("unknown field type");
    }
    let options: Vec<String> = d.options.iter().map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
    if d.field_type == "select" {
        if options.is_empty() {
            return bad_request("a select field needs at least one option");
        }
        let mut seen = std::collections::HashSet::new();
        if !options.iter().all(|o| seen.insert(o.to_lowercase())) {
            return bad_request("options must be unique");
        }
    }
    let section = d.section.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("Общие сведения").to_string();
    Ok(CleanDef {
        key,
        label,
        section,
        field_type: d.field_type.clone(),
        options: if d.field_type == "select" { json!(options) } else { json!([]) },
        help_text: d.help_text.trim().to_string(),
    })
}

pub async fn create_def(State(state): State<AppState>, user: CurrentUser, Json(d): Json<DefInput>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let c = clean_def(&d)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_field_defs (tenant_id, key, label, section, field_type, options, help_text, is_required, show_in_list,
                                         is_archived, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                 COALESCE((SELECT max(sort_order) FROM project_field_defs WHERE tenant_id = $1), 0) + 1)
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&c.key)
    .bind(&c.label)
    .bind(&c.section)
    .bind(&c.field_type)
    .bind(&c.options)
    .bind(&c.help_text)
    .bind(d.is_required)
    .bind(d.show_in_list)
    .bind(d.is_archived)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict(format!("a field with key '{}' already exists", c.key)),
        other => other,
    })?;
    state.audit(user.tenant_id, Some(user.user_id), "project_fields.created", None, Some(id), json!({ "key": c.key })).await;
    Ok(Json(json!({ "id": id })))
}

pub async fn update_def(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(d): Json<DefInput>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let c = clean_def(&d)?;
    let current_type: String = sqlx::query_scalar("SELECT field_type FROM project_field_defs WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    // Changing the type would invalidate stored values.
    if current_type != c.field_type {
        let has_values: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM project_field_values WHERE field_id = $1)")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
        if has_values {
            return bad_request("the field already has values; its type cannot be changed (archive it and create a new one)");
        }
    }
    sqlx::query(
        "UPDATE project_field_defs SET key = $3, label = $4, section = $5, field_type = $6, options = $7, help_text = $8,
                is_required = $9, show_in_list = $10, is_archived = $11
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(&c.key)
    .bind(&c.label)
    .bind(&c.section)
    .bind(&c.field_type)
    .bind(&c.options)
    .bind(&c.help_text)
    .bind(d.is_required)
    .bind(d.show_in_list)
    .bind(d.is_archived)
    .execute(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "project_fields.updated", None, Some(id), json!({ "key": c.key })).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_def(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let key: String = sqlx::query_scalar("DELETE FROM project_field_defs WHERE id = $1 AND tenant_id = $2 RETURNING key")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    state.audit(user.tenant_id, Some(user.user_id), "project_fields.deleted", None, Some(id), json!({ "key": key })).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ReorderRequest {
    pub ids: Vec<Uuid>,
}

pub async fn reorder_defs(State(state): State<AppState>, user: CurrentUser, Json(req): Json<ReorderRequest>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let mut tx = state.db.begin().await?;
    for (i, id) in req.ids.iter().enumerate() {
        sqlx::query("UPDATE project_field_defs SET sort_order = $3 WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(user.tenant_id)
            .bind((i + 1) as f64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Ready-made set of requisites for design and survey work. Fields whose key
/// already exists are skipped, so the preset can be applied repeatedly.
pub async fn apply_preset(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let preset: Vec<(&str, &str, &str, &str, Vec<&str>, bool, bool, &str)> = vec![
        ("code", "Шифр проекта", "Общие сведения", "text", vec![], true, true, "Например: 125-24-ТО"),
        ("work_type", "Вид работ", "Общие сведения", "select",
         vec!["Техническое обследование", "Проектирование", "Сохранение ОКН", "Авторский надзор", "Инженерные изыскания"], false, true, ""),
        ("stage", "Стадия", "Общие сведения", "select", vec!["ПП", "П", "РД", "П+РД", "Обследование"], false, false, ""),
        ("object_name", "Наименование объекта", "Объект", "textarea", vec![], true, false, "Полное наименование по договору"),
        ("object_address", "Адрес объекта", "Объект", "textarea", vec![], false, false, ""),
        ("cadastral_number", "Кадастровый номер", "Объект", "text", vec![], false, false, ""),
        ("client", "Заказчик", "Договор", "company", vec![], true, true, "Выберите из справочника контрагентов или введите наименование"),
        ("client_inn", "ИНН заказчика", "Договор", "text", vec![], false, false, ""),
        ("contract_number", "Номер договора", "Договор", "text", vec![], false, false, ""),
        ("contract_date", "Дата договора", "Договор", "date", vec![], false, false, ""),
        ("contract_amount", "Стоимость работ, руб.", "Договор", "number", vec![], false, false, ""),
        ("deadline", "Срок сдачи по договору", "Договор", "date", vec![], false, true, ""),
        ("chief_engineer", "ГИП", "Участники", "user", vec![], false, true, "Главный инженер проекта"),
        ("chief_architect", "ГАП", "Участники", "user", vec![], false, false, "Главный архитектор проекта"),
        ("expertise_required", "Требуется экспертиза", "Договор", "checkbox", vec![], false, false, ""),
    ];
    let mut created = 0;
    let mut tx = state.db.begin().await?;
    let base: f64 = sqlx::query_scalar("SELECT COALESCE(max(sort_order), 0) FROM project_field_defs WHERE tenant_id = $1")
        .bind(user.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    for (i, (key, label, section, ftype, options, required, in_list, help)) in preset.into_iter().enumerate() {
        let r = sqlx::query(
            "INSERT INTO project_field_defs (tenant_id, key, label, section, field_type, options, help_text, is_required,
                                             show_in_list, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) ON CONFLICT (tenant_id, key) DO NOTHING",
        )
        .bind(user.tenant_id)
        .bind(key)
        .bind(label)
        .bind(section)
        .bind(ftype)
        .bind(json!(options))
        .bind(help)
        .bind(required)
        .bind(in_list)
        .bind(base + (i + 1) as f64)
        .execute(&mut *tx)
        .await?;
        created += r.rows_affected();
    }
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "project_fields.preset_applied", None, None, json!({ "created": created })).await;
    Ok(Json(json!({ "created": created })))
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Object(o) => o.get("id").is_none_or(Value::is_null) && o.get("name").and_then(Value::as_str).is_none_or(|s| s.trim().is_empty()),
        Value::String(s) => s.trim().is_empty(),
        Value::Bool(b) => !b,
        _ => false,
    }
}

/// Validate and normalise a value for a field. `Ok(None)` means "clear".
async fn normalise(state: &AppState, tenant_id: Uuid, def: &FieldDef, v: &Value) -> ApiResult<Option<Value>> {
    if is_empty(v) {
        return Ok(None);
    }
    let err = |msg: &str| ApiError::BadRequest(format!("«{}»: {msg}", def.label));
    let out = match def.field_type.as_str() {
        "text" | "textarea" => {
            let s = v.as_str().ok_or_else(|| err("expected text"))?.trim().to_string();
            let max = if def.field_type == "text" { 500 } else { 5000 };
            if s.chars().count() > max {
                return Err(err(&format!("text is longer than {max} characters")));
            }
            json!(s)
        }
        "number" => {
            let n = match v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => s.trim().replace(' ', "").replace(',', ".").parse::<f64>().ok(),
                _ => None,
            }
            .filter(|n| n.is_finite())
            .ok_or_else(|| err("expected a number"))?;
            json!(n)
        }
        "date" => {
            let s = v.as_str().ok_or_else(|| err("expected a date"))?;
            let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| err("expected a date YYYY-MM-DD"))?;
            json!(d.to_string())
        }
        "select" => {
            let s = v.as_str().ok_or_else(|| err("expected an option"))?;
            let allowed = def.options.as_array().map(|a| a.iter().any(|o| o.as_str() == Some(s))).unwrap_or(false);
            if !allowed {
                return Err(err("value is not in the option list"));
            }
            json!(s)
        }
        "user" => {
            let id = v.as_str().and_then(|s| Uuid::parse_str(s).ok()).ok_or_else(|| err("expected a user"))?;
            let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND tenant_id = $2)")
                .bind(id)
                .bind(tenant_id)
                .fetch_one(&state.db)
                .await?;
            if !ok {
                return Err(err("unknown user"));
            }
            json!(id.to_string())
        }
        "checkbox" => json!(v.as_bool().ok_or_else(|| err("expected true/false"))?),
        "company" => {
            // A directory entry {id, ...} or a typed-in name (string or {name}).
            let id = match v {
                Value::Object(o) => match o.get("id") {
                    Some(Value::String(s)) => Some(Uuid::parse_str(s).map_err(|_| err("expected a company"))?),
                    Some(Value::Null) | None => None,
                    _ => return Err(err("expected a company")),
                },
                _ => None,
            };
            match id {
                Some(id) => {
                    let (name, inn): (String, Option<String>) =
                        sqlx::query_as("SELECT name, inn FROM crm_companies WHERE id = $1 AND tenant_id = $2")
                            .bind(id)
                            .bind(tenant_id)
                            .fetch_optional(&state.db)
                            .await?
                            .ok_or_else(|| err("unknown company"))?;
                    json!({ "id": id, "name": name, "inn": inn })
                }
                None => {
                    let name = match v {
                        Value::String(s) => s.trim().to_string(),
                        Value::Object(o) => o.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                        _ => return Err(err("expected a company")),
                    };
                    if name.is_empty() {
                        return Ok(None);
                    }
                    if name.chars().count() > 500 {
                        return Err(err("text is longer than 500 characters"));
                    }
                    json!({ "id": null, "name": name, "inn": null })
                }
            }
        }
        _ => return Err(err("unsupported field type")),
    };
    Ok(Some(out))
}

/// Current values of a project as `{ key: value }`.
pub async fn project_values(state: &AppState, project_id: Uuid) -> ApiResult<Map<String, Value>> {
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT d.key, v.value FROM project_field_values v JOIN project_field_defs d ON d.id = v.field_id
          WHERE v.project_id = $1",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Apply a `{ key: value }` patch to a project's requisites inside a transaction.
/// Unknown keys are rejected; `required` is checked against the resulting state.
pub async fn save_values(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    patch: &Map<String, Value>,
) -> ApiResult<()> {
    let defs = load_defs(state, tenant_id, true).await?;
    let by_key: HashMap<&str, &FieldDef> = defs.iter().map(|d| (d.key.as_str(), d)).collect();

    let existing: Vec<(Uuid,)> = sqlx::query_as("SELECT field_id FROM project_field_values WHERE project_id = $1")
        .bind(project_id)
        .fetch_all(&mut **tx)
        .await?;
    let mut filled: std::collections::HashSet<Uuid> = existing.into_iter().map(|r| r.0).collect();

    for (key, raw) in patch {
        let def = by_key.get(key.as_str()).ok_or_else(|| ApiError::BadRequest(format!("unknown requisite '{key}'")))?;
        if def.is_archived {
            continue; // archived fields are read-only
        }
        match normalise(state, tenant_id, def, raw).await? {
            Some(v) => {
                sqlx::query(
                    "INSERT INTO project_field_values (project_id, field_id, value, updated_by, updated_at)
                     VALUES ($1, $2, $3, $4, now())
                     ON CONFLICT (project_id, field_id) DO UPDATE SET value = EXCLUDED.value,
                        updated_by = EXCLUDED.updated_by, updated_at = now()",
                )
                .bind(project_id)
                .bind(def.id)
                .bind(&v)
                .bind(user_id)
                .execute(&mut **tx)
                .await?;
                filled.insert(def.id);
            }
            None => {
                sqlx::query("DELETE FROM project_field_values WHERE project_id = $1 AND field_id = $2")
                    .bind(project_id)
                    .bind(def.id)
                    .execute(&mut **tx)
                    .await?;
                filled.remove(&def.id);
            }
        }
    }

    // The first directory field is the project's customer: keep the link in sync.
    if let Some(cust) = customer_def(&defs) {
        if patch.contains_key(&cust.key) {
            sqlx::query(
                "UPDATE projects SET company_id = (
                     SELECT (v.value ->> 'id')::uuid FROM project_field_values v WHERE v.project_id = $1 AND v.field_id = $2)
                  WHERE id = $1",
            )
            .bind(project_id)
            .bind(cust.id)
            .execute(&mut **tx)
            .await?;
        }
    }

    let missing: Vec<&str> = defs
        .iter()
        .filter(|d| d.is_required && !d.is_archived && !filled.contains(&d.id))
        .map(|d| d.label.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(ApiError::BadRequest(format!("required requisites are empty: {}", missing.join(", "))));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ValuesRequest {
    pub values: Map<String, Value>,
}

/// Update a project's requisites (project managers and tenant admins).
pub async fn update_values(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<Uuid>,
    Json(req): Json<ValuesRequest>,
) -> ApiResult<Json<Value>> {
    let role = project_role(&state, &user, project_id).await?;
    if role < ProjectRole::Manager {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    save_values(&state, &mut tx, user.tenant_id, user.user_id, project_id, &req.values).await?;
    tx.commit().await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "project.requisites_updated", Some("project"), Some(project_id), json!({ "keys": req.values.keys().collect::<Vec<_>>() }))
        .await;
    Ok(Json(json!({ "values": project_values(&state, project_id).await? })))
}
