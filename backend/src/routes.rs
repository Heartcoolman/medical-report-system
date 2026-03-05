use axum::{
    extract::DefaultBodyLimit,
    middleware as axum_mw,
    routing::{get, post},
    Json, Router,
};

use crate::auth::{self, Role};
use crate::handlers;
use crate::middleware::MAX_UPLOAD_SIZE;
use crate::AppState;

/// Aggregates all API sub-routes (health, auth, readonly, nurse, doctor, admin)
/// with JWT auth middleware applied as an inner layer.
/// Routes use resource-relative paths (no /api/ prefix) because they will be
/// mounted via `nest` at /api/v1 and /api.
fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { Json(serde_json::json!({ "status": "ok" })) }))
        // Public auth routes (no JWT required, but JWT middleware skips /auth paths)
        .merge(auth_routes())
        // ReadOnly+ : all authenticated users can read
        .merge(readonly_routes())
        // Nurse+ : temperature write, expense read
        .merge(nurse_routes())
        // Doctor+ : patient write, report management, OCR, interpret
        .merge(doctor_routes())
        // Admin only
        .merge(admin_routes())
        // JWT auth middleware applied to all routes except /auth/* and /health
        .layer(axum_mw::from_fn(auth::jwt_auth_middleware))
}

pub fn build_router() -> Router<AppState> {
    Router::new()
        // v1 versioned path
        .nest("/api/v1", api_routes())
        // /api as v1 alias (backward compatible)
        .nest("/api", api_routes())
}

fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/me", get(auth::get_me))
        .route("/auth/refresh", post(auth::refresh))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/devices", get(auth::list_devices))
        .route("/auth/devices/:id", axum::routing::delete(auth::revoke_device))
}

/// Routes accessible by all authenticated users (ReadOnly and above).
/// GET-only endpoints for reading data.
fn readonly_routes() -> Router<AppState> {
    Router::new()
        // Patient list & detail (read)
        .route("/patients", get(handlers::patients::list_patients))
        .route("/patients/:id", get(handlers::patients::get_patient))
        // Report list & detail (read)
        .route(
            "/patients/:patient_id/reports",
            get(handlers::reports::list_reports_by_patient),
        )
        .route(
            "/patients/:patient_id/trends",
            get(handlers::reports::get_trends),
        )
        .route(
            "/patients/:patient_id/trend-items",
            get(handlers::reports::list_trend_items),
        )
        .route(
            "/reports/:report_id",
            get(handlers::reports::get_report_detail),
        )
        .route(
            "/reports/:report_id/interpret-cache",
            get(handlers::reports::get_cached_interpretation),
        )
        .route(
            "/reports/:report_id/test-items",
            get(handlers::reports::get_test_items_by_report),
        )
        // Edit logs (read)
        .route(
            "/edit-logs",
            get(handlers::reports::list_edit_logs_global),
        )
        .route(
            "/reports/:report_id/edit-logs",
            get(handlers::reports::list_edit_logs_by_report),
        )
        // Temperature (read)
        .route(
            "/patients/:patient_id/temperatures",
            get(handlers::temperatures::list_temperatures),
        )
        // Expense (read)
        .route(
            "/patients/:patient_id/expenses",
            get(handlers::expense::list_expenses),
        )
        .route(
            "/expenses/:id",
            get(handlers::expense::get_expense),
        )
        // User settings (API keys)
        .route(
            "/user/settings",
            get(handlers::user_settings::get_settings)
                .put(handlers::user_settings::update_settings),
        )
        // Medications (read)
        .route(
            "/patients/:patient_id/medications",
            get(handlers::medications::list_medications),
        )
        .route(
            "/patients/:patient_id/detected-drugs",
            get(handlers::medications::list_detected_drugs),
        )
        // Timeline (read)
        .route(
            "/patients/:patient_id/timeline",
            get(handlers::stats::get_timeline),
        )
        // Critical alerts (read)
        .route(
            "/stats/critical-alerts",
            get(handlers::stats::get_critical_alerts),
        )
        // Health assessment cache (read)
        .route(
            "/patients/:patient_id/health-assessment-cache",
            get(handlers::health_assessment::get_cached_assessment),
        )
        // Risk prediction
        .route(
            "/patients/:patient_id/risk-prediction",
            get(handlers::risk_prediction::get_risk_prediction),
        )
        // File serving
        .route("/files/:file_id", get(handlers::ocr::serve_file))
}

/// Routes accessible by Nurse and above.
/// Temperature recording, expense viewing.
fn nurse_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/patients/:patient_id/temperatures",
            post(handlers::temperatures::create_temperature),
        )
        .route(
            "/temperatures/:id",
            axum::routing::delete(handlers::temperatures::delete_temperature),
        )
        .layer(axum_mw::from_fn(auth::require_role(Role::Nurse)))
}

/// Routes accessible by Doctor and above.
/// Patient CRUD, report management, OCR, AI interpret, expense management.
fn doctor_routes() -> Router<AppState> {
    Router::new()
        // Patient write
        .route("/patients", post(handlers::patients::create_patient))
        .route(
            "/patients/:id",
            axum::routing::put(handlers::patients::update_patient)
                .delete(handlers::patients::delete_patient),
        )
        // Report write
        .route(
            "/patients/:patient_id/reports",
            post(handlers::reports::create_report),
        )
        .route(
            "/reports/:report_id",
            axum::routing::put(handlers::reports::update_report)
                .delete(handlers::reports::delete_report_handler),
        )
        // Test items write
        .route("/test-items", post(handlers::reports::create_test_item))
        .route(
            "/test-items/:id",
            axum::routing::put(handlers::reports::update_test_item)
                .delete(handlers::reports::delete_test_item_handler),
        )
        // OCR
        .route(
            "/upload",
            post(handlers::ocr::upload_file).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route(
            "/ocr/parse",
            post(handlers::ocr::ocr_parse).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route(
            "/ocr/suggest-groups",
            post(handlers::ocr::suggest_groups),
        )
        .route(
            "/patients/:patient_id/reports/merge-check",
            post(handlers::ocr::merge_check),
        )
        .route(
            "/patients/:patient_id/reports/prefetch-normalize",
            post(handlers::ocr::prefetch_normalize),
        )
        .route(
            "/patients/:patient_id/reports/confirm",
            post(handlers::ocr::batch_confirm),
        )
        // AI Interpret
        .route(
            "/reports/:report_id/interpret",
            get(handlers::interpret::interpret_single_report),
        )
        .route(
            "/patients/:patient_id/interpret-multi",
            get(handlers::interpret::interpret_multi),
        )
        .route(
            "/patients/:patient_id/interpret-all",
            get(handlers::interpret::interpret_all),
        )
        .route(
            "/patients/:patient_id/trends/:item_name/interpret",
            get(handlers::interpret::interpret_trend),
        )
        .route(
            "/patients/:patient_id/trends/:item_name/interpret-time",
            get(handlers::interpret::interpret_trend_time),
        )
        // Expense write
        .route(
            "/patients/:patient_id/expenses/parse",
            post(handlers::expense::parse_expense).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route(
            "/patients/:patient_id/expenses/confirm",
            post(handlers::expense::confirm_expense),
        )
        .route(
            "/patients/:patient_id/expenses/batch-confirm",
            post(handlers::expense::batch_confirm_expense),
        )
        .route(
            "/expenses/parse-chunk",
            post(handlers::expense::parse_chunk).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route(
            "/expenses/merge-chunks",
            post(handlers::expense::merge_chunks),
        )
        .route(
            "/expenses/analyze",
            post(handlers::expense::analyze_expense_day),
        )
        .route(
            "/expenses/:id",
            axum::routing::delete(handlers::expense::delete_expense),
        )
        // Medications write
        .route(
            "/patients/:patient_id/medications",
            post(handlers::medications::create_medication),
        )
        .route(
            "/medications/:id",
            axum::routing::put(handlers::medications::update_medication)
                .delete(handlers::medications::delete_medication),
        )
        // AI Health Assessment
        .route(
            "/patients/:patient_id/health-assessment",
            get(handlers::health_assessment::health_assessment),
        )
        .layer(axum_mw::from_fn(auth::require_role(Role::Doctor)))
}

/// Admin-only routes.
fn admin_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/backfill-canonical-names",
            post(handlers::normalize::backfill_canonical_names),
        )
        .route(
            "/admin/audit-logs",
            get(handlers::audit_handler::list_audit_logs),
        )
        // User management
        .route("/admin/users", get(handlers::admin::list_users))
        .route(
            "/admin/users/:id/role",
            axum::routing::put(handlers::admin::update_user_role),
        )
        .route(
            "/admin/users/:id",
            axum::routing::delete(handlers::admin::delete_user),
        )
        // Backup & Restore
        .route(
            "/admin/backup",
            get(handlers::backup::download_backup),
        )
        .route(
            "/admin/restore",
            post(handlers::backup::restore_backup)
                .layer(DefaultBodyLimit::max(100 * 1024 * 1024)),
        )
        .layer(axum_mw::from_fn(auth::require_role(Role::Admin)))
}
