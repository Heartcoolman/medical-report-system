mod edit_log_repo;
mod expense_repo;
pub mod helpers;
mod interpretation_repo;
pub mod medication_repo;
mod patient_repo;
mod report_repo;
mod temperature_repo;
mod test_item_repo;
mod trend_repo;

use crate::error::AppError;
use crate::models::Report;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

use helpers::backfill_comparator_statuses;

/// Input for batch report creation
pub struct BatchReportInput {
    /// If merging into existing report, set to Some(existing_report_id)
    pub existing_report_id: Option<String>,
    /// The new Report object (only set when creating new, None when merging)
    pub new_report: Option<Report>,
    /// Test items to create
    pub items: Vec<crate::models::TestItem>,
}

#[derive(Clone)]
pub struct Database {
    pub db: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, AppError> {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = Connection::open(path)?;

        // WAL mode + performance PRAGMAs
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA cache_size = -20000;
            PRAGMA foreign_keys = ON;
        ",
        )?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS patients (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                gender TEXT NOT NULL,
                dob TEXT NOT NULL,
                phone TEXT NOT NULL,
                id_number TEXT NOT NULL,
                notes TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS patient_search (
                patient_id TEXT PRIMARY KEY,
                search_blob TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS reports (
                id TEXT PRIMARY KEY,
                patient_id TEXT NOT NULL,
                report_type TEXT NOT NULL,
                hospital TEXT NOT NULL,
                report_date TEXT NOT NULL,
                sample_date TEXT NOT NULL,
                file_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_reports_patient_date ON reports(patient_id, report_date, id);
            CREATE TABLE IF NOT EXISTS temperature_records (
                id TEXT PRIMARY KEY,
                patient_id TEXT NOT NULL,
                recorded_at TEXT NOT NULL,
                value REAL NOT NULL,
                note TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_temperatures_patient_recorded
                ON temperature_records(patient_id, recorded_at, id);
            CREATE TABLE IF NOT EXISTS test_items (
                id TEXT PRIMARY KEY,
                report_id TEXT NOT NULL,
                name TEXT NOT NULL,
                value TEXT NOT NULL,
                unit TEXT NOT NULL,
                reference_range TEXT NOT NULL,
                status TEXT NOT NULL,
                canonical_name TEXT NOT NULL,
                FOREIGN KEY(report_id) REFERENCES reports(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_test_items_report ON test_items(report_id, id);
            CREATE TABLE IF NOT EXISTS edit_logs (
                id TEXT PRIMARY KEY,
                report_id TEXT NOT NULL,
                patient_id TEXT NOT NULL,
                action TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                summary TEXT NOT NULL,
                changes TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_edit_logs_report_created
                ON edit_logs(report_id, created_at, id);
            CREATE INDEX IF NOT EXISTS idx_edit_logs_created
                ON edit_logs(created_at, id);
            CREATE TABLE IF NOT EXISTS report_interpretations (
                report_id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS daily_expenses (
                id TEXT PRIMARY KEY,
                patient_id TEXT NOT NULL,
                expense_date TEXT NOT NULL,
                total_amount REAL NOT NULL,
                drug_analysis TEXT NOT NULL,
                treatment_analysis TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_daily_expenses_patient_date
                ON daily_expenses(patient_id, expense_date, id);
            CREATE TABLE IF NOT EXISTS expense_items (
                id TEXT PRIMARY KEY,
                expense_id TEXT NOT NULL,
                name TEXT NOT NULL,
                category TEXT NOT NULL,
                quantity TEXT NOT NULL,
                amount REAL NOT NULL,
                note TEXT NOT NULL,
                FOREIGN KEY(expense_id) REFERENCES daily_expenses(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_expense_items_expense
                ON expense_items(expense_id, id);
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'readonly',
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS audit_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT,
                ip_address TEXT,
                details TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at);
            CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
            CREATE INDEX IF NOT EXISTS idx_audit_logs_resource ON audit_logs(resource_type);
            CREATE TABLE IF NOT EXISTS user_api_keys (
                user_id TEXT PRIMARY KEY,
                llm_api_key TEXT,
                interpret_api_key TEXT,
                siliconflow_api_key TEXT,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS medications (
                id TEXT PRIMARY KEY,
                patient_id TEXT NOT NULL,
                name TEXT NOT NULL,
                dosage TEXT NOT NULL,
                frequency TEXT NOT NULL,
                start_date TEXT NOT NULL,
                end_date TEXT,
                note TEXT NOT NULL DEFAULT '',
                active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_medications_patient
                ON medications(patient_id, active, start_date);
            "#,
        )?;

        // Migration: add location column to temperature_records
        conn.execute_batch(
            "ALTER TABLE temperature_records ADD COLUMN location TEXT NOT NULL DEFAULT ''",
        )
        .ok(); // ignore error if column already exists

        backfill_comparator_statuses(&conn)?;

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut conn = self
            .db
            .lock()
            .map_err(|_| AppError::Internal("数据库连接锁获取失败".to_string()))?;
        f(&mut conn)
    }
}

#[cfg(test)]
mod tests {
    use super::helpers::*;

    // --- compute_report_categories ---

    #[test]
    fn categories_groups_by_curated_dict() {
        let types = vec![
            "脑脊液常规".to_string(),
            "脑脊液生化".to_string(),
            "脑脊液免疫球蛋白".to_string(),
            "血常规".to_string(),
        ];
        let map = compute_report_categories(&types);
        assert_eq!(map["脑脊液常规"], "脑脊液检查");
        assert_eq!(map["脑脊液生化"], "脑脊液检查");
        assert_eq!(map["脑脊液免疫球蛋白"], "脑脊液检查");
        assert_eq!(map["血常规"], "血常规检查");
    }

    #[test]
    fn categories_no_merge_short_prefix() {
        let types = vec!["血常规".to_string(), "血生化".to_string()];
        let map = compute_report_categories(&types);
        assert_ne!(map["血常规"], map["血生化"]);
    }

    #[test]
    fn categories_thyroid_synonyms() {
        let types = vec![
            "甲功三项".to_string(),
            "甲状腺功能".to_string(),
            "甲功五项".to_string(),
        ];
        let map = compute_report_categories(&types);
        assert_eq!(map["甲功三项"], "甲状腺功能检查");
        assert_eq!(map["甲状腺功能"], "甲状腺功能检查");
        assert_eq!(map["甲功五项"], "甲状腺功能检查");
    }

    #[test]
    fn categories_liver_function_variants() {
        let types = vec![
            "肝功能".to_string(),
            "肝功十项".to_string(),
            "肝功八项".to_string(),
        ];
        let map = compute_report_categories(&types);
        assert_eq!(map["肝功能"], "肝功能检查");
        assert_eq!(map["肝功十项"], "肝功能检查");
        assert_eq!(map["肝功八项"], "肝功能检查");
    }

    #[test]
    fn categories_urine_synonyms() {
        let types = vec!["尿常规".to_string(), "尿液分析".to_string()];
        let map = compute_report_categories(&types);
        assert_eq!(map["尿常规"], "尿常规检查");
        assert_eq!(map["尿液分析"], "尿常规检查");
    }

    #[test]
    fn categories_fallback_prefix_grouping() {
        let types = vec!["某某某检查A".to_string(), "某某某检查B".to_string()];
        let map = compute_report_categories(&types);
        assert_eq!(map["某某某检查A"], map["某某某检查B"]);
    }

    #[test]
    fn categories_empty_input() {
        let map = compute_report_categories(&[]);
        assert!(map.is_empty());
    }

    // --- normalize_trend_item_name ---

    #[test]
    fn normalize_preserves_sensitivity_prefix_via_dict() {
        assert_eq!(normalize_trend_item_name("超敏C反应蛋白"), "超敏C反应蛋白");
        assert_eq!(normalize_trend_item_name("高敏C反应蛋白"), "超敏C反应蛋白");
        assert_eq!(
            normalize_trend_item_name("超高敏C反应蛋白"),
            "超敏C反应蛋白"
        );
        assert_eq!(normalize_trend_item_name("C反应蛋白"), "C反应蛋白");
        assert_ne!(
            normalize_trend_item_name("超敏C反应蛋白"),
            normalize_trend_item_name("C反应蛋白")
        );
    }

    #[test]
    fn normalize_strips_parenthesized_suffix() {
        assert_eq!(normalize_trend_item_name("白蛋白（比色）"), "白蛋白");
        assert_eq!(normalize_trend_item_name("肌酐(酶法)"), "肌酐");
    }

    #[test]
    fn normalize_strips_dingliang_suffix() {
        assert_eq!(
            normalize_trend_item_name("乙肝表面抗原定量"),
            "乙肝表面抗原"
        );
    }

    #[test]
    fn normalize_hbv_dna_aliases() {
        assert_eq!(normalize_trend_item_name("HBV-DNA"), "乙肝病毒DNA");
        assert_eq!(normalize_trend_item_name("HBV_DNA"), "乙肝病毒DNA");
        assert_eq!(normalize_trend_item_name("乙型肝炎病毒DNA"), "乙肝病毒DNA");
    }

    #[test]
    fn normalize_english_hbv_markers() {
        assert_eq!(normalize_trend_item_name("HBsAg"), "乙肝表面抗原");
        assert_eq!(normalize_trend_item_name("HBsAb"), "乙肝表面抗体");
        assert_eq!(normalize_trend_item_name("HBeAg"), "乙肝e抗原");
        assert_eq!(normalize_trend_item_name("HBeAb"), "乙肝e抗体");
        assert_eq!(normalize_trend_item_name("HBcAb"), "乙肝核心抗体");
    }

    #[test]
    fn normalize_unifies_hepatitis_b_names() {
        assert_eq!(
            normalize_trend_item_name("乙型肝炎表面抗原"),
            "乙肝表面抗原"
        );
        assert_eq!(normalize_trend_item_name("乙肝E抗原"), "乙肝e抗原");
    }

    #[test]
    fn normalize_strips_body_fluid_prefix() {
        assert_eq!(normalize_trend_item_name("脑脊液氯"), "氯");
        assert_eq!(normalize_trend_item_name("尿液白细胞"), "白细胞计数");
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize_trend_item_name("  白蛋白  "), "白蛋白");
    }

    #[test]
    fn normalize_plain_name_unchanged() {
        assert_eq!(normalize_trend_item_name("白细胞计数"), "白细胞计数");
    }

    // --- chaos tests ---

    /// 模拟真实场景：多家医院、多种命名风格的报告类型 → 分类引擎能否正确归组
    #[test]
    fn chaos_report_type_classification() {
        let types = vec![
            "肝功能".to_string(),
            "肝功十一项".to_string(),
            "肝功八项".to_string(),
            "肝功全套".to_string(),
            "肝功能检测".to_string(),
            "血常规".to_string(),
            "血常规五分类".to_string(),
            "血细胞分析".to_string(),
            "全血细胞计数".to_string(),
            "血液分析".to_string(),
            "甲功三项".to_string(),
            "甲功五项".to_string(),
            "甲状腺功能".to_string(),
            "甲状腺功能全套".to_string(),
            "凝血四项".to_string(),
            "凝血功能".to_string(),
            "凝血全套".to_string(),
            "凝血七项".to_string(),
            "乙肝五项".to_string(),
            "乙肝两对半".to_string(),
            "乙肝病毒DNA".to_string(),
            "肾功能".to_string(),
            "血脂四项".to_string(),
            "尿常规".to_string(),
            "生化全套".to_string(),
        ];

        let map = compute_report_categories(&types);
        let mut sorted: Vec<_> = map.iter().collect();
        sorted.sort_by(|(a, _), (b, _)| a.cmp(b));
        eprintln!("\n===== 报告类型分类结果 =====");
        for (rt, cat) in &sorted {
            eprintln!("  {:20} → {}", rt, cat);
        }

        let liver = &map["肝功能"];
        assert_eq!(&map["肝功十一项"], liver);
        assert_eq!(&map["肝功八项"], liver);
        assert_eq!(&map["肝功全套"], liver);
        assert_eq!(&map["肝功能检测"], liver);

        let blood = &map["血常规"];
        assert_eq!(&map["血常规五分类"], blood);
        assert_eq!(&map["血细胞分析"], blood);
        assert_eq!(&map["全血细胞计数"], blood);
        assert_eq!(&map["血液分析"], blood);

        let thyroid = &map["甲功三项"];
        assert_eq!(&map["甲功五项"], thyroid);
        assert_eq!(&map["甲状腺功能"], thyroid);
        assert_eq!(&map["甲状腺功能全套"], thyroid);

        let coag = &map["凝血四项"];
        assert_eq!(&map["凝血功能"], coag);
        assert_eq!(&map["凝血全套"], coag);
        assert_eq!(&map["凝血七项"], coag);

        let hbv = &map["乙肝五项"];
        assert_eq!(&map["乙肝两对半"], hbv);
        assert_eq!(&map["乙肝病毒DNA"], hbv);

        assert_ne!(&map["肝功能"], &map["肾功能"]);
        assert_ne!(&map["血常规"], &map["血脂四项"]);
        assert_ne!(&map["血常规"], &map["生化全套"]);
        assert_ne!(&map["尿常规"], &map["血常规"]);
        assert_ne!(&map["肝功能"], &map["生化全套"]);

        let mut categories: Vec<&String> = map.values().collect();
        categories.sort();
        categories.dedup();
        eprintln!(
            "  共 {} 个报告类型 → {} 个分类组",
            types.len(),
            categories.len()
        );
        assert_eq!(categories.len(), 9);
    }

    /// 模拟真实场景：多家医院的混乱检验项目名称 → 趋势归一化能否正确统一
    #[test]
    fn chaos_item_name_normalization() {
        let test_groups: Vec<(&str, Vec<&str>)> = vec![
            (
                "白细胞计数",
                vec!["WBC", "白细胞", "白细胞数", "白细胞总数", "白细胞记数"],
            ),
            (
                "丙氨酸氨基转移酶",
                vec!["ALT", "谷丙转氨酶", "丙氨酸转氨酶"],
            ),
            (
                "天门冬氨酸氨基转移酶",
                vec!["AST", "谷草转氨酶", "天冬氨酸转氨酶"],
            ),
            (
                "超敏C反应蛋白",
                vec![
                    "hs-CRP",
                    "超敏C反应蛋白",
                    "高敏C反应蛋白",
                    "超高敏C反应蛋白",
                    "hsCRP",
                ],
            ),
            (
                "C反应蛋白",
                vec!["CRP", "C反应蛋白", "C-反应蛋白", "常规C反应蛋白"],
            ),
            (
                "乙肝病毒DNA",
                vec!["HBV-DNA", "HBV_DNA", "乙型肝炎病毒DNA", "乙肝病毒DNA"],
            ),
            (
                "乙肝表面抗原",
                vec!["HBsAg", "乙肝表面抗原", "乙型肝炎表面抗原"],
            ),
        ];

        eprintln!("\n===== 项目名称归一化结果 =====");
        let mut total = 0;
        let mut correct = 0;

        for (expected, variants) in &test_groups {
            for variant in variants {
                let result = normalize_trend_item_name(variant);
                let ok = result == *expected;
                total += 1;
                if ok {
                    correct += 1;
                }
                let msg = if ok {
                    String::new()
                } else {
                    format!("(期望: {})", expected)
                };
                eprintln!(
                    "  {} {:30} → {:30} {}",
                    if ok { "✓" } else { "✗" },
                    variant,
                    result,
                    msg
                );
            }
        }

        let crp = normalize_trend_item_name("CRP");
        let hscrp = normalize_trend_item_name("hs-CRP");
        assert_ne!(crp, hscrp, "CRP ≠ hs-CRP");

        let tni = normalize_trend_item_name("心肌肌钙蛋白I");
        let hs_tni = normalize_trend_item_name("高敏心肌肌钙蛋白I");
        assert_ne!(tni, hs_tni, "心肌肌钙蛋白I ≠ 高敏心肌肌钙蛋白I");

        let accuracy = correct as f64 / total as f64 * 100.0;
        eprintln!("\n  准确率: {}/{} = {:.1}%", correct, total, accuracy);
        assert!(
            accuracy >= 99.0,
            "归一化准确率 {:.1}% 未达到 99% 目标",
            accuracy
        );
    }

    // --- search index blob matching ---

    fn make_search_blob(name: &str, phone: &str, id_number: &str) -> String {
        let name_lower = name.to_lowercase();
        let pinyin_full = to_pinyin_string(name);
        let pinyin_init = to_pinyin_initials(name);
        format!(
            "{}\t{}\t{}\t{}\t{}",
            name_lower,
            pinyin_full,
            pinyin_init,
            phone.to_lowercase(),
            id_number.to_lowercase(),
        )
    }

    #[test]
    fn search_blob_original_text() {
        let blob = make_search_blob("张三", "", "");
        assert!(blob.contains("张三"));
    }

    #[test]
    fn search_blob_full_pinyin() {
        let blob = make_search_blob("张三", "", "");
        assert!(blob.contains("zhangsan"));
        assert!(blob.contains("zhang"));
    }

    #[test]
    fn search_blob_initials() {
        let blob = make_search_blob("张三", "", "");
        assert!(blob.contains("zs"));
    }

    #[test]
    fn search_blob_case_insensitive() {
        let blob = make_search_blob("张三", "", "");
        assert!(blob.contains("zs"));
        assert!(blob.contains(&"ZS".to_lowercase()));
    }

    #[test]
    fn search_blob_no_match() {
        let blob = make_search_blob("张三", "", "");
        assert!(!blob.contains("lisi"));
    }

    #[test]
    fn pinyin_helpers_basic() {
        assert_eq!(to_pinyin_string("白细胞"), "baixibao");
        assert_eq!(to_pinyin_initials("白细胞"), "bxb");
        assert_eq!(to_pinyin_string("张三"), "zhangsan");
        assert_eq!(to_pinyin_initials("张三"), "zs");
    }

    #[test]
    fn pinyin_helpers_mixed_chars() {
        assert_eq!(to_pinyin_string("C反应蛋白"), "cfanyingdanbai");
        assert_eq!(to_pinyin_initials("C反应蛋白"), "cfydb");
    }
}
