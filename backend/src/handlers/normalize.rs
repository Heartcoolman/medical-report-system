use axum::{extract::State, Json};
use std::collections::HashMap;

use crate::error::AppError;
use crate::models::{ApiResponse, Report, TestItem};
use crate::AppState;

use super::{get_llm_api_key, LLM_API_URL, LLM_MODEL_FAST};

const SCOPED_KEY_SEP: char = '\u{001F}';

/// Build a context-aware key for one normalized name mapping.
///
/// Format: "{report_type}<US>{name}" where `<US>` is ASCII Unit Separator.
pub(crate) fn scoped_name_key(report_type: &str, name: &str) -> String {
    format!("{}{}{}", report_type.trim(), SCOPED_KEY_SEP, name.trim())
}

fn register_plain_name_mapping(
    plain_map: &mut HashMap<String, Option<String>>,
    name: &str,
    canonical: &str,
) {
    match plain_map.get(name) {
        None => {
            plain_map.insert(name.to_string(), Some(canonical.to_string()));
        }
        Some(Some(existing)) if existing == canonical => {}
        _ => {
            plain_map.insert(name.to_string(), None);
        }
    }
}

/// Normalize item names using the algorithm engine first, with LLM fallback for unresolved names.
///
/// - `items_by_report_type`: new item names grouped by report_type
/// - `existing_canonical_names`: canonical names already in the system (from the same patient)
///
/// Returns a map that always contains scoped keys (`report_type + name`), and
/// additionally contains plain-name keys when the mapping is unambiguous across
/// report types.
pub async fn normalize_item_names(
    client: &reqwest::Client,
    items_by_report_type: &HashMap<String, Vec<String>>,
    existing_canonical_names: &[String],
) -> HashMap<String, String> {
    // Normalize and deduplicate each report type input first, so downstream
    // behavior is deterministic and context-aware.
    let mut normalized_inputs: Vec<(String, Vec<String>)> = items_by_report_type
        .iter()
        .map(|(report_type, names)| {
            let mut deduped: Vec<String> = names
                .iter()
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .map(|name| name.to_string())
                .collect();
            deduped.sort();
            deduped.dedup();
            (report_type.trim().to_string(), deduped)
        })
        .filter(|(_, names)| !names.is_empty())
        .collect();
    normalized_inputs.sort_by(|a, b| a.0.cmp(&b.0));

    if normalized_inputs.is_empty() {
        return HashMap::new();
    }

    let scoped_count: usize = normalized_inputs.iter().map(|(_, names)| names.len()).sum();
    let t0 = std::time::Instant::now();

    let mut full_map: HashMap<String, String> = HashMap::new();
    let mut unresolved_by_report_type: HashMap<String, Vec<String>> = HashMap::new();
    let mut plain_name_map: HashMap<String, Option<String>> = HashMap::new();

    // --- Phase 1: Algorithm engine (rules + dictionary + fuzzy matching) ---
    let mut algo_resolved = 0usize;
    let mut unresolved_count = 0usize;

    for (report_type, names) in &normalized_inputs {
        let mut one_type_input = HashMap::new();
        one_type_input.insert(report_type.clone(), names.clone());

        let algo_results = crate::algorithm_engine::name_normalizer::normalize_batch(
            &one_type_input,
            existing_canonical_names,
        );

        for name in names {
            if let Some(result) = algo_results.get(name) {
                let scoped_key = scoped_name_key(report_type, name);
                match result.method {
                    crate::algorithm_engine::name_normalizer::NormalizeMethod::Unresolved => {
                        unresolved_by_report_type
                            .entry(report_type.clone())
                            .or_default()
                            .push(name.clone());
                        unresolved_count += 1;
                    }
                    _ => {
                        full_map.insert(scoped_key, result.canonical.clone());
                        register_plain_name_mapping(&mut plain_name_map, name, &result.canonical);
                        algo_resolved += 1;
                    }
                }
            } else {
                unresolved_by_report_type
                    .entry(report_type.clone())
                    .or_default()
                    .push(name.clone());
                unresolved_count += 1;
            }
        }
    }

    tracing::info!(
        "算法引擎标准化: {} 个名称(含报告类型上下文), 算法处理 {}, 未解决 {} (耗时 {:.1}ms)",
        scoped_count,
        algo_resolved,
        unresolved_count,
        t0.elapsed().as_secs_f64() * 1000.0
    );

    // --- Phase 2: LLM fallback for unresolved names only ---
    if unresolved_count > 0 {
        tracing::info!("LLM 标准化回退: {} 个未解决名称", unresolved_count);
        let mut llm_resolved = 0usize;

        let mut sorted_types: Vec<String> = unresolved_by_report_type.keys().cloned().collect();
        sorted_types.sort();
        for report_type in sorted_types {
            let names = unresolved_by_report_type
                .get(&report_type)
                .cloned()
                .unwrap_or_default();
            if names.is_empty() {
                continue;
            }

            let mut unresolved_input: HashMap<String, Vec<String>> = HashMap::new();
            unresolved_input.insert(report_type.clone(), names.clone());
            let llm_map =
                llm_normalize_item_names(client, &unresolved_input, existing_canonical_names).await;

            llm_resolved += llm_map.len();
            for name in names {
                let canonical = llm_map.get(&name).cloned().unwrap_or_else(|| name.clone());
                full_map.insert(scoped_name_key(&report_type, &name), canonical.clone());
                register_plain_name_mapping(&mut plain_name_map, &name, &canonical);
            }
        }

        tracing::info!(
            "LLM 标准化回退完成: 返回 {} 个映射 (总耗时 {:.1}s)",
            llm_resolved,
            t0.elapsed().as_secs_f64()
        );
    }

    // Ensure all scoped input names have a mapping (identity fallback).
    for (report_type, names) in &normalized_inputs {
        for name in names {
            let scoped_key = scoped_name_key(report_type, name);
            let canonical = full_map
                .entry(scoped_key)
                .or_insert_with(|| name.clone())
                .clone();
            register_plain_name_mapping(&mut plain_name_map, name, &canonical);
        }
    }

    // Keep plain-name keys only when all report types agree on the same canonical.
    for (name, canonical) in plain_name_map {
        if let Some(canonical) = canonical {
            full_map.entry(name).or_insert(canonical);
        }
    }

    full_map
}

/// LLM-based normalization for names that the algorithm engine couldn't resolve.
async fn llm_normalize_item_names(
    client: &reqwest::Client,
    items_by_report_type: &HashMap<String, Vec<String>>,
    existing_canonical_names: &[String],
) -> HashMap<String, String> {
    let mut prompt = String::from(
        "你是一个医学检验项目名称标准化助手。以下是算法无法识别的检验项目名称，请将它们标准化。\n\n\
         规则：\n\
         1. 英文缩写统一为标准中文名\n\
         2. 旧称/俗称统一为现行标准名\n\
         3. 忽略灵敏度/方法前缀\n\
         4. 统一「定量」后缀\n\
         5. 统一细微文字差异（数/计数等）\n\
         6. 修复 OCR 截断和乱码\n\
         7. 无法确定的保持原名不变\n\
         8. 结合报告类型理解项目含义\n\n"
    );

    if !existing_canonical_names.is_empty() {
        let mut existing_sorted = existing_canonical_names.to_vec();
        existing_sorted.sort();
        existing_sorted.dedup();
        let existing_str = existing_sorted
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(", ");
        prompt.push_str(&format!(
            "【已有标准名称】（优先匹配）：\n[{}]\n\n",
            existing_str
        ));
    }

    let mut sorted_types: Vec<&String> = items_by_report_type.keys().collect();
    sorted_types.sort();
    for rt in &sorted_types {
        let names = items_by_report_type.get(*rt).unwrap();
        let mut deduped = names.clone();
        deduped.sort();
        deduped.dedup();
        let names_str = deduped
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(", ");
        prompt.push_str(&format!("【{}】: [{}]\n", rt, names_str));
    }

    prompt.push_str("\n返回 JSON 对象，key 是原始名称，value 是标准名称。只返回 JSON。");

    match call_llm_json(client, &prompt).await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!("LLM 标准化回退失败: {}", e);
            HashMap::new()
        }
    }
}

/// Low-level: call LLM and parse response as JSON object (HashMap<String, String>)
async fn call_llm_json(
    client: &reqwest::Client,
    prompt: &str,
) -> Result<HashMap<String, String>, String> {
    let body = serde_json::json!({
        "model": LLM_MODEL_FAST,
        "messages": [{ "role": "user", "content": prompt }],
        "enable_thinking": false,
    });

    let resp = super::llm_post_with_retry(client, LLM_API_URL, &get_llm_api_key(), &body).await?;

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 LLM API 响应失败: {}", e))?;

    let content = super::extract_llm_content(&resp_json)?;
    tracing::info!("LLM 原始返回: {}", content);
    super::parse_llm_json_object(&content)
}

/// Backfill canonical_name for ALL existing TestItems.
/// Groups items by report_type for LLM context.
pub async fn backfill_canonical_names(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let db = state.db.clone();

    // Phase 1: Scan ALL test items and group their names by report_type
    let (all_items, items_by_report_type) = tokio::task::spawn_blocking(
        move || -> Result<(Vec<(TestItem, String)>, HashMap<String, Vec<String>>), AppError> {
            let items_tree = db.db.open_tree("test_items")?;
            let reports_tree = db.db.open_tree("reports")?;

            let mut all_items = Vec::new();
            let mut report_type_cache: HashMap<String, String> = HashMap::new();
            let mut by_report_type: HashMap<String, Vec<String>> = HashMap::new();

            for entry in items_tree.iter() {
                let (_, val) = entry?;
                let item: TestItem = serde_json::from_slice(&val)?;

                let report_type = if let Some(rt) = report_type_cache.get(&item.report_id) {
                    rt.clone()
                } else {
                    let rt = if let Some(rv) = reports_tree.get(item.report_id.as_bytes())? {
                        let report: Report = serde_json::from_slice(&rv)?;
                        report.report_type
                    } else {
                        "未知".to_string()
                    };
                    report_type_cache.insert(item.report_id.clone(), rt.clone());
                    rt
                };

                by_report_type
                    .entry(report_type.clone())
                    .or_default()
                    .push(item.name.clone());
                all_items.push((item, report_type));
            }

            Ok((all_items, by_report_type))
        },
    )
    .await
    .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    if all_items.is_empty() {
        return Ok(Json(ApiResponse::ok(
            serde_json::json!({"updated": 0}),
            "无需回填",
        )));
    }

    tracing::info!(
        "回填: 共 {} 个 TestItem, {} 种报告类型",
        all_items.len(),
        items_by_report_type.len()
    );

    // Phase 2: Normalize (backfill has no "existing" canonical names — it IS the source of truth)
    let name_map = normalize_item_names(&state.http_client, &items_by_report_type, &[]).await;

    if name_map.is_empty() {
        return Err(AppError::Internal(
            "LLM 标准化调用失败，未获得任何映射结果".to_string(),
        ));
    }

    tracing::info!("回填: LLM 返回 {} 个映射", name_map.len());

    // Phase 3: Update ALL items in database
    let db = state.db.clone();
    let updated_count = tokio::task::spawn_blocking(move || -> Result<usize, AppError> {
        let items_tree = db.db.open_tree("test_items")?;
        let mut count = 0usize;
        for (mut item, report_type) in all_items {
            let scoped_key = scoped_name_key(&report_type, &item.name);
            if let Some(canonical) = name_map
                .get(&scoped_key)
                .or_else(|| name_map.get(&item.name))
            {
                if item.canonical_name != *canonical {
                    item.canonical_name = canonical.clone();
                    let val = serde_json::to_vec(&item)?;
                    items_tree.insert(item.id.as_bytes(), val)?;
                    count += 1;
                }
            }
        }
        Ok(count)
    })
    .await
    .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    let msg = format!("回填完成，更新了 {} 条记录", updated_count);
    Ok(Json(ApiResponse::ok(
        serde_json::json!({"updated": updated_count}),
        &msg,
    )))
}
