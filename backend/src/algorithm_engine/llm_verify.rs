use std::collections::HashMap;

/// System prompt for merge verification LLM calls.
pub const MERGE_VERIFY_SYSTEM_PROMPT: &str = r#"你是医疗检验报告合并判断专家。判断两份报告是否属于同一次检查的不同页面（应合并）。

输入会同时给出：
- sample_date：检查/采样/送检日期（可能为空）
- report_date：报告出具/审核/打印日期

请先为每份报告计算 effective_date：
- 如果 sample_date 非空，用 sample_date
- 否则用 report_date

日期判断一律基于 effective_date。

判断规则（按优先级）：
1. effective_date 不同 → 一定不合并（不同日期的检查不可能是同一份报告的拆页）
2. 日期相同时，检查项目大量重复 → 不合并（是同日复查）
3. 日期相同、类型相关、项目互补无重叠 → 合并（是同一报告的不同页面）
4. 日期相同但类型完全无关 → 不合并（是独立报告）

只返回 JSON，格式：{"merge": true/false, "reason": "简短理由"}，不要有任何额外文字。"#;

/// Build a few-shot LLM prompt for merge verification.
///
/// This function is synchronous and only constructs the prompt string.
/// The actual LLM call is handled by the handler layer (async).
pub fn build_merge_verify_prompt(
    type_a: &str,
    report_date_a: &str,
    sample_date_a: &str,
    items_a: &[String],
    type_b: &str,
    report_date_b: &str,
    sample_date_b: &str,
    items_b: &[String],
) -> String {
    let items_a_str = if items_a.is_empty() {
        "（无项目信息）".to_string()
    } else {
        items_a.join(", ")
    };
    let items_b_str = if items_b.is_empty() {
        "（无项目信息）".to_string()
    } else {
        items_b.join(", ")
    };

    format!(
        r#"【示例1 → 合并】
报告A: 脑脊液常规, sample_date=2024-03-15, report_date=2024-03-16, items=[潘氏试验, 白细胞计数, 红细胞计数]
报告B: 脑脊液生化, sample_date=2024-03-15, report_date=2024-03-16, items=[葡萄糖, 氯, 蛋白质]
→ {{"merge": true, "reason": "同一脑脊液标本的常规+生化，项目互补"}}

【示例2 → 不合并】
报告A: 血常规, sample_date=2024-03-15, report_date=2024-03-15, items=[白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
报告B: 肝功能, sample_date=2024-03-15, report_date=2024-03-15, items=[丙氨酸氨基转移酶, 天门冬氨酸氨基转移酶, 总胆红素]
→ {{"merge": false, "reason": "完全不同的检查类别，虽同日但是独立报告"}}

【示例3 → 不合并(复查)】
报告A: 血常规, sample_date=2024-03-15, report_date=2024-03-15, items=[白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
报告B: 血常规, sample_date=2024-03-15, report_date=2024-03-15, items=[白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
→ {{"merge": false, "reason": "相同项目重复出现，是同日复查而非拆页"}}

【示例4 → 合并(多页扫描)】
报告A: 生化全套, sample_date=2024-03-15, report_date=2024-03-15, items=[丙氨酸氨基转移酶, 天门冬氨酸氨基转移酶, 总胆红素, 肌酐, 尿素氮]
报告B: 生化全套, sample_date=2024-03-15, report_date=2024-03-15, items=[总胆固醇, 甘油三酯, 高密度脂蛋白胆固醇, 低密度脂蛋白胆固醇, 葡萄糖]
→ {{"merge": true, "reason": "同一生化全套报告拆成两页，项目互补无重叠"}}

【示例5 → 不合并(日期不同)】
报告A: 血常规, sample_date=2024-03-15, report_date=2024-03-15, items=[白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
报告B: 血常规, sample_date=2024-03-18, report_date=2024-03-18, items=[白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
→ {{"merge": false, "reason": "日期不同，是不同日期的检查"}}

【待判断】
报告A: {type_a}, sample_date={sample_date_a}, report_date={report_date_a}, items=[{items_a_str}]
报告B: {type_b}, sample_date={sample_date_b}, report_date={report_date_b}, items=[{items_b_str}]
→ "#
    )
}

/// Parse the LLM merge verification response.
/// Returns Some(true) for merge, Some(false) for no-merge, None if unparseable.
pub fn parse_merge_verify_response(content: &str) -> Option<bool> {
    let trimmed = content.trim();

    // Try to find JSON object
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[start..=end]
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Parse as generic JSON
    if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(json_str) {
        if let Some(merge_val) = map.get("merge") {
            return merge_val.as_bool();
        }
    }

    // Fallback: look for "true" or "false" keywords
    let lower = trimmed.to_lowercase();
    if lower.contains("\"merge\": true") || lower.contains("\"merge\":true") {
        return Some(true);
    }
    if lower.contains("\"merge\": false") || lower.contains("\"merge\":false") {
        return Some(false);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_generation() {
        let prompt = build_merge_verify_prompt(
            "脑脊液常规",
            "2024-03-15",
            "2024-03-15",
            &["潘氏试验".into(), "白细胞计数".into()],
            "脑脊液生化",
            "2024-03-15",
            "2024-03-15",
            &["葡萄糖".into(), "氯".into()],
        );
        assert!(prompt.contains("脑脊液常规"));
        assert!(prompt.contains("脑脊液生化"));
        assert!(prompt.contains("潘氏试验, 白细胞计数"));
        assert!(prompt.contains("葡萄糖, 氯"));
    }

    #[test]
    fn parse_merge_true() {
        let resp = r#"{"merge": true, "reason": "同一标本的不同检查"}"#;
        assert_eq!(parse_merge_verify_response(resp), Some(true));
    }

    #[test]
    fn parse_merge_false() {
        let resp = r#"{"merge": false, "reason": "不同类别"}"#;
        assert_eq!(parse_merge_verify_response(resp), Some(false));
    }

    #[test]
    fn parse_with_surrounding_text() {
        let resp = r#"Based on the analysis: {"merge": true, "reason": "互补项目"} That's my answer."#;
        assert_eq!(parse_merge_verify_response(resp), Some(true));
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse_merge_verify_response("无法判断"), None);
    }

    #[test]
    fn empty_items() {
        let prompt = build_merge_verify_prompt(
            "未知检查",
            "2024-03-15",
            "2024-03-15",
            &[],
            "未知检查",
            "2024-03-15",
            "2024-03-15",
            &[],
        );
        assert!(prompt.contains("（无项目信息）"));
    }
}
