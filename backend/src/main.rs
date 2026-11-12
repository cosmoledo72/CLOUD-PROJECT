//! WorkHub server entry point: configuration, database migrations and routing.

mod accounts;
mod acl;
mod admin;
mod attachments;
mod auth;
mod backups;
mod branding;
mod calendar;
mod chat;
mod comments;
mod config;
mod crm;
mod error;
mod fields;
mod kb;
mod kb_html;
mod file_ops;
mod files;
mod finance;
mod gantt_export;
mod mail;
mod modules;
mod money;
mod notify;
mod ooxml;
mod onlyoffice;
mod project_labels;
mod projects;
mod public_links;
mod ratelimit;
mod registration;
mod scheduling;
mod search;
mod state;
mod storage;
mod task_board;
mod team;
mod templates;
mod textract;
mod time;
mod trash;
mod users;
mod view_settings;

use std::sync::Arc;

use anyhow::Context;
use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue, Method},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

use crate::{auth::handlers as authh, config::Config, state::AppState, storage::LocalStore};

fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { Json(json!({ "status": "ok" })) }))
        // Authentication
        .route("/auth/signup", post(authh::signup))
        .route("/auth/login", post(authh::login))
        .route("/auth/totp/verify", post(authh::verify_totp))
        .route("/auth/logout", post(authh::logout))
        .route("/auth/me", get(authh::me))
        .route("/auth/password", post(authh::change_password))
        .route("/auth/totp/setup", post(authh::totp_setup))
        .route("/auth/totp/enable", post(authh::totp_enable))
        .route("/auth/totp/disable", post(authh::totp_disable))
        .route("/auth/profile", put(team::update_profile))
        // Team directory
        .route("/team", get(team::list))
        // Knowledge base
        .route("/kb/pages", get(kb::tree).post(kb::create))
        .route("/kb/pages/{id}", get(kb::get).put(kb::update).delete(kb::delete))
        .route("/kb/pages/{id}/pin", patch(kb::pin))
        .route("/kb/pages/{id}/restore", post(kb::restore))
        .route("/kb/pages/{id}/versions", get(kb::versions))
        .route("/kb/pages/{id}/versions/{v}", get(kb::version))
        .route("/kb/pages/{id}/versions/{v}/restore", post(kb::restore_version))
        .route("/kb/pages/{id}/files", post(kb::upload))
        .route("/kb/pages/{id}/presence", post(kb::heartbeat))
        .route("/kb/files/{id}", get(kb::file))
        .route("/kb/archive", get(kb::archive))
        .route("/public/s/{token}/kbfile/{id}", get(public_links::kb_file))
        .route("/kb/pages/{id}/move", patch(kb::move_page))
        .route("/kb/search", get(kb::search))
        .route("/search", get(search::search))
        // Chats
        .route("/chats", get(chat::list).post(chat::create_group))
        .route("/chats/unread", get(chat::unread))
        .route("/chats/direct", post(chat::direct))
        .route("/chats/{id}", get(chat::get).put(chat::update_group))
        .route("/chats/{id}/leave", post(chat::leave))
        .route("/chats/{id}/messages", get(chat::messages).post(chat::post))
        .route("/chats/{id}/read", post(chat::read))
        .route("/chat-messages/{id}", patch(chat::edit).delete(chat::delete_message))
        // Directory
        .route("/users", get(users::list_users).post(users::create_user))
        .route("/users/{id}", patch(users::update_user))
        .route("/groups", get(users::list_groups).post(users::create_group))
        .route("/groups/{id}", patch(admin::rename_group).delete(admin::delete_group))
        .route("/groups/{id}/members", put(users::set_group_members))
        // Administration
        .route("/admin/overview", get(admin::overview))
        .route("/admin/audit", get(admin::audit))
        .route("/admin/portal", get(admin::get_portal).patch(admin::update_portal))
        .route("/admin/modules", get(modules::get_modules).put(modules::set_modules))
        .route("/admin/users/{id}/reset-password", post(admin::reset_password))
        .route("/admin/users/{id}/reset-mfa", post(admin::reset_mfa))
        .route("/admin/users/{id}/revoke-sessions", post(admin::revoke_sessions))
        .route("/admin/branding", put(branding::update))
        .route("/admin/branding/asset", post(branding::upload_asset).delete(branding::delete_asset))
        .route("/admin/project-fields", post(fields::create_def))
        .route("/admin/project-fields/preset", post(fields::apply_preset))
        .route("/admin/project-fields/reorder", post(fields::reorder_defs))
        .route("/admin/project-fields/{id}", put(fields::update_def).delete(fields::delete_def))
        // Branding (public by slug for the login page)
        .route("/branding", get(branding::get_own))
        .route("/calendar", get(calendar::get))
        .route("/admin/calendar/days/{date}", put(calendar::set_day).delete(calendar::reset_day))
        .route("/admin/calendar/{year}/holidays", post(calendar::fill_holidays))
        .route("/admin/reserve", get(backups::list))
        .route("/admin/reserve/now", post(backups::request_now))
        .route("/admin/reserve/{name}/{file}", get(backups::download))
        .route("/finance/settings", get(finance::get_settings).put(finance::save_settings))
        .route("/finance/categories", get(finance::list_categories).post(finance::create_category))
        .route("/finance/categories/{id}", patch(finance::update_category))
        .route("/finance/payments", get(finance::list_payments).post(finance::create_payment))
        .route("/finance/payments/{id}", put(finance::update_payment).delete(finance::delete_payment))
        .route("/finance/summary", get(finance::summary))
        .route("/finance/receivables", get(finance::receivables))
        .route("/finance/invoices", get(finance::open_invoices))
        .route("/finance/projects", get(finance::projects_money))
        .route("/projects/{id}/finance", get(finance::project_finance))
        .route("/view-settings/{key}", get(view_settings::get).put(view_settings::put).delete(view_settings::reset))
        .route("/admin/view-settings/{key}", put(view_settings::put_default))
        .route("/notifications", get(notify::list))
        .route("/notifications/read", post(notify::mark_read))
        .route("/notifications/prefs", get(notify::get_prefs).put(notify::save_prefs))
        .route("/notifications/stream", get(notify::stream))
        .route("/tasks/{id}/comments", get(comments::list).post(comments::create))
        .route("/task-comments/{id}", delete(comments::delete))
        .route("/public/invite/check", post(accounts::invite_info))
        .route("/public/invite/accept", post(accounts::accept_invite))
        .route("/public/password/forgot", post(accounts::forgot))
        .route("/public/password/check", post(accounts::reset_info))
        .route("/public/password/reset", post(accounts::reset_password))
        .route("/admin/mail", get(mail::get_settings).put(mail::save_settings))
        .route("/admin/mail/test", post(mail::send_test))
        .route("/admin/mail/outbox/{id}/retry", post(mail::retry))
        .route("/admin/users/{id}/invite", post(accounts::resend_invite))
        .route("/admin/invitations", get(accounts::list_invitations).post(accounts::invite_many))
        .route("/admin/invitations/{id}", delete(accounts::revoke_invitation))
        .route("/admin/registration", get(registration::get_settings).put(registration::save_settings))
        .route("/admin/registration/requests", get(registration::list_requests))
        .route("/admin/registration/requests/{id}/approve", post(registration::approve))
        .route("/admin/registration/requests/{id}/reject", post(registration::reject))
        .route("/public/register", post(registration::register))
        .route("/public/register/confirm", post(registration::confirm))
        .route("/public/branding/{slug}", get(branding::get_public))
        .route("/public/branding/{slug}/{kind}", get(branding::serve_asset))
        .route("/project-fields", get(fields::list_defs))
        // CRM
        .route("/crm/companies", get(crm::companies::list).post(crm::companies::create))
        .route("/crm/companies/check-inn", get(crm::companies::check_inn))
        .route("/crm/companies/import", post(crm::import::import))
        .route("/crm/companies/{id}", get(crm::companies::get).put(crm::companies::update).delete(crm::companies::delete))
        .route("/crm/contacts", get(crm::companies::list_contacts).post(crm::companies::create_contact))
        .route("/crm/contacts/{id}", put(crm::companies::update_contact).delete(crm::companies::delete_contact))
        .route("/crm/stages", get(crm::deals::list_stages).put(crm::deals::save_stages))
        .route("/crm/reports", get(crm::reports::summary))
        .route("/crm/reports/deals", get(crm::reports::deals))
        .route("/crm/reports.xlsx", get(crm::reports::xlsx))
        .route("/crm/lost-reasons", get(crm::reports::lost_reasons))
        .route("/crm/deals", get(crm::deals::list_deals).post(crm::deals::create_deal))
        .route("/crm/deals/{id}", get(crm::deals::get_deal).patch(crm::deals::update_deal).delete(crm::deals::delete_deal))
        .route("/crm/deals/{id}/project", get(crm::deals::project_defaults).post(crm::deals::create_project_from_deal))
        .route("/crm/tasks", get(crm::tasks::list).post(crm::tasks::create))
        .route("/crm/tasks/summary", get(crm::tasks::summary))
        .route("/crm/tasks/{id}", patch(crm::tasks::update).delete(crm::tasks::delete))
        .route("/crm/activities", post(crm::deals::create_activity))
        .route("/crm/activities/{id}", delete(crm::deals::delete_activity))
        .route("/projects/{id}/company", put(crm::deals::link_project_company))
        // Trash
        .route("/trash", get(trash::list))
        .route("/trash/empty", post(trash::empty))
        .route("/trash/{type}/{id}", delete(trash::purge))
        .route("/trash/{type}/{id}/restore", post(trash::restore))
        // Document templates
        .route("/templates", get(templates::list))
        .route("/templates/placeholders", get(templates::placeholders))
        .route("/templates/{id}/download", get(templates::download))
        .route("/admin/templates", post(templates::create))
        .route("/admin/templates/sample", post(templates::install_sample))
        .route("/admin/templates/sales-samples", post(templates::install_sales_samples))
        .route("/admin/organization", get(crm::documents::get_org).put(crm::documents::save_org))
        .route("/admin/numbering", get(crm::documents::get_numbering).put(crm::documents::save_numbering))
        .route("/crm/documents", get(crm::documents::list))
        .route("/crm/documents/next-number", get(crm::documents::next_number))
        .route("/crm/documents/{id}", patch(crm::documents::update))
        .route("/crm/deals/{id}/documents", get(crm::documents::defaults).post(crm::documents::create))
        .route("/crm/companies/{id}/folder", post(crm::documents::open_company_folder))
        .route("/admin/templates/{id}", put(templates::update).delete(templates::delete))
        .route("/admin/templates/{id}/file", post(templates::replace_file))
        .route("/projects/{id}/templates/{template_id}/check", get(templates::check))
        .route("/projects/{id}/generate", post(templates::generate))
        // Documents
        .route("/folders/roots", get(files::roots))
        .route("/storage/usage", get(file_ops::usage))
        .route("/links", get(public_links::get).put(public_links::save))
        .route("/links/{id}", delete(public_links::revoke))
        .route("/admin/links", get(public_links::admin_list))
        .route("/admin/links/settings", put(public_links::admin_toggle))
        .route("/public/s/{token}", get(public_links::open))
        .route("/public/s/{token}/unlock", post(public_links::unlock))
        .route("/public/s/{token}/download/{file}", get(public_links::download))
        .route("/public/s/{token}/zip", get(public_links::zip))
        .route("/public/s/{token}/view/{file}", get(public_links::view))
        .route("/files/copy", post(file_ops::copy))
        .route("/files/zip", post(file_ops::zip))
        .route("/folders", post(files::create_folder))
        .route("/folders/{id}", get(files::get_folder).patch(files::rename_folder).delete(files::delete_folder))
        .route("/folders/{id}/upload", post(files::upload))
        .route("/folders/{id}/new", post(files::new_document))
        .route("/shared", get(files::shared_with_me))
        .route("/files/{id}", get(files::get_file).patch(files::update_file).delete(files::delete_file))
        .route("/files/{id}/download", get(files::download))
        .route("/files/{id}/versions", get(files::versions))
        .route("/files/{id}/versions/{version}/restore", post(files::restore_version))
        .route("/files/{id}/editor", get(onlyoffice::editor_config))
        .route("/shares", get(files::list_shares).post(files::create_share))
        .route("/shares/{id}", delete(files::delete_share))
        // ONLYOFFICE Document Server (machine-to-machine, token-authenticated)
        .route("/oo/download/{id}", get(onlyoffice::ds_download))
        .route("/oo/callback/{id}", post(onlyoffice::ds_callback))
        // Projects
        .route("/projects", get(projects::list_projects).post(projects::create_project))
        .route("/projects/bulk", post(projects::bulk_update))
        .route("/projects/{id}", get(projects::get_project).patch(projects::update_project).delete(projects::delete_project))
        .route("/projects/{id}/fields", put(fields::update_values))
        .route("/projects/{id}/members", post(projects::upsert_member))
        .route("/projects/{id}/members/{user_id}", delete(projects::remove_member))
        .route("/projects/{id}/tasks", get(projects::list_tasks).post(projects::create_task))
        .route("/projects/{id}/gantt/export", get(gantt_export::settings).post(gantt_export::export))
        .route("/projects/{id}/reschedule", post(projects::reschedule))
        .route("/projects/{id}/baseline", post(projects::save_baseline).delete(projects::clear_baseline))
        .route("/tasks", get(task_board::board))
        .route("/tasks/{id}", get(task_board::get_task).patch(projects::update_task).delete(projects::delete_task))
        .route("/tasks/{id}/labels", put(task_board::set_task_labels))
        .route("/tasks/{id}/attachments", get(attachments::list).post(attachments::upload))
        .route("/tasks/{id}/attachments/link", post(attachments::link_existing))
        .route("/tasks/{id}/attachments/{file_id}", delete(attachments::detach))
        .route("/project-labels", get(project_labels::list).post(project_labels::create))
        .route("/project-labels/{id}", patch(project_labels::update).delete(project_labels::delete))
        .route("/projects/{id}/labels", put(project_labels::set_for_project))
        .route("/tasks/{id}/checklist", post(task_board::add_item))
        .route("/checklist-items/{id}", patch(task_board::update_item).delete(task_board::delete_item))
        .route("/task-labels", get(task_board::list_labels).post(task_board::create_label))
        .route("/task-labels/{id}", patch(task_board::update_label).delete(task_board::delete_label))
        .route("/tasks/{id}/dependencies", post(projects::add_dependency))
        .route("/tasks/{id}/dependencies/{dep_id}", delete(projects::remove_dependency))
        .route("/tasks/{id}/time", get(time::list_for_task).post(time::add))
        .route("/tasks/{id}/timer", post(time::start_timer))
        .route("/tasks/{id}/pay", get(time::get_pay).put(time::set_pay))
        .route("/time/timer", get(time::get_timer).delete(time::discard_timer))
        .route("/time/timer/stop", post(time::stop_timer))
        .route("/time/rates", get(time::list_rates))
        .route("/time/rates/{user_id}", put(time::set_rate))
        .route("/time/report", get(time::report))
        .route("/time/report.xlsx", get(time::report_xlsx))
        .route("/time/{id}", patch(time::update).delete(time::delete))
        // Switched-off portal modules answer 403 module_disabled.
        .layer(axum::middleware::from_fn_with_state(state.clone(), modules::guard));

    let mut app = Router::new()
        .nest("/api", api)
        .layer(DefaultBodyLimit::max(state.cfg.max_upload_bytes))
        .layer(TraceLayer::new_for_http());

    if let Some(origin) = &state.cfg.cors_origin {
        let cors = CorsLayer::new()
            .allow_origin(origin.parse::<HeaderValue>().expect("valid CORS_ORIGIN"))
            .allow_credentials(true)
            .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::PUT, Method::DELETE])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
        app = app.layer(cors);
    }
    app.with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `workhub-server healthcheck` — used by the Docker healthcheck (the runtime image has no curl).
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        std::process::exit(if healthcheck() { 0 } else { 1 });
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "workhub_server=info,tower_http=info".into()))
        .init();

    let cfg = Config::from_env()?;
    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&cfg.database_url)
        .await
        .context("connect to database")?;
    sqlx::migrate!("./migrations").run(&db).await.context("run migrations")?;

    let store = LocalStore::new(&cfg.storage_dir)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let state = AppState { db, cfg: Arc::new(cfg.clone()), store: Arc::new(store), http, limiter: Default::default(), mail_wakeup: Default::default(), events: tokio::sync::broadcast::channel(1024).0 };
    trash::spawn_auto_purge(state.clone());
    mail::spawn_worker(state.clone());
    notify::spawn_worker(state.clone());
    textract::spawn_worker(state.clone());
    if let Err(e) = kb::convert_markdown(&state).await {
        tracing::warn!(error = ?e, "knowledge base conversion failed");
    }

    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    tracing::info!(addr = %cfg.bind_addr, "WorkHub server listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

/// Ask the running server for `/api/health` over plain TCP; true on HTTP 200.
fn healthcheck() -> bool {
    use std::io::{Read, Write};
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let port = addr.rsplit(':').next().unwrap_or("8080");
    let Ok(mut s) = std::net::TcpStream::connect(format!("127.0.0.1:{port}")) else { return false };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    if s.write_all(b"GET /api/health HTTP/1.0\r\nHost: localhost\r\n\r\n").is_err() {
        return false;
    }
    let mut head = [0u8; 16];
    let n = s.read(&mut head).unwrap_or(0);
    std::str::from_utf8(&head[..n]).is_ok_and(|h| h.starts_with("HTTP/1.1 200") || h.starts_with("HTTP/1.0 200"))
}
