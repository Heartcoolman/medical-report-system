use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Deserialize)]
struct TaxonomyData {
    categories: Vec<CategoryEntry>,
}

#[derive(Deserialize)]
struct CategoryEntry {
    name: String,
    types: Vec<String>,
}

/// Result of comparing two report types.
#[derive(Debug, Clone)]
pub struct CategoryMatch {
    /// Whether the two types are in the same category.
    pub same_category: bool,
    /// Confidence score: 1.0 = exact dict match, 0.7 = prefix heuristic, 0.0 = unrelated.
    #[allow(dead_code)]
    pub confidence: f32,
    /// The category name (if matched).
    #[allow(dead_code)]
    pub category: Option<String>,
    /// Whether both types were found in the taxonomy dictionary.
    pub both_known: bool,
    /// True when the match was based on Chinese prefix heuristic (less reliable).
    pub prefix_match: bool,
}

/// Static taxonomy loaded once from the embedded JSON dictionary.
static TAXONOMY: LazyLock<Taxonomy> = LazyLock::new(|| {
    let json_bytes = include_bytes!("dictionaries/report_types.json");
    let data: TaxonomyData =
        serde_json::from_slice(json_bytes).expect("无法解析 report_types.json");

    let mut type_to_category: HashMap<String, String> = HashMap::new();
    for cat in &data.categories {
        for t in &cat.types {
            type_to_category.insert(t.clone(), cat.name.clone());
        }
    }

    Taxonomy { type_to_category }
});

struct Taxonomy {
    /// Maps a report type string → category name.
    type_to_category: HashMap<String, String>,
}

/// Public wrapper for lookup_category (used by report_profiles).
#[allow(dead_code)]
pub fn lookup_category_pub(report_type: &str) -> Option<String> {
    lookup_category(report_type).map(|s| s.to_string())
}

/// Look up the category for a given report type.
/// Tries exact match first, then substring containment against dictionary keys.
fn lookup_category(report_type: &str) -> Option<&str> {
    let trimmed = report_type.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Exact match
    if let Some(cat) = TAXONOMY.type_to_category.get(trimmed) {
        return Some(cat.as_str());
    }
    // Check if any known type is a substring of the input (or vice versa).
    // e.g. input "脑脊液常规检查" contains dict key "脑脊液常规".
    let mut best: Option<(&str, usize)> = None;
    for (known_type, cat) in &TAXONOMY.type_to_category {
        if trimmed.contains(known_type.as_str()) || known_type.contains(trimmed) {
            let len = known_type.chars().count();
            if best.map_or(true, |(_, bl)| len > bl) {
                best = Some((cat.as_str(), len));
            }
        }
    }
    best.map(|(cat, _)| cat)
}

/// Compare two report types and determine if they belong to the same category.
pub fn same_category(type_a: &str, type_b: &str) -> CategoryMatch {
    let a = type_a.trim();
    let b = type_b.trim();

    // Identical types
    if a == b {
        let cat = lookup_category(a).map(|s| s.to_string());
        return CategoryMatch {
            same_category: true,
            confidence: 1.0,
            category: cat,
            both_known: lookup_category(a).is_some(),
            prefix_match: false,
        };
    }

    // Both found in taxonomy
    let cat_a = lookup_category(a);
    let cat_b = lookup_category(b);
    let both_known = cat_a.is_some() && cat_b.is_some();

    if let (Some(ca), Some(cb)) = (cat_a, cat_b) {
        if ca == cb {
            return CategoryMatch {
                same_category: true,
                confidence: 1.0,
                category: Some(ca.to_string()),
                both_known: true,
                prefix_match: false,
            };
        }
        return CategoryMatch {
            same_category: false,
            // Confidence is 1.0 here because we are *certain* the two types belong
            // to different categories (both were found in the taxonomy). The confidence
            // value represents certainty of the judgment, not similarity between types.
            confidence: 1.0,
            category: None,
            both_known: true,
            prefix_match: false,
        };
    }

    // Fallback: Chinese common prefix heuristic (≥ 3 chars for higher reliability)
    let prefix_len = a
        .chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    if prefix_len >= 3 {
        return CategoryMatch {
            same_category: true,
            confidence: 0.7,
            category: None,
            both_known,
            prefix_match: true,
        };
    }

    CategoryMatch {
        same_category: false,
        confidence: 0.0,
        category: None,
        both_known,
        prefix_match: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_same_category() {
        let m = same_category("脑脊液常规", "脑脊液生化");
        assert!(m.same_category);
        assert_eq!(m.confidence, 1.0);
        assert_eq!(m.category.as_deref(), Some("脑脊液检查"));
    }

    #[test]
    fn different_category() {
        let m = same_category("血常规", "尿常规");
        assert!(!m.same_category);
    }

    #[test]
    fn hepatitis_b_group() {
        let m = same_category("乙肝五项", "乙肝病毒DNA");
        assert!(m.same_category);
        assert_eq!(m.category.as_deref(), Some("乙肝检查"));
    }

    #[test]
    fn identical_types() {
        let m = same_category("血常规", "血常规");
        assert!(m.same_category);
        assert_eq!(m.confidence, 1.0);
    }

    #[test]
    fn prefix_fallback() {
        // Unknown types sharing a 2+ char prefix
        let m = same_category("脑脊液特殊染色", "脑脊液特殊培养");
        assert!(m.same_category);
        assert_eq!(m.confidence, 0.7);
    }

    #[test]
    fn unrelated() {
        let m = same_category("血常规", "肝功能");
        assert!(!m.same_category);
    }

    #[test]
    fn substring_match() {
        // Input is longer than dict key but contains it
        let m = same_category("脑脊液常规检查", "脑脊液生化");
        assert!(m.same_category);
    }
}
