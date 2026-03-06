use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::algorithm_engine::trend_analyzer::analyze_item_trends;
use crate::auth::AuthUser;
use crate::error::{run_blocking, AppError};
use crate::models::ApiResponse;
use crate::AppState;

use super::{INTERPRET_API_URL, INTERPRET_MODEL};

#[derive(Deserialize)]
pub struct RiskPredictionQuery {
    pub refresh: Option<u8>,
}

pub async fn get_risk_prediction(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Query(query): Query<RiskPredictionQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let force_refresh = query.refresh.unwrap_or(0) == 1;

    // 检查缓存（7天内有效）
    if !force_refresh {
        let db = state.db.clone();
        let pid = patient_id.clone();
        if let Some((content, created_at)) = run_blocking(move || db.get_risk_prediction(&pid)).await? {
            // 检查是否在7天内
            if is_within_days(&created_at, 7) {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(mut data) => {
                        data["cached"] = serde_json::json!(true);
                        data["generated_at"] = serde_json::json!(created_at);
                        return Ok(Json(ApiResponse::ok(data, "查询成功（缓存）")));
                    }
                    Err(e) => {
                        tracing::warn!("risk_prediction cache parse failed: {}", e);
                    }
                }
            }
        }
    }

    // 生成新预测
    let api_key = super::get_interpret_api_key(&state.db, &auth.0.sub)?;
    let prompt = build_risk_prompt(&state, &patient_id).await?;

    let body = serde_json::json!({
        "model": INTERPRET_MODEL,
        "stream": false,
        "temperature": 0.4,
        "max_tokens": 2048,
        "messages": [
            {
                "role": "system",
                "content": "你是医疗风险预测专家，基于患者历史检验数据进行风险评估。\n\n请严格只输出一个JSON对象，不要有任何多余文字和Markdown代码块：\n{\n  \"risk_level\": \"低/中/高\",\n  \"risk_score\": 0-100整数,\n  \"risk_factors\": [\n    {\n      \"category\": \"检验异常/用药风险/体温异常/其他\",\n      \"severity\": \"低/中/高\",\n      \"description\": \"具体描述（大白话，不超过40字）\",\n      \"trend\": \"稳定/好转/恶化/波动\",\n      \"last_value\": \"最新值含单位\",\n      \"reference\": \"参考范围\"\n    }\n  ],\n  \"recommendations\": [\"建议1\", \"建议2\"],\n  \"next_review_date\": \"YYYY-MM-DD或null\"\n}\n\n评分规则：所有正常→0-20（低）；轻度异常无恶化→21-50（中）；严重异常或持续恶化→51-100（高）。\n要求：用大白话；危急值权重最高；不建议具体药物；数据不足时返回低风险并说明。"
            },
            { "role": "user", "content": prompt }
        ],
    });

    let resp = super::llm_post_with_retry(&state.http_client, INTERPRET_API_URL, &api_key, &body)
        .await
        .map_err(|e| AppError::internal(&format!("LLM 请求失败: {}", e)))?;

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::internal(&format!("LLM 响应解析失败: {}", e)))?;

    let content = super::extract_llm_content(&resp_json)
        .map_err(|e| AppError::internal(&format!("LLM 内容提取失败: {}", e)))?;

    let json_str = super::extract_json_block(&content)
        .map_err(|e| AppError::internal(&format!("JSON 提取失败: {}", e)))?;

    let mut data: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| AppError::internal(&format!("JSON 解析失败: {}", e)))?;

    // 从预测结果中提取 risk_level 用于更新患者字段
    let risk_level = data
        .get("risk_level")
        .and_then(|v| v.as_str())
        .map(|s| match s {
            "高" => "high",
            "中" => "medium",
            _ => "low",
        })
        .unwrap_or("low")
        .to_string();

    // 保存缓存
    let db = state.db.clone();
    let pid = patient_id.clone();
    let json_to_save = json_str.clone();
    if let Err(e) = run_blocking(move || db.save_risk_prediction(&pid, &json_to_save)).await {
        tracing::warn!("risk_prediction save failed: {}", e);
    }

    // 更新患者风险字段
    let db = state.db.clone();
    let pid = patient_id.clone();
    let rl = risk_level.clone();
    if let Err(e) = run_blocking(move || db.update_patient_risk_level(&pid, &rl)).await {
        tracing::warn!("risk_prediction update_patient_risk_level failed: {}", e);
    }

    let now = chrono::Utc::now().to_rfc3339();
    data["cached"] = serde_json::json!(false);
    data["generated_at"] = serde_json::json!(now);

    Ok(Json(ApiResponse::ok(data, "风险预测成功")))
}

async fn build_risk_prompt(state: &AppState, patient_id: &str) -> Result<String, AppError> {
    // 获取患者信息
    let db = state.db.clone();
    let pid = patient_id.to_string();
    let patient = run_blocking(move || db.get_patient(&pid))
        .await?
        .ok_or_else(AppError::patient_not_found)?;

    // 获取最近5份报告及检验项目
    let db = state.db.clone();
    let pid = patient_id.to_string();
    let reports = run_blocking(move || {
        let summaries = db.list_reports_with_summary_by_patient(&pid)?;
        let selected_reports: Vec<_> = summaries.into_iter().take(5).map(|summary| summary.report).collect();
        let report_ids: Vec<String> = selected_reports.iter().map(|report| report.id.clone()).collect();
        let items_by_report = db.get_test_items_by_report_ids(&report_ids)?;

        let mut details = Vec::new();
        for report in selected_reports {
            if let Some(items) = items_by_report.get(&report.id) {
                details.push((report, items.clone()));
            }
        }
        Ok::<_, AppError>(details)
    })
    .await?;

    // 获取用药
    let db = state.db.clone();
    let pid = patient_id.to_string();
    let meds = run_blocking(move || db.list_medications_by_patient(&pid)).await?;

    // 获取体温
    let db = state.db.clone();
    let pid = patient_id.to_string();
    let temps = run_blocking(move || db.list_temperatures_by_patient(&pid)).await?;

    // 获取可趋势分析的指标列表（最多分析10个异常指标的趋势）
    let db = state.db.clone();
    let pid = patient_id.to_string();
    let trend_items = run_blocking(move || db.list_trend_items_by_patient(&pid)).await?;

    let mut prompt = format!(
        "患者：{} {} 出生日期：{}\n\n",
        patient.name, patient.gender, patient.dob
    );

    // 最近报告的异常项
    if !reports.is_empty() {
        prompt.push_str("最近检验异常项：\n");
        for (report, items) in &reports {
            let abnormal: Vec<_> = items.iter().filter(|i| i.status.is_abnormal()).collect();
            if !abnormal.is_empty() {
                prompt.push_str(&format!(
                    "【{}】{}\n",
                    report.report_date, report.report_type
                ));
                for item in &abnormal {
                    prompt.push_str(&format!(
                        "  - {}: {} {} (参考: {}) [{}]\n",
                        item.name, item.value, item.unit, item.reference_range, item.status
                    ));
                }
            }
        }
    } else {
        prompt.push_str("暂无检验报告。\n");
    }

    // 关键指标趋势（取异常次数最多的前10个指标）
    let key_items: Vec<_> = trend_items
        .iter()
        .filter(|t| t.count >= 3)
        .take(10)
        .collect();

    if !key_items.is_empty() {
        let db = state.db.clone();
        let pid = patient_id.to_string();
        let item_names: Vec<String> = key_items.iter().map(|ti| ti.item_name.clone()).collect();
        let all_trends = run_blocking(move || {
            let mut results = Vec::new();
            for name in &item_names {
                let points = db.get_trends(&pid, name, None)?;
                results.push((name.clone(), points));
            }
            Ok::<_, AppError>(results)
        })
        .await?;

        prompt.push_str("\n关键指标趋势：\n");
        for (item_name, points) in &all_trends {
            if points.len() >= 2 {
                let trend = analyze_item_trends(points);
                let values: Vec<String> = points
                    .iter()
                    .rev()
                    .take(5)
                    .map(|p| p.value.clone())
                    .collect();
                prompt.push_str(&format!(
                    "  {}（{}次）：{} → 趋势: {}\n",
                    item_name,
                    points.len(),
                    values.join("→"),
                    trend["direction"].as_str().unwrap_or("unknown")
                ));
            }
        }
    }

    // 当前用药
    let active_meds: Vec<_> = meds.iter().filter(|m| m.active).collect();
    if !active_meds.is_empty() {
        prompt.push_str("\n当前用药：\n");
        for med in &active_meds {
            prompt.push_str(&format!("  - {} {} {}\n", med.name, med.dosage, med.frequency));
        }
    }

    // 体温异常
    let high_temps: Vec<_> = temps.iter().filter(|t| t.value >= 37.3).take(10).collect();
    if !high_temps.is_empty() {
        prompt.push_str("\n近期体温异常：\n");
        for t in &high_temps {
            prompt.push_str(&format!("  - {} : {}℃\n", t.recorded_at, t.value));
        }
    }

    prompt.push_str("\n请基于以上数据进行风险预测，输出JSON。");
    Ok(prompt)
}

/// 判断 SQLite datetime 字符串是否在 N 天之内。
/// SQLite datetime() 格式为 "YYYY-MM-DD HH:MM:SS"。
fn is_within_days(created_at: &str, days: i64) -> bool {
    // 尝试解析 SQLite datetime 格式
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S") {
        let age = chrono::Utc::now().naive_utc() - dt;
        return age.num_days() < days;
    }
    // 尝试 RFC3339 格式
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(created_at) {
        let age = chrono::Utc::now() - dt.with_timezone(&chrono::Utc);
        return age.num_days() < days;
    }
    false
}
