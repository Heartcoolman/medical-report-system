use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::StreamExt;
use tokio_stream::Stream;

use crate::error::{run_blocking, AppError};
use crate::AppState;

use super::{get_interpret_api_key, INTERPRET_API_URL, INTERPRET_MODEL};

// ---------------------------------------------------------------------------
// Shared SSE streaming helper
// ---------------------------------------------------------------------------

/// Build the streaming request body and return an SSE stream that
/// forwards `delta.content` chunks from the LLM response.
/// When `save_to` is Some((db, report_id)), the accumulated content is
/// persisted to the database after the stream completes successfully.
fn llm_sse_stream(
    client: reqwest::Client,
    prompt: String,
    save_to: Option<(crate::db::Database, String)>,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        tracing::info!("[interpret] 开始构建 LLM 请求, model={}, url={}", INTERPRET_MODEL, INTERPRET_API_URL);
        let body = serde_json::json!({
            "model": INTERPRET_MODEL,
            "stream": true,
            "temperature": 0.6,
            "max_tokens": 2048,
            "messages": [
                {
                    "role": "system",
                    "content": "你是一位面向普通患者的检验报告解读助手。\n\n请严格只输出一个 JSON 数组（数组元素为字符串），不要输出任何额外文字、不要用 Markdown 代码块。\nJSON 格式如下：\n[\n  \"要点1（1-2句话，大白话）\",\n  \"要点2（1-2句话，大白话）\"\n]\n\n要求：\n1. 数组中每个字符串都是一个独立要点，1-2句话即可\n2. 避免医学术语；如必须使用，需在括号中用通俗话解释\n3. 如果所有指标均正常，数组里直接说明总体正常即可，不要硬找问题\n4. 对于严重异常值（如远超参考范围），要明确建议尽快就医（说明建议就诊科室方向即可）\n5. 不要给出具体药物或治疗方案\n6. 结尾请在数组最后追加一条免责声明：以上解读仅供参考，具体请遵医嘱"
                },
                { "role": "user", "content": prompt }
            ],
        });

        tracing::info!("[interpret] 发送请求到 {}", INTERPRET_API_URL);
        let resp = client
            .post(INTERPRET_API_URL)
            .header("Authorization", format!("Bearer {}", get_interpret_api_key()))
            .json(&body)
            .send()
            .await;
        tracing::info!("[interpret] 请求完成, 结果: {}", if resp.is_ok() { "成功" } else { "失败" });

        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                let msg = format!("LLM API 错误 (HTTP {}): {}", status, text);
                tracing::error!("{}", msg);
                yield Ok(Event::default().data(format!("[错误] {}", msg)));
                return;
            }
            Err(e) => {
                let msg = format!("LLM API 请求失败: {}", e);
                tracing::error!("{}", msg);
                yield Ok(Event::default().data(format!("[错误] {}", msg)));
                return;
            }
        };

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut accumulated_raw = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("读取 LLM 流失败: {}", e);
                    break;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE lines from the buffer
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() || !line.starts_with("data:") {
                    continue;
                }

                let data = line.strip_prefix("data:").unwrap().trim();
                if data == "[DONE]" {
                    let parsed = crate::handlers::extract_json_block(&accumulated_raw)
                        .and_then(|json_str| {
                            serde_json::from_str::<serde_json::Value>(&json_str)
                                .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| json_str))
                                .map_err(|e| format!("解析解读 JSON 失败: {}, 原始: {}", e, accumulated_raw))
                        });

                    // Save accumulated content to DB
                    if let Some((ref db, ref report_id)) = save_to {
                        if let Ok(content) = parsed.clone() {
                            if !content.is_empty() {
                                let db = db.clone();
                                let rid = report_id.clone();
                                if let Err(e) = run_blocking(move || db.save_interpretation(&rid, &content)).await {
                                    tracing::error!("[interpret] 保存解读缓存失败: {}", e);
                                } else {
                                    tracing::info!("[interpret] 解读结果已缓存, report_id={}", report_id);
                                }
                            }
                        } else if !accumulated_raw.is_empty() {
                            tracing::warn!("[interpret] 解读 JSON 解析失败, report_id={}", report_id);
                        }
                    }

                    match parsed {
                        Ok(json) => {
                            yield Ok(Event::default().data(json));
                        }
                        Err(msg) => {
                            tracing::error!("[interpret] {}", msg);
                            yield Ok(Event::default().data(format!("[错误] {}", msg)));
                        }
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
                            accumulated_raw.push_str(&cleaned);
                        }
                    }
                }
            }
        }

        // Save on stream end (even without [DONE])
        if !accumulated_raw.is_empty() {
            let parsed = crate::handlers::extract_json_block(&accumulated_raw)
                .and_then(|json_str| {
                    serde_json::from_str::<serde_json::Value>(&json_str)
                        .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| json_str))
                        .map_err(|e| format!("解析解读 JSON 失败: {}, 原始: {}", e, accumulated_raw))
                });

            if let Some((ref db, ref report_id)) = save_to {
                if let Ok(content) = parsed.clone() {
                    if !content.is_empty() {
                        let db = db.clone();
                        let rid = report_id.clone();
                        if let Err(e) = run_blocking(move || db.save_interpretation(&rid, &content)).await {
                            tracing::error!("[interpret] 保存解读缓存失败: {}", e);
                        }
                    }
                } else {
                    tracing::warn!("[interpret] 解读 JSON 解析失败, report_id={}", report_id);
                }
            }

            match parsed {
                Ok(json) => {
                    yield Ok(Event::default().data(json));
                }
                Err(msg) => {
                    tracing::error!("[interpret] {}", msg);
                    yield Ok(Event::default().data(format!("[错误] {}", msg)));
                }
            }
        }
        yield Ok(Event::default().data("[DONE]"));
    }
}

// ---------------------------------------------------------------------------
// Prompt builders
// ---------------------------------------------------------------------------

fn format_test_items(items: &[crate::models::TestItem]) -> String {
    let mut lines = Vec::with_capacity(items.len());
    for item in items {
        let flag = match item.status {
            crate::models::ItemStatus::High => " ↑偏高",
            crate::models::ItemStatus::Low => " ↓偏低",
            crate::models::ItemStatus::Normal => "",
        };
        lines.push(format!(
            "- {}: {} {} (参考范围: {}){}",
            item.name, item.value, item.unit, item.reference_range, flag
        ));
    }
    lines.join("\n")
}

fn format_report_block(
    report: &crate::models::Report,
    items: &[crate::models::TestItem],
) -> String {
    let mut s = format!(
        "【{}】 日期: {} 医院: {}\n",
        report.report_type,
        report.report_date,
        if report.hospital.is_empty() {
            "未知"
        } else {
            &report.hospital
        }
    );
    s.push_str(&format_test_items(items));
    s
}

fn format_trend_points(item_name: &str, points: &[crate::models::TrendPoint]) -> String {
    let mut lines = vec![format!("检验项目: {}", item_name)];
    for p in points {
        let date = if p.sample_date.is_empty() {
            &p.report_date
        } else {
            &p.sample_date
        };
        let flag = match p.status {
            crate::models::ItemStatus::High => " ↑",
            crate::models::ItemStatus::Low => " ↓",
            crate::models::ItemStatus::Normal => "",
        };
        lines.push(format!(
            "- {}: {} {} (参考: {}){}",
            date, p.value, p.unit, p.reference_range, flag
        ));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// 1. Single report interpretation
// ---------------------------------------------------------------------------

pub async fn interpret_single_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    let db = state.db.clone();
    let id_clone = id.clone();
    let report = run_blocking(move || db.get_report(&id_clone)).await?;
    let report = report.ok_or_else(|| AppError::NotFound("报告不存在".to_string()))?;

    // Load patient info for personalized interpretation
    let db = state.db.clone();
    let pid = report.patient_id.clone();
    let patient = run_blocking(move || db.get_patient(&pid)).await?;

    let db = state.db.clone();
    let rid = report.id.clone();
    let items = run_blocking(move || db.get_test_items_by_report(&rid)).await?;

    let patient_ctx = if let Some(ref p) = patient {
        format!(
            "患者：{} {}{}\n\n",
            p.name,
            p.gender,
            if p.dob.is_empty() {
                String::new()
            } else {
                format!(" 出生日期: {}", p.dob)
            }
        )
    } else {
        String::new()
    };

    let prompt = format!(
        "{}请用大白话解读这份检验报告，并按 system 要求输出 JSON 数组（要点字符串）。\n\n{}\n\n\
         请在要点中覆盖：这份报告主要查什么；哪些指标不正常及通俗解释；总体情况；生活上需要注意什么。",
        patient_ctx,
        format_report_block(&report, &items)
    );

    let save_to = Some((state.db.clone(), id));
    let stream = llm_sse_stream(state.http_client.clone(), prompt, save_to);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// 2. Multi-report interpretation
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct MultiInterpretQuery {
    pub report_ids: String,
}

pub async fn interpret_multi(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Query(params): Query<MultiInterpretQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    let ids: Vec<String> = params
        .report_ids
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if ids.is_empty() {
        return Err(AppError::BadRequest("缺少 report_ids 参数".to_string()));
    }

    // Load patient info
    let db = state.db.clone();
    let pid = patient_id.clone();
    let patient = run_blocking(move || db.get_patient(&pid)).await?;
    let patient = patient.ok_or_else(|| AppError::NotFound("患者不存在".to_string()))?;

    let mut report_blocks = Vec::new();
    for id in &ids {
        let db = state.db.clone();
        let id_clone = id.clone();
        if let Some(report) = run_blocking(move || db.get_report(&id_clone)).await? {
            let db = state.db.clone();
            let rid = report.id.clone();
            let items = run_blocking(move || db.get_test_items_by_report(&rid)).await?;
            report_blocks.push(format_report_block(&report, &items));
        }
    }

    if report_blocks.is_empty() {
        return Err(AppError::NotFound("未找到指定报告".to_string()));
    }

    let prompt = format!(
        "患者：{} {} {}\n\n\
         以下是这位患者的 {} 份检验报告，请用大白话综合解读，并按 system 要求输出 JSON 数组（要点字符串）：\n\n{}\n\n\
         请在要点中覆盖：每份报告主要发现；不同报告之间的关联；同一指标是否持续异常；整体健康状况；需要注意什么以及是否建议就医（就诊方向即可）。",
        patient.name,
        patient.gender,
        if patient.dob.is_empty() {
            String::new()
        } else {
            format!("出生日期: {}", patient.dob)
        },
        report_blocks.len(),
        report_blocks.join("\n\n")
    );

    let stream = llm_sse_stream(state.http_client.clone(), prompt, None);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// 3. All reports interpretation
// ---------------------------------------------------------------------------

pub async fn interpret_all(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    let db = state.db.clone();
    let pid = patient_id.clone();
    let patient = run_blocking(move || db.get_patient(&pid)).await?;
    let patient = patient.ok_or_else(|| AppError::NotFound("患者不存在".to_string()))?;

    let db = state.db.clone();
    let pid = patient_id.clone();
    let reports = run_blocking(move || db.list_reports_by_patient(&pid)).await?;

    if reports.is_empty() {
        return Err(AppError::NotFound("该患者暂无报告".to_string()));
    }

    let mut report_blocks = Vec::new();
    for report in &reports {
        let db = state.db.clone();
        let rid = report.id.clone();
        let items = run_blocking(move || db.get_test_items_by_report(&rid)).await?;
        report_blocks.push(format_report_block(report, &items));
    }

    let prompt = format!(
        "患者：{} {} {}\n\n\
         以下是这位患者的全部 {} 份检验报告，请用大白话全面解读，并按 system 要求输出 JSON 数组（要点字符串）：\n\n{}\n\n\
         请在要点中覆盖：都做了哪些检查及主要发现；哪些指标不正常及通俗解释；异常指标在不同报告间的关联；同一指标是否持续异常；总体状况；最需要关注什么以及建议（就医方向即可）。",
        patient.name,
        patient.gender,
        if patient.dob.is_empty() {
            String::new()
        } else {
            format!("出生日期: {}", patient.dob)
        },
        report_blocks.len(),
        report_blocks.join("\n\n")
    );

    let stream = llm_sse_stream(state.http_client.clone(), prompt, None);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// 4. Trend interpretation
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct TrendInterpretQuery {
    #[serde(default)]
    pub report_type: Option<String>,
}

pub async fn interpret_trend(
    State(state): State<AppState>,
    Path((patient_id, item_name)): Path<(String, String)>,
    Query(params): Query<TrendInterpretQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    let db = state.db.clone();
    let pid = patient_id.clone();
    let name = item_name.clone();
    let rt = params.report_type.clone();
    let points = run_blocking(move || db.get_trends(&pid, &name, rt.as_deref())).await?;

    if points.is_empty() {
        return Err(AppError::NotFound("暂无趋势数据".to_string()));
    }

    let prompt = format!(
        "以下是患者一个检查指标的多次结果，请按 system 要求输出 JSON 数组（要点字符串），用大白话简洁分析：\n\n{}\n\n\
         请在要点中覆盖：趋势方向（升高/降低/波动）；是否异常及偏离程度；是否回到/远离正常区间；这种变化可能说明什么；需不需要注意或就医方向。",
        format_trend_points(&item_name, &points)
    );

    let stream = llm_sse_stream(state.http_client.clone(), prompt, None);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// 5. Time-span change interpretation
// ---------------------------------------------------------------------------

pub async fn interpret_trend_time(
    State(state): State<AppState>,
    Path((patient_id, item_name)): Path<(String, String)>,
    Query(params): Query<TrendInterpretQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    let db = state.db.clone();
    let pid = patient_id.clone();
    let name = item_name.clone();
    let rt = params.report_type.clone();
    let points = run_blocking(move || db.get_trends(&pid, &name, rt.as_deref())).await?;

    if points.len() < 2 {
        return Err(AppError::BadRequest(
            "至少需要2个数据点才能进行时间变化分析".to_string(),
        ));
    }

    // Pre-compute change summaries to enrich the prompt
    let mut changes = Vec::new();
    for i in 1..points.len() {
        let prev = &points[i - 1];
        let curr = &points[i];
        let prev_date = if prev.sample_date.is_empty() {
            &prev.report_date
        } else {
            &prev.sample_date
        };
        let curr_date = if curr.sample_date.is_empty() {
            &curr.report_date
        } else {
            &curr.sample_date
        };
        if let (Ok(pv), Ok(cv)) = (prev.value.parse::<f64>(), curr.value.parse::<f64>()) {
            let diff = cv - pv;
            let pct = if pv.abs() > 1e-9 {
                format!("{:+.1}%", diff / pv * 100.0)
            } else {
                "N/A".to_string()
            };
            changes.push(format!(
                "  {} → {}: {} → {} (变化: {:+.2}, {})",
                prev_date, curr_date, prev.value, curr.value, diff, pct
            ));
        }
    }

    let prompt = format!(
        "以下是患者一个检查指标在不同时间的结果，请按 system 要求输出 JSON 数组（要点字符串），重点说说变化情况：\n\n\
         {}\n\n\
         各次变化：\n{}\n\n\
         请在要点中覆盖：这段时间查了多久/几次；每次变化是否明显、是变好还是变差；有没有特别明显的一次；最近一次是否回到正常区间；整体趋势（好转/恶化/波动）；需要注意或就医方向。",
        format_trend_points(&item_name, &points),
        changes.join("\n")
    );

    let stream = llm_sse_stream(state.http_client.clone(), prompt, None);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
