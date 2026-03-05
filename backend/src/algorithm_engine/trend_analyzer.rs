use chrono::NaiveDate;
use serde_json::json;

use crate::models::{ItemStatus, TrendPoint};

/// 解析检验值字符串为浮点数。
/// 处理以下格式：
/// - 普通数字："12.5" → 12.5
/// - 比较符：">100" "<0.5" "≥10" "≤20" → 去掉前缀解析
/// - 范围："10-20" → 取中值 15.0
/// - 非数值（"阴性"、"+++" 等）→ None
pub fn parse_value_string(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // 比较符前缀
    for prefix in &[">", "<", ">=", "<=", "≥", "≤", "＞", "＜"] {
        if s.starts_with(prefix) {
            let rest = s[prefix.len()..].trim();
            return rest.parse::<f64>().ok();
        }
    }

    // 范围值 "10-20"（注意负数 "-0.5" 不应当被当作范围处理）
    // 只有当连字符不在开头时才尝试范围解析
    if let Some(pos) = s.chars().skip(1).collect::<String>().find('-') {
        let pos = pos + s.chars().next().map_or(0, |c| c.len_utf8()); // 跳过首字符偏移
        let low_str = s[..pos].trim();
        let high_str = s[pos + 1..].trim();
        if let (Ok(low), Ok(high)) = (low_str.parse::<f64>(), high_str.parse::<f64>()) {
            if low <= high {
                return Some((low + high) / 2.0);
            }
        }
    }

    // 直接解析
    s.parse::<f64>().ok()
}

/// 将日期字符串转换为相对第一个日期的天数。
/// 支持 "YYYY-MM-DD" 格式。
fn date_to_days(date_str: &str, base: NaiveDate) -> Option<i64> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .map(|d| (d - base).num_days())
}

/// 从 TrendPoint 中提取有效日期（sample_date 优先，否则用 report_date）。
fn effective_date(p: &TrendPoint) -> &str {
    if p.sample_date.is_empty() {
        &p.report_date
    } else {
        &p.sample_date
    }
}

/// 使用最小二乘法计算线性斜率（单位：值/天）。
/// 如果点数 < 2 或分母接近0，返回 0.0。
pub fn calculate_slope(days: &[i64], values: &[f64]) -> f64 {
    let n = days.len();
    if n < 2 {
        return 0.0;
    }

    let n_f = n as f64;
    let sum_x: f64 = days.iter().map(|&x| x as f64).sum();
    let sum_y: f64 = values.iter().sum();
    let sum_xy: f64 = days
        .iter()
        .zip(values.iter())
        .map(|(&x, &y)| x as f64 * y)
        .sum();
    let sum_x2: f64 = days.iter().map(|&x| (x as f64).powi(2)).sum();

    let denom = n_f * sum_x2 - sum_x.powi(2);
    if denom.abs() < 1e-10 {
        return 0.0;
    }
    (n_f * sum_xy - sum_x * sum_y) / denom
}

/// 判断趋势方向。
/// threshold 为相对上升/下降阈值（占最新值绝对值的比例），默认 1%/天。
pub fn classify_trend_direction(slope: f64, last_value: f64) -> &'static str {
    let threshold = last_value.abs() * 0.01;
    if slope.abs() < threshold {
        "stable"
    } else if slope > 0.0 {
        "rising"
    } else {
        "falling"
    }
}

/// 对一组 TrendPoint 进行趋势分析，返回 JSON 结果。
///
/// 返回格式：
/// ```json
/// {
///   "direction": "rising|falling|stable|insufficient",
///   "slope": 0.12,        // null 时表示数据不足
///   "point_count": 5
/// }
/// ```
pub fn analyze_item_trends(points: &[TrendPoint]) -> serde_json::Value {
    // 提取可解析的点
    let parsed: Vec<(i64, f64)> = {
        if points.is_empty() {
            return json!({"direction": "insufficient", "slope": null, "point_count": 0});
        }

        // 找基准日期
        let base_str = effective_date(&points[0]);
        let base = match NaiveDate::parse_from_str(base_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => {
                return json!({"direction": "insufficient", "slope": null, "point_count": 0});
            }
        };

        points
            .iter()
            .filter_map(|p| {
                let days = date_to_days(effective_date(p), base)?;
                let val = parse_value_string(&p.value)?;
                Some((days, val))
            })
            .collect()
    };

    let count = parsed.len();
    if count < 3 {
        return json!({
            "direction": "insufficient",
            "slope": null,
            "point_count": count
        });
    }

    let days: Vec<i64> = parsed.iter().map(|(d, _)| *d).collect();
    let values: Vec<f64> = parsed.iter().map(|(_, v)| *v).collect();
    let slope = calculate_slope(&days, &values);
    let last_value = *values.last().unwrap();
    let direction = classify_trend_direction(slope, last_value);

    json!({
        "direction": direction,
        "slope": (slope * 1000.0).round() / 1000.0,  // 保留3位小数
        "point_count": count
    })
}

/// 根据患者所有指标的状态和趋势计算总体风险级别。
///
/// 输入：(指标名, 状态, 趋势方向) 列表
/// 返回："low" | "medium" | "high"
pub fn compute_patient_risk_level(items: &[(String, ItemStatus, String)]) -> String {
    let mut has_critical = false;
    let mut abnormal_worsening = 0usize;
    let mut abnormal_count = 0usize;

    for (_, status, trend) in items {
        match status {
            ItemStatus::CriticalHigh | ItemStatus::CriticalLow => {
                has_critical = true;
            }
            ItemStatus::High | ItemStatus::Low => {
                abnormal_count += 1;
                if trend == "rising" || trend == "falling" {
                    abnormal_worsening += 1;
                }
            }
            ItemStatus::Normal => {}
        }
    }

    if has_critical {
        return "high".to_string();
    }
    if abnormal_count >= 3 && abnormal_worsening >= 1 {
        return "high".to_string();
    }
    if abnormal_count >= 1 {
        return "medium".to_string();
    }
    "low".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ItemStatus;

    #[test]
    fn parse_plain_number() {
        assert_eq!(parse_value_string("12.5"), Some(12.5));
        assert_eq!(parse_value_string("0"), Some(0.0));
        assert_eq!(parse_value_string("-1.2"), Some(-1.2));
    }

    #[test]
    fn parse_comparator_prefix() {
        assert_eq!(parse_value_string(">100"), Some(100.0));
        assert_eq!(parse_value_string("<0.5"), Some(0.5));
        assert_eq!(parse_value_string("≥10"), Some(10.0));
        assert_eq!(parse_value_string("≤20"), Some(20.0));
    }

    #[test]
    fn parse_range_midpoint() {
        assert_eq!(parse_value_string("10-20"), Some(15.0));
    }

    #[test]
    fn parse_non_numeric_returns_none() {
        assert_eq!(parse_value_string("阴性"), None);
        assert_eq!(parse_value_string("+++"), None);
        assert_eq!(parse_value_string(""), None);
    }

    #[test]
    fn slope_rising() {
        let days = vec![0, 7, 14, 21];
        let values = vec![10.0, 12.0, 14.0, 16.0];
        let s = calculate_slope(&days, &values);
        assert!(s > 0.0, "斜率应为正数，实际: {}", s);
    }

    #[test]
    fn slope_flat() {
        let days = vec![0, 7, 14];
        let values = vec![10.0, 10.0, 10.0];
        let s = calculate_slope(&days, &values);
        assert!(s.abs() < 1e-9, "斜率应接近0，实际: {}", s);
    }

    #[test]
    fn trend_direction_stable() {
        assert_eq!(classify_trend_direction(0.05, 100.0), "stable");
    }

    #[test]
    fn trend_direction_rising() {
        assert_eq!(classify_trend_direction(2.0, 100.0), "rising");
    }

    #[test]
    fn trend_direction_falling() {
        assert_eq!(classify_trend_direction(-2.0, 100.0), "falling");
    }

    #[test]
    fn risk_level_critical() {
        let items = vec![
            ("WBC".to_string(), ItemStatus::CriticalHigh, "rising".to_string()),
        ];
        assert_eq!(compute_patient_risk_level(&items), "high");
    }

    #[test]
    fn risk_level_medium() {
        let items = vec![
            ("WBC".to_string(), ItemStatus::High, "stable".to_string()),
        ];
        assert_eq!(compute_patient_risk_level(&items), "medium");
    }

    #[test]
    fn risk_level_low() {
        let items: Vec<(String, ItemStatus, String)> = vec![
            ("WBC".to_string(), ItemStatus::Normal, "stable".to_string()),
        ];
        assert_eq!(compute_patient_risk_level(&items), "low");
    }

    #[test]
    fn analyze_insufficient_data() {
        let points = vec![TrendPoint {
            report_date: "2024-01-01".to_string(),
            sample_date: "".to_string(),
            value: "10.0".to_string(),
            unit: "".to_string(),
            status: ItemStatus::Normal,
            reference_range: "".to_string(),
        }];
        let result = analyze_item_trends(&points);
        assert_eq!(result["direction"], "insufficient");
    }
}
