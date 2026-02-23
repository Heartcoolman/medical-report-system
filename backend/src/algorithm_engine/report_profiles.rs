use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Deserialize)]
struct ProfilesData {
    profiles: Vec<ProfileEntry>,
}

#[derive(Deserialize)]
struct ProfileEntry {
    category: String,
    signature_items: Vec<String>,
    common_items: Vec<String>,
}

/// Result of matching items against a report profile.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProfileMatch {
    pub category: String,
    /// Fraction of signature items matched (0.0–1.0).
    pub signature_hit_ratio: f32,
    /// Fraction of common items matched (0.0–1.0).
    pub common_hit_ratio: f32,
    /// Combined confidence score (0.0–1.0).
    pub confidence: f32,
}

/// Result of validating an OCR-reported type against item content.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TypeValidation {
    /// OCR type is consistent with items.
    Consistent,
    /// OCR type contradicts items — inferred category differs.
    Contradicted {
        inferred_category: String,
        confidence: f32,
    },
    /// Not enough item information to validate.
    Insufficient,
}

struct Profile {
    category: String,
    signature_items: HashSet<String>,
    common_items: HashSet<String>,
}

struct ProfileStore {
    profiles: Vec<Profile>,
    /// category name → index in `profiles`
    #[allow(dead_code)]
    category_index: HashMap<String, usize>,
}

static PROFILES: LazyLock<ProfileStore> = LazyLock::new(|| {
    let json_bytes = include_bytes!("dictionaries/report_profiles.json");
    let data: ProfilesData =
        serde_json::from_slice(json_bytes).expect("无法解析 report_profiles.json");

    let mut profiles = Vec::with_capacity(data.profiles.len());
    let mut category_index = HashMap::new();

    for (i, entry) in data.profiles.into_iter().enumerate() {
        category_index.insert(entry.category.clone(), i);
        profiles.push(Profile {
            category: entry.category,
            signature_items: entry.signature_items.into_iter().collect(),
            common_items: entry.common_items.into_iter().collect(),
        });
    }

    ProfileStore {
        profiles,
        category_index,
    }
});

/// Match a list of (normalized) item names against all known profiles.
/// Returns matches sorted by confidence (highest first).
pub fn infer_category(item_names: &[String]) -> Vec<ProfileMatch> {
    if item_names.is_empty() {
        return Vec::new();
    }

    let item_set: HashSet<&str> = item_names.iter().map(|s| s.as_str()).collect();
    let mut matches: Vec<ProfileMatch> = Vec::new();

    for profile in &PROFILES.profiles {
        let sig_total = profile.signature_items.len();
        let sig_hits = if sig_total > 0 {
            profile
                .signature_items
                .iter()
                .filter(|s| item_set.contains(s.as_str()))
                .count()
        } else {
            0
        };

        let common_total = profile.common_items.len();
        let common_hits = if common_total > 0 {
            profile
                .common_items
                .iter()
                .filter(|s| item_set.contains(s.as_str()))
                .count()
        } else {
            0
        };

        let sig_ratio = if sig_total > 0 {
            sig_hits as f32 / sig_total as f32
        } else {
            0.0
        };
        let common_ratio = if common_total > 0 {
            common_hits as f32 / common_total as f32
        } else {
            0.0
        };

        // Require at least one signature hit to consider this profile.
        if sig_hits == 0 {
            continue;
        }

        // Confidence: signature match is worth 70%, common match 30%.
        let confidence = sig_ratio * 0.7 + common_ratio * 0.3;

        matches.push(ProfileMatch {
            category: profile.category.clone(),
            signature_hit_ratio: sig_ratio,
            common_hit_ratio: common_ratio,
            confidence,
        });
    }

    matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    matches
}

/// Validate whether the OCR-reported type is consistent with the item content.
///
/// Uses `report_taxonomy::lookup_category_pub` to find the OCR type's category,
/// then checks if the top inferred category matches.
#[allow(dead_code)]
pub fn validate_type(ocr_type: &str, item_names: &[String]) -> TypeValidation {
    if item_names.len() < 2 {
        return TypeValidation::Insufficient;
    }

    let inferred = infer_category(item_names);
    if inferred.is_empty() {
        return TypeValidation::Insufficient;
    }

    let top = &inferred[0];
    // Need reasonable confidence to make a judgment.
    if top.confidence < 0.4 {
        return TypeValidation::Insufficient;
    }

    // Find the OCR type's category via taxonomy.
    let ocr_category = super::report_taxonomy::lookup_category_pub(ocr_type);

    match ocr_category {
        Some(cat) if cat == top.category => TypeValidation::Consistent,
        Some(_) => {
            // OCR category differs from inferred — only flag contradiction
            // if inferred confidence is strong enough.
            if top.confidence >= 0.5 {
                TypeValidation::Contradicted {
                    inferred_category: top.category.clone(),
                    confidence: top.confidence,
                }
            } else {
                TypeValidation::Consistent // Not confident enough to contradict
            }
        }
        None => {
            // OCR type not in taxonomy — can't validate, but we have inference.
            if top.confidence >= 0.6 {
                TypeValidation::Contradicted {
                    inferred_category: top.category.clone(),
                    confidence: top.confidence,
                }
            } else {
                TypeValidation::Insufficient
            }
        }
    }
}

/// Check if two sets of items belong to the same profile category.
/// Returns (same_profile, confidence) where confidence is the minimum of both matches.
pub fn same_profile(items_a: &[String], items_b: &[String]) -> (bool, f32) {
    let inferred_a = infer_category(items_a);
    let inferred_b = infer_category(items_b);

    match (inferred_a.first(), inferred_b.first()) {
        (Some(a), Some(b)) => {
            let min_conf = a.confidence.min(b.confidence);
            if a.category == b.category && min_conf >= 0.4 {
                (true, min_conf)
            } else if a.category != b.category && a.confidence >= 0.5 && b.confidence >= 0.5 {
                (false, a.confidence.min(b.confidence))
            } else {
                // Not enough confidence either way
                (false, 0.0)
            }
        }
        _ => (false, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_blood_routine() {
        let items: Vec<String> = vec![
            "白细胞计数", "红细胞计数", "血红蛋白", "血小板计数",
            "中性粒细胞计数", "淋巴细胞计数",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let matches = infer_category(&items);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].category, "血常规检查");
        assert!(matches[0].confidence > 0.7);
    }

    #[test]
    fn infer_liver_function() {
        let items: Vec<String> = vec![
            "丙氨酸氨基转移酶", "天门冬氨酸氨基转移酶",
            "总胆红素", "白蛋白", "碱性磷酸酶",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let matches = infer_category(&items);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].category, "肝功能检查");
    }

    #[test]
    fn validate_correct_type() {
        let items: Vec<String> = vec![
            "白细胞计数", "红细胞计数", "血红蛋白", "血小板计数",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let result = validate_type("血常规", &items);
        assert!(matches!(result, TypeValidation::Consistent));
    }

    #[test]
    fn validate_wrong_type() {
        // Items are clearly liver function, but OCR says "血常规"
        let items: Vec<String> = vec![
            "丙氨酸氨基转移酶", "天门冬氨酸氨基转移酶",
            "总胆红素", "白蛋白", "碱性磷酸酶", "γ-谷氨酰转移酶",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let result = validate_type("血常规", &items);
        assert!(matches!(result, TypeValidation::Contradicted { .. }));
        if let TypeValidation::Contradicted {
            inferred_category, ..
        } = result
        {
            assert_eq!(inferred_category, "肝功能检查");
        }
    }

    #[test]
    fn same_profile_blood_routine() {
        let a: Vec<String> = vec!["白细胞计数", "红细胞计数", "血红蛋白", "血小板计数"]
            .into_iter()
            .map(String::from)
            .collect();
        let b: Vec<String> = vec!["中性粒细胞计数", "淋巴细胞计数", "红细胞压积"]
            .into_iter()
            .map(String::from)
            .collect();

        // b only has common_items, no signature hits → should not infer
        let (same, _conf) = same_profile(&a, &b);
        // b has no signature hits so it won't match any profile strongly
        assert!(!same || _conf < 0.3);
    }

    #[test]
    fn different_profiles() {
        let a: Vec<String> = vec!["白细胞计数", "红细胞计数", "血红蛋白", "血小板计数"]
            .into_iter()
            .map(String::from)
            .collect();
        let b: Vec<String> = vec![
            "丙氨酸氨基转移酶", "天门冬氨酸氨基转移酶",
            "总胆红素", "白蛋白",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let (same, conf) = same_profile(&a, &b);
        assert!(!same);
        assert!(conf > 0.4); // Confident they're different
    }

    #[test]
    fn insufficient_items() {
        let items: Vec<String> = vec!["白蛋白"].into_iter().map(String::from).collect();
        let result = validate_type("肝功能", &items);
        assert!(matches!(result, TypeValidation::Insufficient));
    }
}
