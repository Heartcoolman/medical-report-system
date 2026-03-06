use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

use crate::audit;
use crate::auth::AuthUser;
use crate::error::{run_blocking, AppError};
use crate::models::{ApiResponse, PaginatedList, Patient, PatientReq, PatientWithStats};
use crate::AppState;

pub async fn create_patient(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PatientReq>,
) -> Result<Json<ApiResponse<Patient>>, AppError> {
    if let Err(msg) = req.validate() {
        return Err(AppError::validation(msg));
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

    if let Some(ref idx) = state.patient_index {
        let _ = idx.add_or_update(&patient.id, &patient.name, &patient.phone, &patient.notes).await;
        let _ = idx.commit().await;
    }

    let db = state.db.clone();
    let pid = patient.id.clone();
    let pname = patient.name.clone();
    let uid = auth.0.sub.clone();
    let _ = run_blocking(move || {
        audit::log_audit(
            &db, Some(&uid), "create", "patient", Some(&pid), None,
            Some(&format!("创建患者: {}", pname)),
        )
    }).await;

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
        None => Err(AppError::patient_not_found()),
    }
}

pub async fn list_patients(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<PaginatedList<PatientWithStats>>>, AppError> {
    let search = params.get("search").cloned();
    let page: usize = params.get("page").and_then(|v| v.parse().ok()).unwrap_or(1);
    let page_size: usize = params
        .get("page_size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let page_size = if page_size > 100 { 100 } else { page_size };

    if let Some(q) = search {
        if !q.is_empty() {
            // Try Tantivy search first, fallback to SQLite
            if let Some(ref idx) = state.patient_index {
                let idx = idx.clone();
                let q_clone = q.clone();
                let offset = (page.saturating_sub(1)) * page_size;
                let search_result =
                    run_blocking(move || idx.search_paginated(&q_clone, offset, page_size)).await;
                if let Ok((ids, total)) = search_result {
                    if !ids.is_empty() || total > 0 {
                        let db = state.db.clone();
                        let result = run_blocking(move || db.get_patients_by_ids_with_stats(&ids)).await?;
                        let paginated = PaginatedList {
                            items: result,
                            total,
                            page,
                            page_size,
                        };
                        return Ok(Json(ApiResponse::ok(paginated, "查询成功")));
                    }
                }
            }

            // Fallback to SQLite LIKE search
            let db = state.db.clone();
            let paginated =
                run_blocking(move || db.search_patients_with_stats_paginated(&q, page, page_size))
                    .await?;
            return Ok(Json(ApiResponse::ok(paginated, "查询成功")));
        }
    }
    let db = state.db.clone();
    let result =
        run_blocking(move || db.list_patients_with_stats_paginated(page, page_size)).await?;
    Ok(Json(ApiResponse::ok(result, "查询成功")))
}

pub async fn update_patient(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<PatientReq>,
) -> Result<Json<ApiResponse<Patient>>, AppError> {
    if let Err(msg) = req.validate() {
        return Err(AppError::validation(msg));
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

            if let Some(ref idx) = state.patient_index {
                let _ = idx.add_or_update(&patient.id, &patient.name, &patient.phone, &patient.notes).await;
                let _ = idx.commit().await;
            }

            let db = state.db.clone();
            let pid = patient.id.clone();
            let pname = patient.name.clone();
            let uid = auth.0.sub.clone();
            let _ = run_blocking(move || {
                audit::log_audit(
                    &db, Some(&uid), "update", "patient", Some(&pid), None,
                    Some(&format!("更新患者: {}", pname)),
                )
            }).await;

            Ok(Json(ApiResponse::ok(patient, "更新成功")))
        }
        None => Err(AppError::patient_not_found()),
    }
}

pub async fn delete_patient(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let db = state.db.clone();
    let id_clone = id.clone();
    run_blocking(move || db.delete_patient(&id_clone)).await?;

    if let Some(ref idx) = state.patient_index {
        let _ = idx.delete(&id).await;
        let _ = idx.commit().await;
    }

    let db = state.db.clone();
    let uid = auth.0.sub.clone();
    let _ = run_blocking(move || {
        audit::log_audit(
            &db, Some(&uid), "delete", "patient", Some(&id), None,
            Some("删除患者"),
        )
    }).await;

    Ok(Json(ApiResponse::ok_msg("删除成功")))
}

pub async fn rebuild_search_index(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let idx = state.patient_index.as_ref()
        .ok_or_else(|| AppError::internal("搜索索引未初始化"))?
        .clone();
    let db = state.db.clone();

    let patients = run_blocking(move || db.list_patients()).await?;
    let count = patients.len();
    for p in &patients {
        idx.add_or_update(&p.id, &p.name, &p.phone, &p.notes).await?;
    }
    idx.commit().await?;

    Ok(Json(ApiResponse::ok(
        format!("索引重建完成，共 {} 条记录", count),
        "重建成功",
    )))
}
