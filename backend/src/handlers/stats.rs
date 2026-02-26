use axum::{
    extract::State,
    Json,
};
use serde::Serialize;

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
) -> Result<Json<ApiResponse<Vec<CriticalAlert>>>, AppError> {
    let db = state.db.clone();
    let alerts = run_blocking(move || {
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.name, r.id, r.report_type, r.report_date,
                        ti.name, ti.value, ti.unit, ti.reference_range, ti.status
                 FROM test_items ti
                 JOIN reports r ON ti.report_id = r.id
                 JOIN patients p ON r.patient_id = p.id
                 WHERE ti.status IN ('critical_high', 'critical_low')
                 ORDER BY r.report_date DESC, p.name"
            )?;
            let rows = stmt.query_map([], |row| {
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
            let mut alerts = Vec::new();
            for row in rows {
                alerts.push(row.map_err(|e| AppError::Internal(format!("查询危急值失败: {}", e)))?);
            }
            Ok(alerts)
        })
    })
    .await?;
    Ok(Json(ApiResponse::ok(alerts, "查询成功")))
}
