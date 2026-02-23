use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Json, Router,
};

use crate::handlers;
use crate::AppState;

const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;

pub fn build_router() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(|| async { Json(serde_json::json!({ "status": "ok" })) }))
        .merge(patient_routes())
        .merge(report_routes())
        .merge(test_item_routes())
        .merge(edit_log_routes())
        .merge(ocr_routes())
        .merge(temperature_routes())
        .merge(interpret_routes())
        .merge(admin_routes())
}

fn patient_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/patients",
            get(handlers::patients::list_patients).post(handlers::patients::create_patient),
        )
        .route(
            "/api/patients/:id",
            get(handlers::patients::get_patient)
                .put(handlers::patients::update_patient)
                .delete(handlers::patients::delete_patient),
        )
}

fn report_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/patients/:patient_id/reports",
            get(handlers::reports::list_reports_by_patient).post(handlers::reports::create_report),
        )
        .route(
            "/api/patients/:patient_id/trends",
            get(handlers::reports::get_trends),
        )
        .route(
            "/api/patients/:patient_id/trend-items",
            get(handlers::reports::list_trend_items),
        )
        .route(
            "/api/reports/:report_id",
            get(handlers::reports::get_report_detail)
                .put(handlers::reports::update_report)
                .delete(handlers::reports::delete_report_handler),
        )
        .route(
            "/api/reports/:report_id/interpret-cache",
            get(handlers::reports::get_cached_interpretation),
        )
}

fn test_item_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/reports/:report_id/test-items",
            get(handlers::reports::get_test_items_by_report),
        )
        .route("/api/test-items", post(handlers::reports::create_test_item))
        .route(
            "/api/test-items/:id",
            axum::routing::put(handlers::reports::update_test_item)
                .delete(handlers::reports::delete_test_item_handler),
        )
}

fn edit_log_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/edit-logs",
            get(handlers::reports::list_edit_logs_global),
        )
        .route(
            "/api/reports/:report_id/edit-logs",
            get(handlers::reports::list_edit_logs_by_report),
        )
}

fn ocr_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/upload",
            post(handlers::ocr::upload_file).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route(
            "/api/ocr/parse",
            post(handlers::ocr::ocr_parse).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route(
            "/api/ocr/suggest-groups",
            post(handlers::ocr::suggest_groups),
        )
        .route(
            "/api/patients/:patient_id/reports/merge-check",
            post(handlers::ocr::merge_check),
        )
        .route(
            "/api/patients/:patient_id/reports/prefetch-normalize",
            post(handlers::ocr::prefetch_normalize),
        )
        .route(
            "/api/patients/:patient_id/reports/confirm",
            post(handlers::ocr::batch_confirm),
        )
}

fn temperature_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/patients/:patient_id/temperatures",
            get(handlers::temperatures::list_temperatures)
                .post(handlers::temperatures::create_temperature),
        )
        .route(
            "/api/temperatures/:id",
            axum::routing::delete(handlers::temperatures::delete_temperature),
        )
}

fn interpret_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/reports/:report_id/interpret",
            get(handlers::interpret::interpret_single_report),
        )
        .route(
            "/api/patients/:patient_id/interpret-multi",
            get(handlers::interpret::interpret_multi),
        )
        .route(
            "/api/patients/:patient_id/interpret-all",
            get(handlers::interpret::interpret_all),
        )
        .route(
            "/api/patients/:patient_id/trends/:item_name/interpret",
            get(handlers::interpret::interpret_trend),
        )
        .route(
            "/api/patients/:patient_id/trends/:item_name/interpret-time",
            get(handlers::interpret::interpret_trend_time),
        )
}

fn admin_routes() -> Router<AppState> {
    Router::new().route(
        "/api/admin/backfill-canonical-names",
        post(handlers::normalize::backfill_canonical_names),
    )
}
