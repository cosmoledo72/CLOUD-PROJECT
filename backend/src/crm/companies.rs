//! Counterparties (companies) and their contact persons.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{check_user, company_visible_sql, require_edit, validate, visible_company};
use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Input and validation
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default, Clone)]
pub struct CompanyInput {
    pub name: String,
    #[serde(default)]
    pub full_name: String,
    pub inn: Option<String>,
    pub kpp: Option<String>,
    pub ogrn: Option<String>,
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
    pub bank_name: String,
    pub bik: Option<String>,
    pub account: Option<String>,
    pub corr_account: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub responsible_id: Option<Uuid>,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    pub group_id: Option<Uuid>,
}

fn default_visibility() -> String {
    "all".into()
}

/// Trim, drop spaces inside codes, turn empty strings into `None`.
fn code(v: &Option<String>) -> Option<String> {
    v.as_ref().map(|s| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()).filter(|s| !s.is_empty())
}

/// Validated and normalised company data.
pub fn clean(mut c: CompanyInput) -> ApiResult<CompanyInput> {
    c.name = c.name.trim().to_string();
    if c.name.is_empty() || c.name.chars().count() > 300 {
        return bad_request("company name is required (up to 300 chars)");
    }
    c.inn = code(&c.inn);
    c.kpp = code(&c.kpp);
    c.ogrn = code(&c.ogrn);
    c.bik = code(&c.bik);
    c.account = code(&c.account);
    c.corr_account = code(&c.corr_account);
    if let Some(v) = &c.inn {
        if !validate::inn_valid(v) {
            return bad_request("ИНН: неверная длина или контрольная сумма");
        }
    }
    if let Some(v) = &c.kpp {
        if !validate::kpp_valid(v) {
            return bad_request("КПП: ожидается 9 символов");
        }
    }
    if let Some(v) = &c.ogrn {
        if !validate::ogrn_valid(v) {
            return bad_request("ОГРН/ОГРНИП: неверная длина или контрольная сумма");
        }
    }
    if let Some(v) = &c.bik {
        if !validate::bik_valid(v) {
            return bad_request("БИК: ожидается 9 цифр");
        }
    }
    for (acc, corr, label) in [(&c.account, false, "Расчётный счёт"), (&c.corr_account, true, "Корр. счёт")] {
        if let Some(a) = acc {
            match &c.bik {
                Some(bik) if validate::account_valid(bik, a, corr) => {}
                Some(_) => return bad_request(format!("{label}: не сходится контрольный ключ с БИК")),
                None => return bad_request(format!("{label}: укажите БИК банка")),
            }
        }
    }
    if !matches!(c.visibility.as_str(), "all" | "responsible" | "group") {
        return bad_request("visibility must be all, responsible or group");
    }
    if c.visibility == "group" && c.group_id.is_none() {
        return bad_request("choose a group for group visibility");
    }
    if c.visibility != "group" {
        c.group_id = None;
    }
    for s in [&mut c.full_name, &mut c.legal_address, &mut c.postal_address, &mut c.phone, &mut c.email, &mut c.website,
              &mut c.director, &mut c.bank_name, &mut c.notes] {
        *s = s.trim().to_string();
    }
    c.tags = c.tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    c.tags.sort();
    c.tags.dedup();
    Ok(c)
}

async fn check_group(state: &AppState, tenant_id: Uuid, group_id: Option<Uuid>) -> ApiResult<()> {
    if let Some(g) = group_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM groups WHERE id = $1 AND tenant_id = $2)")
            .bind(g)
            .bind(tenant_id)
            .fetch_one(&state.db)
            .await?;
        if !ok {
            return bad_request("unknown group");
        }
    }
    Ok(())
}

fn duplicate_error(e: sqlx::Error) -> ApiError {
    match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("контрагент с таким ИНН и КПП уже есть".into()),
        other => other,
    }
}

/// Insert a validated company; used by the API and the importer.
pub async fn insert_company(state: &AppState, user: &CurrentUser, c: &CompanyInput) -> Result<Uuid, ApiError> {
    sqlx::query_scalar(
        "INSERT INTO crm_companies (tenant_id, name, full_name, inn, kpp, ogrn, legal_address, postal_address, phone, email,
                                    website, director, bank_name, bik, account, corr_account, notes, tags, responsible_id,
                                    visibility, group_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)
         RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(&c.name)
    .bind(&c.full_name)
    .bind(&c.inn)
    .bind(&c.kpp)
    .bind(&c.ogrn)
    .bind(&c.legal_address)
    .bind(&c.postal_address)
    .bind(&c.phone)
    .bind(&c.email)
    .bind(&c.website)
    .bind(&c.director)
    .bind(&c.bank_name)
    .bind(&c.bik)
    .bind(&c.account)
    .bind(&c.corr_account)
    .bind(&c.notes)
    .bind(&c.tags)
    .bind(c.responsible_id)
    .bind(&c.visibility)
    .bind(c.group_id)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(duplicate_error)
}

// ---------------------------------------------------------------------------
// Companies
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct CompanyListRow {
    id: Uuid,
    name: String,
    inn: Option<String>,
    kpp: Option<String>,
    phone: String,
    email: String,
    tags: Vec<String>,
    visibility: String,
    responsible_id: Option<Uuid>,
    responsible_name: Option<String>,
    deals_open: i64,
    deals_amount: f64,
    projects: i64,
    last_activity_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub responsible_id: Option<Uuid>,
    pub tag: Option<String>,
}

pub async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{}%", s.to_lowercase()));
    let rows = sqlx::query_as::<_, CompanyListRow>(&format!(
        "SELECT c.id, c.name, c.inn, c.kpp, c.phone, c.email, c.tags, c.visibility, c.responsible_id,
                u.display_name AS responsible_name,
                (SELECT count(*) FROM crm_deals d JOIN crm_stages s ON s.id = d.stage_id
                  WHERE d.company_id = c.id AND s.kind = 'open') AS deals_open,
                (SELECT COALESCE(sum(d.amount), 0)::float8 FROM crm_deals d JOIN crm_stages s ON s.id = d.stage_id
                  WHERE d.company_id = c.id AND s.kind = 'open') AS deals_amount,
                (SELECT count(*) FROM projects p WHERE p.company_id = c.id) AS projects,
                (SELECT max(a.happened_at) FROM crm_activities a WHERE a.company_id = c.id) AS last_activity_at
           FROM crm_companies c LEFT JOIN users u ON u.id = c.responsible_id
          WHERE c.tenant_id = $1 AND {vis}
            AND ($4::text IS NULL OR lower(c.name) LIKE $4 OR lower(c.full_name) LIKE $4 OR c.inn LIKE $4
                 OR lower(c.email) LIKE $4 OR c.phone LIKE $4)
            AND ($5::uuid IS NULL OR c.responsible_id = $5)
            AND ($6::text IS NULL OR $6 = ANY(c.tags))
          ORDER BY lower(c.name)
          LIMIT 1000",
        vis = company_visible_sql("c", 2, 3)
    ))
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.crm_manager())
    .bind(search)
    .bind(q.responsible_id)
    .bind(q.tag)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "companies": rows })))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CompanyRow {
    pub id: Uuid,
    pub name: String,
    pub full_name: String,
    pub inn: Option<String>,
    pub kpp: Option<String>,
    pub ogrn: Option<String>,
    pub legal_address: String,
    pub postal_address: String,
    pub phone: String,
    pub email: String,
    pub website: String,
    pub director: String,
    pub bank_name: String,
    pub bik: Option<String>,
    pub account: Option<String>,
    pub corr_account: Option<String>,
    pub notes: String,
    pub tags: Vec<String>,
    pub responsible_id: Option<Uuid>,
    pub responsible_name: Option<String>,
    pub visibility: String,
    pub group_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ContactRow {
    pub id: Uuid,
    pub company_id: Option<Uuid>,
    pub company_name: Option<String>,
    pub last_name: String,
    pub first_name: String,
    pub middle_name: String,
    pub position: String,
    pub phone: String,
    pub email: String,
    pub notes: String,
}

const CONTACT_SELECT: &str = "SELECT ct.id, ct.company_id, c.name AS company_name, ct.last_name, ct.first_name, ct.middle_name,
        ct.position, ct.phone, ct.email, ct.notes
   FROM crm_contacts ct LEFT JOIN crm_companies c ON c.id = ct.company_id";

#[derive(Serialize, sqlx::FromRow)]
struct ProjectLink {
    id: Uuid,
    name: String,
    status: String,
}

pub async fn get(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (responsible, created_by) = visible_company(&state, &user, id).await?;
    let company = sqlx::query_as::<_, CompanyRow>(
        "SELECT c.id, c.name, c.full_name, c.inn, c.kpp, c.ogrn, c.legal_address, c.postal_address, c.phone, c.email, c.website,
                c.director, c.bank_name, c.bik, c.account, c.corr_account, c.notes, c.tags, c.responsible_id,
                u.display_name AS responsible_name, c.visibility, c.group_id, c.created_by, c.created_at
           FROM crm_companies c LEFT JOIN users u ON u.id = c.responsible_id WHERE c.id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    let contacts = sqlx::query_as::<_, ContactRow>(&format!("{CONTACT_SELECT} WHERE ct.company_id = $1 ORDER BY ct.last_name, ct.first_name"))
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let projects = sqlx::query_as::<_, ProjectLink>("SELECT id, name, status FROM projects WHERE company_id = $1 ORDER BY created_at DESC")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let deals = super::deals::deals_of_company(&state, &user, id).await?;
    let activities = super::deals::activities(&state, "a.company_id = $1", id).await?;
    Ok(Json(json!({
        "company": company,
        "contacts": contacts,
        "deals": deals,
        "projects": projects,
        "activities": activities,
        "can_edit": super::can_edit(&user, responsible, created_by),
    })))
}

pub async fn create(State(state): State<AppState>, user: CurrentUser, Json(input): Json<CompanyInput>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let mut c = clean(input)?;
    if c.responsible_id.is_none() {
        c.responsible_id = Some(user.user_id);
    }
    check_user(&state, user.tenant_id, c.responsible_id).await?;
    check_group(&state, user.tenant_id, c.group_id).await?;
    let id = insert_company(&state, &user, &c).await?;
    state.audit(user.tenant_id, Some(user.user_id), "crm.company_created", None, Some(id), json!({ "name": c.name })).await;
    notify_responsible(&state, &user, id, &c.name, c.responsible_id).await;
    Ok(Json(json!({ "id": id })))
}

/// Full update of a company card.
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<CompanyInput>,
) -> ApiResult<Json<Value>> {
    let (responsible, created_by) = visible_company(&state, &user, id).await?;
    require_edit(&user, responsible, created_by)?;
    let c = clean(input)?;
    // Only managers may hand a company over to someone else.
    if c.responsible_id != responsible && !user.crm_manager() && c.responsible_id != Some(user.user_id) {
        return Err(ApiError::Forbidden);
    }
    check_user(&state, user.tenant_id, c.responsible_id).await?;
    check_group(&state, user.tenant_id, c.group_id).await?;
    sqlx::query(
        "UPDATE crm_companies SET name = $2, full_name = $3, inn = $4, kpp = $5, ogrn = $6, legal_address = $7,
                postal_address = $8, phone = $9, email = $10, website = $11, director = $12, bank_name = $13, bik = $14,
                account = $15, corr_account = $16, notes = $17, tags = $18, responsible_id = $19, visibility = $20,
                group_id = $21, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&c.name)
    .bind(&c.full_name)
    .bind(&c.inn)
    .bind(&c.kpp)
    .bind(&c.ogrn)
    .bind(&c.legal_address)
    .bind(&c.postal_address)
    .bind(&c.phone)
    .bind(&c.email)
    .bind(&c.website)
    .bind(&c.director)
    .bind(&c.bank_name)
    .bind(&c.bik)
    .bind(&c.account)
    .bind(&c.corr_account)
    .bind(&c.notes)
    .bind(&c.tags)
    .bind(c.responsible_id)
    .bind(&c.visibility)
    .bind(c.group_id)
    .execute(&state.db)
    .await
    .map_err(duplicate_error)?;
    state.audit(user.tenant_id, Some(user.user_id), "crm.company_updated", None, Some(id), json!({ "name": c.name })).await;
    if c.responsible_id != responsible {
        notify_responsible(&state, &user, id, &c.name, c.responsible_id).await;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn notify_responsible(state: &AppState, user: &CurrentUser, id: Uuid, name: &str, to: Option<Uuid>) {
    let Some(to) = to else { return };
    crate::notify::notify(
        state,
        user.tenant_id,
        Some(user.user_id),
        &[to],
        crate::notify::Note {
            kind: "company_assigned",
            title: format!("Вы ответственный за контрагента {}", crate::mail::quoted(name)),
            body: format!("Назначил(а): {}", user.display_name),
            link: format!("/crm/companies/{id}"),
            entity_type: Some("company"),
            entity_id: Some(id),
            ..Default::default()
        },
    )
    .await;
}

pub async fn delete(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let (responsible, created_by) = visible_company(&state, &user, id).await?;
    require_edit(&user, responsible, created_by)?;
    let name: String = sqlx::query_scalar("DELETE FROM crm_companies WHERE id = $1 RETURNING name").bind(id).fetch_one(&state.db).await?;
    // Project requisites keep the customer's name but lose the directory link.
    sqlx::query(
        "UPDATE project_field_values SET value = jsonb_set(value, '{id}', 'null')
          WHERE jsonb_typeof(value) = 'object' AND value ->> 'id' = $1::text",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    state.audit(user.tenant_id, Some(user.user_id), "crm.company_deleted", None, Some(id), json!({ "name": name })).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct InnQuery {
    pub inn: String,
    pub kpp: Option<String>,
    pub exclude: Option<Uuid>,
}

/// Duplicate check while typing an INN. Hidden companies are reported without details.
pub async fn check_inn(State(state): State<AppState>, user: CurrentUser, Query(q): Query<InnQuery>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let inn: String = q.inn.chars().filter(char::is_ascii_digit).collect();
    let valid = validate::inn_valid(&inn);
    let rows: Vec<(Uuid, String, Option<String>, bool)> = sqlx::query_as(&format!(
        "SELECT c.id, c.name, c.kpp, {vis} FROM crm_companies c
          WHERE c.tenant_id = $1 AND c.inn = $4 AND ($5::uuid IS NULL OR c.id <> $5)",
        vis = company_visible_sql("c", 2, 3)
    ))
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.crm_manager())
    .bind(&inn)
    .bind(q.exclude)
    .fetch_all(&state.db)
    .await?;
    let kpp = code(&q.kpp);
    let matches: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, row_kpp, visible)| {
            let same_kpp = row_kpp == kpp;
            if visible {
                json!({ "id": id, "name": name, "kpp": row_kpp, "same_kpp": same_kpp })
            } else {
                json!({ "id": null, "name": "скрытый контрагент другого сотрудника", "kpp": row_kpp, "same_kpp": same_kpp })
            }
        })
        .collect();
    Ok(Json(json!({ "valid": valid, "matches": matches })))
}

// ---------------------------------------------------------------------------
// Contacts
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ContactInput {
    pub company_id: Option<Uuid>,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub middle_name: String,
    #[serde(default)]
    pub position: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub notes: String,
}

/// Contacts are editable when their company is editable (or they have none).
async fn contact_access(state: &AppState, user: &CurrentUser, company_id: Option<Uuid>, edit: bool) -> ApiResult<()> {
    user.require_crm()?;
    if let Some(cid) = company_id {
        let (r, c) = visible_company(state, user, cid).await?;
        if edit {
            require_edit(user, r, c)?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ContactQuery {
    pub q: Option<String>,
    pub company_id: Option<Uuid>,
}

pub async fn list_contacts(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ContactQuery>) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{}%", s.to_lowercase()));
    let rows = sqlx::query_as::<_, ContactRow>(&format!(
        "{CONTACT_SELECT}
          WHERE ct.tenant_id = $1 AND (ct.company_id IS NULL OR {vis})
            AND ($4::text IS NULL OR lower(ct.last_name || ' ' || ct.first_name || ' ' || ct.middle_name) LIKE $4
                 OR lower(ct.email) LIKE $4 OR ct.phone LIKE $4 OR lower(c.name) LIKE $4)
            AND ($5::uuid IS NULL OR ct.company_id = $5)
          ORDER BY ct.last_name, ct.first_name LIMIT 1000",
        vis = company_visible_sql("c", 2, 3)
    ))
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.crm_manager())
    .bind(search)
    .bind(q.company_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "contacts": rows })))
}

fn clean_contact(mut c: ContactInput) -> ApiResult<ContactInput> {
    for s in [&mut c.last_name, &mut c.first_name, &mut c.middle_name, &mut c.position, &mut c.phone, &mut c.email, &mut c.notes] {
        *s = s.trim().to_string();
    }
    if c.last_name.is_empty() && c.first_name.is_empty() {
        return bad_request("last or first name is required");
    }
    Ok(c)
}

pub async fn create_contact(State(state): State<AppState>, user: CurrentUser, Json(input): Json<ContactInput>) -> ApiResult<Json<Value>> {
    let c = clean_contact(input)?;
    contact_access(&state, &user, c.company_id, true).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO crm_contacts (tenant_id, company_id, last_name, first_name, middle_name, position, phone, email, notes, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(user.tenant_id)
    .bind(c.company_id)
    .bind(&c.last_name)
    .bind(&c.first_name)
    .bind(&c.middle_name)
    .bind(&c.position)
    .bind(&c.phone)
    .bind(&c.email)
    .bind(&c.notes)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({ "id": id })))
}

async fn contact_company(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<Option<Uuid>> {
    sqlx::query_scalar::<_, Option<Uuid>>("SELECT company_id FROM crm_contacts WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn update_contact(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ContactInput>,
) -> ApiResult<Json<Value>> {
    let current = contact_company(&state, &user, id).await?;
    contact_access(&state, &user, current, true).await?;
    let c = clean_contact(input)?;
    if c.company_id != current {
        contact_access(&state, &user, c.company_id, true).await?;
    }
    sqlx::query(
        "UPDATE crm_contacts SET company_id = $2, last_name = $3, first_name = $4, middle_name = $5, position = $6,
                phone = $7, email = $8, notes = $9 WHERE id = $1",
    )
    .bind(id)
    .bind(c.company_id)
    .bind(&c.last_name)
    .bind(&c.first_name)
    .bind(&c.middle_name)
    .bind(&c.position)
    .bind(&c.phone)
    .bind(&c.email)
    .bind(&c.notes)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_contact(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let current = contact_company(&state, &user, id).await?;
    contact_access(&state, &user, current, true).await?;
    sqlx::query("DELETE FROM crm_contacts WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> CompanyInput {
        CompanyInput { name: " ПАО Сбербанк ".into(), visibility: "all".into(), ..Default::default() }
    }

    #[test]
    fn normalises_and_validates_codes() {
        let c = clean(CompanyInput {
            inn: Some(" 7707 083 893 ".into()),
            kpp: Some("773601001".into()),
            ogrn: Some("1027700132195".into()),
            bik: Some("044525225".into()),
            account: Some("40702810938000000001".into()),
            corr_account: Some("30101810400000000225".into()),
            tags: vec![" банк ".into(), "банк".into(), "".into()],
            ..input()
        })
        .unwrap();
        assert_eq!(c.name, "ПАО Сбербанк");
        assert_eq!(c.inn.as_deref(), Some("7707083893"));
        assert_eq!(c.tags, vec!["банк"]);
    }

    #[test]
    fn rejects_bad_codes() {
        assert!(clean(CompanyInput { inn: Some("7707083894".into()), ..input() }).is_err());
        assert!(clean(CompanyInput { account: Some("40702810938000000001".into()), ..input() }).is_err(), "account needs BIK");
        assert!(clean(CompanyInput { bik: Some("044525225".into()), account: Some("40702810038000000001".into()), ..input() }).is_err());
        assert!(clean(CompanyInput { visibility: "group".into(), ..input() }).is_err());
        assert!(clean(CompanyInput { name: "  ".into(), ..input() }).is_err());
        assert!(clean(CompanyInput { inn: Some("".into()), ..input() }).unwrap().inn.is_none());
    }
}
