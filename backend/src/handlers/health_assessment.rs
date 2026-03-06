use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::StreamExt;
use tokio_stream::Stream;

use crate::auth::AuthUser;
use crate::db::Database;
use crate::error::{run_blocking, AppError};
use crate::models::ApiResponse;
use crate::AppState;

use super::{INTERPRET_API_URL, INTERPRET_MODEL};

pub async fn health_assessment(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    let api_key = super::get_interpret_api_key(&state.db, &auth.0.sub)?;

    // Gather patient info
    let db = state.db.clone();
    let pid = patient_id.clone();
    let patient = run_blocking(move || db.get_patient(&pid)).await?;
    let patient = patient.ok_or_else(|| AppError::patient_not_found())?;

    // Gather all reports with items
    let db = state.db.clone();
    let pid = patient_id.clone();
    let reports = run_blocking(move || {
        let summaries = db.list_reports_with_summary_by_patient(&pid)?;
        let report_ids: Vec<String> = summaries.iter().map(|summary| summary.report.id.clone()).collect();
        let items_by_report = db.get_test_items_by_report_ids(&report_ids)?;
        let mut details = Vec::new();
        for summary in summaries {
            if let Some(items) = items_by_report.get(&summary.report.id) {
                details.push((summary.report, items.clone()));
            }
        }
        Ok::<_, AppError>(details)
    })
    .await?;

    // Gather medications
    let db = state.db.clone();
    let pid = patient_id.clone();
    let meds = run_blocking(move || db.list_medications_by_patient(&pid)).await?;

    // Gather temperatures
    let db = state.db.clone();
    let pid = patient_id.clone();
    let temps = run_blocking(move || db.list_temperatures_by_patient(&pid)).await?;

    // Build prompt
    let mut prompt = format!(
        "请对以下患者进行综合健康风险评估。\n\n患者信息：\n- 姓名：{}\n- 性别：{}\n- 出生日期：{}\n",
        patient.name, patient.gender, patient.dob
    );

    if !reports.is_empty() {
        prompt.push_str(&format!("\n共有 {} 份检查报告：\n", reports.len()));
        for (report, items) in &reports {
            prompt.push_str(&format!(
                "\n【{}】 {} ({})\n",
                report.report_date, report.report_type, report.hospital
            ));
            for item in items {
                let flag = if item.status.is_abnormal() {
                    format!(" [{}]", item.status)
                } else {
                    String::new()
                };
                prompt.push_str(&format!(
                    "  - {}: {} {} (参考: {}){}\n",
                    item.name, item.value, item.unit, item.reference_range, flag
                ));
            }
        }
    }

    if !meds.is_empty() {
        prompt.push_str("\n当前用药：\n");
        for med in &meds {
            let status = if med.active { "使用中" } else { "已停用" };
            prompt.push_str(&format!(
                "  - {} {} {}（{}）\n",
                med.name, med.dosage, med.frequency, status
            ));
        }
    }

    if !temps.is_empty() {
        let recent: Vec<_> = temps.iter().take(10).collect();
        prompt.push_str("\n最近体温记录：\n");
        for t in recent {
            prompt.push_str(&format!("  - {} : {}℃\n", t.recorded_at, t.value));
        }
    }

    let client = state.http_client.clone();
    let save_to = Some((state.db.clone(), patient_id));
    let stream = build_assessment_stream(client, prompt, api_key, save_to);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn get_cached_assessment(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let db = state.db.clone();
    let pid = patient_id.clone();
    let cached = run_blocking(move || db.get_assessment(&pid)).await?;
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
            message: "暂无缓存评估".to_string(),
        })),
    }
}

fn build_assessment_stream(
    client: reqwest::Client,
    prompt: String,
    api_key: String,
    save_to: Option<(Database, String)>,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        let body = serde_json::json!({
            "model": INTERPRET_MODEL,
            "stream": true,
            "temperature": 0.6,
            "max_tokens": 4096,
            "messages": [
                {
                    "role": "system",
                    "content": "你是一位面向普通患者的健康风险评估助手。\n\n请严格只输出一个 JSON 对象，不要输出任何额外文字、不要用 Markdown 代码块。\nJSON 格式如下：\n{\n  \"overall_status\": \"正常/需关注/需就医\",\n  \"risk_level\": \"低/中/高\",\n  \"summary\": \"一段简要的整体评估（2-3句话）\",\n  \"findings\": [\"发现1\", \"发现2\", ...],\n  \"recommendations\": [\"建议1\", \"建议2\", ...],\n  \"follow_up_suggestions\": [\"随访建议1\", ...],\n  \"disclaimer\": \"以上评估仅供参考，具体请遵医嘱\"\n}\n\n要求：\n1. 用大白话，避免医学术语\n2. 综合所有报告数据、用药信息、体温趋势给出整体评估\n3. 重点关注异常指标的变化趋势\n4. 给出切实可行的生活方式建议\n5. 如有严重异常，明确建议就医科室"
                },
                { "role": "user", "content": prompt }
            ],
        });

        let resp = client
            .post(INTERPRET_API_URL)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await;

        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                yield Ok(Event::default().data(format!("[错误] LLM API 错误 (HTTP {}): {}", status, text)));
                return;
            }
            Err(e) => {
                yield Ok(Event::default().data(format!("[错误] LLM API 请求失败: {}", e)));
                return;
            }
        };

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut accumulated = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(_) => break,
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() || !line.starts_with("data:") {
                    continue;
                }
                let data = line.strip_prefix("data:").unwrap().trim();
                if data == "[DONE]" {
                    let parsed = crate::handlers::extract_json_block(&accumulated)
                        .and_then(|json_str| {
                            serde_json::from_str::<serde_json::Value>(&json_str)
                                .map(|v| serde_json::to_string(&v).unwrap_or(json_str))
                                .map_err(|e| format!("解析 JSON 失败: {}", e))
                        });
                    match parsed {
                        Ok(ref json) => {
                            if let Some((ref db, ref pid)) = save_to {
                                let db = db.clone();
                                let pid = pid.clone();
                                let json_to_save = json.clone();
                                let _ = tokio::task::spawn_blocking(move || {
                                    db.save_assessment(&pid, &json_to_save)
                                }).await;
                            }
                            yield Ok(Event::default().data(json.clone()));
                        }
                        Err(msg) => yield Ok(Event::default().data(format!("[错误] {}", msg))),
                    }
                    yield Ok(Event::default().data("[DONE]"));
                    return;
                }

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(content) = json
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        let cleaned = super::strip_think_blocks(content);
                        if !cleaned.is_empty() {
                            accumulated.push_str(&cleaned);
                        }
                    }
                }
            }
        }

        if !accumulated.is_empty() {
            let parsed = crate::handlers::extract_json_block(&accumulated)
                .and_then(|json_str| {
                    serde_json::from_str::<serde_json::Value>(&json_str)
                        .map(|v| serde_json::to_string(&v).unwrap_or(json_str))
                        .map_err(|e| format!("解析 JSON 失败: {}", e))
                });
            match parsed {
                Ok(ref json) => {
                    if let Some((ref db, ref pid)) = save_to {
                        let db = db.clone();
                        let pid = pid.clone();
                        let json_to_save = json.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            db.save_assessment(&pid, &json_to_save)
                        }).await;
                    }
                    yield Ok(Event::default().data(json.clone()));
                }
                Err(msg) => yield Ok(Event::default().data(format!("[错误] {}", msg))),
            }
        }
        yield Ok(Event::default().data("[DONE]"));
    }
}
