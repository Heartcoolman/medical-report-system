use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Deserialize)]
struct SynonymData {
    synonyms: HashMap<String, String>,
}

/// Static synonym dictionary loaded once from the embedded JSON.
static SYNONYMS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let json_bytes = include_bytes!("dictionaries/item_synonyms.json");
    let data: SynonymData =
        serde_json::from_slice(json_bytes).expect("无法解析 item_synonyms.json");
    data.synonyms
});

/// Result of normalizing a single name.
#[derive(Debug, Clone)]
pub struct NormalizeResult {
    /// The normalized canonical name.
    pub canonical: String,
    /// How the name was resolved.
    pub method: NormalizeMethod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NormalizeMethod {
    /// Matched by rule-based normalization or synonym dictionary.
    Dictionary,
    /// Matched by fuzzy similarity to an existing canonical name.
    FuzzyMatch,
    /// Could not be resolved — needs LLM fallback.
    Unresolved,
}

/// Normalize a batch of item names.
///
/// - `names_by_report_type`: map of report_type → list of item names
/// - `existing_canonical`: canonical names already in the system (for fuzzy matching)
///
/// Returns map of original_name → NormalizeResult.
pub fn normalize_batch(
    names_by_report_type: &HashMap<String, Vec<String>>,
    existing_canonical: &[String],
) -> HashMap<String, NormalizeResult> {
    let mut results: HashMap<String, NormalizeResult> = HashMap::new();

    for (report_type, names) in names_by_report_type {
        for name in names {
            let trimmed = name.trim();
            if trimmed.is_empty() || results.contains_key(trimmed) {
                continue;
            }

            let (rule_or_dict, resolved_by_dict) = rule_dict_normalize(trimmed, report_type);
            if resolved_by_dict {
                results.insert(
                    trimmed.to_string(),
                    NormalizeResult {
                        canonical: rule_or_dict,
                        method: NormalizeMethod::Dictionary,
                    },
                );
                continue;
            }

            // Layer 3: Fuzzy match against existing canonical names
            if !existing_canonical.is_empty() {
                if let Some(matched) = fuzzy_match(&rule_or_dict, existing_canonical) {
                    results.insert(
                        trimmed.to_string(),
                        NormalizeResult {
                            canonical: matched,
                            method: NormalizeMethod::FuzzyMatch,
                        },
                    );
                    continue;
                }
            }

            // No match — mark as unresolved (LLM fallback needed)
            // Use rule-normalized form as the best-effort result
            results.insert(
                trimmed.to_string(),
                NormalizeResult {
                    canonical: rule_or_dict,
                    method: NormalizeMethod::Unresolved,
                },
            );
        }
    }

    results
}

/// Normalize a single item name for non-LLM scoring paths.
///
/// This applies the same rule + dictionary stages as `normalize_batch`, but it
/// does not perform fuzzy matching and never marks unresolved.
pub fn normalize_for_scoring(name: &str, report_type: &str) -> String {
    let (canonical, _) = rule_dict_normalize(name, report_type);
    canonical
}

/// Normalize an item name for trend aggregation.
///
/// Uses a dictionary-first strategy (same as `rule_dict_normalize`) to preserve
/// clinically meaningful prefixes (e.g. "超敏C反应蛋白" stays distinct from
/// "C反应蛋白"), then applies trend-specific post-processing.
pub fn normalize_for_trend(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Dictionary lookup on ORIGINAL name first — protects clinical prefixes
    if let Some(canonical) = SYNONYMS.get(trimmed) {
        return trend_post_process(canonical);
    }
    if is_canonical_value(trimmed) {
        return trend_post_process(trimmed);
    }

    // Rule-based normalization (trends: strip ALL fluid prefixes unconditionally)
    let rule_normalized = rule_normalize(trimmed, "脑脊液尿液粪便");

    // Dictionary lookup on rule-normalized form
    if rule_normalized != trimmed {
        if let Some(canonical) = SYNONYMS.get(&rule_normalized) {
            return trend_post_process(canonical);
        }
        if is_canonical_value(&rule_normalized) {
            return trend_post_process(&rule_normalized);
        }
    }

    trend_post_process(&rule_normalized)
}

/// Trend-specific post-processing: maintain historical key compatibility.
fn trend_post_process(canonical: &str) -> String {
    // HBV DNA: keep "乙肝病毒DNA" for backward-compatible trend keys
    if canonical == "乙肝病毒DNA定量" {
        return "乙肝病毒DNA".to_string();
    }
    canonical.to_string()
}

/// Check if a string is already a canonical value in the synonym dictionary.
static CANONICAL_VALUES: LazyLock<HashSet<String>> =
    LazyLock::new(|| SYNONYMS.values().cloned().collect());

fn is_canonical_value(s: &str) -> bool {
    CANONICAL_VALUES.contains(s)
}

/// Apply rule + dictionary normalization for a single name.
///
/// Returns `(normalized_name, resolved_by_dictionary)`.
fn rule_dict_normalize(name: &str, report_type: &str) -> (String, bool) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return (String::new(), false);
    }

    // Layer 2a: Synonym dictionary lookup on ORIGINAL name first.
    // This prevents rule-based stripping from losing clinically meaningful
    // prefixes (e.g. "超敏C反应蛋白" should NOT become "C反应蛋白").
    if let Some(canonical) = SYNONYMS.get(trimmed) {
        return (canonical.clone(), true);
    }
    // Check if the original name IS a canonical value in the dict
    if is_canonical_value(trimmed) {
        return (trimmed.to_string(), true);
    }

    // Layer 1: Rule-based normalization
    let rule_normalized = rule_normalize(trimmed, report_type);

    // Layer 2b: Synonym dictionary lookup on rule-normalized form
    if rule_normalized != trimmed {
        if let Some(canonical) = SYNONYMS.get(&rule_normalized) {
            return (canonical.clone(), true);
        }
    }

    // Check if the rule-normalized form IS a canonical value in the dict
    if is_canonical_value(&rule_normalized) {
        return (rule_normalized, true);
    }

    (rule_normalized, false)
}

// ---------------------------------------------------------------------------
// Layer 0: Unicode normalization
// ---------------------------------------------------------------------------

/// Normalize Unicode variations commonly seen in OCR output.
fn unicode_normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            // Fullwidth ASCII → halfwidth (Ａ→A, ａ→a, ０→0, etc.)
            '\u{FF01}'..='\u{FF5E}' => {
                out.push(char::from_u32(c as u32 - 0xFEE0).unwrap_or(c));
            }
            // Fullwidth space
            '\u{3000}' => out.push(' '),
            // Greek letter variants
            '\u{0251}' => out.push('α'), // ɑ → α
            '\u{00DF}' => out.push('β'), // ß → β
            // Zero-width characters (strip)
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{00AD}' => {}
            // Fullwidth parentheses → halfwidth (already handled by range above,
            // but these are outside the range)
            '【' => out.push('['),
            '】' => out.push(']'),
            '〔' => out.push('('),
            '〕' => out.push(')'),
            // En-dash / em-dash → hyphen for consistency
            '–' | '—' => out.push('-'),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Layer 1: Rule-based normalization
// ---------------------------------------------------------------------------

/// Apply rule-based normalization to a name, incorporating report type context.
fn rule_normalize(name: &str, report_type: &str) -> String {
    let mut n = name.trim().replace(char::is_whitespace, "");

    // 0. Unicode normalization: fullwidth → halfwidth, unify Greek, strip zero-width
    n = unicode_normalize(&n);

    // 1. Strip sensitivity/method prefixes
    for prefix in ["超高敏", "高敏", "超敏", "常规"] {
        if n.starts_with(prefix) && n.len() > prefix.len() {
            n = n[prefix.len()..].to_string();
            break;
        }
    }

    // 2. Strip trailing method suffixes in parentheses: "白蛋白（比色）" → "白蛋白"
    //    BUT preserve parenthesized content that is clinically meaningful,
    //    e.g. "脂蛋白(a)" → keep as-is, "白介素6(IL-6)" → keep.
    for (open, close) in [('（', '）'), ('(', ')')] {
        if let Some(pos) = n.find(open) {
            let before = &n[..pos];
            if before.is_empty() {
                continue;
            }
            // Extract content inside parentheses
            let after_open = &n[pos + open.len_utf8()..];
            let inner = if let Some(close_pos) = after_open.find(close) {
                &after_open[..close_pos]
            } else {
                after_open
            };
            // Preserve if inner content is a short identifier (e.g. "a", "b",
            // "IL-6") — these are part of the item name, not method suffixes.
            let is_meaningful_id = inner.chars().count() <= 4
                && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            if !is_meaningful_id {
                n = before.to_string();
            }
        }
    }

    // 3. Strip method suffixes: "定量", "测定", "检测", "检查"
    for suffix in ["浓度测定", "活性测定", "定量测定", "测定", "检测", "检查", "定量"] {
        if n.ends_with(suffix) {
            let trimmed = &n[..n.len() - suffix.len()];
            // Only strip if remaining name is ≥2 CJK characters
            let remaining_chars = trimmed.chars().count();
            if remaining_chars >= 2 {
                n = trimmed.to_string();
                break;
            }
        }
    }

    // 4. Normalize HBV DNA aliases
    let upper_compact = n
        .to_uppercase()
        .replace('-', "")
        .replace('_', "")
        .replace('—', "");
    if upper_compact.contains("HBV") && upper_compact.contains("DNA") {
        return "乙肝病毒DNA定量".to_string();
    }

    // 5. English abbreviations for hepatitis B markers
    let uc = upper_compact.as_str();
    if uc == "HBSAG" {
        return "乙肝表面抗原".to_string();
    }
    if uc == "ANTIHBS" || uc == "HBSAB" {
        return "乙肝表面抗体".to_string();
    }
    if uc == "HBEAG" {
        return "乙肝e抗原".to_string();
    }
    if uc == "ANTIHBE" || uc == "HBEAB" {
        return "乙肝e抗体".to_string();
    }
    if uc == "ANTIHBC" || uc == "HBCAB" {
        return "乙肝核心抗体".to_string();
    }

    // 6. Unify 乙型肝炎 → 乙肝
    n = n.replace("乙型肝炎病毒", "乙肝病毒");
    n = n.replace("乙型肝炎", "乙肝");
    if n.contains("乙肝病毒DNA") {
        return "乙肝病毒DNA定量".to_string();
    }

    // 7. Normalize e/E case for 乙肝 markers
    n = n.replace("乙肝E抗原", "乙肝e抗原");
    n = n.replace("乙肝E抗体", "乙肝e抗体");

    // 8. Strip body-fluid prefixes that duplicate the report category context.
    //    Within a "脑脊液" report, "脑脊液氯" and "氯" are the same item.
    let fluid_prefixes = ["脑脊液", "尿液", "粪便"];
    for prefix in fluid_prefixes {
        if report_type.contains(prefix) && n.starts_with(prefix) {
            let rest = &n[prefix.len()..];
            if !rest.is_empty() {
                n = rest.to_string();
            }
            break;
        }
    }

    // 9. Normalize common word variations
    // "数" / "总数" / "数目" → "计数" for cell count items
    let count_suffixes = [
        ("绝对计数", "计数"),
        ("绝对数", "计数"),
        ("绝对值", "计数"),
        ("数目", "计数"),
        ("总数", "计数"),
        ("记数", "计数"),
    ];
    for (from, to) in count_suffixes {
        if n.ends_with(from) {
            let base = &n[..n.len() - from.len()];
            if !base.is_empty() {
                n = format!("{}{}", base, to);
                break;
            }
        }
    }
    // Single "数" suffix for items like "白细胞数" → "白细胞计数"
    // But NOT for items like "中性粒细胞百分比数" etc. Only if preceded by 胞/板/蛋白/球
    if n.ends_with('数')
        && !n.ends_with("计数")
        && !n.ends_with("指数")
        && !n.ends_with("系数")
        && !n.ends_with("常数")
    {
        let base = &n[..n.len() - '数'.len_utf8()];
        if base.ends_with('胞') || base.ends_with("板") || base.ends_with("蛋白") {
            n = format!("{}计数", base);
        }
    }

    n
}

// ---------------------------------------------------------------------------
// Layer 3: Fuzzy matching
// ---------------------------------------------------------------------------

/// Try to fuzzy-match a name against existing canonical names.
/// Returns the best match if similarity > 0.85.
fn fuzzy_match(name: &str, existing: &[String]) -> Option<String> {
    let name_chars: Vec<char> = name.chars().collect();
    if name_chars.is_empty() {
        return None;
    }

    let mut best: Option<(String, f64)> = None;

    for candidate in existing {
        let cand_chars: Vec<char> = candidate.chars().collect();
        if cand_chars.is_empty() {
            continue;
        }

        let sim = jaccard_char_bigram_similarity(&name_chars, &cand_chars);

        // For Chinese names, if the first characters differ, require a higher
        // threshold to prevent false matches like "白细胞计数" ↔ "红细胞计数".
        let threshold = if !name_chars[0].is_ascii()
            && !cand_chars[0].is_ascii()
            && name_chars[0] != cand_chars[0]
        {
            0.95
        } else {
            0.85
        };

        if sim > threshold {
            if best.as_ref().map_or(true, |(_, bs)| sim > *bs) {
                best = Some((candidate.clone(), sim));
            }
        }
    }

    best.map(|(s, _)| s)
}

/// Jaccard similarity on character bigrams.
fn jaccard_char_bigram_similarity(a: &[char], b: &[char]) -> f64 {
    if a.len() < 2 || b.len() < 2 {
        // For very short strings, fall back to exact match
        return if a == b { 1.0 } else { 0.0 };
    }

    let bigrams_a: std::collections::HashSet<(char, char)> =
        a.windows(2).map(|w| (w[0], w[1])).collect();
    let bigrams_b: std::collections::HashSet<(char, char)> =
        b.windows(2).map(|w| (w[0], w[1])).collect();

    let intersection = bigrams_a.intersection(&bigrams_b).count();
    let union = bigrams_a.union(&bigrams_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Convenience: convert normalize_batch results into a simple name→canonical HashMap,
/// separating resolved names from unresolved ones.
#[allow(dead_code)]
pub fn split_results(
    results: &HashMap<String, NormalizeResult>,
) -> (HashMap<String, String>, Vec<String>) {
    let mut resolved: HashMap<String, String> = HashMap::new();
    let mut unresolved: Vec<String> = Vec::new();

    for (name, result) in results {
        match result.method {
            NormalizeMethod::Unresolved => {
                unresolved.push(name.clone());
            }
            _ => {
                resolved.insert(name.clone(), result.canonical.clone());
            }
        }
    }

    (resolved, unresolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_lookup_abbreviation() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "血常规".to_string(),
            vec!["WBC".to_string(), "RBC".to_string()],
        );
        let results = normalize_batch(&by_type, &[]);
        assert_eq!(results["WBC"].canonical, "白细胞计数");
        assert_eq!(results["WBC"].method, NormalizeMethod::Dictionary);
        assert_eq!(results["RBC"].canonical, "红细胞计数");
    }

    #[test]
    fn dict_lookup_old_name() {
        let mut by_type = HashMap::new();
        by_type.insert("肝功能".to_string(), vec!["谷丙转氨酶".to_string()]);
        let results = normalize_batch(&by_type, &[]);
        assert_eq!(results["谷丙转氨酶"].canonical, "丙氨酸氨基转移酶");
    }

    #[test]
    fn sensitivity_prefix_preserved_for_hs_crp() {
        let mut by_type = HashMap::new();
        by_type.insert("感染".to_string(), vec!["超敏C反应蛋白".to_string()]);
        let results = normalize_batch(&by_type, &[]);
        // "超敏C反应蛋白" is a distinct clinical test from "C反应蛋白" — must be preserved
        assert_eq!(results["超敏C反应蛋白"].canonical, "超敏C反应蛋白");
        assert_eq!(results["超敏C反应蛋白"].method, NormalizeMethod::Dictionary);
    }

    #[test]
    fn sensitivity_prefix_stripped_for_non_dict_items() {
        // For items NOT in the dictionary, sensitivity prefix should still be stripped
        let mut by_type = HashMap::new();
        by_type.insert("检验".to_string(), vec!["高敏某特殊蛋白".to_string()]);
        let results = normalize_batch(&by_type, &[]);
        assert_eq!(results["高敏某特殊蛋白"].canonical, "某特殊蛋白");
    }

    #[test]
    fn rule_strips_method_suffix() {
        let mut by_type = HashMap::new();
        by_type.insert("肝功能".to_string(), vec!["白蛋白（比色）".to_string()]);
        let results = normalize_batch(&by_type, &[]);
        assert_eq!(results["白蛋白（比色）"].canonical, "白蛋白");
    }

    #[test]
    fn rule_strips_dingliang() {
        let mut by_type = HashMap::new();
        by_type.insert("乙肝".to_string(), vec!["乙肝表面抗原定量".to_string()]);
        let results = normalize_batch(&by_type, &[]);
        assert_eq!(results["乙肝表面抗原定量"].canonical, "乙肝表面抗原");
    }

    #[test]
    fn hbv_dna_variants() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "乙肝".to_string(),
            vec![
                "HBV-DNA".to_string(),
                "高敏HBV-DNA定量".to_string(),
                "乙型肝炎病毒DNA".to_string(),
            ],
        );
        let results = normalize_batch(&by_type, &[]);
        assert_eq!(results["HBV-DNA"].canonical, "乙肝病毒DNA定量");
        assert_eq!(results["高敏HBV-DNA定量"].canonical, "乙肝病毒DNA定量");
        assert_eq!(results["乙型肝炎病毒DNA"].canonical, "乙肝病毒DNA定量");
    }

    #[test]
    fn fluid_prefix_context() {
        let mut by_type = HashMap::new();
        by_type.insert("脑脊液常规".to_string(), vec!["脑脊液氯".to_string()]);
        let results = normalize_batch(&by_type, &[]);
        // In 脑脊液 report context, "脑脊液氯" → "氯"
        assert_eq!(results["脑脊液氯"].canonical, "氯");
    }

    #[test]
    fn count_suffix_normalization() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "血常规".to_string(),
            vec![
                "白细胞数".to_string(),
                "白细胞总数".to_string(),
                "中性粒细胞绝对值".to_string(),
            ],
        );
        let results = normalize_batch(&by_type, &[]);
        assert_eq!(results["白细胞数"].canonical, "白细胞计数");
        assert_eq!(results["白细胞总数"].canonical, "白细胞计数");
        assert_eq!(results["中性粒细胞绝对值"].canonical, "中性粒细胞计数");
    }

    #[test]
    fn fuzzy_match_existing() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "血常规".to_string(),
            vec!["平均红细胞血红蛋白量".to_string()],
        );
        let existing = vec!["平均红细胞血红蛋白含量".to_string()];
        let results = normalize_batch(&by_type, &existing);
        // Should fuzzy-match or dict-match
        let r = &results["平均红细胞血红蛋白量"];
        assert!(r.canonical == "平均红细胞血红蛋白含量" || r.method == NormalizeMethod::Dictionary);
    }

    #[test]
    fn split_results_separates() {
        let mut by_type = HashMap::new();
        by_type.insert(
            "血常规".to_string(),
            vec!["WBC".to_string(), "某种罕见检查项目XYZ".to_string()],
        );
        let results = normalize_batch(&by_type, &[]);
        let (resolved, unresolved) = split_results(&results);
        assert!(resolved.contains_key("WBC"));
        assert!(unresolved.contains(&"某种罕见检查项目XYZ".to_string()));
    }

    #[test]
    fn normalize_for_scoring_uses_rule_and_dict() {
        let r1 = normalize_for_scoring("白蛋白（比色）", "肝功能");
        assert_eq!(r1, "白蛋白");

        let r2 = normalize_for_scoring("WBC", "血常规");
        assert_eq!(r2, "白细胞计数");
    }

    #[test]
    fn normalize_for_trend_keeps_hbv_dna_compatibility() {
        let r = normalize_for_trend("HBV-DNA");
        assert_eq!(r, "乙肝病毒DNA");
    }

    #[test]
    fn normalize_for_trend_strips_fluid_prefix() {
        let r = normalize_for_trend("脑脊液氯");
        assert_eq!(r, "氯");
    }

    #[test]
    fn normalize_for_trend_uses_dictionary() {
        // Previously normalize_for_trend only used rule_normalize, missing
        // dictionary mappings. Now it should resolve abbreviations.
        assert_eq!(normalize_for_trend("WBC"), "白细胞计数");
        assert_eq!(normalize_for_trend("RBC"), "红细胞计数");
        assert_eq!(normalize_for_trend("谷丙转氨酶"), "丙氨酸氨基转移酶");
        assert_eq!(normalize_for_trend("谷草转氨酶"), "天门冬氨酸氨基转移酶");
        assert_eq!(normalize_for_trend("hs-CRP"), "超敏C反应蛋白");
        assert_eq!(normalize_for_trend("CRP"), "C反应蛋白");
    }

    #[test]
    fn normalize_for_trend_preserves_sensitivity_prefix() {
        // "超敏C反应蛋白" and "C反应蛋白" are clinically distinct — must NOT merge
        assert_eq!(normalize_for_trend("超敏C反应蛋白"), "超敏C反应蛋白");
        assert_eq!(normalize_for_trend("C反应蛋白"), "C反应蛋白");
        assert_ne!(
            normalize_for_trend("超敏C反应蛋白"),
            normalize_for_trend("C反应蛋白")
        );
        // Same for troponin
        assert_eq!(normalize_for_trend("高敏心肌肌钙蛋白I"), "高敏心肌肌钙蛋白I");
        assert_eq!(normalize_for_trend("心肌肌钙蛋白I"), "心肌肌钙蛋白I");
        assert_ne!(
            normalize_for_trend("高敏心肌肌钙蛋白I"),
            normalize_for_trend("心肌肌钙蛋白I")
        );
    }

    #[test]
    fn normalize_for_trend_empty_and_whitespace() {
        assert_eq!(normalize_for_trend(""), "");
        assert_eq!(normalize_for_trend("  "), "");
    }

    #[test]
    fn normalize_for_trend_canonical_passthrough() {
        // Already-canonical names should pass through unchanged
        assert_eq!(normalize_for_trend("白细胞计数"), "白细胞计数");
        assert_eq!(normalize_for_trend("丙氨酸氨基转移酶"), "丙氨酸氨基转移酶");
        assert_eq!(normalize_for_trend("甘油三酯"), "甘油三酯");
    }

    #[test]
    fn normalize_for_trend_sensitivity_unification() {
        // All hs-CRP variants should unify to "超敏C反应蛋白"
        assert_eq!(normalize_for_trend("高敏C反应蛋白"), "超敏C反应蛋白");
        assert_eq!(normalize_for_trend("超高敏C反应蛋白"), "超敏C反应蛋白");
        assert_eq!(normalize_for_trend("hsCRP"), "超敏C反应蛋白");
        assert_eq!(normalize_for_trend("常规C反应蛋白"), "C反应蛋白");
    }

    #[test]
    fn normalize_for_trend_ocr_typo_jishu() {
        // "记数" is a common OCR misread of "计数"
        assert_eq!(normalize_for_trend("白细胞记数"), "白细胞计数");
        assert_eq!(normalize_for_trend("淋巴细胞记数"), "淋巴细胞计数");
        assert_eq!(normalize_for_trend("血小板记数"), "血小板计数");
    }

    #[test]
    fn normalize_for_trend_new_aliases() {
        assert_eq!(normalize_for_trend("GLOB"), "球蛋白");
        assert_eq!(normalize_for_trend("AKP"), "碱性磷酸酶");
        assert_eq!(normalize_for_trend("谷氨酰转移酶"), "γ-谷氨酰转移酶");
        assert_eq!(normalize_for_trend("三酰甘油酯"), "甘油三酯");
    }
}
