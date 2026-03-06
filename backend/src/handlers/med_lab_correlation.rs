use axum::{
    extract::{Path, Query, State},
    Json,
};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{run_blocking, AppError};
use crate::handlers;
use crate::models::ApiResponse;
use crate::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabValue {
    pub date: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedItem {
    pub item_name: String,
    pub canonical_name: String,
    pub before_values: Vec<LabValue>,
    pub during_values: Vec<LabValue>,
    pub before_avg: Option<f64>,
    pub during_avg: Option<f64>,
    pub change_pct: f64,
    pub trend: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedCorrelation {
    pub drug_name: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub affected_items: Vec<AffectedItem>,
    pub llm_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedLabCorrelationResult {
    pub correlations: Vec<MedCorrelation>,
}

#[derive(Debug, Deserialize)]
pub struct MedLabCorrelationQuery {
    pub refresh: Option<u8>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn get_correlation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Query(query): Query<MedLabCorrelationQuery>,
) -> Result<Json<ApiResponse<MedLabCorrelationResult>>, AppError> {
    let force_refresh = query.refresh.unwrap_or(0) == 1;
    let cache_key = format!("medlab:{patient_id}");
    if !force_refresh {
        if let Some(cached) = state.llm_cache.med_lab.get(&cache_key).await {
            if let Ok(result) = serde_json::from_str::<MedLabCorrelationResult>(&cached) {
                return Ok(Json(ApiResponse::ok(result, "查询成功（缓存）")));
            }
        }
    }

    // 1. Load medications
    let db = state.db.clone();
    let pid = patient_id.clone();
    let meds = run_blocking(move || db.list_medications_by_patient(&pid)).await?;

    if meds.is_empty() {
        return Ok(Json(ApiResponse::ok(
            MedLabCorrelationResult { correlations: vec![] },
            "暂无用药记录",
        )));
    }

    // 2. Load all reports with test items for this patient
    let db = state.db.clone();
    let pid = patient_id.clone();
    let reports = run_blocking(move || db.list_reports_by_patient(&pid)).await?;
    let report_ids: Vec<String> = reports.iter().map(|report| report.id.clone()).collect();
    let db = state.db.clone();
    let items_by_report = run_blocking(move || db.get_test_items_by_report_ids(&report_ids)).await?;

    // Collect all (report_date, test_item) pairs
    struct ItemWithDate {
        report_date: String,
        name: String,
        canonical_name: String,
        value: String,
        unit: String,
    }

    let mut all_items: Vec<ItemWithDate> = Vec::new();
    for report in &reports {
        if let Some(items) = items_by_report.get(&report.id) {
            for item in items {
                all_items.push(ItemWithDate {
                    report_date: report.report_date.clone(),
                    name: item.name.clone(),
                    canonical_name: if item.canonical_name.is_empty() {
                        item.name.clone()
                    } else {
                        item.canonical_name.clone()
                    },
                    value: item.value.clone(),
                    unit: item.unit.clone(),
                });
            }
        }
    }

    if all_items.is_empty() {
        return Ok(Json(ApiResponse::ok(
            MedLabCorrelationResult { correlations: vec![] },
            "暂无检验数据",
        )));
    }

    // 3. For each medication, find before/during values
    let mut correlations = Vec::new();

    for med in &meds {
        let start = &med.start_date;
        // before window: 30 days before start_date
        let before_start = shift_date(start, -30);

        let mut item_map: std::collections::HashMap<String, (Vec<LabValue>, Vec<LabValue>, String, String)> =
            std::collections::HashMap::new();

        for item in &all_items {
            if let Ok(val) = item.value.parse::<f64>() {
                let entry = item_map
                    .entry(item.canonical_name.clone())
                    .or_insert_with(|| (Vec::new(), Vec::new(), item.name.clone(), item.unit.clone()));

                let date = &item.report_date;
                if date >= &before_start && date < start {
                    // before
                    entry.0.push(LabValue {
                        date: date.clone(),
                        value: val,
                        unit: item.unit.clone(),
                    });
                } else if date >= start {
                    // during (from start_date onwards, up to end_date if set)
                    let in_range = match &med.end_date {
                        Some(end) => date <= end,
                        None => true,
                    };
                    if in_range {
                        entry.1.push(LabValue {
                            date: date.clone(),
                            value: val,
                            unit: item.unit.clone(),
                        });
                    }
                }
            }
        }

        let mut affected_items = Vec::new();
        for (canonical, (before, during, name, unit)) in &item_map {
            if before.is_empty() || during.is_empty() {
                continue;
            }
            let before_avg = before.iter().map(|v| v.value).sum::<f64>() / before.len() as f64;
            let during_avg = during.iter().map(|v| v.value).sum::<f64>() / during.len() as f64;
            let change_pct = if before_avg.abs() > 1e-9 {
                ((during_avg - before_avg) / before_avg) * 100.0
            } else {
                0.0
            };

            if change_pct.abs() < 10.0 {
                continue;
            }

            let trend = if change_pct > 0.0 {
                "worsened"
            } else {
                "improved"
            };

            affected_items.push(AffectedItem {
                item_name: name.clone(),
                canonical_name: canonical.clone(),
                before_values: before.clone(),
                during_values: during.clone(),
                before_avg: Some(round2(before_avg)),
                during_avg: Some(round2(during_avg)),
                change_pct: round2(change_pct),
                trend: trend.to_string(),
                unit: unit.clone(),
            });
        }

        // Sort by absolute change percentage descending
        affected_items.sort_by(|a, b| {
            b.change_pct
                .abs()
                .partial_cmp(&a.change_pct.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        correlations.push(MedCorrelation {
            drug_name: med.name.clone(),
            start_date: med.start_date.clone(),
            end_date: med.end_date.clone(),
            affected_items,
            llm_summary: None,
        });
    }

    let mut llm_candidates: Vec<(usize, String, usize, f64)> = correlations
        .iter()
        .enumerate()
        .filter_map(|(idx, correlation)| {
            if correlation.affected_items.is_empty() {
                return None;
            }
            let max_change = correlation
                .affected_items
                .iter()
                .map(|item| item.change_pct.abs())
                .fold(0.0, f64::max);
            Some((
                idx,
                build_llm_prompt(
                    &correlation.drug_name,
                    &correlation.start_date,
                    &correlation.end_date,
                    &correlation.affected_items,
                ),
                correlation.affected_items.len(),
                max_change,
            ))
        })
        .collect();
    llm_candidates.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
    });
    llm_candidates.truncate(3);

    let patient_id_for_llm = patient_id.clone();
    let user_id = auth.0.sub.clone();
    let summaries: Vec<(usize, Option<String>)> = stream::iter(llm_candidates.into_iter().map(|(idx, prompt, _, _)| {
        let state = state.clone();
        let patient_id = patient_id_for_llm.clone();
        let user_id = user_id.clone();
        async move {
            let summary = match call_llm_for_summary(&state, &patient_id, &user_id, &prompt).await {
                Ok(content) => Some(content),
                Err(err) => {
                    tracing::warn!("med-lab LLM summary failed: {}", err);
                    None
                }
            };
            (idx, summary)
        }
    }))
    .buffer_unordered(3)
    .collect()
    .await;

    for (idx, summary) in summaries {
        if let Some(correlation) = correlations.get_mut(idx) {
            correlation.llm_summary = summary;
        }
    }

    let result = MedLabCorrelationResult { correlations };
    if let Ok(serialized) = serde_json::to_string(&result) {
        state.llm_cache.med_lab.insert(cache_key, serialized).await;
    }

    Ok(Json(ApiResponse::ok(result, "查询成功")))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn shift_date(date: &str, days: i32) -> String {
    use chrono::NaiveDate;
    if let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        let shifted = d + chrono::Duration::days(days as i64);
        return shifted.format("%Y-%m-%d").to_string();
    }
    date.to_string()
}

fn build_llm_prompt(
    drug_name: &str,
    start_date: &str,
    end_date: &Option<String>,
    items: &[AffectedItem],
) -> String {
    let end_str = end_date.as_deref().unwrap_or("至今");
    let mut items_desc = String::new();
    for item in items {
        items_desc.push_str(&format!(
            "- {}：用药前均值 {:.2}{}，用药期间均值 {:.2}{}，变化 {:.1}%\n",
            item.canonical_name,
            item.before_avg.unwrap_or(0.0),
            item.unit,
            item.during_avg.unwrap_or(0.0),
            item.unit,
            item.change_pct,
        ));
    }
    format!(
        "分析以下用药期间检验指标的变化，给出简短的临床意义分析（不超过200字）：\n\
         药物：{}，用药时间：{} 至 {}\n\
         {}",
        drug_name, start_date, end_str, items_desc
    )
}

async fn call_llm_for_summary(
    state: &AppState,
    _patient_id: &str,
    user_id: &str,
    prompt: &str,
) -> Result<String, String> {
    let api_key = handlers::get_interpret_api_key(&state.db, user_id)
        .map_err(|e| e.message)?;

    let body = serde_json::json!({
        "model": handlers::INTERPRET_MODEL,
        "stream": false,
        "temperature": 0.6,
        "max_tokens": 512,
        "messages": [
            {
                "role": "system",
                "content": "你是一位临床药学分析助手。请根据提供的用药和检验数据，简短分析临床意义。直接输出分析文字，不要输出JSON或其他格式。"
            },
            { "role": "user", "content": prompt }
        ],
    });

    let resp = handlers::llm_post_with_retry(
        &state.http_client,
        handlers::INTERPRET_API_URL,
        &api_key,
        &body,
    )
    .await?;

    let json: serde_json::Value = resp.json().await.map_err(|e| format!("解析LLM响应失败: {}", e))?;
    let content = handlers::extract_llm_content(&json)?;
    Ok(content.trim().to_string())
}
