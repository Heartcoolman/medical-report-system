use crate::crypto;
use crate::error::AppError;
use crate::models::{
    DailyExpense, DailyExpenseDetail, DailyExpenseSummary, EditLog, ExpenseCategory, ExpenseItem, Gender,
    ItemStatus, PaginatedList, Patient, PatientWithStats, Report, ReportSummary,
    TemperatureRecord, TestItem, TrendItemInfo, TrendPoint,
};
use pinyin::ToPinyin;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::path::Path;

const DEFAULT_PAGE_SIZE: usize = 20;

#[derive(Clone)]
pub struct Database {
    pub db: Arc<Mutex<Connection>>,
}

/// Input for batch report creation
pub struct BatchReportInput {
    /// If merging into existing report, set to Some(existing_report_id)
    pub existing_report_id: Option<String>,
    /// The new Report object (only set when creating new, None when merging)
    pub new_report: Option<Report>,
    /// Test items to create
    pub items: Vec<TestItem>,
}

/// Convert a Chinese string to its full pinyin representation (lowercase, no spaces).
/// Non-Chinese characters are kept as-is.
fn to_pinyin_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c.to_pinyin() {
            Some(pinyin) => result.push_str(pinyin.plain()),
            None => result.push(c.to_ascii_lowercase()),
        }
    }
    result
}

/// Convert a Chinese string to its pinyin initials (first letter of each character's pinyin).
/// Non-Chinese characters are kept as-is.
fn to_pinyin_initials(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c.to_pinyin() {
            Some(pinyin) => {
                if let Some(first) = pinyin.plain().chars().next() {
                    result.push(first);
                }
            }
            None => result.push(c.to_ascii_lowercase()),
        }
    }
    result
}

fn gender_to_db(gender: &Gender) -> &'static str {
    match gender {
        Gender::Male => "男",
        Gender::Female => "女",
    }
}

fn parse_gender(value: &str) -> Gender {
    match value {
        "男" => Gender::Male,
        "女" => Gender::Female,
        _ => Gender::Male,
    }
}

fn status_to_db(status: &ItemStatus) -> &'static str {
    match status {
        ItemStatus::CriticalHigh => "CriticalHigh",
        ItemStatus::Normal => "Normal",
        ItemStatus::High => "High",
        ItemStatus::Low => "Low",
        ItemStatus::CriticalLow => "CriticalLow",
    }
}

fn parse_status(value: &str) -> ItemStatus {
    match value.trim().to_lowercase().as_str() {
        "critical_high" | "criticalhigh" => ItemStatus::CriticalHigh,
        "high" => ItemStatus::High,
        "low" => ItemStatus::Low,
        "critical_low" | "criticallow" => ItemStatus::CriticalLow,
        _ => ItemStatus::Normal,
    }
}

fn has_value_comparator_prefix(value: &str) -> bool {
    matches!(
        value.trim().chars().next(),
        Some('<' | '>' | '≤' | '≥' | '＜' | '＞')
    )
}

fn backfill_comparator_statuses(conn: &Connection) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, value, reference_range, status
         FROM test_items
         WHERE reference_range <> ''",
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut updated = 0usize;
    for (id, value, reference_range, raw_status) in rows {
        if !has_value_comparator_prefix(&value) {
            continue;
        }

        let fallback = parse_status(&raw_status);
        let computed = crate::ocr::parser::determine_status_from_value_text(
            &value,
            &reference_range,
            fallback,
        );

        if computed != fallback {
            conn.execute(
                "UPDATE test_items SET status = ?1 WHERE id = ?2",
                params![status_to_db(&computed), id],
            )?;
            updated += 1;
        }
    }

    if updated > 0 {
        tracing::info!("修正了 {} 条比较符项目状态", updated);
    }

    Ok(())
}

fn category_to_db(category: &ExpenseCategory) -> &'static str {
    match category {
        ExpenseCategory::Drug => "drug",
        ExpenseCategory::Test => "test",
        ExpenseCategory::Treatment => "treatment",
        ExpenseCategory::Material => "material",
        ExpenseCategory::Nursing => "nursing",
        ExpenseCategory::Other => "other",
    }
}

fn parse_category(value: &str) -> ExpenseCategory {
    match value.to_lowercase().as_str() {
        "drug" => ExpenseCategory::Drug,
        "test" => ExpenseCategory::Test,
        "treatment" => ExpenseCategory::Treatment,
        "material" => ExpenseCategory::Material,
        "nursing" => ExpenseCategory::Nursing,
        _ => ExpenseCategory::Other,
    }
}

/// Encrypt a patient field if encryption is enabled. Returns plaintext if not.
fn encrypt_patient_field(value: &str) -> Result<String, AppError> {
    if crypto::encryption_enabled() {
        crypto::encrypt_field(value).map_err(|e| AppError::Internal(e))
    } else {
        Ok(value.to_string())
    }
}

/// Decrypt a patient field. Passes through plaintext if not encrypted.
fn decrypt_patient_field(value: &str) -> String {
    crypto::decrypt_field(value).unwrap_or_else(|_| value.to_string())
}

/// Build a Patient from a row, decrypting sensitive fields.
fn patient_from_row(row: &rusqlite::Row) -> rusqlite::Result<Patient> {
    Ok(Patient {
        id: row.get(0)?,
        name: row.get(1)?,
        gender: parse_gender(&row.get::<_, String>(2)?),
        dob: row.get(3)?,
        phone: decrypt_patient_field(&row.get::<_, String>(4)?),
        id_number: decrypt_patient_field(&row.get::<_, String>(5)?),
        notes: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

impl Database {
    pub fn new(path: &str) -> Result<Self, AppError> {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
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
            "#,
        )?;

        backfill_comparator_statuses(&conn)?;

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T, AppError>) -> Result<T, AppError> {
        let mut conn = self
            .db
            .lock()
            .map_err(|_| AppError::Internal("数据库连接锁获取失败".to_string()))?;
        f(&mut conn)
    }

    /// Migrate unencrypted patient sensitive fields (phone, id_number) to encrypted form.
    /// Called at startup when DB_ENCRYPTION_KEY is configured.
    pub fn migrate_encrypt_sensitive_fields(&self) -> Result<usize, AppError> {
        if !crypto::encryption_enabled() {
            return Ok(0);
        }
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, phone, id_number FROM patients"
            )?;
            let rows: Vec<(String, String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let mut migrated = 0usize;
            for (id, phone, id_number) in &rows {
                let phone_needs = !phone.is_empty() && !crypto::is_encrypted(phone);
                let id_needs = !id_number.is_empty() && !crypto::is_encrypted(id_number);
                if !phone_needs && !id_needs {
                    continue;
                }
                let enc_phone = if phone_needs {
                    crypto::encrypt_field(phone).map_err(|e| AppError::Internal(e))?
                } else {
                    phone.clone()
                };
                let enc_id = if id_needs {
                    crypto::encrypt_field(id_number).map_err(|e| AppError::Internal(e))?
                } else {
                    id_number.clone()
                };
                conn.execute(
                    "UPDATE patients SET phone = ?1, id_number = ?2 WHERE id = ?3",
                    params![enc_phone, enc_id, id],
                )?;
                migrated += 1;
            }
            Ok(migrated)
        })
    }

    // --- Patient CRUD ---

    pub fn create_patient(&self, patient: &Patient) -> Result<(), AppError> {
        let enc_phone = encrypt_patient_field(&patient.phone)?;
        let enc_id_number = encrypt_patient_field(&patient.id_number)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO patients (id, name, gender, dob, phone, id_number, notes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    patient.id,
                    patient.name,
                    gender_to_db(&patient.gender),
                    patient.dob,
                    enc_phone,
                    enc_id_number,
                    patient.notes,
                    patient.created_at,
                    patient.updated_at
                ],
            )?;
            self_upsert_search_index(conn, patient)?;
            Ok(())
        })
    }

    pub fn get_patient(&self, id: &str) -> Result<Option<Patient>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, gender, dob, phone, id_number, notes, created_at, updated_at
                 FROM patients WHERE id = ?1",
            )?;
            stmt.query_row([id], patient_from_row)
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn update_patient(&self, patient: &Patient) -> Result<(), AppError> {
        let enc_phone = encrypt_patient_field(&patient.phone)?;
        let enc_id_number = encrypt_patient_field(&patient.id_number)?;
        self.with_conn(|conn| {
            let affected = conn.execute(
                "UPDATE patients
                 SET name = ?2, gender = ?3, dob = ?4, phone = ?5, id_number = ?6, notes = ?7, updated_at = ?8
                 WHERE id = ?1",
                params![
                    patient.id,
                    patient.name,
                    gender_to_db(&patient.gender),
                    patient.dob,
                    enc_phone,
                    enc_id_number,
                    patient.notes,
                    patient.updated_at
                ],
            )?;
            if affected == 0 {
                return Err(AppError::NotFound("患者不存在".to_string()));
            }
            self_upsert_search_index(conn, patient)?;
            Ok(())
        })
    }

    pub fn delete_patient(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;

            let affected = tx.execute("DELETE FROM patients WHERE id = ?1", params![id])?;
            if affected == 0 {
                return Err(AppError::NotFound("患者不存在".to_string()));
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn list_patients(&self) -> Result<Vec<Patient>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, gender, dob, phone, id_number, notes, created_at, updated_at
                 FROM patients ORDER BY id",
            )?;
            let items = stmt
                .query_map([], patient_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(items)
        })
    }

    pub fn list_patients_paginated(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<PaginatedList<Patient>, AppError> {
        let page_size = if page_size == 0 {
            DEFAULT_PAGE_SIZE
        } else if page_size > 100 {
            100
        } else {
            page_size
        };
        let page = if page == 0 { 1 } else { page };
        let skip = (page - 1) * page_size;

        self.with_conn(|conn| {
            let total: usize = conn
                .query_row("SELECT COUNT(*) FROM patients", [], |row| row.get::<_, i64>(0))?
                .try_into()
                .unwrap_or(0);

            let mut stmt = conn.prepare(
                "SELECT id, name, gender, dob, phone, id_number, notes, created_at, updated_at
                 FROM patients ORDER BY created_at ASC, id ASC LIMIT ?1 OFFSET ?2",
            )?;
            let items = stmt
                .query_map(params![page_size as i64, skip as i64], patient_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(PaginatedList {
                items,
                total,
                page,
                page_size,
            })
        })
    }

    pub fn search_patients(&self, query: &str) -> Result<Vec<Patient>, AppError> {
        self.with_conn(|conn| {
            let pattern = format!("%{}%", query.to_lowercase());
            let mut stmt = conn.prepare(
                "SELECT p.id, p.name, p.gender, p.dob, p.phone, p.id_number, p.notes, p.created_at, p.updated_at
                 FROM patients p
                 JOIN patient_search s ON p.id = s.patient_id
                 WHERE s.search_blob LIKE ?1
                 ORDER BY p.id",
            )?;
            let patients = stmt
                .query_map([pattern], patient_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(patients)
        })
    }

    /// Enrich a list of patients with report stats (report_count, last_report_date, total_abnormal).
    fn enrich_patients_with_stats(
        &self,
        patients: Vec<Patient>,
    ) -> Result<Vec<PatientWithStats>, AppError> {
        self.with_conn(|conn| {
            let mut result = Vec::with_capacity(patients.len());
            for patient in patients {
                let (report_count, last_report_date): (i64, Option<String>) = conn.query_row(
                    "SELECT COUNT(*), MAX(report_date) FROM reports WHERE patient_id = ?1",
                    params![patient.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let total_abnormal: i64 = conn.query_row(
                    "SELECT COUNT(*)
                     FROM test_items ti
                     JOIN reports r ON ti.report_id = r.id
                     WHERE r.patient_id = ?1 AND LOWER(ti.status) <> 'normal'",
                    params![patient.id],
                    |row| row.get(0),
                )?;

                result.push(PatientWithStats {
                    patient,
                    report_count: report_count as usize,
                    last_report_date: last_report_date.unwrap_or_default(),
                    total_abnormal: total_abnormal as usize,
                });
            }
            Ok(result)
        })
    }

    pub fn list_patients_with_stats_paginated(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<PaginatedList<PatientWithStats>, AppError> {
        let base = self.list_patients_paginated(page, page_size)?;
        let items = self.enrich_patients_with_stats(base.items)?;
        Ok(PaginatedList {
            items,
            total: base.total,
            page: base.page,
            page_size: base.page_size,
        })
    }

    pub fn search_patients_with_stats(&self, query: &str) -> Result<Vec<PatientWithStats>, AppError> {
        let patients = self.search_patients(query)?;
        self.enrich_patients_with_stats(patients)
    }

    // --- Temperature CRUD ---

    pub fn create_temperature(&self, record: &TemperatureRecord) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO temperature_records (id, patient_id, recorded_at, value, note, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.id,
                    record.patient_id,
                    record.recorded_at,
                    record.value,
                    record.note,
                    record.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_temperatures_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<TemperatureRecord>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, patient_id, recorded_at, value, note, created_at
                 FROM temperature_records
                 WHERE patient_id = ?1
                 ORDER BY recorded_at ASC, id ASC",
            )?;
            let records = stmt
                .query_map([patient_id], |row| {
                    Ok(TemperatureRecord {
                        id: row.get(0)?,
                        patient_id: row.get(1)?,
                        recorded_at: row.get(2)?,
                        value: row.get(3)?,
                        note: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(records)
        })
    }

    pub fn delete_temperature(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM temperature_records WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    // --- Report CRUD ---

    pub fn create_report(&self, report: &Report) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO reports (id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    report.id,
                    report.patient_id,
                    report.report_type,
                    report.hospital,
                    report.report_date,
                    report.sample_date,
                    report.file_path,
                    report.created_at
                ],
            )?;
            Ok(())
        })
    }

    /// Check if a duplicate report exists (same patient + report_type + report_date)
    /// Returns the existing report if found.
    pub fn find_duplicate_report(
        &self,
        patient_id: &str,
        report_type: &str,
        report_date: &str,
    ) -> Result<Option<Report>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                 FROM reports
                 WHERE patient_id = ?1 AND report_date = ?2 AND report_type = ?3
                 LIMIT 1",
                params![patient_id, report_date, report_type],
                |row| {
                    Ok(Report {
                        id: row.get(0)?,
                        patient_id: row.get(1)?,
                        report_type: row.get(2)?,
                        hospital: row.get(3)?,
                        report_date: row.get(4)?,
                        sample_date: row.get(5)?,
                        file_path: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn get_report(&self, id: &str) -> Result<Option<Report>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                 FROM reports WHERE id = ?1",
                [id],
                |row| {
                    Ok(Report {
                        id: row.get(0)?,
                        patient_id: row.get(1)?,
                        report_type: row.get(2)?,
                        hospital: row.get(3)?,
                        report_date: row.get(4)?,
                        sample_date: row.get(5)?,
                        file_path: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn update_report(&self, report: &Report) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                "UPDATE reports
                 SET patient_id = ?2, report_type = ?3, hospital = ?4, report_date = ?5, sample_date = ?6, file_path = ?7
                 WHERE id = ?1",
                params![
                    report.id,
                    report.patient_id,
                    report.report_type,
                    report.hospital,
                    report.report_date,
                    report.sample_date,
                    report.file_path
                ],
            )?;
            if affected == 0 {
                return Err(AppError::NotFound("报告不存在".to_string()));
            }
            Ok(())
        })
    }

    pub fn list_reports_by_patient(&self, patient_id: &str) -> Result<Vec<Report>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                 FROM reports
                 WHERE patient_id = ?1
                 ORDER BY report_date ASC, id ASC",
            )?;
            let reports = stmt
                .query_map([patient_id], |row| {
                    Ok(Report {
                        id: row.get(0)?,
                        patient_id: row.get(1)?,
                        report_type: row.get(2)?,
                        hospital: row.get(3)?,
                        report_date: row.get(4)?,
                        sample_date: row.get(5)?,
                        file_path: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(reports)
        })
    }

    pub fn list_reports_with_summary_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<ReportSummary>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        self.with_conn(|conn| {
            let mut summaries = Vec::with_capacity(reports.len());
            let mut stmt = conn.prepare(
                "SELECT id, name, status FROM test_items WHERE report_id = ?1 ORDER BY id",
            )?;
            for report in reports {
                let mut item_count = 0usize;
                let mut abnormal_count = 0usize;
                let mut abnormal_names = Vec::new();

                let rows = stmt.query_map([&report.id], |row| {
                    let name: String = row.get(1)?;
                    let status: String = row.get(2)?;
                    Ok((name, parse_status(&status)))
                })?;
                for row in rows {
                    let (name, status) = row?;
                    item_count += 1;
                    if status.is_abnormal() {
                        abnormal_count += 1;
                        abnormal_names.push(name);
                    }
                }

                summaries.push(ReportSummary {
                    report,
                    item_count,
                    abnormal_count,
                    abnormal_names,
                });
            }

            Ok(summaries)
        })
    }

    /// List reports and their raw item names for a patient.
    /// Used by suggest-groups to avoid N times DB tree re-open + spawn_blocking overhead.
    pub fn list_reports_with_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<(Report, Vec<String>)>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        self.with_conn(|conn| {
            let mut result = Vec::with_capacity(reports.len());
            let mut stmt = conn.prepare("SELECT name FROM test_items WHERE report_id = ?1 ORDER BY id")?;

            for report in reports {
                let mut item_names = Vec::new();
                let rows = stmt.query_map([&report.id], |row| row.get::<_, String>(0))?;
                for row in rows {
                    item_names.push(row?);
                }
                result.push((report, item_names));
            }

            Ok(result)
        })
    }

    /// List canonical names that already exist in this patient's reports.
    pub fn list_canonical_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<String>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT t.canonical_name
                 FROM test_items t
                 INNER JOIN reports r ON t.report_id = r.id
                 WHERE r.patient_id = ?1 AND t.canonical_name <> ''
                 ORDER BY t.canonical_name ASC",
            )?;

            let rows = stmt.query_map([patient_id], |row| row.get::<_, String>(0))?;
            let names = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(names)
        })
    }

    /// Get all unique test item names for a patient, with count of data points.
    pub fn list_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<(String, usize)>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT CASE WHEN canonical_name = '' THEN name ELSE canonical_name END AS effective_name,
                        COUNT(1) AS count
                 FROM test_items ti
                 INNER JOIN reports r ON ti.report_id = r.id
                 WHERE r.patient_id = ?1
                 GROUP BY effective_name
                 ORDER BY count DESC, effective_name ASC",
            )?;

            let rows = stmt.query_map([patient_id], |row| {
                let name: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((name, count as usize))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    fn list_report_ids_by_patient(&self, patient_id: &str) -> Result<Vec<String>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM reports WHERE patient_id = ?1 ORDER BY report_date ASC, id ASC",
            )?;
            let rows = stmt.query_map([patient_id], |row| row.get(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn delete_report(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM test_items WHERE report_id = ?1", params![id])?;
            tx.execute("DELETE FROM reports WHERE id = ?1", params![id])?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn delete_report_with_index_cleanup(&self, id: &str) -> Result<(), AppError> {
        self.delete_report(id)
    }

    // --- TestItem CRUD ---

    pub fn create_test_item(&self, item: &TestItem) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO test_items (id, report_id, name, value, unit, reference_range, status, canonical_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    item.id,
                    item.report_id,
                    item.name,
                    item.value,
                    item.unit,
                    item.reference_range,
                    status_to_db(&item.status),
                    item.canonical_name
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_test_items_by_report(&self, report_id: &str) -> Result<Vec<TestItem>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                 FROM test_items
                 WHERE report_id = ?1
                 ORDER BY id",
            )?;
            let rows = stmt.query_map([report_id], |row| {
                let mut item = TestItem {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    name: row.get(2)?,
                    value: row.get(3)?,
                    unit: row.get(4)?,
                    reference_range: row.get(5)?,
                    status: parse_status(&row.get::<_, String>(6)?),
                    canonical_name: row.get(7)?,
                };

                Ok(item)
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn get_test_item(&self, id: &str) -> Result<Option<TestItem>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                 FROM test_items
                 WHERE id = ?1",
                [id],
                |row| {
                    Ok(TestItem {
                        id: row.get(0)?,
                        report_id: row.get(1)?,
                        name: row.get(2)?,
                        value: row.get(3)?,
                        unit: row.get(4)?,
                        reference_range: row.get(5)?,
                        status: parse_status(&row.get::<_, String>(6)?),
                        canonical_name: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn update_test_item(&self, item: &TestItem) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                "UPDATE test_items
                 SET report_id = ?2, name = ?3, value = ?4, unit = ?5, reference_range = ?6, status = ?7, canonical_name = ?8
                 WHERE id = ?1",
                params![
                    item.id,
                    item.report_id,
                    item.name,
                    item.value,
                    item.unit,
                    item.reference_range,
                    status_to_db(&item.status),
                    item.canonical_name,
                ],
            )?;
            if affected == 0 {
                return Err(AppError::NotFound("检验项目不存在".to_string()));
            }
            Ok(())
        })
    }

    pub fn delete_test_item(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM test_items WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .optional()?;
            if existing.is_none() {
                return Err(AppError::NotFound("检验项目不存在".to_string()));
            }
            conn.execute("DELETE FROM test_items WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    // --- Edit Log ---

    pub fn create_edit_log(&self, log: &EditLog) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO edit_logs (id, report_id, patient_id, action, target_type, target_id, summary, changes, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    log.id,
                    log.report_id,
                    log.patient_id,
                    log.action,
                    log.target_type,
                    log.target_id,
                    log.summary,
                    serde_json::to_string(&log.changes)?,
                    log.created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_edit_logs_by_report(&self, report_id: &str) -> Result<Vec<EditLog>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, report_id, patient_id, action, target_type, target_id,
                        summary, changes, created_at
                 FROM edit_logs
                 WHERE report_id = ?1
                 ORDER BY created_at DESC, id DESC",
            )?;
            let rows = stmt.query_map([report_id], |row| {
                let changes_text: String = row.get(7)?;
                Ok(EditLog {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    patient_id: row.get(2)?,
                    action: row.get(3)?,
                    target_type: row.get(4)?,
                    target_id: row.get(5)?,
                    summary: row.get(6)?,
                    changes: serde_json::from_str(&changes_text).unwrap_or_default(),
                    created_at: row.get(8)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn list_edit_logs_global(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<PaginatedList<EditLog>, AppError> {
        let page_size = if page_size == 0 {
            DEFAULT_PAGE_SIZE
        } else if page_size > 100 {
            100
        } else {
            page_size
        };
        let page = if page == 0 { 1 } else { page };
        let skip = (page - 1) * page_size;

        self.with_conn(|conn| {
            let total: usize = conn
                .query_row("SELECT COUNT(*) FROM edit_logs", [], |row| row.get::<_, i64>(0))?
                .try_into()
                .unwrap_or(0);

            let mut stmt = conn.prepare(
                "SELECT id, report_id, patient_id, action, target_type, target_id,
                        summary, changes, created_at
                 FROM edit_logs
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt.query_map(params![page_size as i64, skip as i64], |row| {
                let changes_text: String = row.get(7)?;
                Ok(EditLog {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    patient_id: row.get(2)?,
                    action: row.get(3)?,
                    target_type: row.get(4)?,
                    target_id: row.get(5)?,
                    summary: row.get(6)?,
                    changes: serde_json::from_str(&changes_text).unwrap_or_default(),
                    created_at: row.get(8)?,
                })
            })?;

            let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(PaginatedList {
                items,
                total,
                page,
                page_size,
            })
        })
    }

    #[allow(dead_code)]
    fn delete_test_items_by_report(&self, report_id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM test_items WHERE report_id = ?1", [report_id])?;
            Ok(())
        })
    }

    /// Batch create reports and test items in a single atomic operation.
    pub fn batch_create_reports_and_items(
        &self,
        _patient_id: &str,
        inputs: Vec<BatchReportInput>,
    ) -> Result<Vec<(Report, Vec<TestItem>)>, AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let mut results = Vec::new();

            for input in &inputs {
                let (_report_id, report_obj, current_items) = if let Some(ref eid) =
                    input.existing_report_id
                {
                    let report = tx
                        .query_row(
                            "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                             FROM reports WHERE id = ?1",
                            [eid.as_str()],
                            |row| {
                                Ok(Report {
                                    id: row.get(0)?,
                                    patient_id: row.get(1)?,
                                    report_type: row.get(2)?,
                                    hospital: row.get(3)?,
                                    report_date: row.get(4)?,
                                    sample_date: row.get(5)?,
                                    file_path: row.get(6)?,
                                    created_at: row.get(7)?,
                                })
                            },
                        )
                        .optional()?;
                    let report = report.ok_or_else(|| {
                        AppError::NotFound("报告不存在".to_string())
                    })?;

                    let mut stmt = tx.prepare(
                        "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                         FROM test_items WHERE report_id = ?1 ORDER BY id",
                    )?;
                    let rows = stmt.query_map([&report.id], |row| {
                        Ok(TestItem {
                            id: row.get(0)?,
                            report_id: row.get(1)?,
                            name: row.get(2)?,
                            value: row.get(3)?,
                            unit: row.get(4)?,
                            reference_range: row.get(5)?,
                            status: parse_status(&row.get::<_, String>(6)?),
                            canonical_name: row.get(7)?,
                        })
                    })?;
                    let current_items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
                    (report.id.clone(), report, current_items)
                } else if let Some(ref report) = input.new_report {
                    tx.execute(
                        "INSERT INTO reports (id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            report.id,
                            report.patient_id,
                            report.report_type,
                            report.hospital,
                            report.report_date,
                            report.sample_date,
                            report.file_path,
                            report.created_at
                        ],
                    )?;
                    (report.id.clone(), report.clone(), Vec::new())
                } else {
                    continue;
                };

                let mut existing_names: HashSet<String> = current_items
                    .iter()
                    .map(|item| item.name.clone())
                    .collect();
                let mut merged_items = current_items;

                for item in &input.items {
                    if existing_names.contains(&item.name) {
                        continue;
                    }
                    tx.execute(
                        "INSERT INTO test_items (id, report_id, name, value, unit, reference_range, status, canonical_name)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            item.id,
                            item.report_id,
                            item.name,
                            item.value,
                            item.unit,
                            item.reference_range,
                            status_to_db(&item.status),
                            item.canonical_name
                        ],
                    )?;
                    existing_names.insert(item.name.clone());
                    merged_items.push(item.clone());
                }

                results.push((report_obj, merged_items));
            }

            tx.commit()?;
            Ok(results)
        })
    }

    pub fn get_trends(
        &self,
        patient_id: &str,
        item_name: &str,
        report_type: Option<&str>,
    ) -> Result<Vec<TrendPoint>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        let target_name_key = normalize_trend_item_name(item_name);

        self.with_conn(|conn| {
            let mut name_cache: HashMap<String, String> = HashMap::new();
            let mut seen_dates: HashSet<String> = HashSet::new();
            let mut points = Vec::new();

            let mut item_stmt = conn.prepare(
                "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name
                 FROM test_items WHERE report_id = ?1 ORDER BY id",
            )?;

            for report in reports {
                if let Some(rt) = report_type {
                    if !report.report_type.starts_with(rt) {
                        continue;
                    }
                }

                let effective_date = if report.sample_date.is_empty() {
                    report.report_date.clone()
                } else {
                    report.sample_date.clone()
                };
                if seen_dates.contains(&effective_date) {
                    continue;
                }

                let rows = item_stmt.query_map([&report.id], |row| {
                    let item = TestItem {
                        id: row.get(0)?,
                        report_id: row.get(1)?,
                        name: row.get(2)?,
                        value: row.get(3)?,
                        unit: row.get(4)?,
                        reference_range: row.get(5)?,
                        status: parse_status(&row.get::<_, String>(6)?),
                        canonical_name: row.get(7)?,
                    };
                    Ok(item)
                })?;

                for item in rows {
                    let item = item?;
                    let effective_name = if item.canonical_name.is_empty() {
                        item.name
                    } else {
                        item.canonical_name
                    };
                    let candidate_name_key = name_cache
                        .entry(effective_name.clone())
                        .or_insert_with(|| normalize_trend_item_name(&effective_name))
                        .clone();
                    if candidate_name_key != target_name_key {
                        continue;
                    }

                    points.push(TrendPoint {
                        report_date: report.report_date,
                        sample_date: report.sample_date,
                        value: item.value,
                        unit: item.unit,
                        status: item.status,
                        reference_range: item.reference_range,
                    });
                    seen_dates.insert(effective_date);
                    break;
                }
            }

            points.sort_by(|a, b| {
                let da = if a.sample_date.is_empty() {
                    &a.report_date
                } else {
                    &a.sample_date
                };
                let db = if b.sample_date.is_empty() {
                    &b.report_date
                } else {
                    &b.sample_date
                };
                da.cmp(db)
            });
            Ok(points)
        })
    }

    pub fn list_trend_items_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<TrendItemInfo>, AppError> {
        let report_ids = self.list_report_ids_by_patient(patient_id)?;
        if report_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            let mut report_types = Vec::with_capacity(report_ids.len());
            let mut report_info: HashMap<String, (String, String)> = HashMap::new();

            {
                let mut stmt = conn.prepare(
                    "SELECT id, report_type, report_date, sample_date FROM reports WHERE id = ?1",
                )?;
                for rid in &report_ids {
                    let (report_type, sample_date, report_date) = stmt.query_row([rid.as_str()], |row| {
                        let report_type: String = row.get(1)?;
                        let report_date: String = row.get(2)?;
                        let sample_date: String = row.get(3)?;
                        Ok((report_type, sample_date, report_date))
                    })?;
                    report_types.push(report_type.clone());
                    report_info.insert(
                        rid.clone(),
                        (report_type, if sample_date.is_empty() { report_date } else { sample_date }),
                    );
                }
            }

            let category_map = compute_report_categories(&report_types);
            let mut data_points: Vec<(String, String, String)> = Vec::new();
            let mut seen = HashSet::new();

            let mut item_stmt = conn.prepare(
                "SELECT report_id, name, canonical_name
                 FROM test_items
                 WHERE report_id = ?1
                 ORDER BY id",
            )?;

            for rid in &report_ids {
                let (report_type, trend_date) = report_info.get(rid).cloned().unwrap_or_default();
                let category = category_map
                    .get(&report_type)
                    .cloned()
                    .unwrap_or_else(|| report_type.clone());

                let rows = item_stmt.query_map([rid], |row| {
                    let name: String = row.get(1)?;
                    let canonical_name: String = row.get(2)?;
                    let effective = if canonical_name.is_empty() {
                        normalize_trend_item_name(&name)
                    } else {
                        normalize_trend_item_name(&canonical_name)
                    };
                    Ok((rid.clone(), effective))
                })?;

                for row in rows {
                    let (_report, effective_name) = row?;
                    let key = (category.clone(), trend_date.clone(), effective_name.clone());
                    if seen.insert(key) {
                        data_points.push((category.clone(), trend_date.clone(), effective_name));
                    }
                }
            }

            let mut counts: HashMap<(String, String), HashSet<String>> = HashMap::new();
            for (category, date, item_name) in data_points {
                counts
                    .entry((category, item_name))
                    .or_default()
                    .insert(date);
            }

            let mut result: Vec<TrendItemInfo> = counts
                .into_iter()
                .map(|((report_type, item_name), dates)| TrendItemInfo {
                    report_type,
                    item_name,
                    count: dates.len(),
                })
                .collect();

            result.sort_by(|a, b| {
                a.report_type
                    .cmp(&b.report_type)
                    .then(b.count.cmp(&a.count))
                    .then(a.item_name.cmp(&b.item_name))
            });

            Ok(result)
        })
    }

    // --- Report Interpretation Cache ---

    pub fn save_interpretation(&self, report_id: &str, content: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO report_interpretations (report_id, content, created_at)
                 VALUES (?1, ?2, datetime('now'))",
                params![report_id, content],
            )?;
            Ok(())
        })
    }

    pub fn get_interpretation(
        &self,
        report_id: &str,
    ) -> Result<Option<(String, String)>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT content, created_at FROM report_interpretations WHERE report_id = ?1",
                [report_id],
                |row| {
                    let content: String = row.get(0)?;
                    let created_at: String = row.get(1)?;
                    Ok((content, created_at))
                },
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn delete_interpretation(&self, report_id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM report_interpretations WHERE report_id = ?1", [report_id])?;
            Ok(())
        })
    }

    /// Return all items needed by normalization backfill.
    pub fn list_test_items_for_normalization(
        &self,
    ) -> Result<Vec<(TestItem, String)>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.report_id, t.name, t.value, t.unit, t.reference_range, t.status, t.canonical_name,
                        r.report_type
                 FROM test_items t
                 INNER JOIN reports r ON t.report_id = r.id
                 ORDER BY t.id",
            )?;

            let rows = stmt.query_map([], |row| {
                let item = TestItem {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    name: row.get(2)?,
                    value: row.get(3)?,
                    unit: row.get(4)?,
                    reference_range: row.get(5)?,
                    status: parse_status(&row.get::<_, String>(6)?),
                    canonical_name: row.get(7)?,
                };
                let report_type: String = row.get(8)?;
                Ok((item, report_type))
            })?;

            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn update_test_item_canonical_names(
        &self,
        updates: Vec<(String, String)>,
    ) -> Result<usize, AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let mut updated = 0usize;
            {
                let mut stmt = tx.prepare(
                    "UPDATE test_items SET canonical_name = ?1 WHERE id = ?2 AND canonical_name <> ?1",
                )?;
                for (id, canonical) in updates {
                    if stmt.execute([canonical.as_str(), id.as_str()])? > 0 {
                        updated += 1;
                    }
                }
            }
            tx.commit()?;
            Ok(updated)
        })
    }

    /// --- Daily Expense CRUD ---

    pub fn create_expense(
        &self,
        expense: &DailyExpense,
        items: &[ExpenseItem],
    ) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO daily_expenses (id, patient_id, expense_date, total_amount, drug_analysis, treatment_analysis, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    expense.id,
                    expense.patient_id,
                    expense.expense_date,
                    expense.total_amount,
                    expense.drug_analysis,
                    expense.treatment_analysis,
                    expense.created_at
                ],
            )?;

            {
                let mut stmt = tx.prepare(
                    "INSERT INTO expense_items (id, expense_id, name, category, quantity, amount, note)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;
                for item in items {
                    stmt.execute(params![
                        item.id,
                        expense.id,
                        item.name,
                        category_to_db(&item.category),
                        item.quantity,
                        item.amount,
                        item.note,
                    ])?;
                }
            }

            tx.commit()?;
            Ok(())
        })
    }

    pub fn list_expenses_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<DailyExpenseSummary>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, patient_id, expense_date, total_amount, drug_analysis,
                        treatment_analysis, created_at
                 FROM daily_expenses
                 WHERE patient_id = ?1
                 ORDER BY expense_date DESC, id DESC",
            )?;

            let expense_rows = stmt.query_map([patient_id], |row| {
                Ok(DailyExpense {
                    id: row.get(0)?,
                    patient_id: row.get(1)?,
                    expense_date: row.get(2)?,
                    total_amount: row.get(3)?,
                    drug_analysis: row.get(4)?,
                    treatment_analysis: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;

            let mut summaries = Vec::new();
            let mut item_stmt = conn.prepare(
                "SELECT id, expense_id, name, category, quantity, amount, note
                 FROM expense_items WHERE expense_id = ?1 ORDER BY id",
            )?;

            for expense in expense_rows {
                let expense = expense?;
                let item_rows = item_stmt.query_map([&expense.id], |row| {
                    Ok(ExpenseItem {
                        id: row.get(0)?,
                        expense_id: row.get(1)?,
                        name: row.get(2)?,
                        category: parse_category(&row.get::<_, String>(3)?),
                        quantity: row.get(4)?,
                        amount: row.get(5)?,
                        note: row.get(6)?,
                    })
                })?;

                let mut item_count = 0usize;
                let mut drug_count = 0usize;
                let mut test_count = 0usize;
                let mut treatment_count = 0usize;

                for item in item_rows {
                    item_count += 1;
                    match item?.category {
                        ExpenseCategory::Drug => drug_count += 1,
                        ExpenseCategory::Test => test_count += 1,
                        ExpenseCategory::Treatment => treatment_count += 1,
                        _ => {}
                    }
                }

                summaries.push(DailyExpenseSummary {
                    expense,
                    item_count,
                    drug_count,
                    test_count,
                    treatment_count,
                });
            }

            Ok(summaries)
        })
    }

    pub fn get_expense_detail(&self, id: &str) -> Result<Option<DailyExpenseDetail>, AppError> {
        self.with_conn(|conn| {
            let expense = conn
                .query_row(
                    "SELECT id, patient_id, expense_date, total_amount, drug_analysis,
                            treatment_analysis, created_at
                     FROM daily_expenses WHERE id = ?1",
                    [id],
                    |row| {
                        Ok(DailyExpense {
                            id: row.get(0)?,
                            patient_id: row.get(1)?,
                            expense_date: row.get(2)?,
                            total_amount: row.get(3)?,
                            drug_analysis: row.get(4)?,
                            treatment_analysis: row.get(5)?,
                            created_at: row.get(6)?,
                        })
                    },
                )
                .optional()?;

            let Some(expense) = expense else {
                return Ok(None);
            };

            let mut stmt = conn.prepare(
                "SELECT id, expense_id, name, category, quantity, amount, note
                 FROM expense_items WHERE expense_id = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map([id], |row| {
                Ok(ExpenseItem {
                    id: row.get(0)?,
                    expense_id: row.get(1)?,
                    name: row.get(2)?,
                    category: parse_category(&row.get::<_, String>(3)?),
                    quantity: row.get(4)?,
                    amount: row.get(5)?,
                    note: row.get(6)?,
                })
            })?;
            let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(Some(DailyExpenseDetail { expense, items }))
        })
    }

    pub fn delete_expense(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let exists: Option<String> = conn
                .query_row(
                    "SELECT id FROM daily_expenses WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(AppError::NotFound("消费记录不存在".to_string()));
            }

            let tx = conn.transaction()?;
            tx.execute("DELETE FROM expense_items WHERE expense_id = ?1", [id])?;
            tx.execute("DELETE FROM daily_expenses WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(())
        })
    }

}

fn self_upsert_search_index(conn: &Connection, patient: &Patient) -> Result<(), AppError> {
    let name_lower = patient.name.to_lowercase();
    let pinyin_full = to_pinyin_string(&patient.name);
    let pinyin_init = to_pinyin_initials(&patient.name);
    let search_blob = format!(
        "{}\t{}\t{}\t{}\t{}",
        name_lower,
        pinyin_full,
        pinyin_init,
        patient.phone.to_lowercase(),
        patient.id_number.to_lowercase(),
    );
    conn.execute(
        "INSERT INTO patient_search (patient_id, search_blob) VALUES (?1, ?2)
         ON CONFLICT(patient_id) DO UPDATE SET search_blob = excluded.search_blob",
        params![patient.id, search_blob],
    )?;
    Ok(())
}

// Fallback keyword rules for report type categorization.
// Used only when the authoritative taxonomy (`report_types.json`) has no match.
// Category names are aligned with the taxonomy for consistency.
const REPORT_CATEGORY_RULES: &[(&str, &str)] = &[
    ("脑脊液", "脑脊液检查"),
    ("尿常规", "尿常规检查"),
    ("尿液", "尿常规检查"),
    ("尿沉渣", "尿常规检查"),
    ("粪便", "粪便常规检查"),
    ("大便", "粪便常规检查"),
    ("血常规", "血常规检查"),
    ("血细胞", "血常规检查"),
    ("全血细胞", "血常规检查"),
    ("凝血", "凝血功能检查"),
    ("肝功", "肝功能检查"),
    ("肾功", "肾功能检查"),
    ("甲状腺", "甲状腺功能检查"),
    ("甲功", "甲状腺功能检查"),
    ("血脂", "血脂检查"),
    ("血糖", "血糖检查"),
    ("糖化", "血糖检查"),
    ("电解质", "电解质检查"),
    ("乙肝", "乙肝检查"),
    ("乙型肝炎", "乙肝检查"),
    ("HBV", "乙肝检查"),
    ("感染", "感染标志物检查"),
    ("免疫球蛋白", "免疫球蛋白检查"),
    ("补体", "免疫球蛋白检查"),
    ("血沉", "感染标志物检查"),
    ("红细胞沉降", "感染标志物检查"),
    ("血气", "血气分析检查"),
    ("生化", "生化检查"),
    ("肝纤维", "肝纤维化检查"),
    ("肿瘤", "肿瘤标志物检查"),
    ("白带", "体液检查"),
    ("心肌", "心肌标志物检查"),
    ("C反应蛋白", "感染标志物检查"),
    ("CRP", "感染标志物检查"),
];

/// Group similar report_types into categories.
///
/// 1. Try taxonomy dictionary (`report_types.json`).
/// 2. Fall back to keyword rules for types not covered by taxonomy.
/// 3. For still-unmatched types, use common-prefix grouping (min 3 Chinese chars).
fn compute_report_categories(report_types: &[String]) -> HashMap<String, String> {
    let mut mapping = HashMap::new();
    let mut unmatched: Vec<String> = Vec::new();

    let mut unique: Vec<&String> = report_types.iter().collect();
    unique.sort();
    unique.dedup();

    for rt in &unique {
        if let Some(cat) = crate::algorithm_engine::report_taxonomy::lookup_category_pub(rt) {
            mapping.insert((*rt).clone(), cat);
            continue;
        }
        let upper = rt.to_uppercase();
        let mut found = false;
        for &(keyword, category) in REPORT_CATEGORY_RULES {
            if rt.contains(keyword) || upper.contains(&keyword.to_uppercase()) {
                mapping.insert((*rt).clone(), category.to_string());
                found = true;
                break;
            }
        }
        if !found {
            unmatched.push((*rt).clone());
        }
    }

    unmatched.sort();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for rt in &unmatched {
        let mut matched = false;
        for group in groups.iter_mut() {
            let prefix: String = group
                .0
                .chars()
                .zip(rt.chars())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect();
            if prefix.chars().count() >= 3 {
                group.0 = prefix;
                group.1.push(rt.clone());
                matched = true;
                break;
            }
        }
        if !matched {
            groups.push((rt.clone(), vec![rt.clone()]));
        }
    }

    for (category, members) in groups {
        for member in members {
            mapping.insert(member, category.clone());
        }
    }

    mapping
}

/// Normalize common cross-hospital aliases for trend grouping.
/// Items are compared within the same report-type category, so stripping
/// category-specific prefixes (e.g. "脑脊液") is safe.
fn normalize_trend_item_name(name: &str) -> String {
    crate::algorithm_engine::name_normalizer::normalize_for_trend(name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let types = vec![
            "某某某检查A".to_string(),
            "某某某检查B".to_string(),
        ];
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
        assert_eq!(normalize_trend_item_name("超高敏C反应蛋白"), "超敏C反应蛋白");
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
        eprintln!("  共 {} 个报告类型 → {} 个分类组", types.len(), categories.len());
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
            ("丙氨酸氨基转移酶", vec!["ALT", "谷丙转氨酶", "丙氨酸转氨酶"]),
            (
                "天门冬氨酸氨基转移酶",
                vec!["AST", "谷草转氨酶", "天冬氨酸转氨酶"],
            ),
            (
                "超敏C反应蛋白",
                vec!["hs-CRP", "超敏C反应蛋白", "高敏C反应蛋白", "超高敏C反应蛋白", "hsCRP"],
            ),
            (
                "C反应蛋白",
                vec!["CRP", "C反应蛋白", "C-反应蛋白", "常规C反应蛋白"],
            ),
            ("乙肝病毒DNA", vec!["HBV-DNA", "HBV_DNA", "乙型肝炎病毒DNA", "乙肝病毒DNA"]),
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
        assert!(accuracy >= 99.0, "归一化准确率 {:.1}% 未达到 99% 目标", accuracy);
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
