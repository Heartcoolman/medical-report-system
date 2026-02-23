use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{run_blocking, AppError};
use crate::models::{ApiResponse, PaginatedList, Patient, PatientReq, PatientWithStats};
use crate::AppState;

pub async fn create_patient(
    State(state): State<AppState>,
    Json(req): Json<PatientReq>,
) -> Result<Json<ApiResponse<Patient>>, AppError> {
    if let Err(msg) = req.validate() {
        return Err(AppError::BadRequest(msg));
    }
    let now = Utc::now().to_rfc3339();
    let patient = Patient {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        gender: req.gender,
        dob: req.dob,
        phone: req.phone,
        id_number: req.id_number,
        notes: req.notes,
        created_at: now.clone(),
        updated_at: now,
    };
    let db = state.db.clone();
    let p = patient.clone();
    run_blocking(move || db.create_patient(&p)).await?;
    Ok(Json(ApiResponse::ok(patient, "创建成功")))
}

pub async fn get_patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Patient>>, AppError> {
    let db = state.db.clone();
    let id_clone = id.clone();
    let result = run_blocking(move || db.get_patient(&id_clone)).await?;
    match result {
        Some(p) => Ok(Json(ApiResponse::ok(p, "查询成功"))),
        None => Err(AppError::NotFound("患者不存在".to_string())),
    }
}

pub async fn list_patients(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<PaginatedList<PatientWithStats>>>, AppError> {
    let db = state.db.clone();
    let search = params.get("search").cloned();
    let page: usize = params.get("page").and_then(|v| v.parse().ok()).unwrap_or(1);
    let page_size: usize = params
        .get("page_size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let page_size = if page_size > 100 { 100 } else { page_size };

    if let Some(q) = search {
        if !q.is_empty() {
            let result = run_blocking(move || db.search_patients_with_stats(&q)).await?;
            let total = result.len();
            let skip = (page - 1) * page_size;
            let items = result.into_iter().skip(skip).take(page_size).collect();
            let paginated = PaginatedList {
                items,
                total,
                page,
                page_size,
            };
            return Ok(Json(ApiResponse::ok(paginated, "查询成功")));
        }
    }
    let result =
        run_blocking(move || db.list_patients_with_stats_paginated(page, page_size)).await?;
    Ok(Json(ApiResponse::ok(result, "查询成功")))
}

pub async fn update_patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PatientReq>,
) -> Result<Json<ApiResponse<Patient>>, AppError> {
    if let Err(msg) = req.validate() {
        return Err(AppError::BadRequest(msg));
    }
    let now = Utc::now().to_rfc3339();
    let db = state.db.clone();
    let id_clone = id.clone();
    let existing = run_blocking(move || db.get_patient(&id_clone)).await?;
    match existing {
        Some(old) => {
            let patient = Patient {
                id,
                name: req.name,
                gender: req.gender,
                dob: req.dob,
                phone: req.phone,
                id_number: req.id_number,
                notes: req.notes,
                created_at: old.created_at,
                updated_at: now,
            };
            let db = state.db.clone();
            let p = patient.clone();
            run_blocking(move || db.update_patient(&p)).await?;
            Ok(Json(ApiResponse::ok(patient, "更新成功")))
        }
        None => Err(AppError::NotFound("患者不存在".to_string())),
    }
}

pub async fn delete_patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let db = state.db.clone();
    run_blocking(move || db.delete_patient(&id)).await?;
    Ok(Json(ApiResponse::ok_msg("删除成功")))
}
