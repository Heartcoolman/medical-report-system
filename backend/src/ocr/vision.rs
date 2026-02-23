use super::ParsedReport;
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const API_URL: &str = "https://api.siliconflow.cn/v1/chat/completions";
const VISION_MODEL: &str = "Qwen/Qwen3-VL-32B-Instruct";

const SYSTEM_PROMPT: &str = r#"你是一个专业的医疗检验报告识别助手。请从报告中提取以下信息，以严格的 JSON 格式返回，不要包含任何其他文字：
{
  "report_type": "报告类型，应与报告文档的标题或标注一致（如：血常规、肝功能、肾功能、血脂、血糖、尿常规、甲状腺功能、脑脊液常规、脑脊液生化、脑脊液常规生化等）",
  "hospital": "医院名称",
  "sample_date": "检查/采样/送检日期，格式 YYYY-MM-DD",
  "report_date": "报告出具/审核/打印日期，格式 YYYY-MM-DD",
  "items": [
    {
      "name": "检查项名称",
      "value": "结果值，可以是数字（如 5.2）、定性结果（如 阳性、阴性、弱阳性）或滴度（如 1:100）",
      "unit": "单位，定性结果没有单位则填空字符串",
      "reference_range": "参考范围（如 3.5-9.5，或 阴性）",
      "status": "normal 或 high 或 low"
    }
  ]
}
注意：
- sample_date 是检查、采样或送检的日期；report_date 是报告出具、审核或打印的日期
- 如果报告中只有一个日期无法区分，则 sample_date 和 report_date 填相同值
- items 中包含所有有结果的检查项，包括定量（数字）和定性（阳性/阴性等）结果
- status 规则：数值结果根据与参考范围比较判断；阳性/弱阳性/滴度升高 填 high；阴性 填 normal
- 如果无法识别某个字段，用空字符串 ""
- 只返回 JSON，不要有任何额外说明"#;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: serde_json::Value,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Option<Vec<Choice>>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

fn get_api_key() -> String {
    std::env::var("SILICONFLOW_API_KEY").expect("环境变量 SILICONFLOW_API_KEY 未设置")
}

/// Detect MIME type from file extension
fn detect_mime(file_path: &str) -> &'static str {
    let lower = file_path.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    }
}

/// Recognize any file (image or PDF) using the vision model
pub async fn recognize_file_with_client(
    file_path: &str,
    client: &Client,
) -> Result<ParsedReport, String> {
    let api_key = get_api_key();

    // Check file size before reading to avoid OOM with huge files
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
    let metadata = tokio::fs::metadata(file_path)
        .await
        .map_err(|e| format!("获取文件信息失败: {}", e))?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "文件过大（{}MB），最大支持 50MB",
            metadata.len() / 1024 / 1024
        ));
    }

    let bytes = tokio::fs::read(file_path)
        .await
        .map_err(|e| format!("读取文件失败: {}", e))?;

    // Base64 encoding can be CPU-heavy for large files; move it off the async runtime.
    let mime = detect_mime(file_path).to_string();
    let data_url = tokio::task::spawn_blocking(move || {
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        format!("data:{};base64,{}", mime, b64)
    })
    .await
    .map_err(|e| format!("编码文件失败: {}", e))?;

    let content = serde_json::json!([
        {
            "type": "image_url",
            "image_url": { "url": data_url }
        },
        {
            "type": "text",
            "text": "请识别这份医疗检验报告中的所有信息。"
        }
    ]);

    let req = ChatRequest {
        model: VISION_MODEL.to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: serde_json::Value::String(SYSTEM_PROMPT.to_string()),
            },
            Message {
                role: "user".to_string(),
                content,
            },
        ],
    };

    call_api(client, &api_key, &req).await
}

async fn call_api(
    client: &Client,
    api_key: &str,
    req: &ChatRequest,
) -> Result<ParsedReport, String> {
    let body = serde_json::to_value(req).map_err(|e| format!("序列化请求失败: {}", e))?;

    let resp = crate::handlers::llm_post_with_retry(client, API_URL, api_key, &body).await?;

    let chat_resp: ChatResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析 API 响应失败: {}", e))?;

    if let Some(err) = chat_resp.error {
        return Err(format!("API 错误: {}", err.message));
    }

    let content = chat_resp
        .choices
        .and_then(|c| c.into_iter().next())
        .map(|c| c.message.content)
        .ok_or("API 返回为空")?;

    let json_str = extract_json(&content)?;

    let mut report: ParsedReport = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析模型返回的 JSON 失败: {}，原始内容: {}", e, content))?;

    // Re-validate status against reference_range to fix LLM errors
    for item in &mut report.items {
        if let Ok(val) = item.value.parse::<f64>() {
            if !item.reference_range.is_empty() {
                item.status = super::parser::determine_status(val, &item.reference_range).to_string();
            }
        }
    }

    Ok(report)
}

fn extract_json(text: &str) -> Result<String, String> {
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return Ok(after[..end].trim().to_string());
        }
    }
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            return Ok(after[..end].trim().to_string());
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }
    Err(format!("无法从模型输出中提取 JSON: {}", text))
}
