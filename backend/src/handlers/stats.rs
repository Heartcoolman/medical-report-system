use axum::{
    extract::{Query, State},
    Json,
};
use serde::Serialize;

use crate::error::run_blocking;
use crate::error::{AppError, ErrorCode};
use crate::models::{ApiResponse, PaginatedList, PaginationParams};
use crate::AppState;

pub async fn get_timeline(
    State(state): State<AppState>,
    axum::extract::Path(patient_id): axum::extract::Path<String>,
) -> Result<Json<ApiResponse<Vec<crate::db::medication_repo::TimelineEvent>>>, AppError> {
    let db = state.db.clone();
    let events = run_blocking(move || db.get_patient_timeline(&patient_id)).await?;
    Ok(Json(ApiResponse::ok(events, "查询成功")))
}

#[derive(Debug, Clone, Serialize)]
pub struct CriticalAlert {
    pub patient_id: String,
    pub patient_name: String,
    pub report_id: String,
    pub report_type: String,
    pub report_date: String,
    pub item_name: String,
    pub value: String,
    pub unit: String,
    pub reference_range: String,
    pub status: String,
}

pub async fn get_critical_alerts(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let (page, page_size) = pagination.normalize();
    let paginated = pagination.is_paginated();
    let db = state.db.clone();
    let result = run_blocking(move || {
        db.with_conn(|conn| {
            let total: usize = conn.query_row(
                "SELECT COUNT(*) FROM test_items ti
                 WHERE ti.status IN ('CriticalHigh', 'CriticalLow')",
                [],
                |row| row.get::<_, i64>(0),
            ).map_err(|e| AppError::new(ErrorCode::DatabaseError, format!("查询危急值总数失败: {}", e)))? as usize;

            let offset = (page - 1) * page_size;
            let mut stmt = conn.prepare(
                "SELECT p.id, p.name, r.id, r.report_type, r.report_date,
                        ti.name, ti.value, ti.unit, ti.reference_range, ti.status
                 FROM test_items ti
                 JOIN reports r ON ti.report_id = r.id
                 JOIN patients p ON r.patient_id = p.id
                 WHERE ti.status IN ('CriticalHigh', 'CriticalLow')
                 ORDER BY r.report_date DESC, p.name
                 LIMIT ? OFFSET ?"
            )?;
            let rows = stmt.query_map(rusqlite::params![page_size as i64, offset as i64], |row| {
                Ok(CriticalAlert {
                    patient_id: row.get(0)?,
                    patient_name: row.get(1)?,
                    report_id: row.get(2)?,
                    report_type: row.get(3)?,
                    report_date: row.get(4)?,
                    item_name: row.get(5)?,
                    value: row.get(6)?,
                    unit: row.get(7)?,
                    reference_range: row.get(8)?,
                    status: row.get(9)?,
                })
            })?;
            let mut items = Vec::new();
            for row in rows {
                items.push(row.map_err(|e| AppError::new(ErrorCode::DatabaseError, format!("查询危急值失败: {}", e)))?);
            }
            Ok(PaginatedList { items, total, page, page_size })
        })
    })
    .await?;
    if paginated {
        Ok(Json(ApiResponse::ok(serde_json::to_value(&result).unwrap(), "查询成功")))
    } else {
        Ok(Json(ApiResponse::ok(serde_json::to_value(&result.items).unwrap(), "查询成功")))
    }
}
