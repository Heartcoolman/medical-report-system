use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::error::{run_blocking, AppError};
use crate::models::{ApiResponse, CreateTemperatureReq, TemperatureRecord};
use crate::AppState;

pub async fn create_temperature(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<CreateTemperatureReq>,
) -> Result<Json<ApiResponse<TemperatureRecord>>, AppError> {
    if let Err(msg) = req.validate() {
        return Err(AppError::BadRequest(msg));
    }
    // Verify patient exists
    let db = state.db.clone();
    let pid = patient_id.clone();
    let exists = run_blocking(move || db.get_patient(&pid)).await?;
    if exists.is_none() {
        return Err(AppError::NotFound("患者不存在".to_string()));
    }
    let record = TemperatureRecord {
        id: Uuid::new_v4().to_string(),
        patient_id,
        recorded_at: req.recorded_at,
        value: req.value,
        note: req.note,
        created_at: Utc::now().to_rfc3339(),
    };
    let db = state.db.clone();
    let r = record.clone();
    run_blocking(move || db.create_temperature(&r)).await?;
    Ok(Json(ApiResponse::ok(record, "创建成功")))
}

pub async fn list_temperatures(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<TemperatureRecord>>>, AppError> {
    let db = state.db.clone();
    let records = run_blocking(move || db.list_temperatures_by_patient(&patient_id)).await?;
    Ok(Json(ApiResponse::ok(records, "查询成功")))
}

pub async fn delete_temperature(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let db = state.db.clone();
    run_blocking(move || db.delete_temperature(&id)).await?;
    Ok(Json(ApiResponse::ok_msg("删除成功")))
}
