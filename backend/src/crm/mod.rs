//! CRM: counterparties (companies), contacts, sales pipeline, deals and
//! interaction history, linked to projects.
//!
//! Access model:
//! - `users.crm_role`: `none` (no CRM), `user`, `manager`; guests never have CRM;
//!   portal owners/admins act as managers.
//! - A company is visible to managers, to everyone when `visibility = all`,
//!   to its responsible person, and to members of `group_id` when
//!   `visibility = group` (SQL function `crm_company_visible`).
//! - Contacts and deals inherit visibility from their company; records
//!   without a company are visible to all CRM users; a deal is always visible
//!   to its responsible person.
//! - Editing: managers, the responsible person or the creator.

pub mod companies;
pub mod deals;
pub mod documents;
pub mod import;
pub mod tasks;
pub mod validate;

use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

/// SQL condition: company alias `c` is visible to user `$U` with manager flag `$M`.
pub fn company_visible_sql(alias: &str, user_param: usize, manager_param: usize) -> String {
    format!(
        "crm_company_visible({a}.visibility, {a}.responsible_id, {a}.group_id, ${u}, ${m})",
        a = alias,
        u = user_param,
        m = manager_param
    )
}

/// Load a company the user may see; returns (responsible_id, created_by).
pub async fn visible_company(state: &AppState, user: &CurrentUser, id: Uuid) -> ApiResult<(Option<Uuid>, Option<Uuid>)> {
    user.require_crm()?;
    sqlx::query_as(&format!(
        "SELECT c.responsible_id, c.created_by FROM crm_companies c WHERE c.id = $1 AND c.tenant_id = $2 AND {}",
        company_visible_sql("c", 3, 4)
    ))
    .bind(id)
    .bind(user.tenant_id)
    .bind(user.user_id)
    .bind(user.crm_manager())
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)
}

/// Managers, the responsible person and the creator may edit a record.
pub fn can_edit(user: &CurrentUser, responsible: Option<Uuid>, created_by: Option<Uuid>) -> bool {
    user.crm_manager() || responsible == Some(user.user_id) || created_by == Some(user.user_id)
}

pub fn require_edit(user: &CurrentUser, responsible: Option<Uuid>, created_by: Option<Uuid>) -> ApiResult<()> {
    if can_edit(user, responsible, created_by) { Ok(()) } else { Err(ApiError::Forbidden) }
}

/// A user id must belong to the tenant (for responsible persons).
pub async fn check_user(state: &AppState, tenant_id: Uuid, user_id: Option<Uuid>) -> ApiResult<()> {
    if let Some(id) = user_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND tenant_id = $2 AND is_active)")
            .bind(id)
            .bind(tenant_id)
            .fetch_one(&state.db)
            .await?;
        if !ok {
            return Err(ApiError::BadRequest("unknown responsible user".into()));
        }
    }
    Ok(())
}

/// Record a system entry in the interaction history.
pub async fn system_activity(state: &AppState, user: &CurrentUser, company_id: Option<Uuid>, deal_id: Option<Uuid>, body: String) {
    let res = sqlx::query(
        "INSERT INTO crm_activities (tenant_id, company_id, deal_id, kind, body, author_id) VALUES ($1, $2, $3, 'system', $4, $5)",
    )
    .bind(user.tenant_id)
    .bind(company_id)
    .bind(deal_id)
    .bind(body)
    .bind(user.user_id)
    .execute(&state.db)
    .await;
    if let Err(e) = res {
        tracing::warn!(error = ?e, "failed to write CRM system activity");
    }
}
