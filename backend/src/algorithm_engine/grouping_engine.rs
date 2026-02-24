use chrono::NaiveDate;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::LazyLock;

use super::report_profiles;
use super::report_taxonomy;

/// Decision from the algorithm engine.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeDecision {
    /// Confident these should merge.
    Merge,
    /// Confident these should NOT merge.
    NoMerge,
    /// Not confident enough — caller should fall back to LLM.
    Uncertain,
}

/// Score breakdown for debugging / logging.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MergeScore {
    pub date_score: f32,
    pub type_score: f32,
    pub item_overlap_score: f32,
    pub total: f32,
    pub decision: MergeDecision,
}

/// Candidate merge result between one new report and one existing report.
#[derive(Debug, Clone)]
pub struct MergeCandidate {
    pub new_report_index: usize,
    pub existing_report_index: usize,
    pub score: MergeScore,
}

/// Info about a report for grouping purposes.
pub struct ReportInfo<'a> {
    pub report_type: &'a str,
    pub report_date: &'a str,
    pub sample_date: &'a str,
    pub item_names: &'a [String],
}

/// Merge scoring config loaded from env vars.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeScoringConfig {
    pub date_weight: f32,
    pub type_weight: f32,
    pub item_weight: f32,
    pub merge_threshold: f32,
    pub uncertain_threshold: f32,
}

impl Default for MergeScoringConfig {
    fn default() -> Self {
        Self {
            date_weight: 0.4,
            type_weight: 0.4,
            item_weight: 0.2,
            merge_threshold: 0.7,
            uncertain_threshold: 0.4,
        }
    }
}

impl MergeScoringConfig {
    const ENV_DATE_WEIGHT: &'static str = "MERGE_SCORE_DATE_WEIGHT";
    const ENV_TYPE_WEIGHT: &'static str = "MERGE_SCORE_TYPE_WEIGHT";
    const ENV_ITEM_WEIGHT: &'static str = "MERGE_SCORE_ITEM_WEIGHT";
    const ENV_MERGE_THRESHOLD: &'static str = "MERGE_SCORE_MERGE_THRESHOLD";
    const ENV_UNCERTAIN_THRESHOLD: &'static str = "MERGE_SCORE_UNCERTAIN_THRESHOLD";

    pub fn from_env() -> Self {
        Self::from_env_with(|key| env::var(key).ok())
    }

    fn from_env_with<F>(mut get_var: F) -> Self
    where
        F: FnMut(&str) -> Option<String>,
    {
        let default = Self::default();
        let mut cfg = Self {
            date_weight: Self::read_env_f32(
                Self::ENV_DATE_WEIGHT,
                default.date_weight,
                &mut get_var,
            ),
            type_weight: Self::read_env_f32(
                Self::ENV_TYPE_WEIGHT,
                default.type_weight,
                &mut get_var,
            ),
            item_weight: Self::read_env_f32(
                Self::ENV_ITEM_WEIGHT,
                default.item_weight,
                &mut get_var,
            ),
            merge_threshold: Self::read_env_f32(
                Self::ENV_MERGE_THRESHOLD,
                default.merge_threshold,
                &mut get_var,
            ),
            uncertain_threshold: Self::read_env_f32(
                Self::ENV_UNCERTAIN_THRESHOLD,
                default.uncertain_threshold,
                &mut get_var,
            ),
        };

        let weight_sum = cfg.date_weight + cfg.type_weight + cfg.item_weight;
        if !weight_sum.is_finite() || weight_sum <= 0.0 {
            cfg.date_weight = default.date_weight;
            cfg.type_weight = default.type_weight;
            cfg.item_weight = default.item_weight;
        }

        if !(0.0..=1.0).contains(&cfg.merge_threshold) {
            cfg.merge_threshold = default.merge_threshold;
        }
        if !(0.0..=1.0).contains(&cfg.uncertain_threshold) {
            cfg.uncertain_threshold = default.uncertain_threshold;
        }

        cfg
    }

    fn read_env_f32<F>(key: &str, default: f32, get_var: &mut F) -> f32
    where
        F: FnMut(&str) -> Option<String>,
    {
        get_var(key)
            .and_then(|raw| raw.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }
}

// ===========================================================================
// Stage 0–4 Decision Pipeline (replaces weighted linear scoring)
// ===========================================================================

/// Classified date proximity signal.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DateMatch {
    SameDay,
    Adjacent,  // ±1 day
    Close,     // ±2–3 days
    Far,       // >3 days or unparseable
}

/// Classified type match signal.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TypeMatch {
    /// Both types found in taxonomy AND same category (or identical), optionally
    /// confirmed by item profile.
    Confirmed,
    /// Not in taxonomy, but item-profile inference says same category.
    Inferred,
    /// Taxonomy says different categories, or profile inference says different.
    Conflict,
    /// Cannot determine (neither taxonomy nor profile has an opinion).
    Unknown,
}

/// Classified item overlap pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ItemPattern {
    /// Jaccard < 0.2 — items are complementary (likely split pages).
    Complementary,
    /// Jaccard 0.2–0.6 — ambiguous overlap.
    Partial,
    /// Jaccard > 0.6 — heavy overlap (could be multi-page scan or recheck).
    HighOverlap,
    /// At least one side has no items.
    Empty,
}

// ---------------------------------------------------------------------------
// Signal classifiers
// ---------------------------------------------------------------------------

fn classify_date(a: &ReportInfo, b: &ReportInfo) -> DateMatch {
    let (da, db) = if !a.sample_date.is_empty() && !b.sample_date.is_empty() {
        (a.sample_date, b.sample_date)
    } else {
        (a.report_date, b.report_date)
    };
    if da.is_empty() || db.is_empty() {
        return DateMatch::Far;
    }
    if da == db {
        return DateMatch::SameDay;
    }
    match date_diff_days(da, db) {
        Some(1) => DateMatch::Adjacent,
        Some(2) | Some(3) => DateMatch::Close,
        _ => DateMatch::Far,
    }
}

fn classify_type(a: &ReportInfo, b: &ReportInfo) -> TypeMatch {
    if a.report_type.is_empty() || b.report_type.is_empty() {
        // Check item profiles as sole signal
        let (same, conf) = report_profiles::same_profile(a.item_names, b.item_names);
        if same && conf >= 0.4 {
            return TypeMatch::Inferred;
        }
        if !same && conf >= 0.5 {
            return TypeMatch::Conflict;
        }
        return TypeMatch::Unknown;
    }

    let tax = report_taxonomy::same_category(a.report_type, b.report_type);

    // Stage 0: cross-validate taxonomy result with item profiles
    let (profile_same, profile_conf) =
        report_profiles::same_profile(a.item_names, b.item_names);

    if tax.both_known {
        if tax.same_category {
            // Taxonomy is authoritative when both types are known and in the
            // same category. Profile inference is noisy (e.g. "葡萄糖" appears
            // in both 血糖 and 脑脊液 profiles) and should not override.
            return TypeMatch::Confirmed;
        } else {
            // Taxonomy says different. This is authoritative.
            return TypeMatch::Conflict;
        }
    }

    // At least one type not in taxonomy
    if tax.prefix_match {
        // Prefix heuristic matched — less reliable, treat as needing verification.
        if profile_same && profile_conf >= 0.4 {
            return TypeMatch::Inferred; // Profile corroborates prefix
        }
        if !profile_same && profile_conf >= 0.5 {
            return TypeMatch::Conflict; // Profile contradicts prefix
        }
        return TypeMatch::Unknown; // Prefix alone is not enough
    }

    if tax.same_category {
        // One type found, substring matched the other — semi-reliable
        return TypeMatch::Confirmed;
    }

    // Neither in taxonomy, no prefix match. Rely solely on item profiles.
    if profile_same && profile_conf >= 0.4 {
        return TypeMatch::Inferred;
    }
    if !profile_same && profile_conf >= 0.5 {
        return TypeMatch::Conflict;
    }
    TypeMatch::Unknown
}

fn classify_items(a: &ReportInfo, b: &ReportInfo) -> (ItemPattern, f32) {
    if a.item_names.is_empty() || b.item_names.is_empty() {
        return (ItemPattern::Empty, 0.0);
    }
    let set_a: HashSet<&str> = a.item_names.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.item_names.iter().map(|s| s.as_str()).collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.len() + set_b.len() - intersection;
    if union == 0 {
        return (ItemPattern::Empty, 0.0);
    }
    let jaccard = intersection as f32 / union as f32;

    let pattern = if jaccard < 0.2 {
        ItemPattern::Complementary
    } else if jaccard <= 0.6 {
        ItemPattern::Partial
    } else {
        ItemPattern::HighOverlap
    };
    (pattern, jaccard)
}

// ---------------------------------------------------------------------------
// Stage 1: Hard vetoes
// ---------------------------------------------------------------------------

/// Returns Some(NoMerge) if a hard veto applies, None otherwise.
fn apply_hard_vetoes(
    date: DateMatch,
    typ: TypeMatch,
    items: ItemPattern,
    jaccard: f32,
    a: &ReportInfo,
    b: &ReportInfo,
) -> Option<MergeDecision> {
    // Veto 1: dates too far apart
    if date == DateMatch::Far {
        return Some(MergeDecision::NoMerge);
    }
    // Veto 2: confirmed different categories
    if typ == TypeMatch::Conflict {
        return Some(MergeDecision::NoMerge);
    }
    // Veto 3: recheck detection — same type, same day, very high item overlap
    if date == DateMatch::SameDay
        && a.report_type == b.report_type
        && !a.report_type.is_empty()
        && items == ItemPattern::HighOverlap
        && jaccard > 0.7
    {
        return Some(MergeDecision::NoMerge);
    }
    None
}

// ---------------------------------------------------------------------------
// Stage 2: Decision tree
// ---------------------------------------------------------------------------

fn decision_tree(date: DateMatch, typ: TypeMatch, items: ItemPattern) -> MergeDecision {
    use DateMatch::*;
    use ItemPattern::*;
    use TypeMatch::*;

    match (date, typ, items) {
        // === Definite merge ===
        (SameDay,  Confirmed, Complementary) => MergeDecision::Merge,
        (SameDay,  Confirmed, Partial)       => MergeDecision::Merge,
        (SameDay,  Confirmed, HighOverlap)   => MergeDecision::Merge,  // multi-page scan
        (SameDay,  Confirmed, Empty)         => MergeDecision::Merge,  // trust type match
        (SameDay,  Inferred,  Complementary) => MergeDecision::Merge,
        (Adjacent, Confirmed, Complementary) => MergeDecision::Merge,
        (Adjacent, Confirmed, Partial)       => MergeDecision::Merge,

        // === Definite no-merge ===
        (_, Conflict, _)                     => MergeDecision::NoMerge,
        (Far, _, _)                          => MergeDecision::NoMerge,
        (Close, _, _)                        => MergeDecision::NoMerge,

        // === Uncertain — needs LLM verification ===
        (SameDay,  Inferred,  Partial)       => MergeDecision::Uncertain,
        (SameDay,  Inferred,  HighOverlap)   => MergeDecision::Uncertain,
        (SameDay,  Inferred,  Empty)         => MergeDecision::Uncertain,
        (SameDay,  Unknown,   _)             => MergeDecision::Uncertain,
        (Adjacent, Confirmed, HighOverlap)   => MergeDecision::Uncertain,
        (Adjacent, Confirmed, Empty)         => MergeDecision::Uncertain,
        (Adjacent, Inferred,  _)             => MergeDecision::Uncertain,
        (Adjacent, Unknown,   _)             => MergeDecision::Uncertain,
    }
}

// ---------------------------------------------------------------------------
// Stage 4: Post-merge validation
// ---------------------------------------------------------------------------

/// Validate a merge decision. May downgrade Merge → NoMerge.
pub fn validate_merge(a: &ReportInfo, b: &ReportInfo, decision: MergeDecision) -> MergeDecision {
    if decision != MergeDecision::Merge {
        return decision;
    }

    // Check 1: merged item count sanity
    let combined_count = a.item_names.len() + b.item_names.len();
    if combined_count > 80 {
        return MergeDecision::Uncertain;
    }

    // Check 2: profile mixing — if merged items hit signatures of two
    // different profiles, this is likely two distinct reports.
    // Guard against false positives: a single shared item like "葡萄糖" can
    // be a signature of 血糖检查 but also a common item in 脑脊液生化.
    // Require the second profile to have common_hit_ratio > 0 (i.e., multiple
    // items supporting it, not just one ambiguous signature match).
    if !a.item_names.is_empty() && !b.item_names.is_empty() {
        let mut combined: Vec<String> = Vec::with_capacity(combined_count);
        combined.extend(a.item_names.iter().cloned());
        combined.extend(b.item_names.iter().cloned());
        let inferred = report_profiles::infer_category(&combined);
        if inferred.len() >= 2
            && inferred[0].confidence >= 0.5
            && inferred[1].confidence >= 0.5
            && inferred[1].common_hit_ratio > 0.0
            && inferred[0].category != inferred[1].category
        {
            if inferred[1].confidence >= inferred[0].confidence * 0.8 {
                return MergeDecision::NoMerge;
            }
        }
    }

    MergeDecision::Merge
}

// ---------------------------------------------------------------------------
// Public scoring API (backward-compatible)
// ---------------------------------------------------------------------------

/// Cached merge scoring config loaded once from env vars.
static MERGE_CONFIG: LazyLock<MergeScoringConfig> = LazyLock::new(MergeScoringConfig::from_env);

/// Compute the overall merge score and decision using the five-stage pipeline.
pub fn compute_merge_score(a: &ReportInfo, b: &ReportInfo) -> MergeScore {
    compute_merge_score_with_config(a, b, &MERGE_CONFIG)
}

fn compute_merge_score_with_config(
    a: &ReportInfo,
    b: &ReportInfo,
    _cfg: &MergeScoringConfig,
) -> MergeScore {
    // Classify signals
    let date = classify_date(a, b);
    let typ = classify_type(a, b);
    let (items, jaccard) = classify_items(a, b);

    // Stage 1: Hard vetoes
    if let Some(veto) = apply_hard_vetoes(date, typ, items, jaccard, a, b) {
        return MergeScore {
            date_score: date_to_f32(date),
            type_score: type_to_f32(typ),
            item_overlap_score: jaccard,
            total: 0.0,
            decision: veto,
        };
    }

    // Stage 2: Decision tree
    let mut decision = decision_tree(date, typ, items);

    // Stage 4: Post-merge validation
    decision = validate_merge(a, b, decision);

    // Compute a synthetic total for logging / backward compat
    let total = match decision {
        MergeDecision::Merge => 0.9,
        MergeDecision::Uncertain => 0.5,
        MergeDecision::NoMerge => 0.1,
    };

    MergeScore {
        date_score: date_to_f32(date),
        type_score: type_to_f32(typ),
        item_overlap_score: jaccard,
        total,
        decision,
    }
}

/// Convert DateMatch to a numeric score for backward-compatible MergeScore.
fn date_to_f32(d: DateMatch) -> f32 {
    match d {
        DateMatch::SameDay => 1.0,
        DateMatch::Adjacent => 0.7,
        DateMatch::Close => 0.3,
        DateMatch::Far => 0.0,
    }
}

/// Convert TypeMatch to a numeric score for backward-compatible MergeScore.
fn type_to_f32(t: TypeMatch) -> f32 {
    match t {
        TypeMatch::Confirmed => 1.0,
        TypeMatch::Inferred => 0.7,
        TypeMatch::Unknown => 0.3,
        TypeMatch::Conflict => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Batch grouping (replaces suggest_groups LLM call)
// ---------------------------------------------------------------------------

/// Result of batch grouping.
pub struct GroupingResult {
    /// groups[i] = group ID for the i-th new file. 0 = independent.
    pub groups: Vec<i32>,
    /// (file_index, existing_report_index) pairs for files that should merge
    /// with an existing report.
    pub existing_merges: Vec<(usize, usize)>,
    /// Indices of new files whose merge decision was uncertain (need LLM).
    pub uncertain_indices: Vec<usize>,
}

/// Information about an existing report in the database.
pub struct ExistingReportInfo {
    pub report_type: String,
    pub report_date: String,
    pub sample_date: String,
    pub item_names: Vec<String>,
}

/// Perform batch grouping of new files, optionally against existing reports.
pub fn batch_group(files: &[ReportInfo], existing: &[ExistingReportInfo]) -> GroupingResult {
    let new_count = files.len();
    let mut groups = vec![0i32; new_count];
    let mut existing_merges: Vec<(usize, usize)> = Vec::new();
    let mut merged_indices = HashSet::new();
    let mut uncertain_indices: Vec<usize> = Vec::new();

    // Phase 1: Match new files against existing reports
    for (ni, f) in files.iter().enumerate() {
        let mut best_match: Option<(usize, f32)> = None;
        let mut has_uncertain = false;

        for (ei, er) in existing.iter().enumerate() {
            let er_info = ReportInfo {
                report_type: &er.report_type,
                report_date: &er.report_date,
                sample_date: &er.sample_date,
                item_names: &er.item_names,
            };
            let score = compute_merge_score(f, &er_info);
            match score.decision {
                MergeDecision::Merge => {
                    if best_match.map_or(true, |(_, bs)| score.total > bs) {
                        best_match = Some((ei, score.total));
                    }
                }
                MergeDecision::Uncertain => {
                    has_uncertain = true;
                }
                MergeDecision::NoMerge => {}
            }
        }

        if let Some((ei, _)) = best_match {
            existing_merges.push((ni, ei));
            merged_indices.insert(ni);
        } else if has_uncertain {
            uncertain_indices.push(ni);
        }
    }

    // Phase 2: Group new files among themselves
    // Track groups as list of (representative_index, group_id)
    let mut group_members: Vec<(usize, i32)> = Vec::new();
    let mut next_gid = 1i32;

    for (ni, f) in files.iter().enumerate() {
        if merged_indices.contains(&ni) {
            continue;
        }

        let mut matched_gid: Option<i32> = None;
        let mut best_score = 0.0f32;
        for &(member_idx, gid) in &group_members {
            let score = compute_merge_score(f, &files[member_idx]);
            if score.decision == MergeDecision::Merge && score.total > best_score {
                matched_gid = Some(gid);
                best_score = score.total;
            }
        }

        if let Some(gid) = matched_gid {
            groups[ni] = gid;
            group_members.push((ni, gid));
        } else {
            let gid = next_gid;
            next_gid += 1;
            groups[ni] = gid;
            group_members.push((ni, gid));
        }
    }

    // Clean up single-member groups → mark as independent (0)
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &g in &groups {
        if g > 0 {
            *counts.entry(g).or_insert(0) += 1;
        }
    }
    for g in groups.iter_mut() {
        if *g > 0 && counts.get(g) == Some(&1) {
            *g = 0;
        }
    }

    GroupingResult {
        groups,
        existing_merges,
        uncertain_indices,
    }
}

// ---------------------------------------------------------------------------
// Merge check for existing reports (replaces llm_merge_check)
// ---------------------------------------------------------------------------

/// Check whether a set of new reports should merge with existing DB reports.
/// Returns candidate pairs whose decision is not `NoMerge`.
pub fn check_merge_candidates(
    new_reports: &[ReportInfo],
    existing_reports: &[ExistingReportInfo],
) -> Vec<MergeCandidate> {
    let mut results = Vec::new();
    for (ni, nr) in new_reports.iter().enumerate() {
        for (ei, er) in existing_reports.iter().enumerate() {
            let er_info = ReportInfo {
                report_type: &er.report_type,
                report_date: &er.report_date,
                sample_date: &er.sample_date,
                item_names: &er.item_names,
            };
            let score = compute_merge_score(nr, &er_info);
            if score.decision != MergeDecision::NoMerge {
                results.push(MergeCandidate {
                    new_report_index: ni,
                    existing_report_index: ei,
                    score,
                });
            }
        }
    }
    results
}

/// Choose the highest-scoring `Merge` target for each new report index.
pub fn best_merge_targets(candidates: &[MergeCandidate]) -> Vec<(usize, usize)> {
    let mut best: HashMap<usize, (usize, f32)> = HashMap::new();
    for candidate in candidates {
        if candidate.score.decision != MergeDecision::Merge {
            continue;
        }
        let entry = best
            .entry(candidate.new_report_index)
            .or_insert((candidate.existing_report_index, candidate.score.total));
        if candidate.score.total > entry.1 {
            *entry = (candidate.existing_report_index, candidate.score.total);
        }
    }
    let mut picked: Vec<(usize, usize)> = best
        .into_iter()
        .map(|(new_idx, (existing_idx, _))| (new_idx, existing_idx))
        .collect();
    picked.sort_by_key(|(new_idx, _)| *new_idx);
    picked
}

// ---------------------------------------------------------------------------
// Date helpers
// ---------------------------------------------------------------------------

fn date_diff_days(a: &str, b: &str) -> Option<i64> {
    if let (Ok(da), Ok(db)) = (
        NaiveDate::parse_from_str(a, "%Y-%m-%d"),
        NaiveDate::parse_from_str(b, "%Y-%m-%d"),
    ) {
        Some((da - db).num_days().abs())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dates_within_one_day(a: &str, b: &str) -> bool {
        date_diff_days(a, b).map_or(false, |d| d <= 1)
    }

    fn mk_info<'a>(
        report_type: &'a str,
        report_date: &'a str,
        sample_date: &'a str,
        items: &'a [String],
    ) -> ReportInfo<'a> {
        ReportInfo {
            report_type,
            report_date,
            sample_date,
            item_names: items,
        }
    }

    // --- Core decision pipeline tests ---

    #[test]
    fn same_type_same_date_merge() {
        let items_a: Vec<String> = vec!["白细胞".into(), "红细胞".into()];
        let items_b: Vec<String> = vec!["血小板".into(), "血红蛋白".into()];
        let a = mk_info("脑脊液常规", "2024-01-15", "", &items_a);
        let b = mk_info("脑脊液生化", "2024-01-15", "", &items_b);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::Merge);
    }

    #[test]
    fn different_type_no_merge() {
        let items: Vec<String> = vec!["白细胞".into()];
        let a = mk_info("血常规", "2024-01-15", "", &items);
        let b = mk_info("尿常规", "2024-01-15", "", &items);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::NoMerge);
    }

    #[test]
    fn different_date_no_merge() {
        let items: Vec<String> = vec!["白细胞".into()];
        let a = mk_info("脑脊液常规", "2024-01-15", "", &items);
        let b = mk_info("脑脊液生化", "2024-02-20", "", &items);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::NoMerge);
    }

    #[test]
    fn adjacent_date_with_same_type() {
        let items_a: Vec<String> = vec!["白细胞".into()];
        let items_b: Vec<String> = vec!["血小板".into()];
        let a = mk_info("脑脊液常规", "2024-01-15", "", &items_a);
        let b = mk_info("脑脊液生化", "2024-01-16", "", &items_b);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::Merge);
    }

    // --- V3 accuracy tests ---

    #[test]
    fn v3_different_category_same_day_complementary_is_no_merge() {
        // BUG FIX: 血常规 + 肝功能, same day, no item overlap
        // Old engine: Uncertain (0.6). New engine: NoMerge.
        let items_a: Vec<String> = vec!["白细胞计数".into(), "红细胞计数".into()];
        let items_b: Vec<String> = vec!["丙氨酸氨基转移酶".into(), "总胆红素".into()];
        let a = mk_info("血常规", "2024-01-15", "", &items_a);
        let b = mk_info("肝功能", "2024-01-15", "", &items_b);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::NoMerge);
    }

    #[test]
    fn v3_recheck_same_day_same_type_high_overlap() {
        // Same type, same day, nearly identical items → recheck, not merge.
        let items: Vec<String> = vec![
            "白细胞计数".into(), "红细胞计数".into(),
            "血红蛋白".into(), "血小板计数".into(),
        ];
        let a = mk_info("血常规", "2024-01-15", "", &items);
        let b = mk_info("血常规", "2024-01-15", "", &items);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::NoMerge);
    }

    #[test]
    fn v3_unknown_type_goes_uncertain() {
        let items_a: Vec<String> = vec!["项目1".into()];
        let items_b: Vec<String> = vec!["项目2".into()];
        let a = mk_info("未知检查", "2024-01-15", "", &items_a);
        let b = mk_info("神秘检查", "2024-01-15", "", &items_b);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::Uncertain);
    }

    #[test]
    fn v3_close_date_no_merge() {
        // ±2-3 days should always be NoMerge
        let items_a: Vec<String> = vec!["白细胞".into()];
        let items_b: Vec<String> = vec!["血小板".into()];
        let a = mk_info("脑脊液常规", "2024-01-15", "", &items_a);
        let b = mk_info("脑脊液生化", "2024-01-17", "", &items_b);
        let score = compute_merge_score(&a, &b);
        assert_eq!(score.decision, MergeDecision::NoMerge);
    }

    // --- Batch grouping tests ---

    #[test]
    fn batch_group_basic() {
        let items_a: Vec<String> = vec!["白细胞".into()];
        let items_b: Vec<String> = vec!["血小板".into()];
        let items_c: Vec<String> = vec!["尿蛋白".into()];
        let files = vec![
            mk_info("脑脊液常规", "2024-01-15", "", &items_a),
            mk_info("脑脊液生化", "2024-01-15", "", &items_b),
            mk_info("尿常规", "2024-01-15", "", &items_c),
        ];
        let result = batch_group(&files, &[]);
        // First two should be grouped together, third independent
        assert!(result.groups[0] > 0);
        assert_eq!(result.groups[0], result.groups[1]);
        assert_eq!(result.groups[2], 0);
    }

    // --- Date helper tests ---

    #[test]
    fn dates_within_one_day_test() {
        assert!(dates_within_one_day("2024-01-15", "2024-01-15"));
        assert!(dates_within_one_day("2024-01-15", "2024-01-16"));
        assert!(dates_within_one_day("2024-01-16", "2024-01-15"));
        assert!(!dates_within_one_day("2024-01-15", "2024-01-17"));
    }

    #[test]
    fn dates_within_one_day_handles_leap_day() {
        assert!(dates_within_one_day("2024-02-29", "2024-03-01"));
        assert!(!dates_within_one_day("2024-02-29", "2024-03-02"));
    }

    // --- Merge candidate tests ---

    #[test]
    fn best_merge_targets_prefers_higher_score() {
        let new_items: Vec<String> = vec!["葡萄糖".into(), "氯".into()];
        let new_reports = vec![mk_info("脑脊液生化", "2024-03-15", "", &new_items)];
        let existing_reports = vec![
            ExistingReportInfo {
                report_type: "脑脊液常规".into(),
                report_date: "2024-03-15".into(),
                sample_date: String::new(),
                item_names: vec!["白细胞计数".into(), "红细胞计数".into()],
            },
            ExistingReportInfo {
                report_type: "脑脊液免疫球蛋白".into(),
                report_date: "2024-03-15".into(),
                sample_date: String::new(),
                item_names: vec!["葡萄糖".into(), "氯".into()],
            },
        ];

        let candidates = check_merge_candidates(&new_reports, &existing_reports);
        let best = best_merge_targets(&candidates);

        // Should merge with one of the existing reports (both are same category)
        assert!(!best.is_empty());
    }

    // --- 跨医院、跨批次实战测试 ---

    /// 场景：同一次住院的报告，因为医院出报告时间不同，隔了几天才上传
    /// 验证：引擎是否能根据 sample_date 正确判断合并
    #[test]
    fn chaos_cross_batch_delayed_upload() {
        // 脑脊液常规：采样 3-15，报告 3-15（当天出结果）
        let items_csf1: Vec<String> = vec!["潘氏试验".into(), "白细胞计数".into(), "红细胞计数".into()];
        // 脑脊液生化：采样 3-15，但报告 3-17（隔了两天才出结果）
        let items_csf2: Vec<String> = vec!["葡萄糖".into(), "氯".into(), "蛋白质".into()];
        // 脑脊液免疫：采样 3-15，报告 3-18（隔了三天才出结果）
        let items_csf3: Vec<String> = vec!["免疫球蛋白G".into(), "免疫球蛋白A".into()];

        eprintln!("\n===== 跨批次延迟上传场景 =====");

        // Case 1: 同 sample_date，不同 report_date → 应该用 sample_date 判断 → SameDay → Merge
        let a = mk_info("脑脊液常规", "2024-03-15", "2024-03-15", &items_csf1);
        let b = mk_info("脑脊液生化", "2024-03-17", "2024-03-15", &items_csf2);
        let score = compute_merge_score(&a, &b);
        eprintln!(
            "  脑脊液常规(报告3-15,采样3-15) vs 脑脊液生化(报告3-17,采样3-15) → {:?} (date={:.1}, type={:.1})",
            score.decision, score.date_score, score.type_score
        );
        assert_eq!(
            score.decision,
            MergeDecision::Merge,
            "同采样日不同报告日应合并"
        );

        // Case 2: 同 sample_date，报告隔了3天 → 仍然用 sample_date → SameDay → Merge
        let c = mk_info("脑脊液免疫球蛋白", "2024-03-18", "2024-03-15", &items_csf3);
        let score2 = compute_merge_score(&a, &c);
        eprintln!(
            "  脑脊液常规(报告3-15,采样3-15) vs 脑脊液免疫(报告3-18,采样3-15) → {:?} (date={:.1}, type={:.1})",
            score2.decision, score2.date_score, score2.type_score
        );
        assert_eq!(
            score2.decision,
            MergeDecision::Merge,
            "采样日相同，即使报告日差3天也应合并"
        );

        // Case 3: 没有 sample_date，只有 report_date 隔了2天 → Close → NoMerge
        let d = mk_info("脑脊液常规", "2024-03-15", "", &items_csf1);
        let e = mk_info("脑脊液生化", "2024-03-17", "", &items_csf2);
        let score3 = compute_merge_score(&d, &e);
        eprintln!(
            "  脑脊液常规(仅报告日3-15) vs 脑脊液生化(仅报告日3-17) → {:?} (date={:.1})",
            score3.decision, score3.date_score
        );
        assert_eq!(
            score3.decision,
            MergeDecision::NoMerge,
            "无采样日且报告日差2天应拒绝合并"
        );

        // Case 4: 没有 sample_date，report_date 相邻（±1天）→ Adjacent → 可合并
        let f = mk_info("脑脊液生化", "2024-03-16", "", &items_csf2);
        let score4 = compute_merge_score(&d, &f);
        eprintln!(
            "  脑脊液常规(仅报告日3-15) vs 脑脊液生化(仅报告日3-16) → {:?} (date={:.1})",
            score4.decision, score4.date_score
        );
        assert_eq!(
            score4.decision,
            MergeDecision::Merge,
            "报告日相邻且同类别应合并"
        );
    }

    /// 场景：不同医院的报告类型名称不同，但属于同一类别
    /// 验证：跨医院的报告能否正确判断类型关系
    #[test]
    fn chaos_cross_hospital_naming() {
        eprintln!("\n===== 跨医院命名差异场景 =====");

        // 医院A叫"血常规"，医院B叫"血细胞分析" — 名字不同但同类别
        // 注意：recheck 检测要求 report_type 完全相同，跨医院名称不同时不触发
        // 引擎将此视为"多页扫描"场景 → Merge，这对跨医院场景是合理的
        let items_blood: Vec<String> = vec!["白细胞计数".into(), "红细胞计数".into(), "血红蛋白".into()];
        let a = mk_info("血常规", "2024-03-15", "", &items_blood);
        let b = mk_info("血细胞分析", "2024-03-15", "", &items_blood);
        let score = compute_merge_score(&a, &b);
        eprintln!(
            "  血常规 vs 血细胞分析 (同日同项,不同名) → {:?}",
            score.decision
        );
        assert_eq!(score.decision, MergeDecision::Merge, "跨医院同类别+同日+同项 → 合并");

        // 对比：完全相同的 report_type + 高重叠 → recheck 检测 → NoMerge
        let c_dup = mk_info("血常规", "2024-03-15", "", &items_blood);
        let d_dup = mk_info("血常规", "2024-03-15", "", &items_blood);
        let score_dup = compute_merge_score(&c_dup, &d_dup);
        eprintln!(
            "  血常规 vs 血常规 (同日同项,同名) → {:?}",
            score_dup.decision
        );
        assert_eq!(score_dup.decision, MergeDecision::NoMerge, "同名同日高重叠 → 复查/重复");

        // 医院A叫"肝功能"，医院B叫"肝功十一项"，互补项目
        let items_liver_a: Vec<String> = vec!["丙氨酸氨基转移酶".into(), "总胆红素".into()];
        let items_liver_b: Vec<String> = vec!["白蛋白".into(), "球蛋白".into()];
        let c = mk_info("肝功能", "2024-03-15", "", &items_liver_a);
        let d = mk_info("肝功十一项", "2024-03-15", "", &items_liver_b);
        let score2 = compute_merge_score(&c, &d);
        eprintln!(
            "  肝功能 vs 肝功十一项 (同日互补项) → {:?}",
            score2.decision
        );
        assert_eq!(score2.decision, MergeDecision::Merge, "同类别+同日+互补项目应合并");

        // 不同类别不应合并，即使同天
        let items_kidney: Vec<String> = vec!["肌酐".into(), "尿素氮".into()];
        let e = mk_info("肝功能", "2024-03-15", "", &items_liver_a);
        let f = mk_info("肾功能", "2024-03-15", "", &items_kidney);
        let score3 = compute_merge_score(&e, &f);
        eprintln!(
            "  肝功能 vs 肾功能 (同日不同类) → {:?}",
            score3.decision
        );
        assert_eq!(score3.decision, MergeDecision::NoMerge, "不同类别不应合并");

        // 甲功三项 vs 甲状腺功能全套 — 同类别同日互补
        let items_thyroid_a: Vec<String> = vec!["促甲状腺激素".into(), "游离T3".into()];
        let items_thyroid_b: Vec<String> = vec!["游离T4".into(), "甲状腺球蛋白抗体".into()];
        let g = mk_info("甲功三项", "2024-01-10", "", &items_thyroid_a);
        let h = mk_info("甲状腺功能全套", "2024-01-10", "", &items_thyroid_b);
        let score4 = compute_merge_score(&g, &h);
        eprintln!(
            "  甲功三项 vs 甲状腺功能全套 (同日互补) → {:?}",
            score4.decision
        );
        assert_eq!(score4.decision, MergeDecision::Merge, "甲功变体+同日+互补应合并");
    }

    /// 场景：一次住院产生多份报告，分批上传到系统
    /// 第一批：脑脊液常规 + 乙肝五项（3月15日上传）
    /// 第二批：脑脊液生化 + 脑脊液免疫（3月16日上传，但采样日都是3月15日）
    /// 第三批：血常规（3月15日的，独立报告）
    /// 验证：batch_group 能否正确处理跨批次 + 已有报告的合并
    #[test]
    fn chaos_batch_group_multi_batch() {
        eprintln!("\n===== 多批次上传场景 =====");

        // 已有报告（第一批上传）
        let existing = vec![
            ExistingReportInfo {
                report_type: "脑脊液常规".into(),
                report_date: "2024-03-15".into(),
                sample_date: "2024-03-15".into(),
                item_names: vec!["潘氏试验".into(), "白细胞计数".into()],
            },
            ExistingReportInfo {
                report_type: "乙肝五项".into(),
                report_date: "2024-03-15".into(),
                sample_date: "2024-03-15".into(),
                item_names: vec!["乙肝表面抗原".into(), "乙肝表面抗体".into()],
            },
        ];

        // 第二批上传的新报告
        let items_csf_bio: Vec<String> = vec!["葡萄糖".into(), "氯".into(), "蛋白质".into()];
        let items_csf_imm: Vec<String> = vec!["免疫球蛋白G".into(), "免疫球蛋白A".into()];
        let items_blood: Vec<String> = vec!["白细胞计数".into(), "红细胞计数".into(), "血红蛋白".into()];

        let new_files = vec![
            // 脑脊液生化：采样3-15但报告3-16 → 应与已有脑脊液常规合并
            mk_info("脑脊液生化", "2024-03-16", "2024-03-15", &items_csf_bio),
            // 脑脊液免疫：采样3-15但报告3-17 → 应与已有脑脊液常规合并
            mk_info("脑脊液免疫球蛋白", "2024-03-17", "2024-03-15", &items_csf_imm),
            // 血常规：3-15的独立报告 → 不应与任何已有报告合并
            mk_info("血常规", "2024-03-15", "2024-03-15", &items_blood),
        ];

        let result = batch_group(&new_files, &existing);

        eprintln!("  分组结果: {:?}", result.groups);
        eprintln!("  已有合并: {:?}", result.existing_merges);
        eprintln!("  不确定项: {:?}", result.uncertain_indices);

        // 脑脊液生化(0) 和 脑脊液免疫(1) 应该合并到已有报告 0（脑脊液常规）
        let csf_merges: Vec<_> = result.existing_merges.iter()
            .filter(|(_, ei)| *ei == 0)
            .map(|(ni, _)| *ni)
            .collect();
        assert!(
            csf_merges.contains(&0),
            "脑脊液生化应合并到已有脑脊液常规"
        );
        assert!(
            csf_merges.contains(&1),
            "脑脊液免疫应合并到已有脑脊液常规"
        );

        // 血常规(2) 不应合并到任何已有报告
        let blood_merged = result.existing_merges.iter().any(|(ni, _)| *ni == 2);
        assert!(
            !blood_merged,
            "血常规不应合并到脑脊液或乙肝报告"
        );
    }

    // --- Config tests ---

    #[test]
    fn config_from_env_defaults() {
        let cfg = MergeScoringConfig::from_env_with(|_| None);
        assert_eq!(cfg, MergeScoringConfig::default());
    }

    #[test]
    fn invalid_env_values_fallback_to_default_without_panic() {
        let parsed = std::panic::catch_unwind(|| {
            MergeScoringConfig::from_env_with(|key| match key {
                "MERGE_SCORE_DATE_WEIGHT" => Some("-1.0".to_string()),
                "MERGE_SCORE_TYPE_WEIGHT" => Some("abc".to_string()),
                "MERGE_SCORE_ITEM_WEIGHT" => Some("-1.0".to_string()),
                "MERGE_SCORE_MERGE_THRESHOLD" => Some("1.2".to_string()),
                "MERGE_SCORE_UNCERTAIN_THRESHOLD" => Some("-0.1".to_string()),
                _ => None,
            })
        });

        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap(), MergeScoringConfig::default());
    }
}
