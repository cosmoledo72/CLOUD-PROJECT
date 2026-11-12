//! Employee card (iteration 28б): personal and work data, requisites for
//! documents (fixed set plus fields the administrator defines), photo and
//! signature, groups, projects, access and security actions.
//!
//! Who sees what: colleagues see the work card and contacts (birthday as day
//! and month only); the employee and administrators also see the full date of
//! birth, requisites, custom fields, signature and security state. The
//! employee edits their own data and requisites; administrators edit anyone's,
//! plus access, groups and the login email.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use bytes::Bytes;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    state::AppState,
    storage::BlobStore,
};

const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;

#[derive(sqlx::FromRow)]
struct ProfileRow {
    id: Uuid,
    email: String,
    display_name: String,
    role: String,
    crm_role: String,
    finance_role: String,
    is_active: bool,
    totp_enabled: bool,
    invite_pending: bool,
    position: String,
    phone: String,
    last_name: String,
    first_name: String,
    middle_name: String,
    birth_date: Option<NaiveDate>,
    department: String,
    manager_id: Option<Uuid>,
    manager_name: Option<String>,
    hired_on: Option<NaiveDate>,
    phone_mobile: String,
    telegram: String,
    doc_short_name: String,
    doc_position: String,
    name_genitive: String,
    name_dative: String,
    nrs_number: String,
    qual_cert: String,
    power_of_attorney: String,
    avatar_at: Option<DateTime<Utc>>,
    signature_at: Option<DateTime<Utc>>,
    last_seen_at: Option<DateTime<Utc>>,
    hour_rate: Option<f64>,
    created_at: DateTime<Utc>,
}

/// May `user` edit the data of `id` (self or administrator)?
fn can_edit(user: &CurrentUser, id: Uuid) -> bool {
    user.user_id == id || user.is_admin()
}

/// Guests see only themselves and the people they work with is a project
/// question; on the card they see only their own.
fn require_view(user: &CurrentUser, id: Uuid) -> ApiResult<()> {
    if user.is_guest() && user.user_id != id {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

async fn load_row(state: &AppState, tenant_id: Uuid, id: Uuid) -> ApiResult<ProfileRow> {
    Ok(sqlx::query_as::<_, ProfileRow>(
        "SELECT u.id, u.email::text AS email, u.display_name, u.role, u.crm_role, u.finance_role, u.is_active, u.totp_enabled,
                u.invite_pending, u.position, u.phone, u.last_name, u.first_name, u.middle_name, u.birth_date, u.department,
                u.manager_id, m.display_name AS manager_name, u.hired_on, u.phone_mobile, u.telegram, u.doc_short_name,
                u.doc_position, u.name_genitive, u.name_dative, u.nrs_number, u.qual_cert, u.power_of_attorney,
                u.avatar_at, u.signature_at, u.last_seen_at, u.hour_rate::float8 AS hour_rate, u.created_at
           FROM users u LEFT JOIN users m ON m.id = u.manager_id
          WHERE u.id = $1 AND u.tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?)
}

/// `GET /api/users/{id}/profile`
pub async fn get(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    require_view(&user, id)?;
    let u = load_row(&state, user.tenant_id, id).await?;
    if !u.is_active && !user.is_admin() && user.user_id != id {
        return Err(ApiError::NotFound);
    }
    let own = can_edit(&user, id);
    let rate_visible = own || user.finance_view();
    // Name parts not filled in yet: suggest them from «Имя Фамилия».
    let (mut first, mut last) = (u.first_name.clone(), u.last_name.clone());
    if first.is_empty() && last.is_empty() && u.middle_name.is_empty() {
        let w: Vec<&str> = u.display_name.split_whitespace().collect();
        if w.len() == 2 {
            (first, last) = (w[0].to_string(), w[1].to_string());
        }
    }

    let groups: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT g.id, g.name FROM group_members gm JOIN groups g ON g.id = gm.group_id WHERE gm.user_id = $1 ORDER BY lower(g.name)",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    // Projects of the employee that the viewer may see as well.
    let projects: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT p.id, p.name, p.status, pm.role FROM project_members pm JOIN projects p ON p.id = pm.project_id
          WHERE pm.user_id = $1 AND p.tenant_id = $2
            AND ($3 OR EXISTS (SELECT 1 FROM project_members v WHERE v.project_id = p.id AND v.user_id = $4))
          ORDER BY p.status = 'closed', lower(p.name)",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(user.is_admin())
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;
    let (open_tasks, overdue_tasks): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE status <> 'done'), count(*) FILTER (WHERE status <> 'done' AND due_date < current_date)
           FROM tasks WHERE assignee_id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    let hours_month: Option<i64> = if own || user.finance_view() {
        Some(
            sqlx::query_scalar(
                "SELECT COALESCE(sum(te.minutes), 0)::bigint FROM time_entries te JOIN tenants t ON t.id = te.tenant_id
                  WHERE te.user_id = $1 AND date_trunc('month', te.work_date) = date_trunc('month', (now() AT TIME ZONE t.timezone)::date)",
            )
            .bind(id)
            .fetch_one(&state.db)
            .await?,
        )
    } else {
        None
    };

    let mut card = json!({
        "id": u.id, "email": u.email, "display_name": u.display_name, "role": u.role, "is_active": u.is_active,
        "position": u.position, "department": u.department, "phone": u.phone, "phone_mobile": u.phone_mobile, "telegram": u.telegram,
        "last_name": last, "first_name": first, "middle_name": u.middle_name,
        "manager_id": u.manager_id, "manager_name": u.manager_name, "hired_on": u.hired_on,
        "birthday": u.birth_date.map(|d| format!("{:02}.{:02}", d.day(), d.month())),
        "avatar_at": u.avatar_at, "last_seen_at": u.last_seen_at, "created_at": u.created_at,
        "crm_role": u.crm_role, "finance_role": u.finance_role, "totp_enabled": u.totp_enabled, "invite_pending": u.invite_pending,
    });
    if own {
        card["birth_date"] = json!(u.birth_date);
        for (k, v) in [
            ("doc_short_name", &u.doc_short_name),
            ("doc_position", &u.doc_position),
            ("name_genitive", &u.name_genitive),
            ("name_dative", &u.name_dative),
            ("nrs_number", &u.nrs_number),
            ("qual_cert", &u.qual_cert),
            ("power_of_attorney", &u.power_of_attorney),
        ] {
            card[k] = json!(v);
        }
        card["signature_at"] = json!(u.signature_at);
    }
    if rate_visible {
        card["hour_rate"] = json!(u.hour_rate);
    }
    let custom: Vec<Value> = if own {
        let rows: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
            "SELECT d.id, d.label, d.field_type, v.value FROM user_field_defs d
               LEFT JOIN user_field_values v ON v.field_id = d.id AND v.user_id = $2
              WHERE d.tenant_id = $1 ORDER BY d.sort_order, d.created_at",
        )
        .bind(user.tenant_id)
        .bind(id)
        .fetch_all(&state.db)
        .await?;
        rows.into_iter().map(|(fid, label, t, v)| json!({ "id": fid, "label": label, "field_type": t, "value": v.unwrap_or_default() })).collect()
    } else {
        vec![]
    };
    Ok(Json(json!({
        "user": card,
        "can_edit": own,
        "can_admin": user.is_admin(),
        "rate_visible": rate_visible,
        "groups": groups.into_iter().map(|(gid, name)| json!({ "id": gid, "name": name })).collect::<Vec<_>>(),
        "projects": projects.into_iter().map(|(pid, name, status, role)| json!({ "id": pid, "name": name, "status": status, "role": role })).collect::<Vec<_>>(),
        "stats": { "open_tasks": open_tasks, "overdue_tasks": overdue_tasks, "hours_month": hours_month },
        "custom": custom,
    })))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ProfileInput {
    pub display_name: Option<String>,
    pub last_name: Option<String>,
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    /// `""` clears.
    pub birth_date: Option<String>,
    pub position: Option<String>,
    pub department: Option<String>,
    /// `""` clears.
    pub manager_id: Option<String>,
    pub hired_on: Option<String>,
    pub phone: Option<String>,
    pub phone_mobile: Option<String>,
    pub telegram: Option<String>,
    pub doc_short_name: Option<String>,
    pub doc_position: Option<String>,
    pub name_genitive: Option<String>,
    pub name_dative: Option<String>,
    pub nrs_number: Option<String>,
    pub qual_cert: Option<String>,
    pub power_of_attorney: Option<String>,
    /// Custom requisites: field id → value (`""` clears).
    pub custom: Option<HashMap<Uuid, String>>,
}

fn text(v: &Option<String>, label: &str, max: usize) -> ApiResult<Option<String>> {
    match v {
        None => Ok(None),
        Some(s) => {
            let t = s.trim();
            if t.chars().count() > max {
                return Err(ApiError::BadRequest(format!("{label}: не длиннее {max} символов")));
            }
            if t.chars().any(|c| c.is_control() && c != '\n') {
                return Err(ApiError::BadRequest(format!("{label}: недопустимые символы")));
            }
            Ok(Some(t.to_string()))
        }
    }
}

/// Phone numbers: digits, spaces, `+ - ( )`, and an extension («доб.»).
pub fn valid_phone(p: &str) -> bool {
    let p = p.trim();
    if p.chars().count() > 40 {
        return false;
    }
    let has_ext = p.to_lowercase().contains("доб");
    let base = match p.to_lowercase().find("доб") {
        Some(i) => {
            let (a, b) = p.split_at(i);
            let ext = b.trim_start_matches(|c: char| c.is_alphabetic() || c == '.' || c == ' ');
            if !ext.chars().all(|c| c.is_ascii_digit()) || ext.is_empty() {
                return false;
            }
            a.trim_end_matches([' ', ','])
        }
        None => p,
    };
    base.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c)) && ((base.is_empty() && !has_ext) || base.chars().any(|c| c.is_ascii_digit()))
}

fn phone(v: &Option<String>, label: &str) -> ApiResult<Option<String>> {
    match v {
        None => Ok(None),
        Some(s) if valid_phone(s) => Ok(Some(s.trim().to_string())),
        Some(_) => Err(ApiError::BadRequest(format!("{label}: цифры, пробелы, «+», «-», скобки и «доб.», не длиннее 40 символов"))),
    }
}

fn date(v: &Option<String>, label: &str) -> ApiResult<Option<Option<NaiveDate>>> {
    match v.as_deref().map(str::trim) {
        None => Ok(None),
        Some("") => Ok(Some(None)),
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .filter(|d| d.year() >= 1900 && d.year() <= 2100)
            .map(|d| Some(Some(d)))
            .ok_or_else(|| ApiError::BadRequest(format!("{label}: дата в формате ГГГГ-ММ-ДД"))),
    }
}

/// `PUT /api/users/{id}/profile` — the employee or an administrator.
pub async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<ProfileInput>) -> ApiResult<Json<Value>> {
    if !can_edit(&user, id) {
        return Err(ApiError::Forbidden);
    }
    let cur = load_row(&state, user.tenant_id, id).await?;
    let last = text(&req.last_name, "Фамилия", 60)?.unwrap_or(cur.last_name.clone());
    let first = text(&req.first_name, "Имя", 60)?.unwrap_or(cur.first_name.clone());
    let middle = text(&req.middle_name, "Отчество", 60)?.unwrap_or(cur.middle_name.clone());
    let mut display = text(&req.display_name, "Имя в портале", 100)?.unwrap_or(cur.display_name.clone());
    if display.is_empty() {
        display = [first.as_str(), last.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
    }
    if display.is_empty() {
        return bad_request("Имя в портале: укажите, как вас показывать коллегам");
    }
    let birth = date(&req.birth_date, "Дата рождения")?.unwrap_or(cur.birth_date);
    if birth.is_some_and(|d| d > Utc::now().date_naive()) {
        return bad_request("Дата рождения: не позже сегодняшнего дня");
    }
    let hired = date(&req.hired_on, "В компании с")?.unwrap_or(cur.hired_on);
    let manager = match req.manager_id.as_deref().map(str::trim) {
        None => cur.manager_id,
        Some("") => None,
        Some(s) => {
            let m = Uuid::parse_str(s).map_err(|_| ApiError::BadRequest("Руководитель: неизвестный сотрудник".into()))?;
            if m == id {
                return bad_request("Руководитель: нельзя указать самого сотрудника");
            }
            let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND tenant_id = $2)")
                .bind(m)
                .bind(user.tenant_id)
                .fetch_one(&state.db)
                .await?;
            if !ok {
                return bad_request("Руководитель: неизвестный сотрудник");
            }
            Some(m)
        }
    };
    let telegram = match text(&req.telegram, "Telegram", 40)? {
        None => cur.telegram.clone(),
        Some(t) if t.is_empty() => t,
        Some(t) => {
            let name = t.trim_start_matches("https://t.me/").trim_start_matches('@');
            if name.len() < 4 || name.len() > 32 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return bad_request("Telegram: имя пользователя латиницей, например @ivanov");
            }
            format!("@{name}")
        }
    };
    let position = text(&req.position, "Должность", 100)?.unwrap_or(cur.position.clone());
    let department = text(&req.department, "Отдел", 100)?.unwrap_or(cur.department.clone());
    let work_phone = phone(&req.phone, "Рабочий телефон")?.unwrap_or(cur.phone.clone());
    let mobile = phone(&req.phone_mobile, "Мобильный")?.unwrap_or(cur.phone_mobile.clone());
    let short = text(&req.doc_short_name, "Фамилия и инициалы", 80)?.unwrap_or(cur.doc_short_name.clone());
    let doc_pos = text(&req.doc_position, "Должность в документах", 150)?.unwrap_or(cur.doc_position.clone());
    let gen = text(&req.name_genitive, "ФИО в родительном падеже", 150)?.unwrap_or(cur.name_genitive.clone());
    let dat = text(&req.name_dative, "ФИО в дательном падеже", 150)?.unwrap_or(cur.name_dative.clone());
    let nrs = text(&req.nrs_number, "Номер в НРС", 40)?.unwrap_or(cur.nrs_number.clone());
    let cert = text(&req.qual_cert, "Квалификационный аттестат", 200)?.unwrap_or(cur.qual_cert.clone());
    let poa = text(&req.power_of_attorney, "Доверенность", 300)?.unwrap_or(cur.power_of_attorney.clone());

    // Custom requisites.
    let mut custom: Vec<(Uuid, String)> = vec![];
    if let Some(map) = &req.custom {
        let defs: HashMap<Uuid, (String, String)> = sqlx::query_as::<_, (Uuid, String, String)>(
            "SELECT id, label, field_type FROM user_field_defs WHERE tenant_id = $1",
        )
        .bind(user.tenant_id)
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|(i, l, t)| (i, (l, t)))
        .collect();
        for (fid, v) in map {
            let (label, t) = defs.get(fid).ok_or_else(|| ApiError::BadRequest("неизвестное поле реквизитов".into()))?;
            let v = v.trim().to_string();
            let max = if t == "textarea" { 2000 } else { 300 };
            if v.chars().count() > max {
                return Err(ApiError::BadRequest(format!("{label}: не длиннее {max} символов")));
            }
            if !v.is_empty() {
                match t.as_str() {
                    "date" if NaiveDate::parse_from_str(&v, "%Y-%m-%d").is_err() => {
                        return Err(ApiError::BadRequest(format!("{label}: дата в формате ГГГГ-ММ-ДД")));
                    }
                    "number" if v.replace(',', ".").replace(' ', "").parse::<f64>().is_err() => {
                        return Err(ApiError::BadRequest(format!("{label}: число")));
                    }
                    _ => {}
                }
            }
            custom.push((*fid, v));
        }
    }

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE users SET display_name = $3, last_name = $4, first_name = $5, middle_name = $6, birth_date = $7, position = $8,
                department = $9, manager_id = $10, hired_on = $11, phone = $12, phone_mobile = $13, telegram = $14,
                doc_short_name = $15, doc_position = $16, name_genitive = $17, name_dative = $18, nrs_number = $19,
                qual_cert = $20, power_of_attorney = $21
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(user.tenant_id)
    .bind(&display)
    .bind(&last)
    .bind(&first)
    .bind(&middle)
    .bind(birth)
    .bind(&position)
    .bind(&department)
    .bind(manager)
    .bind(hired)
    .bind(&work_phone)
    .bind(&mobile)
    .bind(&telegram)
    .bind(&short)
    .bind(&doc_pos)
    .bind(&gen)
    .bind(&dat)
    .bind(&nrs)
    .bind(&cert)
    .bind(&poa)
    .execute(&mut *tx)
    .await?;
    for (fid, v) in &custom {
        if v.is_empty() {
            sqlx::query("DELETE FROM user_field_values WHERE user_id = $1 AND field_id = $2").bind(id).bind(fid).execute(&mut *tx).await?;
        } else {
            sqlx::query(
                "INSERT INTO user_field_values (user_id, field_id, value) VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, field_id) DO UPDATE SET value = EXCLUDED.value",
            )
            .bind(id)
            .bind(fid)
            .bind(v)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    if user.user_id != id {
        state.audit(user.tenant_id, Some(user.user_id), "user.profile_updated", Some("user"), Some(id), json!({})).await;
    }
    Ok(Json(json!({ "ok": true, "display_name": display, "telegram": telegram })))
}

#[derive(Deserialize)]
pub struct GroupsInput {
    pub group_ids: Vec<Uuid>,
}

/// `PUT /api/users/{id}/groups` — administrators.
pub async fn set_groups(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(req): Json<GroupsInput>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    load_row(&state, user.tenant_id, id).await?;
    let valid: i64 = sqlx::query_scalar("SELECT count(*) FROM groups WHERE tenant_id = $1 AND id = ANY($2)")
        .bind(user.tenant_id)
        .bind(&req.group_ids)
        .fetch_one(&state.db)
        .await?;
    let mut ids = req.group_ids.clone();
    ids.sort();
    ids.dedup();
    if valid as usize != ids.len() {
        return bad_request("неизвестная группа");
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM group_members gm USING groups g WHERE gm.group_id = g.id AND g.tenant_id = $1 AND gm.user_id = $2")
        .bind(user.tenant_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for g in &ids {
        sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)").bind(g).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "user.groups_changed", Some("user"), Some(id), json!({ "groups": ids.len() })).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Photo and signature
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Image {
    Avatar,
    Signature,
}

impl Image {
    fn column(self) -> &'static str {
        match self {
            Image::Avatar => "avatar",
            Image::Signature => "signature",
        }
    }
}

/// PNG, JPEG or WEBP by magic bytes.
fn image_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

async fn upload(state: &AppState, user: &CurrentUser, id: Uuid, kind: Image, mut multipart: Multipart) -> ApiResult<Json<Value>> {
    if !can_edit(user, id) {
        return Err(ApiError::Forbidden);
    }
    load_row(state, user.tenant_id, id).await?;
    let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? else {
        return bad_request("файл не передан");
    };
    let data = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if data.len() > MAX_IMAGE_BYTES {
        return Err(ApiError::TooLarge("изображение — не больше 2 МБ".into()));
    }
    let ct = image_type(&data).ok_or_else(|| ApiError::BadRequest("нужна картинка PNG, JPEG или WEBP".into()))?;
    if kind == Image::Signature && ct != "image/png" {
        return bad_request("подпись — PNG с прозрачным фоном");
    }
    let col = kind.column();
    let key = format!("{}/people/{}-{}-{}", user.tenant_id, id.simple(), col, Uuid::new_v4().simple());
    state.store.put(&key, data).await?;
    let old: Option<String> = sqlx::query_scalar(&format!(
        "UPDATE users u SET {col}_key = $3, {col}_at = now() FROM users o
          WHERE u.id = $1 AND u.tenant_id = $2 AND o.id = u.id RETURNING o.{col}_key"
    ))
    .bind(id)
    .bind(user.tenant_id)
    .bind(&key)
    .fetch_one(&state.db)
    .await?;
    if let Some(old) = old {
        let _ = state.store.delete(&old).await;
    }
    state.audit(user.tenant_id, Some(user.user_id), &format!("user.{col}_changed"), Some("user"), Some(id), json!({})).await;
    let at: DateTime<Utc> = sqlx::query_scalar(&format!("SELECT {col}_at FROM users WHERE id = $1")).bind(id).fetch_one(&state.db).await?;
    Ok(Json(json!({ "at": at })))
}

async fn remove(state: &AppState, user: &CurrentUser, id: Uuid, kind: Image) -> ApiResult<Json<Value>> {
    if !can_edit(user, id) {
        return Err(ApiError::Forbidden);
    }
    let col = kind.column();
    let old: Option<Option<String>> = sqlx::query_scalar(&format!(
        "UPDATE users u SET {col}_key = NULL, {col}_at = NULL FROM users o
          WHERE u.id = $1 AND u.tenant_id = $2 AND o.id = u.id RETURNING o.{col}_key"
    ))
    .bind(id)
    .bind(user.tenant_id)
    .fetch_optional(&state.db)
    .await?;
    let Some(old) = old else { return Err(ApiError::NotFound) };
    if let Some(k) = old {
        let _ = state.store.delete(&k).await;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn serve(state: &AppState, user: &CurrentUser, id: Uuid, kind: Image) -> ApiResult<Response> {
    if kind == Image::Signature && !can_edit(user, id) {
        return Err(ApiError::NotFound);
    }
    let col = kind.column();
    let key: Option<String> = sqlx::query_scalar::<_, Option<String>>(&format!("SELECT {col}_key FROM users WHERE id = $1 AND tenant_id = $2"))
        .bind(id)
        .bind(user.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .flatten();
    let key = key.ok_or(ApiError::NotFound)?;
    let data: Bytes = state.store.get(&key).await?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, image_type(&data).unwrap_or("application/octet-stream"))
        .header(header::CONTENT_LENGTH, data.len())
        // The URL carries the upload time (`?v=`), so a new photo is a new URL.
        .header(header::CACHE_CONTROL, "private, max-age=86400")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(data))
        .expect("valid response"))
}

pub async fn upload_avatar(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, m: Multipart) -> ApiResult<Json<Value>> {
    upload(&state, &user, id, Image::Avatar, m).await
}
pub async fn delete_avatar(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    remove(&state, &user, id, Image::Avatar).await
}
pub async fn avatar(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Response> {
    serve(&state, &user, id, Image::Avatar).await
}
pub async fn upload_signature(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, m: Multipart) -> ApiResult<Json<Value>> {
    upload(&state, &user, id, Image::Signature, m).await
}
pub async fn delete_signature(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    remove(&state, &user, id, Image::Signature).await
}
pub async fn signature(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> ApiResult<Response> {
    serve(&state, &user, id, Image::Signature).await
}

/// `GET /api/users/avatars` — who has a photo (id → upload time), so every
/// avatar on the page can show it.
pub async fn avatars(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(Uuid, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, avatar_at FROM users WHERE tenant_id = $1 AND avatar_at IS NOT NULL")
            .bind(user.tenant_id)
            .fetch_all(&state.db)
            .await?;
    let map: serde_json::Map<String, Value> = rows.into_iter().map(|(i, at)| (i.to_string(), json!(at.timestamp()))).collect();
    Ok(Json(json!({ "avatars": map })))
}

// ---------------------------------------------------------------------------
// Custom requisites (administrator)
// ---------------------------------------------------------------------------

/// `GET /api/user-fields`
pub async fn list_fields(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, label, field_type FROM user_field_defs WHERE tenant_id = $1 ORDER BY sort_order, created_at")
            .bind(user.tenant_id)
            .fetch_all(&state.db)
            .await?;
    Ok(Json(json!({ "fields": rows.into_iter().map(|(i, l, t)| json!({ "id": i, "label": l, "field_type": t })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
pub struct FieldInput {
    pub id: Option<Uuid>,
    pub label: String,
    pub field_type: String,
}

#[derive(Deserialize)]
pub struct FieldsInput {
    pub fields: Vec<FieldInput>,
}

/// `PUT /api/admin/user-fields` — the whole list in order; fields left out
/// are deleted with their values.
pub async fn save_fields(State(state): State<AppState>, user: CurrentUser, Json(req): Json<FieldsInput>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    if req.fields.len() > 40 {
        return bad_request("не больше 40 полей");
    }
    let mut seen = std::collections::HashSet::new();
    for f in &req.fields {
        let l = f.label.trim();
        if l.is_empty() || l.chars().count() > 120 {
            return bad_request("название поля — от 1 до 120 символов");
        }
        if !seen.insert(l.to_lowercase()) {
            return Err(ApiError::BadRequest(format!("поле «{l}» указано дважды")));
        }
        if !matches!(f.field_type.as_str(), "text" | "textarea" | "date" | "number") {
            return bad_request("тип поля: text, textarea, date или number");
        }
    }
    let mut tx = state.db.begin().await?;
    let keep: Vec<Uuid> = req.fields.iter().filter_map(|f| f.id).collect();
    sqlx::query("DELETE FROM user_field_defs WHERE tenant_id = $1 AND NOT (id = ANY($2))")
        .bind(user.tenant_id)
        .bind(&keep)
        .execute(&mut *tx)
        .await?;
    for (i, f) in req.fields.iter().enumerate() {
        match f.id {
            Some(fid) => {
                let n = sqlx::query("UPDATE user_field_defs SET label = $3, field_type = $4, sort_order = $5 WHERE id = $1 AND tenant_id = $2")
                    .bind(fid)
                    .bind(user.tenant_id)
                    .bind(f.label.trim())
                    .bind(&f.field_type)
                    .bind(i as i32)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                if n == 0 {
                    return Err(ApiError::NotFound);
                }
            }
            None => {
                sqlx::query("INSERT INTO user_field_defs (tenant_id, label, field_type, sort_order) VALUES ($1, $2, $3, $4)")
                    .bind(user.tenant_id)
                    .bind(f.label.trim())
                    .bind(&f.field_type)
                    .bind(i as i32)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    state.audit(user.tenant_id, Some(user.user_id), "user_fields.saved", None, None, json!({ "fields": req.fields.len() })).await;
    list_fields(State(state), user).await
}

// ---------------------------------------------------------------------------
// Last visit
// ---------------------------------------------------------------------------

/// Remember that the person used the portal; written at most every 5 minutes
/// per person.
pub fn touch(state: &AppState, user_id: Uuid) {
    static SEEN: OnceLock<Mutex<HashMap<Uuid, Instant>>> = OnceLock::new();
    let now = Instant::now();
    {
        let mut m = SEEN.get_or_init(Default::default).lock().expect("seen lock");
        if m.get(&user_id).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(300)) {
            return;
        }
        if m.len() > 10_000 {
            m.clear();
        }
        m.insert(user_id, now);
    }
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE users SET last_seen_at = now() WHERE id = $1").bind(user_id).execute(&db).await;
    });
}

/// Placeholders about the author of a generated document (`user.*`).
pub async fn author_values(state: &AppState, user: &CurrentUser) -> ApiResult<Vec<(String, String)>> {
    let u = load_row(state, user.tenant_id, user.user_id).await?;
    let full = [u.last_name.as_str(), u.first_name.as_str(), u.middle_name.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let short = if !u.doc_short_name.is_empty() { u.doc_short_name.clone() } else { short_name(&u.last_name, &u.first_name, &u.middle_name) };
    Ok(vec![
        ("user.full_name".into(), if full.is_empty() { u.display_name.clone() } else { full }),
        ("user.short_name".into(), short),
        ("user.position".into(), if u.doc_position.is_empty() { u.position.clone() } else { u.doc_position.clone() }),
        ("user.name_genitive".into(), u.name_genitive),
        ("user.name_dative".into(), u.name_dative),
        ("user.nrs".into(), u.nrs_number),
        ("user.qual_cert".into(), u.qual_cert),
        ("user.power_of_attorney".into(), u.power_of_attorney),
        ("user.phone".into(), u.phone),
    ])
}

/// «Петрова А. С.» from the parts of the name.
pub fn short_name(last: &str, first: &str, middle: &str) -> String {
    let init = |s: &str| s.chars().next().map(|c| format!("{c}.")).unwrap_or_default();
    let initials = [init(first), init(middle)].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ");
    match (last.is_empty(), initials.is_empty()) {
        (true, _) => String::new(),
        (false, true) => last.to_string(),
        (false, false) => format!("{last} {initials}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_and_short_names() {
        assert!(valid_phone("+7 (4242) 49-00-12, доб. 214"));
        assert!(valid_phone("+7 914 123-45-67"));
        assert!(valid_phone(""));
        assert!(!valid_phone("cosmoledo@gmail.com"));
        assert!(!valid_phone("доб. 12"));
        assert!(!valid_phone("+7 914, доб. abc"));
        assert_eq!(short_name("Петрова", "Анна", "Сергеевна"), "Петрова А. С.");
        assert_eq!(short_name("Петрова", "Анна", ""), "Петрова А.");
        assert_eq!(short_name("", "Анна", ""), "");
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nxxxx"), Some("image/png"));
        assert_eq!(image_type(b"GIF89a"), None);
    }
}
