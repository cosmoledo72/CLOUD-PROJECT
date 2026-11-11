//! Portal modules that an administrator can switch off (Администрирование →
//! Модули). A disabled module disappears from the interface, its API answers
//! 403 `module_disabled`, and it sends no notifications. Data is never deleted:
//! switching the module back on restores everything.

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Serialize)]
pub struct Module {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Works only while the parent module is on.
    pub parent: Option<&'static str>,
}

pub const MODULES: &[Module] = &[
    Module { key: "projects", label: "Проекты", description: "Проекты, задачи, раздел «Задачи», файлы проектов", parent: None },
    Module { key: "planning", label: "Планирование", description: "Диаграмма Ганта, связи задач, автосдвиг, критический путь, базовый план", parent: Some("projects") },
    Module { key: "time", label: "Учёт времени", description: "Записи времени по задачам", parent: Some("projects") },
    Module { key: "templates", label: "Шаблоны документов", description: "«Документ из шаблона» в проектах", parent: Some("projects") },
    Module { key: "storage", label: "Хранилище", description: "Раздел «Хранилище»: мои и общие документы, доступные мне, корзина", parent: None },
    Module { key: "kb", label: "База знаний", description: "Раздел «База знаний»", parent: None },
    Module { key: "chats", label: "Чаты", description: "Раздел «Чаты» и каналы проектов", parent: None },
    Module { key: "crm", label: "CRM", description: "Контрагенты, сделки, дела, документы продаж", parent: None },
    Module { key: "finance", label: "Финансы", description: "Платежи, аналитика, дебиторская задолженность, деньги проектов", parent: None },
    Module { key: "team", label: "Команда", description: "Справочник сотрудников", parent: None },
];

pub fn is_known(key: &str) -> bool {
    MODULES.iter().any(|m| m.key == key)
}

/// Is `key` on, given the portal's list of switched-off modules (a child is
/// off when its parent is off)?
pub fn enabled(disabled: &[String], key: &str) -> bool {
    let off = |k: &str| disabled.iter().any(|d| d == k);
    match MODULES.iter().find(|m| m.key == key) {
        Some(m) => !off(m.key) && m.parent.is_none_or(|p| !off(p)),
        None => true,
    }
}

/// Keys of the modules that are on.
pub fn enabled_keys(disabled: &[String]) -> Vec<&'static str> {
    MODULES.iter().filter(|m| enabled(disabled, m.key)).map(|m| m.key).collect()
}

/// Which module an API path belongs to (paths are relative to `/api`).
/// Project documents (`/folders/{id}`, `/files/{id}`) stay available without
/// the storage module: they are reached from the project's "Файлы" tab.
pub fn module_of(method: &axum::http::Method, path: &str) -> Option<&'static str> {
    let p = path.trim_end_matches('/');
    let seg: Vec<&str> = p.trim_start_matches('/').split('/').collect();
    let s = |i: usize| seg.get(i).copied().unwrap_or("");
    match s(0) {
        "projects" => Some(match s(2) {
            "finance" => "finance",
            "reschedule" | "baseline" => "planning",
            "templates" | "generate" => "templates",
            _ => "projects",
        }),
        "tasks" => Some(match s(2) {
            "dependencies" => "planning",
            "time" => "time",
            _ => "projects",
        }),
        "task-labels" | "project-labels" | "checklist-items" | "task-comments" | "project-fields" => Some("projects"),
        "kb" => Some("kb"),
        "chats" | "chat-messages" => Some("chats"),
        "crm" => Some(if s(1) == "deals" && s(3) == "project" && method == axum::http::Method::POST { "projects" } else { "crm" }),
        "team" => Some("team"),
        "finance" => Some("finance"),
        "shared" | "trash" => Some("storage"),
        "folders" if s(1) == "roots" => Some("storage"),
        _ => None,
    }
}

fn disabled_response(key: &str) -> Response {
    let label = MODULES.iter().find(|m| m.key == key).map(|m| m.label).unwrap_or(key);
    (
        axum::http::StatusCode::FORBIDDEN,
        Json(json!({ "error": "module_disabled", "module": key, "message": format!("Модуль «{label}» отключён администратором портала") })),
    )
        .into_response()
}

/// Middleware on `/api`: rejects requests to a switched-off module. Requests
/// outside any module (auth, admin, public pages) pass untouched.
pub async fn guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let path = req.uri().path().strip_prefix("/api").unwrap_or(req.uri().path()).to_string();
    let Some(module) = module_of(req.method(), &path) else {
        return next.run(req).await;
    };
    let (mut parts, body) = req.into_parts();
    let user = match <CurrentUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(&mut parts, &state).await {
        Ok(u) => u,
        // Let the handler produce its usual 401.
        Err(_) => return next.run(Request::from_parts(parts, body)).await,
    };
    let crm_project = module == "projects" && path.starts_with("/crm/");
    if !user.module_on(module) || (crm_project && !user.module_on("crm")) {
        return disabled_response(if crm_project && user.module_on("projects") { "crm" } else { module });
    }
    next.run(Request::from_parts(parts, body)).await
}

// ---------------------------------------------------------------------------
// Admin API
// ---------------------------------------------------------------------------

pub async fn load_disabled(state: &AppState, tenant_id: uuid::Uuid) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT disabled_modules FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(&state.db).await?)
}

pub async fn get_modules(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let disabled = load_disabled(&state, user.tenant_id).await?;
    let list: Vec<Value> = MODULES
        .iter()
        .map(|m| {
            json!({
                "key": m.key, "label": m.label, "description": m.description, "parent": m.parent,
                "switched_on": !disabled.iter().any(|d| d == m.key),
                "enabled": enabled(&disabled, m.key),
            })
        })
        .collect();
    Ok(Json(json!({ "modules": list })))
}

#[derive(Deserialize)]
pub struct SetModules {
    /// Keys of the modules to switch off; every other module is on.
    pub disabled: Vec<String>,
}

pub async fn set_modules(State(state): State<AppState>, user: CurrentUser, Json(req): Json<SetModules>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    let mut disabled: Vec<String> = Vec::new();
    for k in req.disabled {
        if !is_known(&k) {
            return Err(ApiError::BadRequest(format!("unknown module '{k}'")));
        }
        if !disabled.contains(&k) {
            disabled.push(k);
        }
    }
    // Keep the catalogue order for a stable audit record.
    disabled.sort_by_key(|k| MODULES.iter().position(|m| m.key == k));
    let before = load_disabled(&state, user.tenant_id).await?;
    sqlx::query("UPDATE tenants SET disabled_modules = $2 WHERE id = $1").bind(user.tenant_id).bind(&disabled).execute(&state.db).await?;
    if before != disabled {
        state
            .audit(user.tenant_id, Some(user.user_id), "portal.modules", Some("tenant"), Some(user.tenant_id), json!({ "disabled": disabled, "before": before }))
            .await;
    }
    get_modules(State(state), CurrentUser { disabled_modules: disabled, ..user }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    #[test]
    fn children_follow_parents() {
        let d = vec!["projects".to_string()];
        assert!(!enabled(&d, "projects") && !enabled(&d, "planning") && !enabled(&d, "time"));
        assert!(enabled(&d, "crm") && enabled(&d, "storage"));
        let d = vec!["planning".to_string()];
        assert!(enabled(&d, "projects") && !enabled(&d, "planning") && enabled(&d, "time"));
        assert_eq!(enabled_keys(&[]).len(), MODULES.len());
    }

    #[test]
    fn paths_map_to_modules() {
        let g = Method::GET;
        assert_eq!(module_of(&g, "/projects"), Some("projects"));
        assert_eq!(module_of(&Method::POST, "/projects/x/baseline"), Some("planning"));
        assert_eq!(module_of(&Method::POST, "/projects/x/generate"), Some("templates"));
        assert_eq!(module_of(&g, "/tasks/x/time"), Some("time"));
        assert_eq!(module_of(&Method::POST, "/tasks/x/dependencies/y"), Some("planning"));
        assert_eq!(module_of(&g, "/tasks"), Some("projects"));
        assert_eq!(module_of(&g, "/folders/roots"), Some("storage"));
        assert_eq!(module_of(&g, "/folders/abc"), None);
        assert_eq!(module_of(&g, "/files/abc/download"), None);
        assert_eq!(module_of(&g, "/crm/deals/x/project"), Some("crm"));
        assert_eq!(module_of(&Method::POST, "/crm/deals/x/project"), Some("projects"));
        assert_eq!(module_of(&g, "/chat-messages/x"), Some("chats"));
        assert_eq!(module_of(&g, "/finance/payments"), Some("finance"));
        assert_eq!(module_of(&g, "/projects/x/finance"), Some("finance"));
        assert_eq!(module_of(&g, "/auth/me"), None);
        assert_eq!(module_of(&g, "/admin/modules"), None);
    }
}
