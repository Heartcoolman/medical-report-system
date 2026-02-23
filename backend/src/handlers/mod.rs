pub mod interpret;
pub mod normalize;
pub mod ocr;
pub mod patients;
pub mod reports;
pub mod temperatures;

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

/// Read LLM_API_KEY from environment variable.
pub fn get_llm_api_key() -> String {
    std::env::var("LLM_API_KEY").expect("环境变量 LLM_API_KEY 未设置")
}

/// Read INTERPRET_API_KEY from environment variable.
pub fn get_interpret_api_key() -> String {
    std::env::var("INTERPRET_API_KEY").expect("环境变量 INTERPRET_API_KEY 未设置")
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

/// Extract a JSON object `{...}` from LLM text output and deserialize it.
pub fn parse_llm_json_object(
    content: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let trimmed = content.trim();
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };
    serde_json::from_str(json_str)
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
