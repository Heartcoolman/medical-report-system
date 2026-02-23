use regex::Regex;
use std::sync::LazyLock;

use super::{ParsedItem, ParsedReport};
use crate::models::ItemStatus;

// Precompiled regexes
static RE_HOSPITAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([\u4e00-\u9fa5]{2,20}(?:医院|卫生院|诊所|医疗中心|检验中心))")
        .expect("无法编译医院正则")
});

/// Matches dates like "2024-01-15", "2024/01/15", "2024年1月15日"
static RE_DATE_1: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{4}[-/年]\d{1,2}[-/月]\d{1,2})[日号]?").expect("无法编译日期正则1")
});

/// Matches dates like "2024.01.15"
static RE_DATE_2: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d{4}\.\d{1,2}\.\d{1,2})").expect("无法编译日期正则2"));

/// Matches a keyword-date pair like "检查日期：2024-01-15" or "报告日期 2024年1月15日"
static RE_LABELED_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"([检查采样送检报告审核打印发布]{2}[日期时间]*)[\s：:]*?(\d{4}[-/年.]\d{1,2}[-/月.]\d{1,2})[日号]?"
    ).expect("无法编译标签日期正则")
});

static RE_ITEM_FULL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"([\u4e00-\u9fa5a-zA-Z][\u4e00-\u9fa5a-zA-Z\s]{1,20})\s+([\d.]+)\s*([a-zA-Z%/\^0-9μ]*[a-zA-Z%/\^μ][a-zA-Z%/\^0-9μ]*)\s+([\d.]+[-~\u{ff5e}][\d.]+)"
    ).expect("无法编译检查项正则")
});

static RE_ITEM_FALLBACK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([\u4e00-\u9fa5]{2,10})\s+([\d.]+)\s*([^\s]*)\s*(?:[↑↓HL]|偏[高低])?")
        .expect("无法编译检查项回退正则")
});

pub fn parse_report_text(text: &str) -> ParsedReport {
    let report_type = detect_report_type(text);
    let hospital = extract_hospital(text);
    let (sample_date, report_date) = extract_dates(text);
    let items = extract_items(text);

    ParsedReport {
        report_type,
        hospital,
        report_date,
        sample_date,
        items,
    }
}

fn detect_report_type(text: &str) -> String {
    let types = [
        ("血常规", &["血常规", "血细胞分析", "CBC"][..]),
        ("肝功能", &["肝功能", "肝功", "ALT", "AST", "转氨酶"]),
        ("肾功能", &["肾功能", "肾功", "肌酐", "尿素氮", "BUN"]),
        ("血脂", &["血脂", "胆固醇", "甘油三酯", "HDL", "LDL"]),
        ("血糖", &["血糖", "葡萄糖", "糖化血红蛋白", "HbA1c"]),
        ("尿常规", &["尿常规", "尿液分析"]),
        ("甲状腺功能", &["甲状腺", "TSH", "T3", "T4", "甲功"]),
    ];
    for (name, keywords) in &types {
        for kw in *keywords {
            if text.contains(kw) {
                return name.to_string();
            }
        }
    }
    "检验报告".to_string()
}

fn extract_hospital(text: &str) -> String {
    RE_HOSPITAL
        .find(text)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn normalize_date_str(raw: &str) -> String {
    raw.replace('年', "-")
        .replace('月', "-")
        .replace('/', "-")
        .replace('.', "-")
}

const SAMPLE_KEYWORDS: &[&str] = &["检查", "采样", "送检"];
const REPORT_KEYWORDS: &[&str] = &["报告", "审核", "打印", "发布"];

/// Extract (sample_date, report_date) from text.
/// Uses keyword context to classify; falls back to first date for both if ambiguous.
fn extract_dates(text: &str) -> (String, String) {
    let mut sample_date = String::new();
    let mut report_date = String::new();

    // First pass: find keyword-labeled dates
    for caps in RE_LABELED_DATE.captures_iter(text) {
        let label = &caps[1];
        let date = normalize_date_str(&caps[2]);
        if sample_date.is_empty() && SAMPLE_KEYWORDS.iter().any(|kw| label.contains(kw)) {
            sample_date = date;
        } else if report_date.is_empty() && REPORT_KEYWORDS.iter().any(|kw| label.contains(kw)) {
            report_date = date;
        }
        if !sample_date.is_empty() && !report_date.is_empty() {
            break;
        }
    }

    // Fallback: use first unlabeled date for any missing field
    if sample_date.is_empty() || report_date.is_empty() {
        let fallback = extract_first_date(text);
        if sample_date.is_empty() {
            sample_date = fallback.clone();
        }
        if report_date.is_empty() {
            report_date = fallback;
        }
    }

    (sample_date, report_date)
}

fn extract_first_date(text: &str) -> String {
    for re in [&*RE_DATE_1, &*RE_DATE_2] {
        if let Some(caps) = re.captures(text) {
            let date_str = caps.get(1).expect("日期正则应有捕获组1").as_str();
            return normalize_date_str(date_str);
        }
    }
    String::new()
}

fn extract_items(text: &str) -> Vec<ParsedItem> {
    let mut items = Vec::new();

    for caps in RE_ITEM_FULL.captures_iter(text) {
        let name = caps[1].trim().to_string();
        let value: f64 = match caps[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let unit = caps[3].to_string();
        let range_str = caps[4].to_string();
        let status = determine_status(value, &range_str);

        items.push(ParsedItem {
            name,
            value: caps[2].to_string(),
            unit,
            reference_range: range_str,
            status: status.to_string(),
        });
    }

    // Fallback pattern
    if items.is_empty() {
        for caps in RE_ITEM_FALLBACK.captures_iter(text) {
            let name = caps[1].trim().to_string();
            let value_str = caps[2].to_string();
            let unit = caps
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            items.push(ParsedItem {
                name,
                value: value_str,
                unit,
                reference_range: String::new(),
                status: ItemStatus::Normal.to_string(),
            });
        }
    }

    items
}

static RE_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    // Support both single-dash (3.5-9.5) and double-dash (0--0.06) separators,
    // as well as tilde variants (~, ～) and em-dash (—). Double-dash must be
    // tried first so that the second group is not mistakenly parsed as negative.
    Regex::new(r"(-?\d+\.?\d*)\s*(?:--|[—\-~～])\s*(-?\d+\.?\d*)").expect("无法编译参考范围正则")
});

/// Matches upper-bound-only ranges like "<34", "＜1.3", "≤5"
static RE_UPPER_BOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([<＜≤])\s*(\d+\.?\d*)$").expect("无法编译上限正则"));

/// Matches lower-bound-only ranges like ">0.5", "＞1", "≥60"
static RE_LOWER_BOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([>＞≥])\s*(\d+\.?\d*)$").expect("无法编译下限正则"));

pub fn determine_status(value: f64, range: &str) -> ItemStatus {
    let range = range.trim();

    // Upper-bound only: <X (strict) or ≤X (inclusive)
    if let Some(caps) = RE_UPPER_BOUND.captures(range) {
        if let Ok(threshold) = caps[2].parse::<f64>() {
            let is_high = if &caps[1] == "≤" {
                value > threshold
            } else {
                value >= threshold
            };
            return if is_high {
                ItemStatus::High
            } else {
                ItemStatus::Normal
            };
        }
    }

    // Lower-bound only: >X (strict) or ≥X (inclusive)
    if let Some(caps) = RE_LOWER_BOUND.captures(range) {
        if let Ok(threshold) = caps[2].parse::<f64>() {
            let is_low = if &caps[1] == "≥" {
                value < threshold
            } else {
                value <= threshold
            };
            return if is_low {
                ItemStatus::Low
            } else {
                ItemStatus::Normal
            };
        }
    }

    // Range: low-high
    if let Some(caps) = RE_RANGE.captures(range) {
        if let (Ok(low), Ok(high)) = (caps[1].parse::<f64>(), caps[2].parse::<f64>()) {
            if value < low {
                return ItemStatus::Low;
            } else if value > high {
                return ItemStatus::High;
            }
            return ItemStatus::Normal;
        }
    }

    ItemStatus::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_range() {
        assert_eq!(determine_status(5.0, "3.5-7.5"), ItemStatus::Normal);
        assert_eq!(determine_status(3.5, "3.5-7.5"), ItemStatus::Normal);
        assert_eq!(determine_status(7.5, "3.5-7.5"), ItemStatus::Normal);
    }

    #[test]
    fn test_high_value() {
        assert_eq!(determine_status(8.0, "3.5-7.5"), ItemStatus::High);
        assert_eq!(determine_status(100.0, "0-10"), ItemStatus::High);
    }

    #[test]
    fn test_low_value() {
        assert_eq!(determine_status(2.0, "3.5-7.5"), ItemStatus::Low);
        assert_eq!(determine_status(0.0, "1-10"), ItemStatus::Low);
    }

    #[test]
    fn test_negative_lower_bound() {
        // e.g. reference range "-0.5~1.5"
        assert_eq!(determine_status(0.5, "-0.5~1.5"), ItemStatus::Normal);
        assert_eq!(determine_status(-0.3, "-0.5~1.5"), ItemStatus::Normal);
        assert_eq!(determine_status(-0.5, "-0.5~1.5"), ItemStatus::Normal);
        assert_eq!(determine_status(-1.0, "-0.5~1.5"), ItemStatus::Low);
        assert_eq!(determine_status(2.0, "-0.5~1.5"), ItemStatus::High);
    }

    #[test]
    fn test_negative_both_bounds() {
        assert_eq!(determine_status(-3.0, "-5.0~-1.0"), ItemStatus::Normal);
        assert_eq!(determine_status(-6.0, "-5.0~-1.0"), ItemStatus::Low);
        assert_eq!(determine_status(0.0, "-5.0~-1.0"), ItemStatus::High);
    }

    #[test]
    fn test_tilde_separators() {
        assert_eq!(determine_status(5.0, "3.5～7.5"), ItemStatus::Normal);
        assert_eq!(determine_status(5.0, "3.5~7.5"), ItemStatus::Normal);
        assert_eq!(determine_status(5.0, "3.5-7.5"), ItemStatus::Normal);
    }

    #[test]
    fn test_double_dash_separator() {
        // Hospital reports often use "--" as range separator
        assert_eq!(determine_status(0.02, "0--0.06"), ItemStatus::Normal);
        assert_eq!(determine_status(0.07, "0--0.06"), ItemStatus::High);
        assert_eq!(determine_status(126.0, "115--150"), ItemStatus::Normal);
        assert_eq!(determine_status(93.4, "82--100"), ItemStatus::Normal);
        assert_eq!(determine_status(30.42, "0--10"), ItemStatus::High);
        assert_eq!(determine_status(7.18, "1.8--6.3"), ItemStatus::High);
        assert_eq!(determine_status(0.01, "0.02--0.52"), ItemStatus::Low);
    }

    #[test]
    fn test_invalid_format() {
        assert_eq!(determine_status(5.0, "abc"), ItemStatus::Normal);
        assert_eq!(determine_status(5.0, ""), ItemStatus::Normal);
        assert_eq!(determine_status(5.0, "阴性"), ItemStatus::Normal);
    }

    #[test]
    fn test_upper_bound_only() {
        // <X: value >= X is high
        assert_eq!(determine_status(185.0, "<34"), ItemStatus::High);
        assert_eq!(determine_status(33.9, "<34"), ItemStatus::Normal);
        assert_eq!(determine_status(34.0, "<34"), ItemStatus::High); // exact boundary: abnormal
        assert_eq!(determine_status(2.23, "<1.3"), ItemStatus::High);
        assert_eq!(determine_status(1.29, "<1.3"), ItemStatus::Normal);
        assert_eq!(determine_status(20.8, "<5"), ItemStatus::High);
        // Full-width <
        assert_eq!(determine_status(10.0, "＜5"), ItemStatus::High);
        // ≤X: value > X is high (boundary is normal)
        assert_eq!(determine_status(5.1, "≤5"), ItemStatus::High);
        assert_eq!(determine_status(5.0, "≤5"), ItemStatus::Normal); // boundary inclusive
        assert_eq!(determine_status(4.9, "≤5"), ItemStatus::Normal);
    }

    #[test]
    fn test_lower_bound_only() {
        assert_eq!(determine_status(55.0, ">60"), ItemStatus::Low);
        assert_eq!(determine_status(61.0, ">60"), ItemStatus::Normal);
        assert_eq!(determine_status(60.0, "≥60"), ItemStatus::Normal);
        assert_eq!(determine_status(59.0, "≥60"), ItemStatus::Low);
    }
}
