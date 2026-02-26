use axum::{
    extract::State,
    Json,
};

use crate::error::run_blocking;
use crate::error::AppError;
use crate::models::ApiResponse;
use crate::AppState;

pub async fn get_timeline(
    State(state): State<AppState>,
    axum::extract::Path(patient_id): axum::extract::Path<String>,
) -> Result<Json<ApiResponse<Vec<crate::db::medication_repo::TimelineEvent>>>, AppError> {
    let db = state.db.clone();
    let events = run_blocking(move || db.get_patient_timeline(&patient_id)).await?;
    Ok(Json(ApiResponse::ok(events, "查询成功")))
}
