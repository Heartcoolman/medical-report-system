use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::error::{run_blocking, AppError};
use crate::models::{
    ApiResponse, CreateMedicationReq, DetectedDrug, Medication, UpdateMedicationReq,
};
use crate::AppState;

// --- Medications ---

pub async fn list_medications(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Medication>>>, AppError> {
    let db = state.db.clone();
    let meds = run_blocking(move || db.list_medications_by_patient(&patient_id)).await?;
    Ok(Json(ApiResponse::ok(meds, "查询成功")))
}

pub async fn create_medication(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<CreateMedicationReq>,
) -> Result<Json<ApiResponse<Medication>>, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::validation("药品名称不能为空"));
    }
    let med = Medication {
        id: Uuid::new_v4().to_string(),
        patient_id,
        name: req.name,
        dosage: req.dosage,
        frequency: req.frequency,
        start_date: req.start_date,
        end_date: req.end_date,
        note: req.note,
        active: true,
        created_at: Utc::now().to_rfc3339(),
    };
    let db = state.db.clone();
    let m = med.clone();
    run_blocking(move || db.create_medication(&m)).await?;
    Ok(Json(ApiResponse::ok(med, "创建成功")))
}

pub async fn update_medication(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMedicationReq>,
) -> Result<Json<ApiResponse<Medication>>, AppError> {
    let db = state.db.clone();
    let id_c = id.clone();
    // We need to get the existing medication first - fetch from all patients
    // Since we don't have a get_medication_by_id, let's add a simple approach
    let db2 = state.db.clone();
    let existing = run_blocking(move || {
        db2.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, patient_id, name, dosage, frequency, start_date, end_date, note, active, created_at
                 FROM medications WHERE id = ?1",
            )?;
            let mut rows = stmt.query_map(rusqlite::params![id_c], |row| {
                Ok(Medication {
                    id: row.get(0)?,
                    patient_id: row.get(1)?,
                    name: row.get(2)?,
                    dosage: row.get(3)?,
                    frequency: row.get(4)?,
                    start_date: row.get(5)?,
                    end_date: row.get(6)?,
                    note: row.get(7)?,
                    active: row.get::<_, i32>(8)? != 0,
                    created_at: row.get(9)?,
                })
            })?;
            match rows.next() {
                Some(Ok(m)) => Ok(Some(m)),
                _ => Ok(None),
            }
        })
    })
    .await?;

    match existing {
        Some(mut med) => {
            if let Some(name) = req.name {
                med.name = name;
            }
            if let Some(dosage) = req.dosage {
                med.dosage = dosage;
            }
            if let Some(frequency) = req.frequency {
                med.frequency = frequency;
            }
            if let Some(start_date) = req.start_date {
                med.start_date = start_date;
            }
            if let Some(end_date) = req.end_date {
                med.end_date = Some(end_date);
            }
            if let Some(note) = req.note {
                med.note = note;
            }
            if let Some(active) = req.active {
                med.active = active;
            }
            let m = med.clone();
            run_blocking(move || db.update_medication(&m)).await?;
            Ok(Json(ApiResponse::ok(med, "更新成功")))
        }
        None => Err(AppError::medication_not_found()),
    }
}

pub async fn delete_medication(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let db = state.db.clone();
    run_blocking(move || db.delete_medication(&id)).await?;
    Ok(Json(ApiResponse::ok_msg("删除成功")))
}

// --- Detected Drugs from Expenses ---

pub async fn list_detected_drugs(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<DetectedDrug>>>, AppError> {
    let db = state.db.clone();
    let drugs = run_blocking(move || db.list_detected_drugs(&patient_id)).await?;
    Ok(Json(ApiResponse::ok(drugs, "查询成功")))
}
