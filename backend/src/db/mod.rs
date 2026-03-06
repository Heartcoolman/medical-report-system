mod assessment_repo;
mod edit_log_repo;
mod expense_repo;
mod file_repo;
pub mod helpers;
mod interpretation_repo;
pub mod medication_repo;
mod patient_repo;
mod refresh_token_repo;
mod report_repo;
mod risk_prediction_repo;
mod temperature_repo;
mod test_item_repo;
mod trend_repo;

use crate::error::AppError;
use crate::models::Report;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

use helpers::backfill_comparator_statuses;
use helpers::backfill_severity_statuses;

const POOL_MAX_SIZE: u32 = 8;
const POOL_CONNECTION_TIMEOUT_SECS: u64 = 10;

/// Input for batch report creation
pub struct BatchReportInput {
    /// If merging into existing report, set to Some(existing_report_id)
    pub existing_report_id: Option<String>,
    /// The new Report object (only set when creating new, None when merging)
    pub new_report: Option<Report>,
    /// Test items to create
    pub items: Vec<crate::models::TestItem>,
}

/// r2d2 connection manager for rusqlite
struct SqliteConnectionManager {
    path: PathBuf,
}

impl SqliteConnectionManager {
    fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl r2d2::ManageConnection for SqliteConnectionManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(&self.path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA cache_size = -20000;
            PRAGMA foreign_keys = ON;
            ",
        )?;
        Ok(conn)
    }

    fn is_valid(&self, conn: &mut Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch("SELECT 1")
    }

    fn has_broken(&self, _conn: &mut Connection) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct Database {
    pool: r2d2::Pool<SqliteConnectionManager>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, AppError> {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let manager = SqliteConnectionManager::new(path);
        let pool = r2d2::Pool::builder()
            .max_size(POOL_MAX_SIZE)
            .connection_timeout(std::time::Duration::from_secs(POOL_CONNECTION_TIMEOUT_SECS))
            .test_on_check_out(true)
            .build(manager)
            .map_err(|e| AppError::internal(format!("连接池初始化失败: {}", e)))?;

        // Run migrations on a dedicated connection
        let conn = pool
            .get()
            .map_err(|e| AppError::internal(format!("获取迁移连接失败: {}", e)))?;

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
                user_id TEXT,
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
            CREATE TABLE IF NOT EXISTS health_assessments (
                patient_id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS refresh_tokens (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                token_hash TEXT NOT NULL UNIQUE,
                device_name TEXT NOT NULL DEFAULT '',
                device_type TEXT NOT NULL DEFAULT '',
                ip_address TEXT NOT NULL DEFAULT '',
                user_agent TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                last_used_at TEXT NOT NULL,
                revoked INTEGER NOT NULL DEFAULT 0,
                replaced_by TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user
                ON refresh_tokens(user_id, revoked, expires_at);
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash
                ON refresh_tokens(token_hash);
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires
                ON refresh_tokens(expires_at);
            CREATE TABLE IF NOT EXISTS uploaded_files (
                id TEXT PRIMARY KEY,
                original_name TEXT NOT NULL,
                safe_name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                is_temporary INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS risk_predictions (
                patient_id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS rag_embeddings (
                id TEXT PRIMARY KEY,
                patient_id TEXT NOT NULL,
                chunk_type TEXT NOT NULL,
                source_id TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_rag_patient ON rag_embeddings(patient_id, chunk_type);
            CREATE TABLE IF NOT EXISTS patient_assignments (
                user_id TEXT NOT NULL,
                patient_id TEXT NOT NULL,
                assigned_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (user_id, patient_id),
                FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_patient_assignments_patient
                ON patient_assignments(patient_id);
            "#,
        )?;

        // Migration: add location column to temperature_records
        conn.execute_batch(
            "ALTER TABLE temperature_records ADD COLUMN location TEXT NOT NULL DEFAULT ''",
        )
        .ok();

        // Migration: add operator columns to edit_logs
        let _ = conn.execute("ALTER TABLE edit_logs ADD COLUMN operator_id TEXT", []);
        let _ = conn.execute("ALTER TABLE edit_logs ADD COLUMN operator_name TEXT", []);

        // Migration: rename siliconflow_api_key → zhipu_api_key (historical)
        let _ = conn.execute("ALTER TABLE user_api_keys RENAME COLUMN siliconflow_api_key TO zhipu_api_key", []);
        // Migration: rename zhipu_api_key → siliconflow_api_key
        let _ = conn.execute("ALTER TABLE user_api_keys RENAME COLUMN zhipu_api_key TO siliconflow_api_key", []);

        // Migration: add risk_level column to patients
        let _ = conn.execute("ALTER TABLE patients ADD COLUMN risk_level TEXT NOT NULL DEFAULT 'low'", []);

        // Migration: add third-party auth columns to users
        let _ = conn.execute("ALTER TABLE users ADD COLUMN wechat_openid TEXT", []);
        let _ = conn.execute("ALTER TABLE users ADD COLUMN apple_id TEXT", []);
        let _ = conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_users_wechat_openid ON users(wechat_openid) WHERE wechat_openid IS NOT NULL", []);
        let _ = conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_users_apple_id ON users(apple_id) WHERE apple_id IS NOT NULL", []);

        // Migration: make password_hash nullable for third-party-only accounts
        // SQLite doesn't support ALTER COLUMN, but new rows can insert '' for password_hash

        backfill_comparator_statuses(&conn)?;
        backfill_severity_statuses(&conn)?;

        Ok(Self { pool })
    }

    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut conn = self.pool.get().map_err(|e| {
            AppError::internal(format!("获取数据库连接失败: {}", e))
        })?;
        f(&mut conn)
    }

    /// Assign a patient to a user (for resource-level access control)
    pub fn assign_patient_to_user(&self, user_id: &str, patient_id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO patient_assignments (user_id, patient_id) VALUES (?1, ?2)",
                rusqlite::params![user_id, patient_id],
            )?;
            Ok(())
        })
    }

    /// Check if a user has access to a patient (Admin always has access)
    pub fn user_has_patient_access(&self, user_id: &str, patient_id: &str, role: &str) -> Result<bool, AppError> {
        if role == "admin" {
            return Ok(true);
        }
        self.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM patient_assignments WHERE user_id = ?1 AND patient_id = ?2",
                rusqlite::params![user_id, patient_id],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
    }

    /// Find a user by WeChat openid. Returns (id, username, role).
    pub fn find_user_by_wechat_openid(&self, openid: &str) -> Result<Option<(String, String, String)>, AppError> {
        self.with_conn(|conn| {
            let result = conn.query_row(
                "SELECT id, username, role FROM users WHERE wechat_openid = ?1",
                rusqlite::params![openid],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            );
            match result {
                Ok(tuple) => Ok(Some(tuple)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(AppError::from(e)),
            }
        })
    }

    /// Create a new user via WeChat login (no password).
    pub fn create_wechat_user(
        &self,
        user_id: &str,
        username: &str,
        openid: &str,
        role: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO users (id, username, password_hash, role, created_at, wechat_openid) VALUES (?1, ?2, '', ?3, ?4, ?5)",
                rusqlite::params![user_id, username, role, created_at, openid],
            )?;
            Ok(())
        })
    }

    /// Bind a WeChat openid to an existing user account.
    pub fn bind_wechat_openid(&self, user_id: &str, openid: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let existing: Option<String> = conn.query_row(
                "SELECT id FROM users WHERE wechat_openid = ?1 AND id != ?2",
                rusqlite::params![openid, user_id],
                |row| row.get(0),
            ).ok();
            if existing.is_some() {
                return Err(AppError::new(
                    crate::error::ErrorCode::AuthUsernameConflict,
                    "该微信账号已绑定其他用户",
                ));
            }
            conn.execute(
                "UPDATE users SET wechat_openid = ?1 WHERE id = ?2",
                rusqlite::params![openid, user_id],
            )?;
            Ok(())
        })
    }

    /// List patient IDs assigned to a user
    pub fn list_assigned_patient_ids(&self, user_id: &str) -> Result<Vec<String>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT patient_id FROM patient_assignments WHERE user_id = ?1",
            )?;
            let ids = stmt.query_map(rusqlite::params![user_id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?;
            Ok(ids)
        })
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

    // --- r2d2 connection pool integration tests ---

    use crate::models::Gender;

    fn make_test_db() -> super::Database {
        std::fs::create_dir_all("test_data").ok();
        let path = format!("test_data/test_{}.db", uuid::Uuid::new_v4());
        super::Database::new(&path).expect("test db init")
    }

    fn make_patient(name: &str, gender: Gender) -> crate::models::Patient {
        crate::models::Patient {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            gender,
            dob: "1990-01-01".into(),
            phone: format!("138{:08}", rand::random::<u32>() % 100000000),
            id_number: format!("110101{:012}", rand::random::<u64>() % 1000000000000),
            notes: "".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn pool_create_and_get_patient() {
        let db = make_test_db();
        let p = make_patient("张三", Gender::Male);
        db.create_patient(&p).unwrap();
        let found = db.get_patient(&p.id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "张三");
    }

    #[test]
    fn pool_update_and_delete_patient() {
        let db = make_test_db();
        let mut p = make_patient("李四", Gender::Female);
        db.create_patient(&p).unwrap();
        p.name = "李四改".into();
        db.update_patient(&p).unwrap();
        let found = db.get_patient(&p.id).unwrap().unwrap();
        assert_eq!(found.name, "李四改");

        db.delete_patient(&p.id).unwrap();
        assert!(db.get_patient(&p.id).unwrap().is_none());
    }

    #[test]
    fn pool_patient_assignments() {
        let db = make_test_db();
        let user_id = uuid::Uuid::new_v4().to_string();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![&user_id, "testuser", "hash", "doctor", "2024-01-01T00:00:00Z"],
            )?;
            Ok(())
        }).unwrap();

        let p = make_patient("测试", Gender::Male);
        let pid = p.id.clone();
        db.create_patient(&p).unwrap();

        assert!(!db.user_has_patient_access(&user_id, &pid, "doctor").unwrap());
        assert!(db.user_has_patient_access(&user_id, &pid, "admin").unwrap());

        db.assign_patient_to_user(&user_id, &pid).unwrap();
        assert!(db.user_has_patient_access(&user_id, &pid, "doctor").unwrap());
        assert!(db.list_assigned_patient_ids(&user_id).unwrap().contains(&pid));
    }

    #[test]
    fn pool_concurrent_access() {
        let db = make_test_db();
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let db = db.clone();
                std::thread::spawn(move || {
                    let p = make_patient(&format!("并发{}", i), Gender::Male);
                    db.create_patient(&p).unwrap();
                    assert!(db.get_patient(&p.id).unwrap().is_some());
                })
            })
            .collect();
        for h in handles {
            h.join().expect("thread join");
        }
    }
}
