use std::collections::HashMap;

/// Build a few-shot LLM prompt for merge verification.
///
/// This function is synchronous and only constructs the prompt string.
/// The actual LLM call is handled by the handler layer (async).
pub fn build_merge_verify_prompt(
    type_a: &str,
    date_a: &str,
    items_a: &[String],
    type_b: &str,
    date_b: &str,
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
        r#"你是医疗检验报告合并判断专家。判断两份报告是否属于同一次检查的不同页面（应合并）。

【示例1 → 合并】
报告A: 脑脊液常规, 2024-03-15, [潘氏试验, 白细胞计数, 红细胞计数]
报告B: 脑脊液生化, 2024-03-15, [葡萄糖, 氯, 蛋白质]
→ {{"merge": true, "reason": "同一脑脊液标本的常规+生化，项目互补"}}

【示例2 → 不合并】
报告A: 血常规, 2024-03-15, [白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
报告B: 肝功能, 2024-03-15, [丙氨酸氨基转移酶, 天门冬氨酸氨基转移酶, 总胆红素]
→ {{"merge": false, "reason": "完全不同的检查类别，虽同日但是独立报告"}}

【示例3 → 不合并(复查)】
报告A: 血常规, 2024-03-15, [白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
报告B: 血常规, 2024-03-15, [白细胞计数, 红细胞计数, 血红蛋白, 血小板计数]
→ {{"merge": false, "reason": "相同项目重复出现，是同日复查而非拆页"}}

【示例4 → 合并(多页扫描)】
报告A: 生化全套, 2024-03-15, [丙氨酸氨基转移酶, 天门冬氨酸氨基转移酶, 总胆红素, 肌酐, 尿素氮]
报告B: 生化全套, 2024-03-15, [总胆固醇, 甘油三酯, 高密度脂蛋白胆固醇, 低密度脂蛋白胆固醇, 葡萄糖]
→ {{"merge": true, "reason": "同一生化全套报告拆成两页，项目互补无重叠"}}

【待判断】
报告A: {type_a}, {date_a}, [{items_a_str}]
报告B: {type_b}, {date_b}, [{items_b_str}]
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
            &["潘氏试验".into(), "白细胞计数".into()],
            "脑脊液生化",
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
            &[],
            "未知检查",
            "2024-03-15",
            &[],
        );
        assert!(prompt.contains("（无项目信息）"));
    }
}
