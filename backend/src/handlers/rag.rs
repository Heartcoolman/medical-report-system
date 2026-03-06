use axum::{
    extract::{Path, State},
    Json,
};
use futures_util::{stream, StreamExt, TryStreamExt};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{run_blocking, AppError, ErrorCode};
use crate::models::ApiResponse;
use crate::AppState;

use super::{INTERPRET_API_URL, INTERPRET_MODEL};

// ---------------------------------------------------------------------------
// Embedding API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

async fn get_embedding(
    http_client: &reqwest::Client,
    api_key: &str,
    text: &str,
) -> Result<Vec<f32>, AppError> {
    let body = serde_json::json!({
        "model": "BAAI/bge-m3",
        "input": [text],
    });

    let resp = http_client
        .post("https://api.siliconflow.cn/v1/embeddings")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::new(ErrorCode::LlmApiFailed, format!("Embedding API 请求失败: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::new(
            ErrorCode::LlmApiFailed,
            format!("Embedding API 错误 (HTTP {}): {}", status, text),
        ));
    }

    let result: EmbeddingResponse = resp
        .json()
        .await
        .map_err(|e| AppError::new(ErrorCode::LlmApiFailed, format!("Embedding API 响应解析失败: {}", e)))?;

    result
        .data
        .into_iter()
        .next()
        .map(|d| d.embedding)
        .ok_or_else(|| AppError::new(ErrorCode::LlmApiFailed, "Embedding API 返回空数据"))
}

// ---------------------------------------------------------------------------
// Cosine similarity
// ---------------------------------------------------------------------------

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b + 1e-8)
}

// ---------------------------------------------------------------------------
// Text chunk generation
// ---------------------------------------------------------------------------

fn status_zh(status: &crate::models::ItemStatus) -> &'static str {
    match status {
        crate::models::ItemStatus::Normal => "正常",
        crate::models::ItemStatus::High => "偏高",
        crate::models::ItemStatus::Low => "偏低",
        crate::models::ItemStatus::CriticalHigh => "危急偏高",
        crate::models::ItemStatus::CriticalLow => "危急偏低",
    }
}

fn build_report_chunk(
    report: &crate::models::Report,
    items: &[crate::models::TestItem],
) -> String {
    let mut lines = vec![format!(
        "报告日期：{}，类型：{}",
        report.report_date, report.report_type
    )];
    lines.push("指标数据：".to_string());
    for item in items {
        let name = if item.canonical_name.is_empty() {
            &item.name
        } else {
            &item.canonical_name
        };
        lines.push(format!(
            "- {}: {} {} [{}]（参考：{}）",
            name,
            item.value,
            item.unit,
            status_zh(&item.status),
            item.reference_range,
        ));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// POST /patients/:patient_id/rag/build
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub(crate) struct BuildResult {
    chunks_indexed: usize,
}

pub async fn build_rag(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<BuildResult>>, AppError> {
    let siliconflow_key = super::get_siliconflow_api_key(&state.db, &auth.0.sub)?;
    if siliconflow_key.is_empty() {
        return Err(AppError::new(
            ErrorCode::MissingParameter,
            "未配置 SiliconFlow API Key，请在设置中配置",
        ));
    }

    // Load all reports for the patient
    let db = state.db.clone();
    let pid = patient_id.clone();
    let reports = run_blocking(move || db.list_reports_by_patient(&pid)).await?;

    if reports.is_empty() {
        return Err(AppError::new(ErrorCode::NoData, "该患者暂无报告"));
    }

    let report_ids: Vec<String> = reports.iter().map(|report| report.id.clone()).collect();
    let db = state.db.clone();
    let items_by_report = run_blocking(move || db.get_test_items_by_report_ids(&report_ids)).await?;

    let chunks: Vec<(String, String)> = reports
        .iter()
        .filter_map(|report| {
            let items = items_by_report.get(&report.id)?;
            if items.is_empty() {
                return None;
            }
            Some((report.id.clone(), build_report_chunk(report, items)))
        })
        .collect();

    let chunks_indexed = chunks.len();
    if chunks_indexed == 0 {
        return Err(AppError::new(ErrorCode::NoData, "该患者暂无可索引的检验数据"));
    }

    let http_client = state.http_client.clone();
    let indexed_chunks: Vec<(String, String, String)> = stream::iter(chunks.into_iter().map(|(source_id, content)| {
        let http_client = http_client.clone();
        let siliconflow_key = siliconflow_key.clone();
        async move {
            let embedding = get_embedding(&http_client, &siliconflow_key, &content).await?;
            let embedding_json = serde_json::to_string(&embedding)
                .map_err(|e| AppError::internal(format!("序列化 embedding 失败: {}", e)))?;
            Ok::<_, AppError>((source_id, content, embedding_json))
        }
    }))
    .buffer_unordered(4)
    .try_collect()
    .await?;

    let db = state.db.clone();
    let pid = patient_id.clone();
    run_blocking(move || {
        db.with_conn(|conn| {
            let tx = conn.transaction()?;
            {
                let mut select_stmt = tx.prepare(
                    "SELECT id FROM rag_embeddings WHERE patient_id = ?1 AND source_id = ?2",
                )?;
                let mut update_stmt = tx.prepare(
                    "UPDATE rag_embeddings SET content = ?1, embedding = ?2, updated_at = datetime('now') WHERE id = ?3",
                )?;
                let mut insert_stmt = tx.prepare(
                    "INSERT INTO rag_embeddings (id, patient_id, chunk_type, source_id, content, embedding, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))",
                )?;

                for (source_id, content, embedding_json) in indexed_chunks {
                    let existing: Option<String> = select_stmt
                        .query_row(rusqlite::params![&pid, &source_id], |row| row.get(0))
                        .optional()?;

                    if let Some(existing_id) = existing {
                        update_stmt.execute(rusqlite::params![content, embedding_json, existing_id])?;
                    } else {
                        let id = uuid::Uuid::new_v4().to_string();
                        insert_stmt.execute(rusqlite::params![
                            id,
                            &pid,
                            "report_summary",
                            source_id,
                            content,
                            embedding_json
                        ])?;
                    }
                }
            }
            tx.commit()?;
            Ok(())
        })
    })
    .await?;

    Ok(Json(ApiResponse::ok(
        BuildResult { chunks_indexed },
        "知识库构建完成",
    )))
}

// ---------------------------------------------------------------------------
// POST /patients/:patient_id/rag/query
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct QueryRequest {
    question: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
}

fn default_top_k() -> usize {
    5
}

#[derive(Serialize, Clone)]
pub struct RagSource {
    chunk_type: String,
    content_preview: String,
    score: f32,
}

#[derive(Serialize)]
pub struct QueryResult {
    answer: String,
    sources: Vec<RagSource>,
}

pub async fn query_rag(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<ApiResponse<QueryResult>>, AppError> {
    if req.question.trim().is_empty() {
        return Err(AppError::validation("问题不能为空"));
    }

    let siliconflow_key = super::get_siliconflow_api_key(&state.db, &auth.0.sub)?;
    if siliconflow_key.is_empty() {
        return Err(AppError::new(
            ErrorCode::MissingParameter,
            "未配置 SiliconFlow API Key",
        ));
    }
    let interpret_key = super::get_interpret_api_key(&state.db, &auth.0.sub)?;

    // Embed the question
    let question_embedding = get_embedding(&state.http_client, &siliconflow_key, &req.question).await?;

    // Load all embeddings for this patient
    let db = state.db.clone();
    let pid = patient_id.clone();
    let rows = run_blocking(move || {
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT chunk_type, content, embedding FROM rag_embeddings WHERE patient_id = ?1",
            )?;
            let results = stmt
                .query_map(rusqlite::params![pid], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(results)
        })
    })
    .await?;

    if rows.is_empty() {
        return Err(AppError::new(
            ErrorCode::NoData,
            "知识库为空，请先构建知识库",
        ));
    }

    // Compute similarities and get top_k
    let mut scored: Vec<(f32, String, String)> = Vec::with_capacity(rows.len());
    for (chunk_type, content, embedding_json) in &rows {
        let emb: Vec<f32> = match serde_json::from_str(embedding_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("RAG embedding parse failed for chunk: {}", e);
                continue;
            }
        };
        if emb.is_empty() {
            continue;
        }
        let score = cosine_similarity(&question_embedding, &emb);
        scored.push((score, chunk_type.clone(), content.clone()));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(req.top_k.min(20));

    // Build context and sources
    let context: String = scored
        .iter()
        .map(|(_, _, content)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let sources: Vec<RagSource> = scored
        .iter()
        .map(|(score, chunk_type, content)| {
            let preview = if content.len() > 50 {
                format!("{}...", &content.chars().take(50).collect::<String>())
            } else {
                content.clone()
            };
            RagSource {
                chunk_type: chunk_type.clone(),
                content_preview: preview,
                score: *score,
            }
        })
        .collect();

    // Call LLM for answer
    let prompt = format!(
        "你是一个专业的医学助手。以下是患者的部分检验数据（不含患者身份信息），请基于这些数据回答问题。\n\n\
         检验数据参考：\n{}\n\n\
         问题：{}\n\n\
         请给出专业但易懂的分析。如果数据不足以回答问题，请明确说明。不要进行过度诊断，建议咨询专业医生。",
        context, req.question
    );

    let body = serde_json::json!({
        "model": INTERPRET_MODEL,
        "temperature": 0.6,
        "max_tokens": 2048,
        "messages": [
            { "role": "user", "content": prompt }
        ],
    });

    let resp = state
        .http_client
        .post(INTERPRET_API_URL)
        .header("Authorization", format!("Bearer {}", interpret_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::new(ErrorCode::LlmApiFailed, format!("LLM API 请求失败: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::new(
            ErrorCode::LlmApiFailed,
            format!("LLM API 错误 (HTTP {}): {}", status, text),
        ));
    }

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::new(ErrorCode::LlmApiFailed, format!("LLM 响应解析失败: {}", e)))?;

    let raw_answer = super::extract_llm_content(&resp_json)
        .map_err(|e| AppError::new(ErrorCode::LlmApiFailed, e))?;

    Ok(Json(ApiResponse::ok(
        QueryResult {
            answer: raw_answer,
            sources,
        },
        "查询成功",
    )))
}

// ---------------------------------------------------------------------------
// GET /patients/:patient_id/rag/status
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RagStatus {
    indexed_chunks: usize,
    last_built: Option<String>,
}

pub async fn get_rag_status(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<RagStatus>>, AppError> {
    let db = state.db.clone();
    let pid = patient_id;
    let status = run_blocking(move || {
        db.with_conn(|conn| {
            let count: usize = conn
                .query_row(
                    "SELECT COUNT(*) FROM rag_embeddings WHERE patient_id = ?1",
                    rusqlite::params![pid],
                    |row| row.get::<_, i64>(0),
                )?
                .try_into()
                .unwrap_or(0);

            let last_built: Option<String> = conn
                .query_row(
                    "SELECT MAX(updated_at) FROM rag_embeddings WHERE patient_id = ?1",
                    rusqlite::params![pid],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();

            Ok(RagStatus {
                indexed_chunks: count,
                last_built,
            })
        })
    })
    .await?;

    Ok(Json(ApiResponse::ok(status, "获取成功")))
}
