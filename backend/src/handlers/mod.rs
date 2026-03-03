pub mod admin;
pub mod audit_handler;
pub mod backup;
pub mod expense;
pub mod health_assessment;
pub mod interpret;
pub mod medications;
pub mod normalize;
pub mod ocr;
pub mod patients;
pub mod reports;
pub mod stats;
pub mod temperatures;
pub mod user_settings;

// Shared LLM API constants (used by OCR, normalization, grouping, merge check)
pub const LLM_API_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions";
#[allow(dead_code)]
pub const LLM_MODEL: &str = "qwen3.5-plus";

// Fast model for structured JSON tasks (normalization, grouping, merge check).
// enable_thinking=false disables the reasoning/think blocks for faster responses.
pub const LLM_MODEL_FAST: &str = "qwen3.5-plus";

// Interpret-only LLM constants (AI 智能解读专用)
pub const INTERPRET_API_URL: &str = "https://api.pucode.com/v1/chat/completions";
pub const INTERPRET_MODEL: &str = "gemini-3.1-pro-high";

/// Read LLM_API_KEY: prefer user key, fallback to environment variable.
pub fn get_llm_api_key(db: &crate::db::Database, user_id: &str) -> String {
    if let Some(key) = user_settings::get_user_api_key(db, user_id, "llm") {
        return key;
    }
    std::env::var("LLM_API_KEY").expect("环境变量 LLM_API_KEY 未设置")
}

/// Read INTERPRET_API_KEY: prefer user key, fallback to environment variable.
pub fn get_interpret_api_key(db: &crate::db::Database, user_id: &str) -> String {
    if let Some(key) = user_settings::get_user_api_key(db, user_id, "interpret") {
        return key;
    }
    std::env::var("INTERPRET_API_KEY").expect("环境变量 INTERPRET_API_KEY 未设置")
}

/// Read ZHIPU_API_KEY: prefer user key, fallback to environment variable.
pub fn get_zhipu_api_key(db: &crate::db::Database, user_id: &str) -> String {
    if let Some(key) = user_settings::get_user_api_key(db, user_id, "zhipu") {
        return key;
    }
    std::env::var("ZHIPU_API_KEY").expect("环境变量 ZHIPU_API_KEY 未设置")
}

/// Strip `<think>...</think>` blocks from LLM responses.
/// Handles multiple blocks and unterminated opening tags.
pub fn strip_think_blocks(content: &str) -> String {
    let mut cleaned = content.to_string();
    while let Some(start) = cleaned.find("<think>") {
        if let Some(end) = cleaned.find("</think>") {
            cleaned = format!("{}{}", &cleaned[..start], &cleaned[end + 8..]);
        } else {
            cleaned = cleaned[..start].to_string();
            break;
        }
    }
    cleaned
}

/// Extract the content string from a standard OpenAI-compatible chat completion response.
pub fn extract_llm_content(resp_json: &serde_json::Value) -> Result<String, String> {
    resp_json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| strip_think_blocks(s))
        .ok_or_else(|| format!("LLM API 返回格式异常: {}", resp_json))
}

pub fn extract_json_block(content: &str) -> Result<String, String> {
    let trimmed = content.trim();

    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.find("```") {
            return Ok(after[..end].trim().to_string());
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        if let Some(end) = after.find("```") {
            return Ok(after[..end].trim().to_string());
        }
    }

    // Use bracket-depth matching to find the correct end of JSON
    // Try whichever bracket appears first in the text
    let brace_pos = trimmed.find('{');
    let bracket_pos = trimmed.find('[');
    match (brace_pos, bracket_pos) {
        (Some(b), Some(a)) if a < b => {
            // '[' comes first — likely an array like [{...}]
            if let Some(result) = find_balanced_json(trimmed, '[', ']') {
                return Ok(result);
            }
            if let Some(result) = find_balanced_json(trimmed, '{', '}') {
                return Ok(result);
            }
        }
        _ => {
            if let Some(result) = find_balanced_json(trimmed, '{', '}') {
                return Ok(result);
            }
            if let Some(result) = find_balanced_json(trimmed, '[', ']') {
                return Ok(result);
            }
        }
    }

    Err(format!("无法从模型输出中提取 JSON: {}", content))
}

pub fn find_balanced_json(s: &str, open: char, close: char) -> Option<String> {
    let start = s.find(open)?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    for (i, ch) in s[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(s[start..start + i + ch.len_utf8()].to_string());
            }
        }
    }
    None
}

/// Extract a JSON object `{...}` from LLM text output and deserialize it.
pub fn parse_llm_json_object(
    content: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let json_str = extract_json_block(content)?;
    serde_json::from_str(&json_str)
        .map_err(|e| format!("解析 LLM JSON 结果失败: {}, 原始: {}", e, content))
}

/// Send a POST request to an LLM API with retry (up to 2 retries, exponential backoff).
/// Returns the response on success, or the last error on failure.
pub async fn llm_post_with_retry(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> Result<reqwest::Response, String> {
    let max_retries = 2u32;
    let mut last_err = String::new();
    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
            tracing::info!("LLM 重试 #{}, 等待 {:?}", attempt, delay);
            tokio::time::sleep(delay).await;
        }
        match client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(body)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => return Ok(resp),
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                last_err = format!("HTTP {}: {}", status, text);
                // Don't retry client errors (4xx) except 429 (rate limit)
                if status.is_client_error() && status.as_u16() != 429 {
                    return Err(last_err);
                }
            }
            Err(e) => {
                last_err = format!("请求失败: {}", e);
            }
        }
    }
    Err(last_err)
}
