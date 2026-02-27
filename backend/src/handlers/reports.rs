use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{run_blocking, AppError, ErrorCode};
use crate::models::{
    ApiResponse, CreateReportReq, CreateTestItemReq, EditLog, FieldChange, PaginatedList,
    PaginationParams, Report, ReportDetail, ReportSummary, TestItem, TrendItemInfo, TrendPoint,
    UpdateTestItemReq,
};
use crate::AppState;

#[derive(Deserialize)]
pub struct UpdateReportReq {
    pub report_type: Option<String>,
    pub hospital: Option<String>,
    pub report_date: Option<String>,
    pub sample_date: Option<String>,
}

pub async fn create_report(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<CreateReportReq>,
) -> Result<Json<ApiResponse<Report>>, AppError> {
    if let Err(msg) = req.validate() {
        return Err(AppError::validation(msg));
    }
    // Verify patient exists
    let db = state.db.clone();
    let pid = patient_id.clone();
    let exists = run_blocking(move || db.get_patient(&pid)).await?;
    if exists.is_none() {
        return Err(AppError::patient_not_found());
    }
    let report = Report {
        id: Uuid::new_v4().to_string(),
        patient_id,
        report_type: req.report_type,
        hospital: req.hospital,
        report_date: req.report_date,
        sample_date: req.sample_date,
        file_path: req.file_path,
        created_at: Utc::now().to_rfc3339(),
    };
    let db = state.db.clone();
    let r = report.clone();
    run_blocking(move || db.create_report(&r)).await?;
    Ok(Json(ApiResponse::ok(report, "创建成功")))
}

pub async fn get_report_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ReportDetail>>, AppError> {
    let db = state.db.clone();
    let id_clone = id.clone();
    let report = run_blocking(move || db.get_report(&id_clone)).await?;
    match report {
        Some(report) => {
            let db = state.db.clone();
            let rid = report.id.clone();
            let items = run_blocking(move || db.get_test_items_by_report(&rid)).await?;
            let detail = ReportDetail {
                report,
                test_items: items,
            };
            Ok(Json(ApiResponse::ok(detail, "查询成功")))
        }
        None => Err(AppError::report_not_found()),
    }
}

pub async fn list_reports_by_patient(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<ApiResponse<PaginatedList<ReportSummary>>>, AppError> {
    let (page, page_size) = pagination.normalize();
    let db = state.db.clone();
    let result =
        run_blocking(move || db.list_reports_with_summary_by_patient_paginated(&patient_id, page, page_size)).await?;
    Ok(Json(ApiResponse::ok(result, "查询成功")))
}

pub async fn delete_report_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    // Invalidate cached interpretation before deleting
    let db = state.db.clone();
    let rid = id.clone();
    let _ = run_blocking(move || db.delete_interpretation(&rid)).await;
    let db = state.db.clone();
    run_blocking(move || db.delete_report_with_index_cleanup(&id)).await?;
    Ok(Json(ApiResponse::ok_msg("删除成功")))
}

pub async fn update_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: AuthUser,
    Json(req): Json<UpdateReportReq>,
) -> Result<Json<ApiResponse<Report>>, AppError> {
    let db = state.db.clone();
    let id_clone = id.clone();
    let existing = run_blocking(move || db.get_report(&id_clone)).await?;
    match existing {
        Some(old_report) => {
            let mut report = old_report.clone();
            let mut changes = Vec::new();
            if let Some(ref rt) = req.report_type {
                if *rt != old_report.report_type {
                    changes.push(FieldChange {
                        field: "报告类型".to_string(),
                        old_value: old_report.report_type.clone(),
                        new_value: rt.clone(),
                    });
                }
                report.report_type = rt.clone();
            }
            if let Some(ref h) = req.hospital {
                if *h != old_report.hospital {
                    changes.push(FieldChange {
                        field: "医院".to_string(),
                        old_value: old_report.hospital.clone(),
                        new_value: h.clone(),
                    });
                }
                report.hospital = h.clone();
            }
            if let Some(ref rd) = req.report_date {
                if *rd != old_report.report_date {
                    changes.push(FieldChange {
                        field: "报告日期".to_string(),
                        old_value: old_report.report_date.clone(),
                        new_value: rd.clone(),
                    });
                }
                report.report_date = rd.clone();
            }
            if let Some(ref sd) = req.sample_date {
                if *sd != old_report.sample_date {
                    changes.push(FieldChange {
                        field: "采样日期".to_string(),
                        old_value: old_report.sample_date.clone(),
                        new_value: sd.clone(),
                    });
                }
                report.sample_date = sd.clone();
            }
            let db = state.db.clone();
            let r = report.clone();
            run_blocking(move || db.update_report(&r)).await?;
            // Invalidate cached interpretation
            let db = state.db.clone();
            let rid = id.clone();
            let _ = run_blocking(move || db.delete_interpretation(&rid)).await;
            // Log changes
            if !changes.is_empty() {
                let changed_fields: Vec<&str> = changes.iter().map(|c| c.field.as_str()).collect();
                let summary = format!("修改了报告的{}", changed_fields.join("、"));
                let log = EditLog {
                    id: Uuid::new_v4().to_string(),
                    report_id: report.id.clone(),
                    patient_id: report.patient_id.clone(),
                    action: "update".to_string(),
                    target_type: "report".to_string(),
                    target_id: report.id.clone(),
                    summary,
                    changes,
                    created_at: Utc::now().to_rfc3339(),
                    operator_id: Some(auth.0.sub.clone()),
                    operator_name: Some(auth.0.username.clone()),
                };
                let db = state.db.clone();
                let _ = run_blocking(move || db.create_edit_log(&log)).await;
            }
            Ok(Json(ApiResponse::ok(report, "更新成功")))
        }
        None => Err(AppError::report_not_found()),
    }
}

pub async fn create_test_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateTestItemReq>,
) -> Result<Json<ApiResponse<TestItem>>, AppError> {
    // Get report to find patient_id for logging
    let db = state.db.clone();
    let rid = req.report_id.clone();
    let report = run_blocking(move || db.get_report(&rid)).await?;
    let report = report.ok_or_else(|| AppError::report_not_found())?;

    let item = TestItem {
        id: Uuid::new_v4().to_string(),
        report_id: req.report_id,
        name: req.name,
        value: req.value,
        unit: req.unit,
        reference_range: req.reference_range,
        status: req.status,
        canonical_name: String::new(),
    };
    let db = state.db.clone();
    let i = item.clone();
    run_blocking(move || db.create_test_item(&i)).await?;
    // Invalidate cached interpretation for the report
    let db = state.db.clone();
    let rid2 = item.report_id.clone();
    let _ = run_blocking(move || db.delete_interpretation(&rid2)).await;
    // Log creation
    let log = EditLog {
        id: Uuid::new_v4().to_string(),
        report_id: item.report_id.clone(),
        patient_id: report.patient_id.clone(),
        action: "create".to_string(),
        target_type: "test_item".to_string(),
        target_id: item.id.clone(),
        summary: format!("新增了检验项目「{}」", item.name),
        changes: vec![],
        created_at: Utc::now().to_rfc3339(),
        operator_id: Some(auth.0.sub.clone()),
        operator_name: Some(auth.0.username.clone()),
    };
    let db = state.db.clone();
    let _ = run_blocking(move || db.create_edit_log(&log)).await;
    Ok(Json(ApiResponse::ok(item, "创建成功")))
}

pub async fn get_test_items_by_report(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<TestItem>>>, AppError> {
    let db = state.db.clone();
    let items = run_blocking(move || db.get_test_items_by_report(&report_id)).await?;
    Ok(Json(ApiResponse::ok(items, "查询成功")))
}

pub async fn get_trends(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<TrendPoint>>>, AppError> {
    let item_name = match params.get("item_name") {
        Some(name) => name.clone(),
        None => return Err(AppError::new(ErrorCode::MissingParameter, "缺少 item_name 参数")),
    };
    let report_type = params.get("report_type").cloned();
    let db = state.db.clone();
    let points =
        run_blocking(move || db.get_trends(&patient_id, &item_name, report_type.as_deref()))
            .await?;
    Ok(Json(ApiResponse::ok(points, "查询成功")))
}

pub async fn list_trend_items(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<TrendItemInfo>>>, AppError> {
    let db = state.db.clone();
    let items = run_blocking(move || db.list_trend_items_by_patient(&patient_id)).await?;
    Ok(Json(ApiResponse::ok(items, "查询成功")))
}

pub async fn update_test_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: AuthUser,
    Json(req): Json<UpdateTestItemReq>,
) -> Result<Json<ApiResponse<TestItem>>, AppError> {
    let db = state.db.clone();
    let id_clone = id.clone();
    let old_item = run_blocking(move || db.get_test_item(&id_clone)).await?;
    let old_item = old_item.ok_or_else(|| AppError::test_item_not_found())?;

    let mut item = old_item.clone();
    let mut changes = Vec::new();

    if let Some(ref name) = req.name {
        if *name != old_item.name {
            changes.push(FieldChange {
                field: "名称".to_string(),
                old_value: old_item.name.clone(),
                new_value: name.clone(),
            });
        }
        item.name = name.clone();
    }
    if let Some(ref value) = req.value {
        if *value != old_item.value {
            changes.push(FieldChange {
                field: "结果值".to_string(),
                old_value: old_item.value.clone(),
                new_value: value.clone(),
            });
        }
        item.value = value.clone();
    }
    if let Some(ref unit) = req.unit {
        if *unit != old_item.unit {
            changes.push(FieldChange {
                field: "单位".to_string(),
                old_value: old_item.unit.clone(),
                new_value: unit.clone(),
            });
        }
        item.unit = unit.clone();
    }
    if let Some(ref rr) = req.reference_range {
        if *rr != old_item.reference_range {
            changes.push(FieldChange {
                field: "参考范围".to_string(),
                old_value: old_item.reference_range.clone(),
                new_value: rr.clone(),
            });
        }
        item.reference_range = rr.clone();
    }
    if let Some(ref status) = req.status {
        if *status != old_item.status {
            changes.push(FieldChange {
                field: "状态".to_string(),
                old_value: old_item.status.to_string(),
                new_value: status.to_string(),
            });
        }
        item.status = status.clone();
    }

    let db = state.db.clone();
    let i = item.clone();
    run_blocking(move || db.update_test_item(&i)).await?;

    // Invalidate cached interpretation
    let db = state.db.clone();
    let rid = item.report_id.clone();
    let _ = run_blocking(move || db.delete_interpretation(&rid)).await;

    // Log changes
    if !changes.is_empty() {
        let db = state.db.clone();
        let rid = item.report_id.clone();
        let report = run_blocking(move || db.get_report(&rid)).await?;
        let patient_id = report.map(|r| r.patient_id).unwrap_or_default();

        let changed_fields: Vec<&str> = changes.iter().map(|c| c.field.as_str()).collect();
        let summary = format!("修改了检验项目「{}」的{}", item.name, changed_fields.join("、"));
        let log = EditLog {
            id: Uuid::new_v4().to_string(),
            report_id: item.report_id.clone(),
            patient_id,
            action: "update".to_string(),
            target_type: "test_item".to_string(),
            target_id: item.id.clone(),
            summary,
            changes,
            created_at: Utc::now().to_rfc3339(),
            operator_id: Some(auth.0.sub.clone()),
            operator_name: Some(auth.0.username.clone()),
        };
        let db = state.db.clone();
        let _ = run_blocking(move || db.create_edit_log(&log)).await;
    }

    Ok(Json(ApiResponse::ok(item, "更新成功")))
}

pub async fn delete_test_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<()>>, AppError> {
    // Get item before deletion for logging
    let db = state.db.clone();
    let id_clone = id.clone();
    let item = run_blocking(move || db.get_test_item(&id_clone)).await?;
    let item = item.ok_or_else(|| AppError::test_item_not_found())?;

    // Get report for patient_id
    let db = state.db.clone();
    let rid = item.report_id.clone();
    let report = run_blocking(move || db.get_report(&rid)).await?;
    let patient_id = report.as_ref().map(|r| r.patient_id.clone()).unwrap_or_default();

    let db = state.db.clone();
    let id_clone2 = id.clone();
    run_blocking(move || db.delete_test_item(&id_clone2)).await?;

    // Invalidate cached interpretation
    let db = state.db.clone();
    let rid = item.report_id.clone();
    let _ = run_blocking(move || db.delete_interpretation(&rid)).await;

    // Log deletion
    let log = EditLog {
        id: Uuid::new_v4().to_string(),
        report_id: item.report_id.clone(),
        patient_id,
        action: "delete".to_string(),
        target_type: "test_item".to_string(),
        target_id: item.id.clone(),
        summary: format!("删除了检验项目「{}」(值: {}{})", item.name, item.value, if item.unit.is_empty() { String::new() } else { format!(" {}", item.unit) }),
        changes: vec![],
        created_at: Utc::now().to_rfc3339(),
        operator_id: Some(auth.0.sub.clone()),
        operator_name: Some(auth.0.username.clone()),
    };
    let db = state.db.clone();
    let _ = run_blocking(move || db.create_edit_log(&log)).await;

    Ok(Json(ApiResponse::ok_msg("删除成功")))
}

pub async fn list_edit_logs_by_report(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<EditLog>>>, AppError> {
    let db = state.db.clone();
    let logs = run_blocking(move || db.list_edit_logs_by_report(&report_id)).await?;
    Ok(Json(ApiResponse::ok(logs, "查询成功")))
}

pub async fn list_edit_logs_global(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<PaginatedList<EditLog>>>, AppError> {
    let page: usize = params
        .get("page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let page_size: usize = params
        .get("page_size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let db = state.db.clone();
    let result = run_blocking(move || db.list_edit_logs_global(page, page_size)).await?;
    Ok(Json(ApiResponse::ok(result, "查询成功")))
}

pub async fn get_cached_interpretation(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let db = state.db.clone();
    let rid = report_id.clone();
    let cached = run_blocking(move || db.get_interpretation(&rid)).await?;
    match cached {
        Some((content, created_at)) => {
            let parsed_content: serde_json::Value =
                serde_json::from_str(&content).unwrap_or_else(|_| serde_json::Value::String(content.clone()));
            let data = serde_json::json!({
                "content": parsed_content,
                "created_at": created_at,
            });
            Ok(Json(ApiResponse::ok(data, "查询成功")))
        }
        None => Ok(Json(ApiResponse {
            success: true,
            data: None,
            message: "暂无缓存解读".to_string(),
        })),
    }
}
