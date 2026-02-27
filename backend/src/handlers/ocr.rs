use axum::{
    extract::{Multipart, Path, State},
    http::header,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use uuid::Uuid;

use crate::error::{AppError, ErrorCode};
use crate::models::{ApiResponse, FileUploadResult, ItemStatus, Report, ReportDetail, TestItem};

fn parse_item_status(s: &str) -> ItemStatus {
    match s.trim().to_lowercase().as_str() {
        "critical_high" | "criticalhigh" => ItemStatus::CriticalHigh,
        "high" => ItemStatus::High,
        "low" => ItemStatus::Low,
        "critical_low" | "criticallow" => ItemStatus::CriticalLow,
        _ => ItemStatus::Normal,
    }
}
use crate::ocr::ParsedReport;
use crate::AppState;

const UPLOADS_DIR: &str = "uploads";

async fn save_upload_file(multipart: &mut Multipart) -> Result<(String, String, usize), AppError> {
    match multipart.next_field().await {
        Ok(Some(field)) => {
            let fname = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}.bin", Uuid::new_v4()));

            // Validate file extension
            crate::middleware::validate_file_extension(&fname)
                .map_err(|e| AppError::new(ErrorCode::FileTypeNotAllowed, e))?;

            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::new(ErrorCode::UploadReadFailed, format!("读取上传数据失败: {}", e)))?;

            // Validate file size (defense in depth, DefaultBodyLimit also enforces this)
            if data.len() > crate::middleware::MAX_UPLOAD_SIZE {
                return Err(AppError::new(ErrorCode::FileTooLarge, format!(
                    "文件大小 {} 超过限制 {}MB",
                    data.len(),
                    crate::middleware::MAX_UPLOAD_SIZE / 1024 / 1024
                )));
            }

            // Validate file type via magic bytes
            let detected_ext = crate::middleware::validate_file_magic_bytes(&data)
                .map_err(|e| AppError::new(ErrorCode::FileTypeNotAllowed, e))?;

            // Generate safe random filename to prevent path traversal
            let safe_name = crate::middleware::generate_safe_filename(detected_ext);
            let path = format!("{}/{}", UPLOADS_DIR, safe_name);
            let size = data.len();

            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| AppError::internal(format!("写入文件失败: {}", e)))?;
            Ok((path, fname, size))
        }
        Ok(None) => Err(AppError::new(ErrorCode::UploadEmpty, "未找到上传文件")),
        Err(e) => Err(AppError::new(ErrorCode::UploadReadFailed, format!("读取上传字段失败: {}", e))),
    }
}

fn mime_type_from_path(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".pdf") {
        "application/pdf"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

pub async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<FileUploadResult>>, AppError> {
    let (path, original_name, size) = save_upload_file(&mut multipart).await?;

    // Extract safe_name from the path (strip "uploads/" prefix)
    let safe_name = path
        .strip_prefix(&format!("{}/", UPLOADS_DIR))
        .unwrap_or(&path)
        .to_string();

    let file_id = Uuid::new_v4().to_string();
    let mime_type = mime_type_from_path(&path).to_string();

    let db = state.db.clone();
    let fid = file_id.clone();
    let sn = safe_name.clone();
    let on = original_name.clone();
    let mt = mime_type.clone();
    tokio::task::spawn_blocking(move || {
        db.insert_uploaded_file(&fid, &on, &sn, &mt, size, false)
    })
    .await
    .map_err(|e| AppError::internal(format!("任务执行失败: {}", e)))??;

    Ok(Json(ApiResponse::ok(
        FileUploadResult {
            file_id: file_id.clone(),
            url: format!("/api/files/{}", file_id),
            original_name,
            mime_type,
            size,
        },
        "上传成功",
    )))
}

/// OCR parse only - returns parsed report data without creating any records
/// Used for preview/review before confirming
#[derive(Serialize)]
pub struct OcrParseResult {
    pub file_id: String,
    pub file_path: String,
    pub file_name: String,
    pub parsed: ParsedReport,
}

pub async fn ocr_parse(
    auth: crate::auth::AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<OcrParseResult>>, AppError> {
    let siliconflow_key = super::get_siliconflow_api_key(&state.db, &auth.0.sub);
    let (file_path, file_name, size) = save_upload_file(&mut multipart).await?;
    let client = state.http_client.clone();

    let lower = file_path.to_lowercase();
    let is_supported = lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".pdf");

    if !is_supported {
        let _ = tokio::fs::remove_file(&file_path).await;
        return Err(AppError::new(ErrorCode::FileTypeNotAllowed,
            "不支持的文件格式，请上传 PDF 或图片文件",
        ));
    }

    // All supported formats go through the vision model directly
    let fp = file_path.clone();
    let c = client.clone();
    let parsed = match crate::ocr::vision::recognize_file_with_client(&fp, &c, &siliconflow_key).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("视觉模型识别失败: {}", e);
            // Fallback: for images, try Tesseract OCR + regex; for PDF, no fallback
            if lower.ends_with(".pdf") {
                let _ = tokio::fs::remove_file(&file_path).await;
                return Err(AppError::new(ErrorCode::OcrFailed, format!("PDF 识别失败: {}", e)));
            }
            let fp2 = file_path.clone();
            match tokio::task::spawn_blocking(move || crate::ocr::image::extract_image_text(&fp2))
                .await
                .map_err(|e| AppError::internal(format!("任务执行失败: {}", e)))?
            {
                Ok(text) => crate::ocr::parser::parse_report_text(&text),
                Err(e2) => {
                    let _ = tokio::fs::remove_file(&file_path).await;
                    return Err(AppError::new(ErrorCode::OcrFailed, format!(
                        "识别失败: {}; OCR也失败: {}",
                        e, e2
                    )));
                }
            }
        }
    };

    // OCR 识别完成，保存元数据（标记为临时文件，后续可定期清理）
    let safe_name = file_path
        .strip_prefix(&format!("{}/", UPLOADS_DIR))
        .unwrap_or(&file_path)
        .to_string();
    let file_id = Uuid::new_v4().to_string();
    let mime_type = mime_type_from_path(&file_path).to_string();

    let db = state.db.clone();
    let fid = file_id.clone();
    let sn = safe_name.clone();
    let on = file_name.clone();
    let mt = mime_type.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.insert_uploaded_file(&fid, &on, &sn, &mt, size, true)
    })
    .await;

    Ok(Json(ApiResponse::ok(
        OcrParseResult {
            file_id,
            file_path,
            file_name,
            parsed,
        },
        "解析成功",
    )))
}

pub async fn serve_file(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let db = state.db.clone();
    let fid = file_id.clone();
    let row = tokio::task::spawn_blocking(move || db.get_uploaded_file(&fid))
        .await
        .map_err(|e| AppError::internal(format!("任务执行失败: {}", e)))??;

    let file_path = format!("{}/{}", UPLOADS_DIR, row.safe_name);
    let data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| AppError::internal(format!("读取文件失败: {}", e)))?;

    let content_disposition = format!(
        "inline; filename=\"{}\"",
        row.original_name.replace('"', "\\\"")
    );

    Ok((
        [
            (header::CONTENT_TYPE, row.mime_type),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        data,
    ))
}

/// Suggest merge groups using algorithm engine
#[derive(Deserialize)]
pub struct SuggestGroupsReq {
    pub patient_id: Option<String>,
    pub files: Vec<SuggestFileInfo>,
}

#[derive(Deserialize, Serialize)]
pub struct SuggestFileInfo {
    pub file_name: String,
    pub report_type: String,
    pub report_date: String,
    #[serde(default)]
    pub sample_date: String,
    pub item_names: Vec<String>,
}

#[derive(Serialize)]
pub struct SuggestGroupsResult {
    pub groups: Vec<i32>,
    pub existing_merges: Vec<ExistingMerge>,
}

#[derive(Serialize, Clone)]
pub struct ExistingMerge {
    pub file_index: usize,
    pub report_id: String,
    pub report_type: String,
    pub report_date: String,
}

fn normalize_names_for_grouping(report_type: &str, item_names: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = item_names
        .iter()
        .map(|name| {
            crate::algorithm_engine::name_normalizer::normalize_for_scoring(name, report_type)
        })
        .filter(|name| !name.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_test_items_for_grouping(report_type: &str, items: &[TestItem]) -> Vec<String> {
    let mut normalized: Vec<String> = items
        .iter()
        .map(|item| {
            crate::algorithm_engine::name_normalizer::normalize_for_scoring(&item.name, report_type)
        })
        .filter(|name| !name.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

pub async fn suggest_groups(
    auth: crate::auth::AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SuggestGroupsReq>,
) -> Result<Json<ApiResponse<SuggestGroupsResult>>, AppError> {
    let api_key = super::get_llm_api_key(&state.db, &auth.0.sub);
    let empty_result = SuggestGroupsResult {
        groups: vec![0; req.files.len()],
        existing_merges: vec![],
    };

    if req.files.is_empty() {
        return Ok(Json(ApiResponse::ok(empty_result, "无文件")));
    }

    // Load existing reports + item names in one DB pass (if patient_id provided)
    let existing_reports: Vec<(Report, Vec<String>)> = if let Some(ref pid) = req.patient_id {
        let db = state.db.clone();
        let pid_clone = pid.clone();
        tokio::task::spawn_blocking(move || db.list_reports_with_item_names_by_patient(&pid_clone))
            .await
            .map_err(|e| AppError::internal(format!("任务执行失败: {}", e)))??
    } else {
        Vec::new()
    };

    let has_existing = !existing_reports.is_empty();
    let new_count = req.files.len();

    // Only 1 new file and no existing → skip grouping
    if new_count < 2 && !has_existing {
        return Ok(Json(ApiResponse::ok(empty_result, "无需分组")));
    }

    // --- Algorithm Engine: score-based grouping ---
    let normalized_file_items: Vec<Vec<String>> = req
        .files
        .iter()
        .map(|f| normalize_names_for_grouping(&f.report_type, &f.item_names))
        .collect();

    let file_infos: Vec<crate::algorithm_engine::grouping_engine::ReportInfo> = req
        .files
        .iter()
        .enumerate()
        .map(
            |(i, f)| crate::algorithm_engine::grouping_engine::ReportInfo {
                report_type: &f.report_type,
                report_date: &f.report_date,
                sample_date: &f.sample_date,
                item_names: &normalized_file_items[i],
            },
        )
        .collect();

    let existing_infos: Vec<crate::algorithm_engine::grouping_engine::ExistingReportInfo> =
        existing_reports
            .iter()
            .map(
                |(r, item_names)| crate::algorithm_engine::grouping_engine::ExistingReportInfo {
                    report_type: r.report_type.clone(),
                    report_date: r.report_date.clone(),
                    sample_date: r.sample_date.clone(),
                    item_names: normalize_names_for_grouping(&r.report_type, item_names),
                },
            )
            .collect();

    let algo_result =
        crate::algorithm_engine::grouping_engine::batch_group(&file_infos, &existing_infos);

    let algo_resolved = new_count - algo_result.uncertain_indices.len();
    tracing::info!(
        "算法引擎分组: {} 个文件, 算法处理 {}, 不确定 {}",
        new_count,
        algo_resolved,
        algo_result.uncertain_indices.len()
    );

    // existing_merges only comes from algorithm Merge results.
    let existing_merges: Vec<ExistingMerge> = algo_result
        .existing_merges
        .iter()
        .map(|(ni, ei)| ExistingMerge {
            file_index: *ni,
            report_id: existing_reports[*ei].0.id.clone(),
            report_type: existing_reports[*ei].0.report_type.clone(),
            report_date: existing_reports[*ei].0.report_date.clone(),
        })
        .collect();

    // --- Stage 3: LLM verification for uncertain cases ---
    let mut groups = algo_result.groups.clone();
    let mut extra_existing_merges: Vec<ExistingMerge> = Vec::new();

    if !algo_result.uncertain_indices.is_empty() {
        tracing::info!(
            "算法引擎: {} 个不确定文件，启动 LLM 验证",
            algo_result.uncertain_indices.len()
        );

        let client = state.http_client.clone();
        let mut llm_verified = 0usize;

        for &ui in &algo_result.uncertain_indices {
            let f = &file_infos[ui];

            // Try to find best uncertain match against existing reports
            let mut best_existing: Option<(usize, f32)> = None;
            for (ei, er) in existing_infos.iter().enumerate() {
                let er_info = crate::algorithm_engine::grouping_engine::ReportInfo {
                    report_type: &er.report_type,
                    report_date: &er.report_date,
                    sample_date: &er.sample_date,
                    item_names: &er.item_names,
                };
                let score = crate::algorithm_engine::grouping_engine::compute_merge_score(f, &er_info);
                if score.decision == crate::algorithm_engine::grouping_engine::MergeDecision::Uncertain
                    && best_existing.map_or(true, |(_, bs)| score.total > bs)
                {
                    best_existing = Some((ei, score.total));
                }
            }

            // Try LLM verification against existing reports
            if let Some((ei, _)) = best_existing {
                let prompt = crate::algorithm_engine::llm_verify::build_merge_verify_prompt(
                    f.report_type,
                    f.report_date,
                    f.sample_date,
                    f.item_names,
                    &existing_infos[ei].report_type,
                    &existing_infos[ei].report_date,
                    &existing_infos[ei].sample_date,
                    &existing_infos[ei].item_names,
                );
                match llm_verify_merge(&client, &prompt, &api_key).await {
                    Some(true) => {
                        extra_existing_merges.push(ExistingMerge {
                            file_index: ui,
                            report_id: existing_reports[ei].0.id.clone(),
                            report_type: existing_reports[ei].0.report_type.clone(),
                            report_date: existing_reports[ei].0.report_date.clone(),
                        });
                        llm_verified += 1;
                    }
                    Some(false) => {
                        llm_verified += 1;
                    }
                    None => {
                        tracing::warn!("LLM 验证失败，保守处理文件 {}", ui);
                    }
                }
                continue;
            }

            // Try LLM verification against other uncertain new files for grouping
            for &uj in &algo_result.uncertain_indices {
                if uj <= ui || groups[uj] != 0 {
                    continue;
                }
                let other = &file_infos[uj];
                let score = crate::algorithm_engine::grouping_engine::compute_merge_score(f, other);
                if score.decision != crate::algorithm_engine::grouping_engine::MergeDecision::Uncertain {
                    continue;
                }
                let prompt = crate::algorithm_engine::llm_verify::build_merge_verify_prompt(
                    f.report_type,
                    f.report_date,
                    f.sample_date,
                    f.item_names,
                    other.report_type,
                    other.report_date,
                    other.sample_date,
                    other.item_names,
                );
                if let Some(true) = llm_verify_merge(&client, &prompt, &api_key).await {
                    // Group them together
                    if groups[ui] == 0 {
                        // Assign a new group ID
                        let max_gid = groups.iter().max().copied().unwrap_or(0);
                        groups[ui] = max_gid + 1;
                    }
                    groups[uj] = groups[ui];
                    llm_verified += 1;
                }
            }
        }

        tracing::info!("LLM 验证完成: {} 个不确定文件已解决", llm_verified);
    }

    // Merge existing_merges from algo + LLM
    let mut all_existing_merges = existing_merges;
    all_existing_merges.extend(extra_existing_merges);

    Ok(Json(ApiResponse::ok(
        SuggestGroupsResult {
            groups,
            existing_merges: all_existing_merges,
        },
        "分组建议生成成功",
    )))
}

/// Call LLM to verify a merge decision for uncertain cases.
/// Returns Some(true) for merge, Some(false) for no-merge, None on error.
async fn llm_verify_merge(client: &reqwest::Client, prompt: &str, api_key: &str) -> Option<bool> {
    let body = serde_json::json!({
        "model": super::LLM_MODEL_FAST,
        "messages": [
            { "role": "system", "content": crate::algorithm_engine::llm_verify::MERGE_VERIFY_SYSTEM_PROMPT },
            { "role": "user", "content": prompt },
        ],
        "temperature": 0.0,
        "enable_thinking": false,
    });

    match super::llm_post_with_retry(client, super::LLM_API_URL, api_key, &body).await {
        Ok(resp) => {
            let resp_json: serde_json::Value = resp.json().await.ok()?;
            let content = super::extract_llm_content(&resp_json).ok()?;
            tracing::info!("LLM 合并验证返回: {}", content);
            crate::algorithm_engine::llm_verify::parse_merge_verify_response(&content)
        }
        Err(e) => {
            tracing::warn!("LLM 合并验证请求失败: {}", e);
            None
        }
    }
}

/// Batch confirm - create reports from reviewed/merged parsed data
#[derive(Deserialize)]
pub struct ConfirmReportReq {
    pub existing_report_id: Option<String>,
    pub report_type: String,
    pub hospital: String,
    pub report_date: String,
    #[serde(default)]
    pub sample_date: String,
    pub file_paths: Vec<String>,
    pub items: Vec<ConfirmItemReq>,
}

#[derive(Deserialize)]
pub struct ConfirmItemReq {
    pub name: String,
    pub value: String,
    pub unit: String,
    pub reference_range: String,
    pub status: String,
}

#[derive(Deserialize)]
pub struct BatchConfirmReq {
    pub reports: Vec<ConfirmReportReq>,
    #[serde(default)]
    pub prefetched_name_map: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub skip_merge_check: bool,
}

/// Merge check result
#[derive(Serialize)]
pub struct MergeCheckResult {
    pub merges: Vec<MergeDecision>,
}

#[derive(Serialize)]
pub struct MergeDecision {
    pub input_index: usize,
    pub existing_report_id: String,
    pub existing_report_type: String,
}

fn build_items_by_report_type(
    reports: &[ConfirmReportReq],
) -> std::collections::HashMap<String, Vec<String>> {
    let mut items_by_report_type: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for report_req in reports {
        let report_type = report_req.report_type.trim().to_string();
        if report_type.is_empty() {
            continue;
        }
        for item in &report_req.items {
            let name = item.name.trim();
            if name.is_empty() {
                continue;
            }
            items_by_report_type
                .entry(report_type.clone())
                .or_default()
                .push(name.to_string());
        }
    }
    items_by_report_type
}

fn build_normalize_prefetch_key(patient_id: &str, reports: &[ConfirmReportReq]) -> String {
    let mut parts: Vec<String> = reports
        .iter()
        .map(|r| {
            let mut names: Vec<String> = r
                .items
                .iter()
                .map(|it| it.name.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect();
            names.sort();
            names.dedup();
            format!(
                "{}|{}|{}",
                r.report_type.trim(),
                r.report_date.trim(),
                names.join("||")
            )
        })
        .collect();
    parts.sort();

    let raw = format!("{}::{}", patient_id, parts.join("###"));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    format!("{}:{:x}", patient_id, hasher.finish())
}

async fn get_or_compute_normalize_name_map(
    state: &AppState,
    patient_id: &str,
    reports: &[ConfirmReportReq],
    api_key: &str,
) -> std::collections::HashMap<String, String> {
    let items_by_report_type = build_items_by_report_type(reports);
    if items_by_report_type.is_empty() {
        return std::collections::HashMap::new();
    }

    let cache_key = build_normalize_prefetch_key(patient_id, reports);

    if let Some(cached) = state
        .normalize_prefetch_cache
        .read()
        .await
        .get(&cache_key)
        .cloned()
    {
        return cached;
    }

    let key_lock = {
        let mut locks = state.normalize_prefetch_locks.lock().await;
        locks
            .entry(cache_key.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };

    let name_map = {
        let _guard = key_lock.lock().await;

        // Extract cache read into a separate let-binding so the RwLockReadGuard
        // is dropped at the semicolon, BEFORE the else branch executes.
        // (if let temporaries live through the entire if/else block in Rust,
        //  which would deadlock when we try to acquire a write lock below.)
        let double_check = state
            .normalize_prefetch_cache
            .read()
            .await
            .get(&cache_key)
            .cloned();

        if let Some(cached) = double_check {
            cached
        } else {
            let t_start = std::time::Instant::now();
            let existing_canonical: Vec<String> = {
                let db = state.db.clone();
                let pid = patient_id.to_string();
                match tokio::task::spawn_blocking(move || {
                    db.list_canonical_item_names_by_patient(&pid)
                })
                .await
                {
                    Ok(Ok(names)) => names,
                    _ => Vec::new(),
                }
            };
            tracing::info!(
                "标准化预处理: 加载已有 canonical names 耗时 {:.1}s, 共 {} 个",
                t_start.elapsed().as_secs_f64(),
                existing_canonical.len()
            );

            let computed = super::normalize::normalize_item_names(
                &state.http_client,
                &items_by_report_type,
                &existing_canonical,
                api_key,
            )
            .await;

            tracing::info!(
                "标准化预处理: normalize_item_names 总耗时 {:.1}s, 返回 {} 个映射",
                t_start.elapsed().as_secs_f64(),
                computed.len()
            );

            let mut cache = state.normalize_prefetch_cache.write().await;
            cache.insert(cache_key.clone(), computed.clone());
            if cache.len() > 128 {
                // FIFO eviction: remove the oldest (first-inserted) entry
                cache.shift_remove_index(0);
            }
            computed
        }
    };

    {
        let mut locks = state.normalize_prefetch_locks.lock().await;
        if let Some(existing) = locks.get(&cache_key) {
            if std::sync::Arc::ptr_eq(existing, &key_lock) {
                locks.remove(&cache_key);
            }
        }
    }

    name_map
}

/// Merge check endpoint: run LLM merge detection separately so frontend can show progress.
pub async fn merge_check(
    auth: crate::auth::AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<BatchConfirmReq>,
) -> Result<Json<ApiResponse<MergeCheckResult>>, AppError> {
    let api_key = super::get_llm_api_key(&state.db, &auth.0.sub);
    tracing::info!(
        ">>> merge_check 收到请求: patient={}, reports={}",
        patient_id,
        req.reports.len()
    );
    let t0 = std::time::Instant::now();
    // Build inputs (same structure as batch_confirm, but we only care about report metadata)
    let inputs: Vec<crate::db::BatchReportInput> = req
        .reports
        .iter()
        .map(|report_req| {
            let (existing_report_id, new_report) =
                if let Some(ref eid) = report_req.existing_report_id {
                    (Some(eid.clone()), None)
                } else {
                    let file_path = report_req.file_paths.first().cloned().unwrap_or_default();
                    let report = Report {
                        id: Uuid::new_v4().to_string(),
                        patient_id: patient_id.clone(),
                        report_type: report_req.report_type.clone(),
                        hospital: report_req.hospital.clone(),
                        report_date: report_req.report_date.clone(),
                        sample_date: report_req.sample_date.clone(),
                        file_path,
                        created_at: Utc::now().to_rfc3339(),
                    };
                    (None, Some(report))
                };

            let report_id = existing_report_id
                .as_deref()
                .unwrap_or_else(|| new_report.as_ref().unwrap().id.as_str())
                .to_string();

            let items: Vec<TestItem> = report_req
                .items
                .iter()
                .map(|item| TestItem {
                    id: Uuid::new_v4().to_string(),
                    report_id: report_id.clone(),
                    name: item.name.clone(),
                    value: item.value.clone(),
                    unit: item.unit.clone(),
                    reference_range: item.reference_range.clone(),
                    status: parse_item_status(&item.status),
                    canonical_name: String::new(),
                })
                .collect();

            crate::db::BatchReportInput {
                existing_report_id,
                new_report,
                items,
            }
        })
        .collect();

    // Collect new reports that might need merging
    let new_report_indices: Vec<usize> = inputs
        .iter()
        .enumerate()
        .filter(|(_, inp)| inp.existing_report_id.is_none() && inp.new_report.is_some())
        .map(|(i, _)| i)
        .collect();

    if new_report_indices.is_empty() {
        tracing::info!(
            "<<< merge_check 提前返回: {:.1}s, 无需合并检测",
            t0.elapsed().as_secs_f64()
        );
        return Ok(Json(ApiResponse::ok(
            MergeCheckResult { merges: vec![] },
            "无需合并检测",
        )));
    }

    // Load existing reports from DB
    let db = state.db.clone();
    let pid = patient_id.clone();
    let existing_reports =
        tokio::task::spawn_blocking(move || db.list_reports_with_item_names_by_patient(&pid))
            .await
            .map_err(|e| AppError::internal(format!("任务执行失败: {}", e)))??;

    if existing_reports.is_empty() {
        tracing::info!(
            "<<< merge_check 提前返回: {:.1}s, 无已有报告",
            t0.elapsed().as_secs_f64()
        );
        return Ok(Json(ApiResponse::ok(
            MergeCheckResult { merges: vec![] },
            "无已有报告",
        )));
    }

    // --- Algorithm Engine: score-based merge check ---
    let new_items_owned: Vec<Vec<String>> = new_report_indices
        .iter()
        .map(|&idx| {
            let nr = inputs[idx].new_report.as_ref().unwrap();
            normalize_test_items_for_grouping(&nr.report_type, &inputs[idx].items)
        })
        .collect();
    let new_infos: Vec<crate::algorithm_engine::grouping_engine::ReportInfo> = new_report_indices
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let nr = inputs[idx].new_report.as_ref().unwrap();
            crate::algorithm_engine::grouping_engine::ReportInfo {
                report_type: &nr.report_type,
                report_date: &nr.report_date,
                sample_date: &nr.sample_date,
                item_names: &new_items_owned[i],
            }
        })
        .collect();

    let existing_infos: Vec<crate::algorithm_engine::grouping_engine::ExistingReportInfo> =
        existing_reports
            .iter()
            .map(
                |(r, item_names)| crate::algorithm_engine::grouping_engine::ExistingReportInfo {
                    report_type: r.report_type.clone(),
                    report_date: r.report_date.clone(),
                    sample_date: r.sample_date.clone(),
                    item_names: normalize_names_for_grouping(&r.report_type, item_names),
                },
            )
            .collect();

    let candidates = crate::algorithm_engine::grouping_engine::check_merge_candidates(
        &new_infos,
        &existing_infos,
    );

    // Collect confirmed merges (best score per input wins)
    let mut merges: Vec<MergeDecision> = Vec::new();
    for (ni_local, ei) in crate::algorithm_engine::grouping_engine::best_merge_targets(&candidates)
    {
        let input_idx = new_report_indices[ni_local];
        merges.push(MergeDecision {
            input_index: input_idx,
            existing_report_id: existing_reports[ei].0.id.clone(),
            existing_report_type: existing_reports[ei].0.report_type.clone(),
        });
    }

    let algo_count = merges.len();

    // --- Stage 3: LLM verification for uncertain merge candidates ---
    let uncertain_candidates: Vec<&crate::algorithm_engine::grouping_engine::MergeCandidate> =
        candidates
            .iter()
            .filter(|c| {
                c.score.decision
                    == crate::algorithm_engine::grouping_engine::MergeDecision::Uncertain
            })
            .collect();
    let uncertain_count = uncertain_candidates.len();

    if uncertain_count > 0 {
        tracing::info!(
            "算法引擎合并检测: 确定合并 {}, 不确定 {} → 启动 LLM 验证",
            algo_count,
            uncertain_count
        );

        let client = state.http_client.clone();
        // Group uncertain candidates by new_report_index, pick best per new report
        let mut best_uncertain: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for uc in &uncertain_candidates {
            best_uncertain
                .entry(uc.new_report_index)
                .or_insert(uc.existing_report_index);
        }

        for (ni_local, ei) in best_uncertain {
            // Skip if already confirmed merged
            if merges
                .iter()
                .any(|m| m.input_index == new_report_indices[ni_local])
            {
                continue;
            }

            let nr = &new_infos[ni_local];
            let er = &existing_infos[ei];
            let prompt = crate::algorithm_engine::llm_verify::build_merge_verify_prompt(
                nr.report_type,
                nr.report_date,
                nr.sample_date,
                nr.item_names,
                &er.report_type,
                &er.report_date,
                &er.sample_date,
                &er.item_names,
            );

            if let Some(true) = llm_verify_merge(&client, &prompt, &api_key).await {
                let input_idx = new_report_indices[ni_local];
                merges.push(MergeDecision {
                    input_index: input_idx,
                    existing_report_id: existing_reports[ei].0.id.clone(),
                    existing_report_type: existing_reports[ei].0.report_type.clone(),
                });
            }
        }

        tracing::info!(
            "LLM 验证后合并检测: 总计 {} 份报告可合并",
            merges.len()
        );
    } else {
        tracing::info!(
            "算法引擎合并检测: 确定合并 {}, 不确定 0",
            algo_count
        );
    }

    let msg = format!("合并检测完成，发现 {} 份报告可合并", merges.len());
    tracing::info!(
        "<<< merge_check 完成: {:.1}s, {}",
        t0.elapsed().as_secs_f64(),
        msg
    );
    Ok(Json(ApiResponse::ok(MergeCheckResult { merges }, &msg)))
}

/// Prefetch normalization map in advance, so confirm step can reuse it.
pub async fn prefetch_normalize(
    auth: crate::auth::AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<BatchConfirmReq>,
) -> Result<Json<ApiResponse<std::collections::HashMap<String, String>>>, AppError> {
    let api_key = super::get_llm_api_key(&state.db, &auth.0.sub);
    tracing::info!(
        ">>> prefetch_normalize 收到请求: patient={}, reports={}",
        patient_id,
        req.reports.len()
    );
    let t0 = std::time::Instant::now();
    let name_map = get_or_compute_normalize_name_map(&state, &patient_id, &req.reports, &api_key).await;
    tracing::info!(
        "<<< prefetch_normalize 完成: {:.1}s, 映射数={}",
        t0.elapsed().as_secs_f64(),
        name_map.len()
    );
    if name_map.is_empty() {
        return Ok(Json(ApiResponse::ok(
            std::collections::HashMap::new(),
            "无可标准化项目",
        )));
    }

    Ok(Json(ApiResponse::ok(name_map, "标准化预热完成")))
}

pub async fn batch_confirm(
    auth: crate::auth::AuthUser,
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<BatchConfirmReq>,
) -> Result<Json<ApiResponse<Vec<ReportDetail>>>, AppError> {
    let api_key = super::get_llm_api_key(&state.db, &auth.0.sub);
    // Validate inputs upfront (no DB needed)
    for report_req in &req.reports {
        if report_req.report_type.trim().is_empty() {
            return Err(AppError::validation("报告类型不能为空"));
        }
        if report_req.report_date.trim().is_empty() {
            return Err(AppError::validation("报告日期不能为空"));
        }
        if let Err(msg) = crate::models::validate_date(&report_req.report_date, "报告日期") {
            return Err(AppError::validation(msg));
        }
        if !report_req.sample_date.trim().is_empty() {
            if let Err(msg) = crate::models::validate_date(&report_req.sample_date, "检查日期")
            {
                return Err(AppError::validation(msg));
            }
        }
    }

    // Prepare all Report and TestItem objects in memory
    let mut inputs: Vec<crate::db::BatchReportInput> = req
        .reports
        .iter()
        .map(|report_req| {
            let (existing_report_id, new_report) =
                if let Some(ref eid) = report_req.existing_report_id {
                    (Some(eid.clone()), None)
                } else {
                    let file_path = report_req.file_paths.first().cloned().unwrap_or_default();
                    let report = Report {
                        id: Uuid::new_v4().to_string(),
                        patient_id: patient_id.clone(),
                        report_type: report_req.report_type.clone(),
                        hospital: report_req.hospital.clone(),
                        report_date: report_req.report_date.clone(),
                        sample_date: report_req.sample_date.clone(),
                        file_path,
                        created_at: Utc::now().to_rfc3339(),
                    };
                    (None, Some(report))
                };

            let report_id = existing_report_id
                .as_deref()
                .unwrap_or_else(|| new_report.as_ref().unwrap().id.as_str())
                .to_string();

            let items: Vec<TestItem> = report_req
                .items
                .iter()
                .map(|item| TestItem {
                    id: Uuid::new_v4().to_string(),
                    report_id: report_id.clone(),
                    name: item.name.clone(),
                    value: item.value.clone(),
                    unit: item.unit.clone(),
                    reference_range: item.reference_range.clone(),
                    status: parse_item_status(&item.status),
                    canonical_name: String::new(),
                })
                .collect();

            crate::db::BatchReportInput {
                existing_report_id,
                new_report,
                items,
            }
        })
        .collect();

    // Normalize item names via LLM before persisting.
    // If frontend has prefetched a map in advance, only normalize missing names.
    {
        let mut name_map = req.prefetched_name_map.clone();
        name_map.retain(|k, v| !k.trim().is_empty() && !v.trim().is_empty());

        let mut has_missing_name = false;
        for (report_idx, input) in inputs.iter().enumerate() {
            let report_type = req
                .reports
                .get(report_idx)
                .map(|r| r.report_type.as_str())
                .unwrap_or("");
            for item in &input.items {
                let scoped_key = super::normalize::scoped_name_key(report_type, &item.name);
                if item.name.trim().is_empty()
                    || name_map.contains_key(&scoped_key)
                    || name_map.contains_key(&item.name)
                {
                    continue;
                }
                has_missing_name = true;
                break;
            }
            if has_missing_name {
                break;
            }
        }

        if has_missing_name {
            let shared_name_map =
                get_or_compute_normalize_name_map(&state, &patient_id, &req.reports, &api_key).await;
            for (k, v) in shared_name_map {
                name_map.entry(k).or_insert(v);
            }
        }

        if !name_map.is_empty() {
            for (report_idx, input) in inputs.iter_mut().enumerate() {
                let report_type = req
                    .reports
                    .get(report_idx)
                    .map(|r| r.report_type.as_str())
                    .unwrap_or("");
                for item in input.items.iter_mut() {
                    let scoped_key = super::normalize::scoped_name_key(report_type, &item.name);
                    if let Some(canonical) = name_map
                        .get(&scoped_key)
                        .or_else(|| name_map.get(&item.name))
                    {
                        item.canonical_name = canonical.clone();
                    }
                }
            }
        }
    }

    // Algorithm-engine merge detection: for new reports without existing_report_id,
    // check if DB has existing reports that should be merged.
    // Skip if frontend already did merge-check separately.
    if !req.skip_merge_check {
        let new_report_indices: Vec<usize> = inputs
            .iter()
            .enumerate()
            .filter(|(_, inp)| inp.existing_report_id.is_none() && inp.new_report.is_some())
            .map(|(i, _)| i)
            .collect();

        if !new_report_indices.is_empty() {
            let db = state.db.clone();
            let pid = patient_id.clone();
            let existing_reports = tokio::task::spawn_blocking(move || {
                db.list_reports_with_item_names_by_patient(&pid)
            })
            .await
            .map_err(|e| AppError::internal(format!("任务执行失败: {}", e)))??;

            if !existing_reports.is_empty() {
                let new_items_owned: Vec<Vec<String>> = new_report_indices
                    .iter()
                    .map(|&idx| {
                        let nr = inputs[idx].new_report.as_ref().unwrap();
                        normalize_test_items_for_grouping(&nr.report_type, &inputs[idx].items)
                    })
                    .collect();
                let new_infos: Vec<crate::algorithm_engine::grouping_engine::ReportInfo> =
                    new_report_indices
                        .iter()
                        .enumerate()
                        .map(|(i, &idx)| {
                            let nr = inputs[idx].new_report.as_ref().unwrap();
                            crate::algorithm_engine::grouping_engine::ReportInfo {
                                report_type: &nr.report_type,
                                report_date: &nr.report_date,
                                sample_date: &nr.sample_date,
                                item_names: &new_items_owned[i],
                            }
                        })
                        .collect();
                let existing_infos: Vec<
                    crate::algorithm_engine::grouping_engine::ExistingReportInfo,
                > = existing_reports
                    .iter()
                    .map(|(r, item_names)| {
                        crate::algorithm_engine::grouping_engine::ExistingReportInfo {
                            report_type: r.report_type.clone(),
                            report_date: r.report_date.clone(),
                            sample_date: r.sample_date.clone(),
                            item_names: normalize_names_for_grouping(&r.report_type, item_names),
                        }
                    })
                    .collect();

                let candidates = crate::algorithm_engine::grouping_engine::check_merge_candidates(
                    &new_infos,
                    &existing_infos,
                );

                for (ni_local, ei) in
                    crate::algorithm_engine::grouping_engine::best_merge_targets(&candidates)
                {
                    let input_idx = new_report_indices[ni_local];
                    inputs[input_idx].existing_report_id = Some(existing_reports[ei].0.id.clone());
                    inputs[input_idx].new_report = None;
                }
            }
        }
    }

    // Single spawn_blocking call: validate duplicates + batch write + build response
    let db = state.db.clone();
    let pid = patient_id.clone();
    let batch_results =
        tokio::task::spawn_blocking(move || db.batch_create_reports_and_items(&pid, inputs))
            .await
            .map_err(|e| AppError::internal(format!("任务执行失败: {}", e)))??;

    let results: Vec<ReportDetail> = batch_results
        .into_iter()
        .map(|(report, test_items)| ReportDetail { report, test_items })
        .collect();

    // Invalidate cached interpretations for all affected reports
    for detail in &results {
        let db = state.db.clone();
        let rid = detail.report.id.clone();
        let _ = tokio::task::spawn_blocking(move || db.delete_interpretation(&rid)).await;
    }

    let msg = format!("成功保存 {} 份报告", results.len());
    Ok(Json(ApiResponse::ok(results, &msg)))
}
