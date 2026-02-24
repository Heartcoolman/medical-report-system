use axum::{
    extract::{Multipart, Path, State},
    Json,
};
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::io::Cursor;

use serde::de;
use crate::error::AppError;
use crate::models::{
    ApiResponse, BatchConfirmExpenseReq, ConfirmExpenseReq, DailyExpense, DailyExpenseDetail,
    DailyExpenseSummary, ExpenseItem,
};
use crate::AppState;

// --- Vision prompt for expense list parsing ---

const EXPENSE_VISION_PROMPT: &str = r#"从住院消费清单截图中提取项目。按日期分组，每天一个独立对象，返回JSON数组。
格式：[{"d":"YYYY-MM-DD","t":合计,"items":[{"n":"名称","q":"×数量","a":金额}]}]
多天示例（注意每天是数组中独立的元素）：
[{"d":"2025-01-01","t":100,"items":[{"n":"药A","q":"×1","a":50},{"n":"药B","q":"×2","a":50}]},{"d":"2025-01-02","t":80,"items":[{"n":"药C","q":"×1","a":80}]}]
重要：每天必须是数组中单独的{}对象，绝对不要在同一个对象里写多个items字段！
规则：n=原始名称，q=数量（无则""），a=金额（退费为负），t=当日合计
只返回JSON"#;

// --- Drug/treatment analysis prompt ---

const ANALYSIS_SYSTEM_PROMPT: &str = r#"你是一位资深临床药师和医学专家。根据患者当日的住院消费清单，分析医生的用药方案和治疗思路。请用简明扼要的中文回答。"#;

fn build_analysis_prompt(items: &[ParsedExpenseItem]) -> String {
    let mut drug_list = Vec::new();
    let mut test_list = Vec::new();
    let mut treatment_list = Vec::new();
    let mut other_list = Vec::new();

    for item in items {
        let entry = if item.quantity.is_empty() {
            format!("- {}", item.name)
        } else {
            format!("- {} ({})", item.name, item.quantity)
        };
        match item.category.as_str() {
            "drug" => drug_list.push(entry),
            "test" => test_list.push(entry),
            "treatment" => treatment_list.push(entry),
            _ => other_list.push(entry),
        }
    }

    let mut prompt = String::from("以下是患者今日的住院消费清单项目：\n\n");

    if !drug_list.is_empty() {
        prompt.push_str("【药品】\n");
        prompt.push_str(&drug_list.join("\n"));
        prompt.push_str("\n\n");
    }
    if !test_list.is_empty() {
        prompt.push_str("【检查化验】\n");
        prompt.push_str(&test_list.join("\n"));
        prompt.push_str("\n\n");
    }
    if !treatment_list.is_empty() {
        prompt.push_str("【治疗操作】\n");
        prompt.push_str(&treatment_list.join("\n"));
        prompt.push_str("\n\n");
    }
    if !other_list.is_empty() {
        prompt.push_str("【其他】\n");
        prompt.push_str(&other_list.join("\n"));
        prompt.push_str("\n\n");
    }

    prompt.push_str(r#"请分两部分回答，用 JSON 格式返回：
{
  "drug_analysis": "用药分析：列出主要药物的作用、用药目的，推断可能的诊断方向（100-300字）",
  "treatment_analysis": "治疗方案分析：综合所有项目（药品+检查+治疗），推断医生的整体治疗策略和思路（100-300字）"
}
只返回 JSON，不要有任何额外说明。"#);

    prompt
}

// --- Flexible f64 deserializer: accepts both number and string ---

fn flexible_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct F64Visitor;
    impl<'de> de::Visitor<'de> for F64Visitor {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or numeric string")
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> { Ok(v) }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> { Ok(v as f64) }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> { Ok(v as f64) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            v.trim().parse::<f64>().map_err(de::Error::custom)
        }
    }
    deserializer.deserialize_any(F64Visitor)
}

fn flexible_f64_default<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: de::Deserializer<'de>,
{
    flexible_f64(deserializer).or(Ok(0.0))
}

// --- Compact types from vision model (short keys to minimize tokens) ---

#[derive(Debug, Deserialize, Clone)]
struct CompactDay {
    #[serde(alias = "d", alias = "expense_date", default)]
    d: String,
    #[serde(alias = "t", alias = "total_amount", default, deserialize_with = "flexible_f64_default")]
    t: f64,
    #[serde(default)]
    items: Vec<CompactItem>,
}

#[derive(Debug, Deserialize, Clone)]
struct CompactItem {
    #[serde(alias = "n", alias = "name", default)]
    n: String,
    #[serde(alias = "q", alias = "quantity", default)]
    q: String,
    #[serde(alias = "a", alias = "amount", default, deserialize_with = "flexible_f64_default")]
    a: f64,
    #[serde(alias = "category", default)]
    category: String,
}

// --- Full parsed types (used by rest of the system) ---

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ParsedExpenseDay {
    #[serde(default)]
    pub expense_date: String,
    #[serde(default, deserialize_with = "flexible_f64_default")]
    pub total_amount: f64,
    #[serde(default)]
    pub items: Vec<ParsedExpenseItem>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ParsedExpenseItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub quantity: String,
    #[serde(default, deserialize_with = "flexible_f64_default")]
    pub amount: f64,
    #[serde(default)]
    pub note: String,
}

// --- Keyword-based category classification ---

fn classify_category(name: &str) -> &'static str {
    let n = name.to_lowercase();
    // Nursing
    if n.contains("护理") || n.contains("床位") || n.contains("陪护") || n.contains("诊查费") {
        return "nursing";
    }
    // Test / examination
    if n.contains("检验") || n.contains("检查") || n.contains("化验") || n.contains("血常规")
        || n.contains("尿常规") || n.contains("生化") || n.contains("培养") || n.contains("涂片")
        || n.contains("CT") || n.contains("X线") || n.contains("超声") || n.contains("心电")
        || n.contains("磁共振") || n.contains("MRI") || n.contains("病理") || n.contains("免疫")
        || n.contains("抗体") || n.contains("测定") || n.contains("分析")
    {
        return "test";
    }
    // Material
    if n.contains("一次性") || n.contains("导管") || n.contains("留置针") || n.contains("注射器")
        || n.contains("输液器") || n.contains("敷料") || n.contains("接头") || n.contains("冲管")
        || n.contains("采血管") || n.contains("针头") || n.contains("引流") || n.contains("纱布")
        || n.contains("棉签") || n.contains("手套")
    {
        return "material";
    }
    // Treatment
    if n.contains("注射") || n.contains("输液") || n.contains("穿刺") || n.contains("封堵")
        || n.contains("调配") || n.contains("换药") || n.contains("治疗") || n.contains("手术")
        || n.contains("麻醉") || n.contains("抢救") || n.contains("加收") || n.contains("冲洗")
        || n.contains("灌肠") || n.contains("吸氧") || n.contains("雾化")
    {
        return "treatment";
    }
    // Drug (broad match last)
    if n.contains("片") || n.contains("胶囊") || n.contains("注射液") || n.contains("口服液")
        || n.contains("颗粒") || n.contains("滴眼") || n.contains("软膏") || n.contains("溶液")
        || n.contains("氯化钠") || n.contains("葡萄糖") || n.contains("灭菌注射用水")
        || n.contains("冻干粉") || n.contains("混悬") || n.contains("乳剂")
        || n.contains("药") || n.contains("素") || n.contains("霉") || n.contains("唑")
        || n.contains("他汀") || n.contains("普利") || n.contains("沙坦") || n.contains("洛尔")
        || n.contains("西泮") || n.contains("曲辛") || n.contains("噻吨") || n.contains("肝素")
        || n.contains("甘草") || n.contains("集）") || n.contains("（集")
    {
        return "drug";
    }
    "other"
}

/// Strip <think>...</think> blocks from model output (Qwen3 models include them by default)
fn strip_think_blocks(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result[start..].find("</think>") {
            result = format!("{}{}", &result[..start], &result[start + end + 8..]);
        } else {
            // Unclosed <think> — strip everything from <think> onwards
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Pre-process JSON string to fix duplicate "items" fields in the same object.
/// LLM sometimes outputs {"d":"...","t":...,"items":[...],"items":[...]} instead of
/// separate objects per day. This merges all "items" arrays into one.
fn merge_duplicate_items_fields(json: &str) -> String {
    // Quick check: if no duplicate "items" pattern, return as-is
    let first = json.find("\"items\"");
    if first.is_none() {
        return json.to_string();
    }
    let first_pos = first.unwrap();
    let rest = &json[first_pos + 7..];
    if rest.find("\"items\"").is_none() {
        // Only one "items" key total, or one per object - likely fine
        // But could still be duplicate within one object; let serde handle it
        return json.to_string();
    }

    // Parse as serde_json::Value using a streaming approach won't work for duplicates.
    // Instead, manually reconstruct: find each top-level object in the array,
    // and within each object, collect all "items" arrays and merge them.
    let mut result = String::with_capacity(json.len());
    let mut chars: Vec<char> = json.chars().collect();
    let mut i = 0;

    // Find the opening '['
    while i < chars.len() && chars[i] != '[' {
        result.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() {
        return json.to_string();
    }
    result.push('[');
    i += 1;

    let mut first_obj = true;
    while i < chars.len() {
        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() || chars[i] == ']' {
            break;
        }
        if chars[i] == ',' {
            i += 1;
            continue;
        }
        if chars[i] != '{' {
            // Not an object, just copy
            result.push(chars[i]);
            i += 1;
            continue;
        }

        // Extract one top-level object by bracket depth
        let obj_start = i;
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        let mut obj_end = i;
        while i < chars.len() {
            let ch = chars[i];
            if esc { esc = false; i += 1; continue; }
            if ch == '\\' && in_str { esc = true; i += 1; continue; }
            if ch == '"' { in_str = !in_str; i += 1; continue; }
            if in_str { i += 1; continue; }
            if ch == '{' { depth += 1; }
            else if ch == '}' {
                depth -= 1;
                if depth == 0 { obj_end = i; i += 1; break; }
            }
            i += 1;
        }

        let obj_str: String = chars[obj_start..=obj_end].iter().collect();

        // Check if this object has duplicate "items" keys
        let items_count = obj_str.matches("\"items\"").count();
        if items_count <= 1 {
            if !first_obj { result.push(','); }
            result.push_str(&obj_str);
            first_obj = false;
            continue;
        }

        // Has duplicate "items" - extract all items arrays and merge
        // Strategy: parse the object, collecting all items arrays
        let mut all_items = String::from("[");
        let mut other_fields = Vec::new();
        let mut j = 1; // skip opening {
        let obj_chars: Vec<char> = obj_str.chars().collect();

        while j < obj_chars.len() {
            // skip whitespace
            while j < obj_chars.len() && obj_chars[j].is_whitespace() { j += 1; }
            if j >= obj_chars.len() || obj_chars[j] == '}' { break; }
            if obj_chars[j] == ',' { j += 1; continue; }

            // Expect a key
            if obj_chars[j] != '"' { j += 1; continue; }
            let key_start = j;
            j += 1;
            while j < obj_chars.len() && obj_chars[j] != '"' {
                if obj_chars[j] == '\\' { j += 1; }
                j += 1;
            }
            j += 1; // closing quote
            let key: String = obj_chars[key_start..j].iter().collect();

            // skip colon and whitespace
            while j < obj_chars.len() && (obj_chars[j] == ':' || obj_chars[j].is_whitespace()) { j += 1; }

            // Extract value
            let val_start = j;
            if j < obj_chars.len() && (obj_chars[j] == '[' || obj_chars[j] == '{') {
                let open_ch = obj_chars[j];
                let close_ch = if open_ch == '[' { ']' } else { '}' };
                let mut vd = 0i32;
                let mut vs = false;
                let mut ve = false;
                while j < obj_chars.len() {
                    let ch = obj_chars[j];
                    if ve { ve = false; j += 1; continue; }
                    if ch == '\\' && vs { ve = true; j += 1; continue; }
                    if ch == '"' { vs = !vs; j += 1; continue; }
                    if vs { j += 1; continue; }
                    if ch == open_ch { vd += 1; }
                    else if ch == close_ch { vd -= 1; if vd == 0 { j += 1; break; } }
                    j += 1;
                }
            } else {
                // primitive value
                while j < obj_chars.len() && obj_chars[j] != ',' && obj_chars[j] != '}' { j += 1; }
            }
            let val: String = obj_chars[val_start..j].iter().collect();

            if key == "\"items\"" {
                // Append items (strip outer brackets)
                let trimmed = val.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    let inner = &trimmed[1..trimmed.len()-1].trim();
                    if !inner.is_empty() {
                        if all_items.len() > 1 { all_items.push(','); }
                        all_items.push_str(inner);
                    }
                }
            } else {
                other_fields.push(format!("{}:{}", key, val.trim()));
            }
        }
        all_items.push(']');

        // Reconstruct object
        if !first_obj { result.push(','); }
        result.push('{');
        for (fi, f) in other_fields.iter().enumerate() {
            if fi > 0 { result.push(','); }
            result.push_str(f);
        }
        if !other_fields.is_empty() { result.push(','); }
        result.push_str("\"items\":");
        result.push_str(&all_items);
        result.push('}');
        first_obj = false;
    }

    result.push(']');
    tracing::info!("merge_duplicate_items: {} -> {} 字符", json.len(), result.len());
    result
}

/// Try parsing a JSON string as expense data using multiple format strategies.
/// Returns None if all attempts fail.
fn try_parse_expense_json(json_str: &str) -> Option<Vec<ParsedExpenseDay>> {
    let preview: String = json_str.chars().take(300).collect();
    tracing::info!("try_parse 输入 {} 字符, 前300: {}", json_str.len(), preview);

    // Strategy 1: parse as array of compact format
    match serde_json::from_str::<Vec<CompactDay>>(json_str) {
        Ok(days) if !days.is_empty() => {
            let total_items: usize = days.iter().map(|d| d.items.len()).sum();
            tracing::info!("S1成功(精简数组): {} 天, {} 项", days.len(), total_items);
            return Some(compact_to_full(days));
        }
        Ok(_) => tracing::info!("S1: 空数组"),
        Err(e) => tracing::info!("S1失败: {}", e),
    }
    // Strategy 2: parse as array of full format
    match serde_json::from_str::<Vec<ParsedExpenseDay>>(json_str) {
        Ok(days) if !days.is_empty() => {
            let total_items: usize = days.iter().map(|d| d.items.len()).sum();
            tracing::info!("S2成功(完整数组): {} 天, {} 项", days.len(), total_items);
            return Some(days);
        }
        Ok(_) => tracing::info!("S2: 空数组"),
        Err(e) => tracing::info!("S2失败: {}", e),
    }
    // Strategy 3: extract balanced array from the string (handles trailing chars)
    if let Some(clean) = super::find_balanced_json(json_str, '[', ']') {
        if clean.len() != json_str.len() {
            tracing::info!("S3 清理尾部: {} -> {} 字符", json_str.len(), clean.len());
            if let Ok(days) = serde_json::from_str::<Vec<CompactDay>>(&clean) {
                let total_items: usize = days.iter().map(|d| d.items.len()).sum();
                tracing::info!("S3(精简): {} 天, {} 项", days.len(), total_items);
                if !days.is_empty() && total_items > 0 {
                    tracing::info!("S3成功(精简): {} 天, {} 项", days.len(), total_items);
                    return Some(compact_to_full(days));
                }
            }
            if let Ok(days) = serde_json::from_str::<Vec<ParsedExpenseDay>>(&clean) {
                let total_items: usize = days.iter().map(|d| d.items.len()).sum();
                if !days.is_empty() && total_items > 0 { return Some(days); }
            }
        }
    }
    // Strategy 4: single object
    if let Some(clean) = super::find_balanced_json(json_str, '{', '}') {
        tracing::info!("S4 单对象提取: {} 字符", clean.len());
        if let Ok(day) = serde_json::from_str::<CompactDay>(&clean) {
            if !day.items.is_empty() {
                return Some(compact_to_full(vec![day]));
            }
        }
        if let Ok(day) = serde_json::from_str::<ParsedExpenseDay>(&clean) {
            if !day.items.is_empty() {
                return Some(vec![day]);
            }
        }
    }
    // Strategy 5: repair truncated JSON + flatten nested days
    let repaired = repair_truncated_json(json_str);
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&repaired) {
        let days = flatten_nested_days(&val);
        let total_items: usize = days.iter().map(|d| d.items.len()).sum();
        tracing::info!("S5 修复+展平: {} 天, {} 项", days.len(), total_items);
        if !days.is_empty() && total_items > 0 {
            return Some(days);
        }
    }
    None
}

/// Repair truncated JSON by closing all unclosed brackets/braces.
/// Also handles truncation mid-string by closing the string first.
fn repair_truncated_json(s: &str) -> String {
    let trimmed = s.trim();
    let mut result = String::from(trimmed);
    let mut in_str = false;
    let mut esc = false;
    let mut stack: Vec<char> = Vec::new(); // tracks expected closing brackets

    for ch in trimmed.chars() {
        if esc { esc = false; continue; }
        if ch == '\\' && in_str { esc = true; continue; }
        if ch == '"' { in_str = !in_str; continue; }
        if in_str { continue; }
        match ch {
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => { stack.pop(); }
            _ => {}
        }
    }

    // If truncated inside a string value, close it and add a dummy value
    if in_str {
        result.push('"');
    }

    // Close all unclosed brackets in reverse order
    while let Some(close) = stack.pop() {
        result.push(close);
    }

    result
}

/// Recursively extract all day objects from a potentially nested JSON structure.
/// The model sometimes nests day2 inside day1's items array instead of making a flat array.
/// This function walks the Value tree and collects all objects that look like days.
fn flatten_nested_days(val: &serde_json::Value) -> Vec<ParsedExpenseDay> {
    let mut days = Vec::new();
    collect_day_objects(val, &mut days);
    days
}

fn collect_day_objects(val: &serde_json::Value, days: &mut Vec<ParsedExpenseDay>) {
    match val {
        serde_json::Value::Array(arr) => {
            for item in arr {
                collect_day_objects(item, days);
            }
        }
        serde_json::Value::Object(obj) => {
            let has_date = obj.contains_key("d") || obj.contains_key("expense_date");
            let has_items = obj.contains_key("items");
            if has_date && has_items {
                // This is a day object
                let date = obj.get("d").or_else(|| obj.get("expense_date"))
                    .and_then(|v| v.as_str()).unwrap_or("").to_string();
                let total = obj.get("t").or_else(|| obj.get("total_amount"))
                    .and_then(|v| v.as_f64()).unwrap_or(0.0);

                let mut items = Vec::new();
                if let Some(serde_json::Value::Array(arr)) = obj.get("items") {
                    for item_val in arr {
                        if let Some(item_obj) = item_val.as_object() {
                            // Check if this is a nested day object
                            let is_nested_day = (item_obj.contains_key("d") || item_obj.contains_key("expense_date"))
                                && item_obj.contains_key("items");
                            if is_nested_day {
                                collect_day_objects(item_val, days);
                            } else {
                                // Regular item
                                let name = item_obj.get("n").or_else(|| item_obj.get("name"))
                                    .and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let qty = item_obj.get("q").or_else(|| item_obj.get("quantity"))
                                    .and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let amount = item_obj.get("a").or_else(|| item_obj.get("amount"))
                                    .and_then(|v| v.as_f64()).unwrap_or(0.0);
                                let category = item_obj.get("category")
                                    .and_then(|v| v.as_str()).unwrap_or("");
                                let cat = if category.is_empty() {
                                    classify_category(&name).to_string()
                                } else {
                                    category.to_string()
                                };
                                if !name.is_empty() {
                                    items.push(ParsedExpenseItem {
                                        name,
                                        category: cat,
                                        quantity: qty,
                                        amount,
                                        note: String::new(),
                                    });
                                }
                            }
                        }
                    }
                }
                days.push(ParsedExpenseDay {
                    expense_date: date,
                    total_amount: total,
                    items,
                });
            }
        }
        _ => {}
    }
}

fn compact_to_full(days: Vec<CompactDay>) -> Vec<ParsedExpenseDay> {
    days.into_iter()
        .map(|d| ParsedExpenseDay {
            expense_date: d.d,
            total_amount: d.t,
            items: d.items.into_iter().map(|item| {
                let cat = if item.category.is_empty() {
                    classify_category(&item.n).to_string()
                } else {
                    item.category
                };
                ParsedExpenseItem {
                    name: item.n,
                    category: cat,
                    quantity: item.q,
                    amount: item.a,
                    note: String::new(),
                }
            }).collect(),
        })
        .collect()
}

#[derive(Debug, Serialize, Clone)]
pub struct DayParseResult {
    pub parsed: ParsedExpenseDay,
    pub drug_analysis: String,
    pub treatment_analysis: String,
}

#[derive(Serialize)]
pub struct ExpenseParseResponse {
    pub days: Vec<DayParseResult>,
}

// --- Handlers ---

/// Read upload file directly into memory (no disk I/O)
async fn read_upload_bytes(multipart: &mut Multipart) -> Result<(Vec<u8>, String), AppError> {
    match multipart.next_field().await {
        Ok(Some(field)) => {
            let fname = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}.bin", Uuid::new_v4()));
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("读取上传数据失败: {}", e)))?;
            Ok((data.to_vec(), fname))
        }
        Ok(None) => Err(AppError::BadRequest("未找到上传文件".to_string())),
        Err(e) => Err(AppError::BadRequest(format!("读取上传字段失败: {}", e))),
    }
}

/// Compress image to WebP format with max dimension constraint (runs in blocking thread).
/// Returns (webp_bytes, mime_type). PDF files are returned as-is.
fn compress_image_to_webp(raw_bytes: &[u8], file_name: &str, max_dim: u32) -> Result<(Vec<u8>, &'static str), String> {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".pdf") {
        return Ok((raw_bytes.to_vec(), "application/pdf"));
    }

    let img = image::load_from_memory(raw_bytes)
        .map_err(|e| format!("解析图片失败: {}", e))?;

    let (w, h) = (img.width(), img.height());
    let resized = if w > max_dim || h > max_dim {
        let ratio = max_dim as f64 / w.max(h) as f64;
        let new_w = (w as f64 * ratio) as u32;
        let new_h = (h as f64 * ratio) as u32;
        tracing::info!("图片缩放: {}x{} -> {}x{}", w, h, new_w, new_h);
        img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3)
    } else {
        tracing::info!("图片尺寸 {}x{} 无需缩放", w, h);
        img
    };

    let mut buf = Cursor::new(Vec::new());
    resized.write_to(&mut buf, image::ImageFormat::WebP)
        .map_err(|e| format!("WebP 编码失败: {}", e))?;

    let webp_bytes = buf.into_inner();
    tracing::info!("图片压缩: {} -> {} bytes (WebP)", raw_bytes.len(), webp_bytes.len());
    Ok((webp_bytes, "image/webp"))
}

/// Parse expense list screenshot using vision model + LLM analysis
pub async fn parse_expense(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<ExpenseParseResponse>>, AppError> {
    let (raw_bytes, file_name) = read_upload_bytes(&mut multipart).await?;
    let client = state.http_client.clone();

    let lower = file_name.to_lowercase();
    let is_supported = lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".pdf");

    if !is_supported {
        return Err(AppError::BadRequest(
            "不支持的文件格式，请上传图片或 PDF 文件".to_string(),
        ));
    }

    let orig_size = raw_bytes.len();
    tracing::info!("开始识别消费清单: {} ({} bytes)", file_name, orig_size);

    // Step 1: Vision model to extract expense items (may contain multiple days)
    let parsed_days = recognize_expense_bytes(&raw_bytes, &file_name, &client).await.map_err(|e| {
        tracing::warn!("消费清单识别失败: {}", e);
        AppError::Internal(format!("消费清单识别失败: {}", e))
    })?;
    let total_items: usize = parsed_days.iter().map(|d| d.items.len()).sum();
    tracing::info!("消费清单识别完成, 共 {} 天 {} 项", parsed_days.len(), total_items);

    // Return recognition results immediately (analysis is done separately via /api/expenses/analyze)
    let day_results: Vec<DayParseResult> = parsed_days
        .into_iter()
        .map(|day| DayParseResult {
            parsed: day,
            drug_analysis: String::new(),
            treatment_analysis: String::new(),
        })
        .collect();
    tracing::info!("消费清单识别完成，返回 {} 天结果（分析将按需调用）", day_results.len());

    Ok(Json(ApiResponse::ok(
        ExpenseParseResponse {
            days: day_results,
        },
        "消费清单解析成功",
    )))
}

/// Confirm and save parsed expense data
pub async fn confirm_expense(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<ConfirmExpenseReq>,
) -> Result<Json<ApiResponse<DailyExpenseDetail>>, AppError> {
    // Validate patient exists
    let db = state.db.clone();
    let pid = patient_id.clone();
    let patient = tokio::task::spawn_blocking(move || db.get_patient(&pid))
        .await
        .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    if patient.is_none() {
        return Err(AppError::NotFound("患者不存在".to_string()));
    }

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let expense_id = Uuid::new_v4().to_string();

    let expense = DailyExpense {
        id: expense_id.clone(),
        patient_id: patient_id.clone(),
        expense_date: req.expense_date,
        total_amount: req.total_amount,
        drug_analysis: req.drug_analysis,
        treatment_analysis: req.treatment_analysis,
        created_at: now,
    };

    let items: Vec<ExpenseItem> = req
        .items
        .into_iter()
        .map(|item| ExpenseItem {
            id: Uuid::new_v4().to_string(),
            expense_id: expense_id.clone(),
            name: item.name,
            category: item.category,
            quantity: item.quantity,
            amount: item.amount,
            note: item.note,
        })
        .collect();

    let db = state.db.clone();
    let exp_clone = expense.clone();
    let items_clone = items.clone();
    tokio::task::spawn_blocking(move || db.create_expense(&exp_clone, &items_clone))
        .await
        .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    Ok(Json(ApiResponse::ok(
        DailyExpenseDetail {
            expense,
            items,
        },
        "消费记录保存成功",
    )))
}

/// Batch confirm and save multiple days of expense data in one request
pub async fn batch_confirm_expense(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    Json(req): Json<BatchConfirmExpenseReq>,
) -> Result<Json<ApiResponse<Vec<DailyExpenseDetail>>>, AppError> {
    if req.days.is_empty() {
        return Ok(Json(ApiResponse::ok(vec![], "无数据需要保存")));
    }

    // Validate patient exists
    let db = state.db.clone();
    let pid = patient_id.clone();
    let patient = tokio::task::spawn_blocking(move || db.get_patient(&pid))
        .await
        .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    if patient.is_none() {
        return Err(AppError::NotFound("患者不存在".to_string()));
    }

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut results = Vec::with_capacity(req.days.len());

    for day_req in req.days {
        if day_req.items.is_empty() {
            continue;
        }

        let expense_id = Uuid::new_v4().to_string();
        let expense = DailyExpense {
            id: expense_id.clone(),
            patient_id: patient_id.clone(),
            expense_date: day_req.expense_date,
            total_amount: day_req.total_amount,
            drug_analysis: day_req.drug_analysis,
            treatment_analysis: day_req.treatment_analysis,
            created_at: now.clone(),
        };

        let items: Vec<ExpenseItem> = day_req
            .items
            .into_iter()
            .map(|item| ExpenseItem {
                id: Uuid::new_v4().to_string(),
                expense_id: expense_id.clone(),
                name: item.name,
                category: item.category,
                quantity: item.quantity,
                amount: item.amount,
                note: item.note,
            })
            .collect();

        let db = state.db.clone();
        let exp_clone = expense.clone();
        let items_clone = items.clone();
        tokio::task::spawn_blocking(move || db.create_expense(&exp_clone, &items_clone))
            .await
            .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

        results.push(DailyExpenseDetail { expense, items });
    }

    let count = results.len();
    Ok(Json(ApiResponse::ok(
        results,
        &format!("{}天消费记录保存成功", count),
    )))
}

/// List all expenses for a patient
pub async fn list_expenses(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<DailyExpenseSummary>>>, AppError> {
    let db = state.db.clone();
    let summaries = tokio::task::spawn_blocking(move || db.list_expenses_by_patient(&patient_id))
        .await
        .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    Ok(Json(ApiResponse::ok(summaries, "查询成功")))
}

/// Get expense detail
pub async fn get_expense(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<DailyExpenseDetail>>, AppError> {
    let db = state.db.clone();
    let detail = tokio::task::spawn_blocking(move || db.get_expense_detail(&id))
        .await
        .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    match detail {
        Some(d) => Ok(Json(ApiResponse::ok(d, "查询成功"))),
        None => Err(AppError::NotFound("消费记录不存在".to_string())),
    }
}

/// Delete expense record
pub async fn delete_expense(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.delete_expense(&id))
        .await
        .map_err(|e| AppError::Internal(format!("任务执行失败: {}", e)))??;

    Ok(Json(ApiResponse::ok_msg("删除成功")))
}

// --- Vision model call for expense parsing (in-memory, WebP compressed) ---

async fn recognize_expense_bytes(
    raw_bytes: &[u8],
    file_name: &str,
    client: &reqwest::Client,
) -> Result<Vec<ParsedExpenseDay>, String> {
    let api_key = std::env::var("SILICONFLOW_API_KEY")
        .map_err(|_| "环境变量 SILICONFLOW_API_KEY 未设置".to_string())?;

    const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;
    if raw_bytes.len() > MAX_FILE_SIZE {
        return Err(format!(
            "文件过大（{}MB），最大支持 50MB",
            raw_bytes.len() / 1024 / 1024
        ));
    }

    // Compress image to WebP (resize max 1500px) in blocking thread; PDF passes through
    let raw_owned = raw_bytes.to_vec();
    let fname_owned = file_name.to_string();
    let (compressed, mime) = tokio::task::spawn_blocking(move || {
        compress_image_to_webp(&raw_owned, &fname_owned, 1500)
    })
    .await
    .map_err(|e| format!("图片压缩任务失败: {}", e))??;

    let data_url = tokio::task::spawn_blocking(move || {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&compressed);
        tracing::info!("Base64 编码: {} bytes -> {} chars", compressed.len(), b64.len());
        format!("data:{};base64,{}", mime, b64)
    })
    .await
    .map_err(|e| format!("编码文件失败: {}", e))?;

    let body = serde_json::json!({
        "model": "Qwen/Qwen3-VL-32B-Instruct",
        "messages": [
            {
                "role": "system",
                "content": EXPENSE_VISION_PROMPT
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": { "url": data_url }
                    },
                    {
                        "type": "text",
                        "text": "请提取这份消费清单中所有日期的项目。"
                    }
                ]
            }
        ],
        "temperature": 0.1,
        "max_tokens": 8192
    });

    let api_url = "https://api.siliconflow.cn/v1/chat/completions";
    tracing::info!("调用 Vision API (enable_thinking=false, 600s超时): {}", api_url);
    let resp = client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(600))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Vision API 请求失败: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Vision API 错误 HTTP {}: {}", status, text));
    }

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 API 响应失败: {}", e))?;

    if let Some(err) = resp_json.get("error") {
        return Err(format!("API 错误: {}", err));
    }

    let raw_content = super::extract_llm_content(&resp_json)?;
    tracing::info!("消费清单原始内容 {} 字符", raw_content.len());

    // Strip <think>...</think> blocks as fallback (should be rare with enable_thinking=false)
    let content = strip_think_blocks(&raw_content);
    if content.len() != raw_content.len() {
        tracing::info!("剥离think块: {} -> {} 字符", raw_content.len(), content.len());
    }

    // Try extract_json_block first (works for well-formed JSON)
    let json_str = super::extract_json_block(&content);

    if let Ok(ref js) = json_str {
        tracing::info!("JSON提取成功 {} 字符", js.len());
        if let Some(days) = try_parse_expense_json(js) {
            return Ok(days);
        }
    } else {
        tracing::info!("JSON提取失败 (截断?), 尝试修复");
    }

    // Fallback: repair truncated JSON + flatten nested days (handles both truncation and nesting)
    let repaired = repair_truncated_json(&content);
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&repaired) {
        let days = flatten_nested_days(&val);
        let total_items: usize = days.iter().map(|d| d.items.len()).sum();
        tracing::info!("修复+展平: {} 天, {} 项", days.len(), total_items);
        if !days.is_empty() && total_items > 0 {
            return Ok(days);
        }
    }

    let preview = if content.chars().count() > 200 {
        let p: String = content.chars().take(200).collect();
        format!("{}...(共{}字符)", p, content.len())
    } else {
        content.clone()
    };
    let log_preview: String = content.chars().take(500).collect();
    tracing::error!("消费清单解析全部失败, 原始内容前500: {}", log_preview);
    Err(format!("消费清单识别结果格式异常，请重试。内容预览: {}", preview))
}

// --- Analyze expense day handler (separate endpoint) ---

#[derive(Deserialize)]
pub struct AnalyzeExpenseReq {
    pub items: Vec<ParsedExpenseItem>,
}

#[derive(Serialize)]
pub struct AnalyzeExpenseResp {
    pub drug_analysis: String,
    pub treatment_analysis: String,
}

/// Analyze a single day's expense items using LLM
pub async fn analyze_expense_day(
    State(state): State<AppState>,
    Json(req): Json<AnalyzeExpenseReq>,
) -> Result<Json<ApiResponse<AnalyzeExpenseResp>>, AppError> {
    if req.items.is_empty() {
        return Ok(Json(ApiResponse::ok(
            AnalyzeExpenseResp {
                drug_analysis: String::new(),
                treatment_analysis: String::new(),
            },
            "无项目需要分析",
        )));
    }

    let client = state.http_client.clone();
    let (drug_analysis, treatment_analysis) =
        analyze_treatment(&client, &req.items).await.unwrap_or_else(|e| {
            tracing::warn!("治疗方案分析失败: {}", e);
            (String::new(), String::new())
        });

    Ok(Json(ApiResponse::ok(
        AnalyzeExpenseResp {
            drug_analysis,
            treatment_analysis,
        },
        "分析完成",
    )))
}

// --- LLM analysis of treatment plan ---

async fn analyze_treatment(
    client: &reqwest::Client,
    items: &[ParsedExpenseItem],
) -> Result<(String, String), String> {
    let prompt = build_analysis_prompt(items);
    let api_key = super::get_llm_api_key();

    let body = serde_json::json!({
        "model": super::LLM_MODEL_FAST,
        "messages": [
            { "role": "system", "content": ANALYSIS_SYSTEM_PROMPT },
            { "role": "user", "content": prompt },
        ],
        "temperature": 0.3
    });

    let resp = super::llm_post_with_retry(client, super::LLM_API_URL, &api_key, &body).await?;
    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析分析结果失败: {}", e))?;

    let content = super::extract_llm_content(&resp_json)?;
    let json_str = super::extract_json_block(&content)?;

    #[derive(Deserialize)]
    struct AnalysisResult {
        #[serde(default)]
        drug_analysis: String,
        #[serde(default)]
        treatment_analysis: String,
    }

    let result: AnalysisResult = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析分析 JSON 失败: {}", e))?;

    Ok((result.drug_analysis, result.treatment_analysis))
}

// --- Chunk-based parallel parsing ---

const CHUNK_VISION_PROMPT: &str = r#"从住院消费清单截图的一个局部区域中提取项目。这可能是完整清单的一部分，可能缺少表头或日期信息。
按日期分组，每天一个独立对象，返回JSON数组。
格式：[{"d":"YYYY-MM-DD","t":合计,"items":[{"n":"名称","q":"×数量","a":金额}]}]
规则：n=原始名称，q=数量（无则""），a=金额（退费为负），t=当日合计（如无法计算填0）
如果无法确定日期，d填""。尽可能提取所有可见的项目行。
只返回JSON"#;

/// Parse a single image chunk (strip) from a split expense list
pub async fn parse_chunk(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<Vec<ParsedExpenseDay>>>, AppError> {
    let (raw_bytes, file_name) = read_upload_bytes(&mut multipart).await?;
    let client = state.http_client.clone();

    let lower = file_name.to_lowercase();
    let is_supported = lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp");

    if !is_supported {
        return Err(AppError::BadRequest(
            "不支持的文件格式，请上传图片文件".to_string(),
        ));
    }

    tracing::info!("开始识别消费清单条带: {} ({} bytes)", file_name, raw_bytes.len());

    let parsed_days = recognize_chunk_bytes(&raw_bytes, &file_name, &client).await.map_err(|e| {
        tracing::warn!("条带识别失败: {}", e);
        AppError::Internal(format!("条带识别失败: {}", e))
    })?;

    let total_items: usize = parsed_days.iter().map(|d| d.items.len()).sum();
    tracing::info!("条带识别完成: {} 天, {} 项", parsed_days.len(), total_items);

    Ok(Json(ApiResponse::ok(parsed_days, "条带解析成功")))
}

/// Vision model call for a single chunk (similar to recognize_expense_bytes but with chunk-specific prompt)
async fn recognize_chunk_bytes(
    raw_bytes: &[u8],
    file_name: &str,
    client: &reqwest::Client,
) -> Result<Vec<ParsedExpenseDay>, String> {
    let api_key = std::env::var("SILICONFLOW_API_KEY")
        .map_err(|_| "环境变量 SILICONFLOW_API_KEY 未设置".to_string())?;

    // Compress image to WebP in blocking thread
    let raw_owned = raw_bytes.to_vec();
    let fname_owned = file_name.to_string();
    let (compressed, mime) = tokio::task::spawn_blocking(move || {
        compress_image_to_webp(&raw_owned, &fname_owned, 1500)
    })
    .await
    .map_err(|e| format!("图片压缩任务失败: {}", e))??;

    let data_url = tokio::task::spawn_blocking(move || {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&compressed);
        format!("data:{};base64,{}", mime, b64)
    })
    .await
    .map_err(|e| format!("编码文件失败: {}", e))?;

    let body = serde_json::json!({
        "model": "Qwen/Qwen3-VL-32B-Instruct",
        "messages": [
            {
                "role": "system",
                "content": CHUNK_VISION_PROMPT
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": { "url": data_url }
                    },
                    {
                        "type": "text",
                        "text": "请提取这部分消费清单中所有可见的项目。"
                    }
                ]
            }
        ],
        "temperature": 0.1,
        "max_tokens": 4096
    });

    let api_url = "https://api.siliconflow.cn/v1/chat/completions";
    let resp = client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(300))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Vision API 请求失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Vision API 错误 HTTP {}: {}", status, text));
    }

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 API 响应失败: {}", e))?;

    if let Some(err) = resp_json.get("error") {
        return Err(format!("API 错误: {}", err));
    }

    let raw_content = super::extract_llm_content(&resp_json)?;
    let content = strip_think_blocks(&raw_content);

    let json_str = super::extract_json_block(&content);
    if let Ok(ref js) = json_str {
        if let Some(days) = try_parse_expense_json(js) {
            return Ok(days);
        }
    }

    // Fallback: repair truncated JSON
    let repaired = repair_truncated_json(&content);
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&repaired) {
        let days = flatten_nested_days(&val);
        let total_items: usize = days.iter().map(|d| d.items.len()).sum();
        if !days.is_empty() && total_items > 0 {
            return Ok(days);
        }
    }

    Err("条带识别结果格式异常".to_string())
}

// --- Merge chunks handler ---

#[derive(Debug, Deserialize)]
pub struct ChunkResult {
    pub chunk_index: usize,
    pub days: Vec<ParsedExpenseDay>,
}

#[derive(Debug, Deserialize)]
pub struct MergeChunksReq {
    pub chunks: Vec<ChunkResult>,
}

/// Merge multiple chunk recognition results using LLM text model
pub async fn merge_chunks(
    State(state): State<AppState>,
    Json(req): Json<MergeChunksReq>,
) -> Result<Json<ApiResponse<ExpenseParseResponse>>, AppError> {
    if req.chunks.is_empty() {
        return Ok(Json(ApiResponse::ok(
            ExpenseParseResponse { days: vec![] },
            "无数据",
        )));
    }

    // If only one chunk, return directly without LLM merge
    if req.chunks.len() == 1 {
        let days = req.chunks.into_iter().next().unwrap().days;
        let day_results: Vec<DayParseResult> = days
            .into_iter()
            .map(|day| DayParseResult {
                parsed: day,
                drug_analysis: String::new(),
                treatment_analysis: String::new(),
            })
            .collect();
        return Ok(Json(ApiResponse::ok(
            ExpenseParseResponse { days: day_results },
            "解析成功",
        )));
    }

    // Sort chunks by index
    let mut chunks = req.chunks;
    chunks.sort_by_key(|c| c.chunk_index);

    // Build merge prompt with all chunk data
    let mut chunks_text = String::new();
    for chunk in &chunks {
        chunks_text.push_str(&format!("\n--- 区域 {} ---\n", chunk.chunk_index + 1));
        for day in &chunk.days {
            let date_str = if day.expense_date.is_empty() { "未知日期" } else { &day.expense_date };
            chunks_text.push_str(&format!("日期: {}, 合计: {:.2}\n", date_str, day.total_amount));
            for item in &day.items {
                chunks_text.push_str(&format!("  - {} {} ¥{:.2}\n", item.name, item.quantity, item.amount));
            }
        }
    }

    let merge_prompt = format!(
        r#"以下是同一张住院消费清单图片被分成 {} 个区域后分别识别的结果。相邻区域之间有重叠，可能导致部分项目重复出现。

请你合并这些结果，完成以下任务：
1. 去除重复项目（相同名称、数量、金额的项目只保留一个）
2. 将所有项目按日期正确归组
3. 如果某些项目缺少日期（标为"未知日期"），根据上下文推断其所属日期
4. 重新计算每天的合计金额
5. 按日期升序排列

返回合并后的JSON数组，格式：
[{{"d":"YYYY-MM-DD","t":合计,"items":[{{"n":"名称","q":"×数量","a":金额}}]}}]
只返回JSON，不要有任何额外说明。

识别结果：
{}"#,
        chunks.len(),
        chunks_text
    );

    let client = state.http_client.clone();
    let api_key = super::get_llm_api_key();

    let body = serde_json::json!({
        "model": super::LLM_MODEL_FAST,
        "messages": [
            {
                "role": "system",
                "content": "你是一个数据整理助手。请根据多个局部识别结果，合并去重后输出完整的消费清单数据。"
            },
            {
                "role": "user",
                "content": merge_prompt
            }
        ],
        "temperature": 0.1,
        "max_tokens": 8192
    });

    tracing::info!("调用 LLM 合并 {} 个条带的识别结果", chunks.len());

    let resp = super::llm_post_with_retry(&client, super::LLM_API_URL, &api_key, &body)
        .await
        .map_err(|e| AppError::Internal(format!("合并请求失败: {}", e)))?;

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("解析合并响应失败: {}", e)))?;

    let raw_content = super::extract_llm_content(&resp_json)
        .map_err(|e| AppError::Internal(e))?;
    let content = strip_think_blocks(&raw_content);

    let json_str = super::extract_json_block(&content)
        .map_err(|e| AppError::Internal(format!("合并结果提取 JSON 失败: {}", e)))?;

    let merged_days = try_parse_expense_json(&json_str)
        .ok_or_else(|| AppError::Internal("合并结果解析失败".to_string()))?;

    let total_items: usize = merged_days.iter().map(|d| d.items.len()).sum();
    tracing::info!("合并完成: {} 天, {} 项", merged_days.len(), total_items);

    let day_results: Vec<DayParseResult> = merged_days
        .into_iter()
        .map(|day| DayParseResult {
            parsed: day,
            drug_analysis: String::new(),
            treatment_analysis: String::new(),
        })
        .collect();

    Ok(Json(ApiResponse::ok(
        ExpenseParseResponse { days: day_results },
        "合并解析成功",
    )))
}
