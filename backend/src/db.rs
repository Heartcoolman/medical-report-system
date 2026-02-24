use crate::error::AppError;
use crate::models::{
    ConfirmExpenseItemReq, DailyExpense, DailyExpenseDetail, DailyExpenseSummary, EditLog,
    ExpenseCategory, ExpenseItem, ItemStatus, PaginatedList, Patient, PatientWithStats, Report,
    ReportSummary, TemperatureRecord, TestItem, TrendItemInfo, TrendPoint,
};
use pinyin::ToPinyin;
use sled::Db;
use sled::Transactional;
use std::sync::Arc;

const DEFAULT_PAGE_SIZE: usize = 20;

#[derive(Clone)]
pub struct Database {
    pub db: Arc<Db>,
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


impl Database {
    pub fn new(path: &str) -> Result<Self, AppError> {
        let db = sled::open(path)?;
        db.open_tree("patients")?;
        db.open_tree("reports")?;
        db.open_tree("test_items")?;
        db.open_tree("idx_patient_reports")?;
        db.open_tree("idx_report_items")?;
        db.open_tree("temperature_records")?;
        db.open_tree("idx_patient_temperatures")?;
        db.open_tree("report_interpretations")?;
        db.open_tree("idx_patient_search")?;
        db.open_tree("idx_patient_ordered")?;
        db.open_tree("edit_logs")?;
        db.open_tree("idx_report_edit_logs")?;
        db.open_tree("idx_edit_logs_ordered")?;
        db.open_tree("daily_expenses")?;
        db.open_tree("expense_items")?;
        db.open_tree("idx_patient_expenses")?;
        db.open_tree("idx_expense_items")?;
        Ok(Self { db: Arc::new(db) })
    }

    // --- Patient CRUD ---

    pub fn create_patient(&self, patient: &Patient) -> Result<(), AppError> {
        let tree = self.db.open_tree("patients")?;
        let val = serde_json::to_vec(patient)?;
        tree.insert(patient.id.as_bytes(), val)?;
        self.upsert_search_index(patient)?;
        self.upsert_ordered_index(patient)?;
        Ok(())
    }

    pub fn get_patient(&self, id: &str) -> Result<Option<Patient>, AppError> {
        let tree = self.db.open_tree("patients")?;
        match tree.get(id.as_bytes())? {
            Some(val) => {
                let p: Patient = serde_json::from_slice(&val)?;
                Ok(Some(p))
            }
            None => Ok(None),
        }
    }

    pub fn update_patient(&self, patient: &Patient) -> Result<(), AppError> {
        let tree = self.db.open_tree("patients")?;
        // Read old record to remove its ordered index entry
        if let Some(old_val) = tree.get(patient.id.as_bytes())? {
            let old: Patient = serde_json::from_slice(&old_val)?;
            let old_key = format!("{}:{}", old.created_at, old.id);
            let ord_idx = self.db.open_tree("idx_patient_ordered")?;
            ord_idx.remove(old_key.as_bytes())?;
        } else {
            return Err(AppError::NotFound("患者不存在".to_string()));
        }
        let val = serde_json::to_vec(patient)?;
        tree.insert(patient.id.as_bytes(), val)?;
        self.upsert_search_index(patient)?;
        self.upsert_ordered_index(patient)?;
        Ok(())
    }

    pub fn delete_patient(&self, id: &str) -> Result<(), AppError> {
        let tree = self.db.open_tree("patients")?;
        let idx = self.db.open_tree("idx_patient_reports")?;

        // Clean up search index and ordered index
        let search_idx = self.db.open_tree("idx_patient_search")?;
        search_idx.remove(id.as_bytes())?;
        // Remove ordered index entry
        if let Some(val) = tree.get(id.as_bytes())? {
            let p: Patient = serde_json::from_slice(&val)?;
            let ord_key = format!("{}:{}", p.created_at, p.id);
            let ord_idx = self.db.open_tree("idx_patient_ordered")?;
            ord_idx.remove(ord_key.as_bytes())?;
        }

        // Delete temperature records first
        self.delete_temperatures_by_patient(id)?;

        // Delete expense records
        self.delete_expenses_by_patient(id)?;

        // Collect report IDs first
        let report_ids = self.list_report_ids_by_patient(id)?;

        // Delete children first (reports and their items).
        // This order ensures that if a failure occurs midway, the parent still exists
        // and a retry can pick up the remaining children.
        for rid in report_ids {
            self.delete_report(&rid)?;
        }

        // Atomically clean up patient-report index entries and delete the patient record.
        // Using sled transaction to ensure these two steps are atomic — if the process
        // crashes between index cleanup and patient deletion, we won't leave orphan indexes.
        let id_bytes = id.as_bytes().to_vec();
        let prefix = format!("{}:", id);
        let idx_keys: Vec<Vec<u8>> = idx
            .scan_prefix(prefix.as_bytes())
            .filter_map(|entry| entry.ok().map(|(k, _)| k.to_vec()))
            .collect();

        (&tree, &idx)
            .transaction(|(tx_tree, tx_idx)| {
                for k in &idx_keys {
                    tx_idx.remove(k.as_slice())?;
                }
                tx_tree.remove(id_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("删除患者事务失败: {}", e))
            })?;

        Ok(())
    }

    pub fn list_patients(&self) -> Result<Vec<Patient>, AppError> {
        let tree = self.db.open_tree("patients")?;
        let mut patients = Vec::new();
        for entry in tree.iter() {
            let (_, val) = entry?;
            let p: Patient = serde_json::from_slice(&val)?;
            patients.push(p);
        }
        Ok(patients)
    }

    pub fn list_patients_paginated(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<PaginatedList<Patient>, AppError> {
        let tree = self.db.open_tree("patients")?;
        let ord_idx = self.db.open_tree("idx_patient_ordered")?;
        let page_size = if page_size == 0 {
            DEFAULT_PAGE_SIZE
        } else if page_size > 100 {
            100
        } else {
            page_size
        };
        let page = if page == 0 { 1 } else { page };
        let skip = (page - 1) * page_size;

        let total = ord_idx.len();
        // If ordered index is empty, fall back to patients tree (for backward compat)
        if total == 0 && tree.len() > 0 {
            let mut items = Vec::with_capacity(page_size);
            for entry in tree.iter().skip(skip) {
                if items.len() >= page_size {
                    break;
                }
                let (_, val) = entry?;
                let p: Patient = serde_json::from_slice(&val)?;
                items.push(p);
            }
            return Ok(PaginatedList {
                items,
                total: tree.len(),
                page,
                page_size,
            });
        }

        let mut items = Vec::with_capacity(page_size);
        for entry in ord_idx.iter().skip(skip) {
            if items.len() >= page_size {
                break;
            }
            let (_, patient_id) = entry?;
            if let Some(val) = tree.get(&patient_id)? {
                let p: Patient = serde_json::from_slice(&val)?;
                items.push(p);
            }
        }

        Ok(PaginatedList {
            items,
            total,
            page,
            page_size,
        })
    }

    pub fn search_patients(&self, query: &str) -> Result<Vec<Patient>, AppError> {
        let tree = self.db.open_tree("patients")?;
        let idx = self.db.open_tree("idx_patient_search")?;
        let q = query.to_lowercase();
        let mut patients = Vec::new();
        for entry in idx.iter() {
            let (k, v) = entry?;
            let search_text = String::from_utf8_lossy(&v);
            if search_text.contains(&q) {
                if let Some(val) = tree.get(&k)? {
                    let p: Patient = serde_json::from_slice(&val)?;
                    patients.push(p);
                }
            }
        }
        Ok(patients)
    }

    /// Write ordered index entry: key = "{created_at}:{id}", value = patient_id bytes.
    /// Sled stores keys in lexicographic order, so created_at prefix gives time-ordered iteration.
    fn upsert_ordered_index(&self, patient: &Patient) -> Result<(), AppError> {
        let idx = self.db.open_tree("idx_patient_ordered")?;
        let key = format!("{}:{}", patient.created_at, patient.id);
        idx.insert(key.as_bytes(), patient.id.as_bytes())?;
        Ok(())
    }

    /// Write pre-computed search terms (name, pinyin, initials, phone, id_number)
    /// into idx_patient_search for fast lookups.
    fn upsert_search_index(&self, patient: &Patient) -> Result<(), AppError> {
        let idx = self.db.open_tree("idx_patient_search")?;
        let name_lower = patient.name.to_lowercase();
        let pinyin_full = to_pinyin_string(&patient.name);
        let pinyin_init = to_pinyin_initials(&patient.name);
        let search_blob = format!(
            "{}\t{}\t{}\t{}\t{}",
            name_lower, pinyin_full, pinyin_init,
            patient.phone.to_lowercase(),
            patient.id_number.to_lowercase(),
        );
        idx.insert(patient.id.as_bytes(), search_blob.as_bytes())?;
        Ok(())
    }

    /// Enrich a list of patients with report stats (report_count, last_report_date, total_abnormal).
    fn enrich_patients_with_stats(
        &self,
        patients: Vec<Patient>,
    ) -> Result<Vec<PatientWithStats>, AppError> {
        let idx_pr = self.db.open_tree("idx_patient_reports")?;
        let reports_tree = self.db.open_tree("reports")?;
        let idx_ri = self.db.open_tree("idx_report_items")?;
        let items_tree = self.db.open_tree("test_items")?;

        let mut result = Vec::with_capacity(patients.len());
        for patient in patients {
            let prefix = format!("{}:", patient.id);
            let mut report_count: usize = 0;
            let mut last_report_date = String::new();
            let mut total_abnormal: usize = 0;

            for entry in idx_pr.scan_prefix(prefix.as_bytes()) {
                let (k, _) = entry?;
                let key_str = String::from_utf8_lossy(&k);
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() >= 3 {
                    let report_id = parts[2];
                    if let Some(val) = reports_tree.get(report_id.as_bytes())? {
                        let r: Report = serde_json::from_slice(&val)?;
                        report_count += 1;
                        if r.report_date > last_report_date {
                            last_report_date = r.report_date.clone();
                        }
                        let item_prefix = format!("{}:", report_id);
                        for item_entry in idx_ri.scan_prefix(item_prefix.as_bytes()) {
                            let (ik, _) = item_entry?;
                            let ik_str = String::from_utf8_lossy(&ik);
                            let item_id = match ik_str.split_once(':') {
                                Some((_, id)) if !id.is_empty() => id,
                                _ => continue,
                            };
                            if let Some(iv) = items_tree.get(item_id.as_bytes())? {
                                let item: TestItem = serde_json::from_slice(&iv)?;
                                if item.status == ItemStatus::High || item.status == ItemStatus::Low
                                {
                                    total_abnormal += 1;
                                }
                            }
                        }
                    }
                }
            }

            result.push(PatientWithStats {
                patient,
                report_count,
                last_report_date,
                total_abnormal,
            });
        }
        Ok(result)
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
        let tree = self.db.open_tree("temperature_records")?;
        let idx = self.db.open_tree("idx_patient_temperatures")?;
        let val = serde_json::to_vec(record)?;
        let id_bytes = record.id.as_bytes().to_vec();
        let idx_key = format!("{}:{}:{}", record.patient_id, record.recorded_at, record.id);
        let idx_key_bytes = idx_key.into_bytes();

        (&tree, &idx)
            .transaction(|(tx_tree, tx_idx)| {
                tx_tree.insert(id_bytes.as_slice(), val.as_slice())?;
                tx_idx.insert(idx_key_bytes.as_slice(), b"" as &[u8])?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("创建体温记录事务失败: {}", e))
            })?;
        Ok(())
    }

    pub fn list_temperatures_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<TemperatureRecord>, AppError> {
        let idx = self.db.open_tree("idx_patient_temperatures")?;
        let tree = self.db.open_tree("temperature_records")?;
        let prefix = format!("{}:", patient_id);
        let mut records = Vec::new();
        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let mut parts = key_str.splitn(2, ':');
            let _ = parts.next();
            if let Some(suffix) = parts.next() {
                if let Some(last_colon) = suffix.rfind(':') {
                    let record_id = &suffix[last_colon + 1..];
                    if let Some(val) = tree.get(record_id.as_bytes())? {
                        let r: TemperatureRecord = serde_json::from_slice(&val)?;
                        records.push(r);
                    }
                }
            }
        }
        Ok(records)
    }

    pub fn delete_temperature(&self, id: &str) -> Result<(), AppError> {
        let tree = self.db.open_tree("temperature_records")?;
        let idx = self.db.open_tree("idx_patient_temperatures")?;
        // Get record to find patient_id for index cleanup
        let idx_key_bytes = if let Some(val) = tree.get(id.as_bytes())? {
            let record: TemperatureRecord = serde_json::from_slice(&val)?;
            let idx_key = format!("{}:{}:{}", record.patient_id, record.recorded_at, record.id);
            Some(idx_key.into_bytes())
        } else {
            None
        };
        let id_bytes = id.as_bytes().to_vec();

        (&tree, &idx)
            .transaction(|(tx_tree, tx_idx)| {
                if let Some(ref ik) = idx_key_bytes {
                    tx_idx.remove(ik.as_slice())?;
                }
                tx_tree.remove(id_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("删除体温记录事务失败: {}", e))
            })?;
        Ok(())
    }

    fn delete_temperatures_by_patient(&self, patient_id: &str) -> Result<(), AppError> {
        let tree = self.db.open_tree("temperature_records")?;
        let idx = self.db.open_tree("idx_patient_temperatures")?;
        let prefix = format!("{}:", patient_id);

        let mut record_ids: Vec<Vec<u8>> = Vec::new();
        let mut idx_keys: Vec<Vec<u8>> = Vec::new();

        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let suffix = key_str.strip_prefix(&prefix).unwrap_or("");
            if let Some(last_colon) = suffix.rfind(':') {
                let record_id = &suffix[last_colon + 1..];
                record_ids.push(record_id.as_bytes().to_vec());
            }
            idx_keys.push(k.to_vec());
        }

        (&tree, &idx)
            .transaction(|(tx_tree, tx_idx)| {
                for rid in &record_ids {
                    tx_tree.remove(rid.as_slice())?;
                }
                for ik in &idx_keys {
                    tx_idx.remove(ik.as_slice())?;
                }
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("删除患者体温记录事务失败: {}", e))
            })?;
        Ok(())
    }

    // --- Report CRUD ---

    pub fn create_report(&self, report: &Report) -> Result<(), AppError> {
        let tree = self.db.open_tree("reports")?;
        let idx = self.db.open_tree("idx_patient_reports")?;
        let val = serde_json::to_vec(report)?;
        let id_bytes = report.id.as_bytes().to_vec();
        let idx_key = format!("{}:{}:{}", report.patient_id, report.report_date, report.id);
        let idx_key_bytes = idx_key.into_bytes();

        (&tree, &idx)
            .transaction(|(tx_tree, tx_idx)| {
                tx_tree.insert(id_bytes.as_slice(), val.as_slice())?;
                tx_idx.insert(idx_key_bytes.as_slice(), b"" as &[u8])?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("创建报告事务失败: {}", e))
            })?;
        Ok(())
    }

    /// Check if a duplicate report exists (same patient + report_type + report_date)
    /// Returns the existing report if found
    pub fn find_duplicate_report(
        &self,
        patient_id: &str,
        report_type: &str,
        report_date: &str,
    ) -> Result<Option<Report>, AppError> {
        let idx = self.db.open_tree("idx_patient_reports")?;
        let tree = self.db.open_tree("reports")?;
        let prefix = format!("{}:{}:", patient_id, report_date);
        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                let report_id = parts[2];
                if let Some(val) = tree.get(report_id.as_bytes())? {
                    let r: Report = serde_json::from_slice(&val)?;
                    if r.report_type == report_type {
                        return Ok(Some(r));
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn get_report(&self, id: &str) -> Result<Option<Report>, AppError> {
        let tree = self.db.open_tree("reports")?;
        match tree.get(id.as_bytes())? {
            Some(val) => {
                let r: Report = serde_json::from_slice(&val)?;
                Ok(Some(r))
            }
            None => Ok(None),
        }
    }

    pub fn update_report(&self, report: &Report) -> Result<(), AppError> {
        let tree = self.db.open_tree("reports")?;
        if !tree.contains_key(report.id.as_bytes())? {
            return Err(AppError::NotFound("报告不存在".to_string()));
        }
        let val = serde_json::to_vec(report)?;
        tree.insert(report.id.as_bytes(), val)?;
        Ok(())
    }

    pub fn list_reports_by_patient(&self, patient_id: &str) -> Result<Vec<Report>, AppError> {
        let idx = self.db.open_tree("idx_patient_reports")?;
        let tree = self.db.open_tree("reports")?;
        let prefix = format!("{}:", patient_id);
        let mut reports = Vec::new();
        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                let report_id = parts[2];
                if let Some(val) = tree.get(report_id.as_bytes())? {
                    let r: Report = serde_json::from_slice(&val)?;
                    reports.push(r);
                }
            }
        }
        Ok(reports)
    }

    pub fn list_reports_with_summary_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<ReportSummary>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        let idx_ri = self.db.open_tree("idx_report_items")?;
        let items_tree = self.db.open_tree("test_items")?;

        let mut summaries = Vec::with_capacity(reports.len());
        let mut item_counts: std::collections::HashMap<String, usize> =
            reports.iter().map(|r| (r.id.clone(), 0)).collect();
        let mut abnormal_counts: std::collections::HashMap<String, usize> =
            reports.iter().map(|r| (r.id.clone(), 0)).collect();
        let mut abnormal_names_map: std::collections::HashMap<String, Vec<String>> =
            reports.iter().map(|r| (r.id.clone(), Vec::new())).collect();

        for report in &reports {
            let prefix = format!("{}:", report.id);
            for entry in idx_ri.scan_prefix(prefix.as_bytes()) {
                let (k, _) = entry?;
                let key_str = String::from_utf8_lossy(&k);
                let item_id = match key_str.split_once(':') {
                    Some((_, id)) if !id.is_empty() => id,
                    _ => continue,
                };

                if let Some(val) = items_tree.get(item_id.as_bytes())? {
                    let item: TestItem = serde_json::from_slice(&val)?;
                    if let Some(count) = item_counts.get_mut(&report.id) {
                        *count += 1;
                    }
                    if item.status == ItemStatus::High || item.status == ItemStatus::Low {
                        if let Some(ab) = abnormal_counts.get_mut(&report.id) {
                            *ab += 1;
                        }
                        if let Some(names) = abnormal_names_map.get_mut(&report.id) {
                            names.push(item.name.clone());
                        }
                    }
                }
            }
        }
        for report in &reports {
            let id = report.id.as_str();
            summaries.push(ReportSummary {
                report: report.clone(),
                item_count: item_counts.get(id).copied().unwrap_or(0),
                abnormal_count: abnormal_counts.get(id).copied().unwrap_or(0),
                abnormal_names: abnormal_names_map.remove(id).unwrap_or_default(),
            });
        }
        Ok(summaries)
    }

    /// List reports and their raw item names for a patient.
    /// Used by suggest-groups to avoid N times DB tree re-open + spawn_blocking overhead.
    pub fn list_reports_with_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<(Report, Vec<String>)>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        let idx_ri = self.db.open_tree("idx_report_items")?;
        let items_tree = self.db.open_tree("test_items")?;

        let mut result = Vec::with_capacity(reports.len());
        for report in reports {
            let mut item_names: Vec<String> = Vec::new();
            let prefix = format!("{}:", report.id);
            for entry in idx_ri.scan_prefix(prefix.as_bytes()) {
                let (k, _) = entry?;
                let key_str = String::from_utf8_lossy(&k);
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() >= 2 {
                    if let Some(val) = items_tree.get(parts[1].as_bytes())? {
                        let item: TestItem = serde_json::from_slice(&val)?;
                        item_names.push(item.name);
                    }
                }
            }
            result.push((report, item_names));
        }

        Ok(result)
    }

    /// List canonical names that already exist in this patient's reports.
    pub fn list_canonical_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<String>, AppError> {
        let reports_tree = self.db.open_tree("reports")?;
        let idx_ri = self.db.open_tree("idx_report_items")?;
        let items_tree = self.db.open_tree("test_items")?;
        let report_ids = self.list_report_ids_by_patient(patient_id)?;

        let mut canonical_set: std::collections::HashSet<String> = std::collections::HashSet::new();

        for rid in report_ids {
            // Indexes may contain stale entries in edge cases; skip missing reports safely.
            if reports_tree.get(rid.as_bytes())?.is_none() {
                continue;
            }

            let item_prefix = format!("{}:", rid);
            for entry in idx_ri.scan_prefix(item_prefix.as_bytes()) {
                let (ik, _) = entry?;
                let ik_str = String::from_utf8_lossy(&ik);
                let ik_parts: Vec<&str> = ik_str.split(':').collect();
                if ik_parts.len() >= 2 {
                    if let Some(val) = items_tree.get(ik_parts[1].as_bytes())? {
                        let item: TestItem = serde_json::from_slice(&val)?;
                        if !item.canonical_name.is_empty() {
                            canonical_set.insert(item.canonical_name);
                        }
                    }
                }
            }
        }

        let mut canonical_names: Vec<String> = canonical_set.into_iter().collect();
        canonical_names.sort();
        Ok(canonical_names)
    }

    /// Get all unique test item names for a patient, with count of data points
    pub fn list_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<(String, usize)>, AppError> {
        let report_ids = self.list_report_ids_by_patient(patient_id)?;
        let idx_ri = self.db.open_tree("idx_report_items")?;
        let items_tree = self.db.open_tree("test_items")?;

        let mut name_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for rid in &report_ids {
            let prefix = format!("{}:", rid);
            for entry in idx_ri.scan_prefix(prefix.as_bytes()) {
                let (k, _) = entry?;
                let key_str = String::from_utf8_lossy(&k);
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() >= 2 {
                    if let Some(val) = items_tree.get(parts[1].as_bytes())? {
                        let item: TestItem = serde_json::from_slice(&val)?;
                        let effective_name = if item.canonical_name.is_empty() {
                            item.name
                        } else {
                            item.canonical_name
                        };
                        *name_counts.entry(effective_name).or_insert(0) += 1;
                    }
                }
            }
        }

        let mut result: Vec<(String, usize)> = name_counts.into_iter().collect();
        result.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(result)
    }

    fn list_report_ids_by_patient(&self, patient_id: &str) -> Result<Vec<String>, AppError> {
        let idx = self.db.open_tree("idx_patient_reports")?;
        let prefix = format!("{}:", patient_id);
        let mut ids = Vec::new();
        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                ids.push(parts[2].to_string());
            }
        }
        Ok(ids)
    }

    pub fn delete_report(&self, id: &str) -> Result<(), AppError> {
        let reports_tree = self.db.open_tree("reports")?;
        let items_tree = self.db.open_tree("test_items")?;
        let idx_ri = self.db.open_tree("idx_report_items")?;

        // Collect all keys to remove before the transaction
        let prefix = format!("{}:", id);
        let mut item_ids: Vec<Vec<u8>> = Vec::new();
        let mut idx_keys: Vec<Vec<u8>> = Vec::new();
        for entry in idx_ri.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 2 {
                item_ids.push(parts[1].as_bytes().to_vec());
            }
            idx_keys.push(k.to_vec());
        }

        let id_bytes = id.as_bytes().to_vec();

        // Atomically remove report, its test items, and index entries
        vec![reports_tree, items_tree, idx_ri]
            .transaction(|tx_trees| {
                for item_id in &item_ids {
                    tx_trees[1].remove(item_id.as_slice())?;
                }
                for ik in &idx_keys {
                    tx_trees[2].remove(ik.as_slice())?;
                }
                tx_trees[0].remove(id_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("删除报告事务失败: {}", e))
            })?;
        Ok(())
    }

    pub fn delete_report_with_index_cleanup(&self, id: &str) -> Result<(), AppError> {
        // Get report to find patient_id for index cleanup
        if let Some(report) = self.get_report(id)? {
            let idx = self.db.open_tree("idx_patient_reports")?;
            let prefix = format!("{}:", report.patient_id);
            let suffix = format!(":{}", id);
            let keys_to_remove: Vec<sled::IVec> = idx
                .scan_prefix(prefix.as_bytes())
                .filter_map(|entry| entry.ok())
                .filter(|(k, _)| String::from_utf8_lossy(k).ends_with(&suffix))
                .map(|(k, _)| k)
                .collect();
            for k in keys_to_remove {
                idx.remove(k)?;
            }
        }
        self.delete_report(id)
    }

    // --- TestItem CRUD ---

    pub fn create_test_item(&self, item: &TestItem) -> Result<(), AppError> {
        let tree = self.db.open_tree("test_items")?;
        let idx = self.db.open_tree("idx_report_items")?;
        let val = serde_json::to_vec(item)?;
        let id_bytes = item.id.as_bytes().to_vec();
        let idx_key = format!("{}:{}", item.report_id, item.id);
        let idx_key_bytes = idx_key.into_bytes();

        (&tree, &idx)
            .transaction(|(tx_tree, tx_idx)| {
                tx_tree.insert(id_bytes.as_slice(), val.as_slice())?;
                tx_idx.insert(idx_key_bytes.as_slice(), b"" as &[u8])?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("创建检验项事务失败: {}", e))
            })?;
        Ok(())
    }

    pub fn get_test_items_by_report(&self, report_id: &str) -> Result<Vec<TestItem>, AppError> {
        let idx = self.db.open_tree("idx_report_items")?;
        let tree = self.db.open_tree("test_items")?;
        let prefix = format!("{}:", report_id);
        let mut items = Vec::new();
        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 2 {
                let item_id = parts[1];
                if let Some(val) = tree.get(item_id.as_bytes())? {
                    let mut item: TestItem = serde_json::from_slice(&val)?;
                    // Recompute status at read time: the stored status may be stale
                    // if the reference_range was corrected after initial OCR parsing.
                    // This ensures the UI always shows the correct high/low/normal badge.
                    if let Ok(v) = item.value.parse::<f64>() {
                        if !item.reference_range.is_empty() {
                            item.status =
                                crate::ocr::parser::determine_status(v, &item.reference_range);
                        }
                    }
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    pub fn get_test_item(&self, id: &str) -> Result<Option<TestItem>, AppError> {
        let tree = self.db.open_tree("test_items")?;
        match tree.get(id.as_bytes())? {
            Some(val) => {
                let item: TestItem = serde_json::from_slice(&val)?;
                Ok(Some(item))
            }
            None => Ok(None),
        }
    }

    pub fn update_test_item(&self, item: &TestItem) -> Result<(), AppError> {
        let tree = self.db.open_tree("test_items")?;
        if !tree.contains_key(item.id.as_bytes())? {
            return Err(AppError::NotFound("检验项目不存在".to_string()));
        }
        let val = serde_json::to_vec(item)?;
        tree.insert(item.id.as_bytes(), val)?;
        Ok(())
    }

    pub fn delete_test_item(&self, id: &str) -> Result<(), AppError> {
        let tree = self.db.open_tree("test_items")?;
        let idx = self.db.open_tree("idx_report_items")?;

        // Get the item to find its report_id for index cleanup
        let item = match tree.get(id.as_bytes())? {
            Some(val) => serde_json::from_slice::<TestItem>(&val)?,
            None => return Err(AppError::NotFound("检验项目不存在".to_string())),
        };

        let id_bytes = id.as_bytes().to_vec();
        let idx_key = format!("{}:{}", item.report_id, item.id);
        let idx_key_bytes = idx_key.into_bytes();

        (&tree, &idx)
            .transaction(|(tx_tree, tx_idx)| {
                tx_tree.remove(id_bytes.as_slice())?;
                tx_idx.remove(idx_key_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("删除检验项事务失败: {}", e))
            })?;
        Ok(())
    }

    // --- Edit Log ---

    pub fn create_edit_log(&self, log: &EditLog) -> Result<(), AppError> {
        let tree = self.db.open_tree("edit_logs")?;
        let idx_report = self.db.open_tree("idx_report_edit_logs")?;
        let idx_ordered = self.db.open_tree("idx_edit_logs_ordered")?;

        let val = serde_json::to_vec(log)?;
        let id_bytes = log.id.as_bytes().to_vec();
        let report_idx_key = format!("{}:{}:{}", log.report_id, log.created_at, log.id);
        let ordered_idx_key = format!("{}:{}", log.created_at, log.id);

        tree.insert(id_bytes.as_slice(), val.as_slice())?;
        idx_report.insert(report_idx_key.as_bytes(), log.id.as_bytes())?;
        idx_ordered.insert(ordered_idx_key.as_bytes(), log.id.as_bytes())?;
        Ok(())
    }

    pub fn list_edit_logs_by_report(&self, report_id: &str) -> Result<Vec<EditLog>, AppError> {
        let tree = self.db.open_tree("edit_logs")?;
        let idx = self.db.open_tree("idx_report_edit_logs")?;
        let prefix = format!("{}:", report_id);
        let mut logs = Vec::new();
        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (_, log_id) = entry?;
            if let Some(val) = tree.get(&log_id)? {
                let log: EditLog = serde_json::from_slice(&val)?;
                logs.push(log);
            }
        }
        logs.reverse();
        Ok(logs)
    }

    pub fn list_edit_logs_global(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<PaginatedList<EditLog>, AppError> {
        let tree = self.db.open_tree("edit_logs")?;
        let idx = self.db.open_tree("idx_edit_logs_ordered")?;

        let page_size = if page_size == 0 {
            DEFAULT_PAGE_SIZE
        } else if page_size > 100 {
            100
        } else {
            page_size
        };
        let page = if page == 0 { 1 } else { page };
        let total = idx.len();

        // Iterate in reverse (newest first)
        let skip = (page - 1) * page_size;
        let mut items = Vec::with_capacity(page_size);
        for entry in idx.iter().rev().skip(skip) {
            if items.len() >= page_size {
                break;
            }
            let (_, log_id) = entry?;
            if let Some(val) = tree.get(&log_id)? {
                let log: EditLog = serde_json::from_slice(&val)?;
                items.push(log);
            }
        }

        Ok(PaginatedList {
            items,
            total,
            page,
            page_size,
        })
    }

    #[allow(dead_code)]
    fn delete_test_items_by_report(&self, report_id: &str) -> Result<(), AppError> {
        let idx = self.db.open_tree("idx_report_items")?;
        let tree = self.db.open_tree("test_items")?;
        let prefix = format!("{}:", report_id);
        let entries: Vec<(sled::IVec, sled::IVec)> = idx
            .scan_prefix(prefix.as_bytes())
            .filter_map(|entry| entry.ok())
            .collect();
        for (k, _) in entries {
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 2 {
                tree.remove(parts[1].as_bytes())?;
            }
            idx.remove(k)?;
        }
        Ok(())
    }

    /// Batch create reports and test items in a single atomic operation.
    /// Opens each tree once, merges duplicates, then writes all data transactionally.
    pub fn batch_create_reports_and_items(
        &self,
        _patient_id: &str,
        inputs: Vec<BatchReportInput>,
    ) -> Result<Vec<(Report, Vec<TestItem>)>, AppError> {
        let reports_tree = self.db.open_tree("reports")?;
        let items_tree = self.db.open_tree("test_items")?;
        let idx_pr = self.db.open_tree("idx_patient_reports")?;
        let idx_ri = self.db.open_tree("idx_report_items")?;

        // Collect all insert operations before the transaction
        let mut report_inserts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut item_inserts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut idx_pr_inserts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut idx_ri_inserts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut results = Vec::new();

        for input in &inputs {
            let (report_id, report_obj) = if let Some(ref eid) = input.existing_report_id {
                let report = reports_tree
                    .get(eid.as_bytes())?
                    .map(|v| serde_json::from_slice::<Report>(&v))
                    .transpose()
                    .map_err(|e| AppError::Internal(e.to_string()))?
                    .ok_or_else(|| AppError::NotFound("报告不存在".to_string()))?;
                (eid.clone(), report)
            } else if let Some(ref report) = input.new_report {
                let val = serde_json::to_vec(report)?;
                report_inserts.push((report.id.as_bytes().to_vec(), val));
                let idx_key = format!("{}:{}:{}", report.patient_id, report.report_date, report.id);
                idx_pr_inserts.push((idx_key.into_bytes(), b"".to_vec()));
                (report.id.clone(), report.clone())
            } else {
                continue;
            };

            // Get existing item names for dedup (only for existing reports)
            let mut existing_items = Vec::new();
            let mut existing_item_names = std::collections::HashSet::new();
            if input.existing_report_id.is_some() {
                let prefix = format!("{}:", report_id);
                for entry in idx_ri.scan_prefix(prefix.as_bytes()) {
                    let (k, _) = entry?;
                    let key_str = String::from_utf8_lossy(&k);
                    let parts: Vec<&str> = key_str.split(':').collect();
                    if parts.len() >= 2 {
                        if let Some(val) = items_tree.get(parts[1].as_bytes())? {
                            let item: TestItem = serde_json::from_slice(&val)?;
                            existing_item_names.insert(item.name.clone());
                            existing_items.push(item);
                        }
                    }
                }
            }

            // Add new items (skip duplicates by name)
            let mut new_items = Vec::new();
            for item in &input.items {
                if existing_item_names.contains(&item.name) {
                    continue;
                }
                existing_item_names.insert(item.name.clone());
                let val = serde_json::to_vec(item)?;
                item_inserts.push((item.id.as_bytes().to_vec(), val));
                let idx_key = format!("{}:{}", item.report_id, item.id);
                idx_ri_inserts.push((idx_key.into_bytes(), b"".to_vec()));
                new_items.push(item.clone());
            }

            existing_items.extend(new_items);
            results.push((report_obj, existing_items));
        }

        // Apply all inserts atomically across all 4 trees
        vec![reports_tree, items_tree, idx_pr, idx_ri]
            .transaction(|tx_trees| {
                for (k, v) in &report_inserts {
                    tx_trees[0].insert(k.as_slice(), v.as_slice())?;
                }
                for (k, v) in &item_inserts {
                    tx_trees[1].insert(k.as_slice(), v.as_slice())?;
                }
                for (k, v) in &idx_pr_inserts {
                    tx_trees[2].insert(k.as_slice(), v.as_slice())?;
                }
                for (k, v) in &idx_ri_inserts {
                    tx_trees[3].insert(k.as_slice(), v.as_slice())?;
                }
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("批量创建报告事务失败: {}", e))
            })?;

        Ok(results)
    }

    pub fn get_trends(
        &self,
        patient_id: &str,
        item_name: &str,
        report_type: Option<&str>,
    ) -> Result<Vec<TrendPoint>, AppError> {
        // Optimized: collect all report IDs and batch-fetch test items
        let idx_pr = self.db.open_tree("idx_patient_reports")?;
        let reports_tree = self.db.open_tree("reports")?;
        let idx_ri = self.db.open_tree("idx_report_items")?;
        let items_tree = self.db.open_tree("test_items")?;
        let target_name_key = normalize_trend_item_name(item_name);

        let prefix = format!("{}:", patient_id);
        // Use effective date (sample_date preferred, fallback to report_date) as dedup key
        let mut seen_dates: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut points = Vec::new();
        // Cache normalized names to avoid repeated computation across items
        let mut name_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for entry in idx_pr.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            let report_id = parts[2];

            // Get report date
            let report_val = match reports_tree.get(report_id.as_bytes())? {
                Some(v) => v,
                None => continue,
            };
            let report: Report = serde_json::from_slice(&report_val)?;

            // Filter by report_type category prefix if specified
            if let Some(rt) = report_type {
                if !report.report_type.starts_with(rt) {
                    continue;
                }
            }

            // Effective date: prefer sample_date, fallback to report_date
            let effective_date = if report.sample_date.is_empty() {
                &report.report_date
            } else {
                &report.sample_date
            };

            // Skip if we already have a data point for this date
            if seen_dates.contains(effective_date) {
                continue;
            }

            // Scan test items for this report, filter by name (take first match only)
            // Also match truncated names: if stored name is a prefix of the queried name,
            // it's likely an OCR truncation of the same item.
            let item_prefix = format!("{}:", report_id);
            for item_entry in idx_ri.scan_prefix(item_prefix.as_bytes()) {
                let (ik, _) = item_entry?;
                let ik_str = String::from_utf8_lossy(&ik);
                let ik_parts: Vec<&str> = ik_str.split(':').collect();
                if ik_parts.len() < 2 {
                    continue;
                }
                if let Some(item_val) = items_tree.get(ik_parts[1].as_bytes())? {
                    let item: TestItem = serde_json::from_slice(&item_val)?;
                    let effective_name = if item.canonical_name.is_empty() {
                        &item.name
                    } else {
                        &item.canonical_name
                    };
                    let candidate_name_key = name_cache
                        .entry(effective_name.to_string())
                        .or_insert_with(|| normalize_trend_item_name(effective_name))
                        .clone();
                    if candidate_name_key == target_name_key
                    {
                        seen_dates.insert(effective_date.to_string());
                        let status = if let Ok(val) = item.value.parse::<f64>() {
                            if !item.reference_range.is_empty() {
                                crate::ocr::parser::determine_status(val, &item.reference_range)
                            } else {
                                item.status
                            }
                        } else {
                            item.status
                        };
                        points.push(TrendPoint {
                            report_date: report.report_date.clone(),
                            sample_date: report.sample_date.clone(),
                            value: item.value,
                            unit: item.unit,
                            status,
                            reference_range: item.reference_range,
                        });
                        break;
                    }
                }
            }
        }

        // Sort by effective date (sample_date preferred)
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
    }

    pub fn list_trend_items_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<TrendItemInfo>, AppError> {
        let report_ids = self.list_report_ids_by_patient(patient_id)?;
        let reports_tree = self.db.open_tree("reports")?;
        let idx_ri = self.db.open_tree("idx_report_items")?;
        let items_tree = self.db.open_tree("test_items")?;

        // Collect all report_types and build category mapping
        let mut all_report_types: Vec<String> = Vec::new();
        for rid in &report_ids {
            if let Some(v) = reports_tree.get(rid.as_bytes())? {
                let report: Report = serde_json::from_slice(&v)?;
                all_report_types.push(report.report_type);
            }
        }
        let category_map = compute_report_categories(&all_report_types);

        // Collect all raw data points as (category, date, item_name)
        let mut data_points: Vec<(String, String, String)> = Vec::new();
        // Dedup: same (category, date, item_name) counts only once
        let mut seen: std::collections::HashSet<(String, String, String)> =
            std::collections::HashSet::new();

        for rid in &report_ids {
            let report = match reports_tree.get(rid.as_bytes())? {
                Some(v) => serde_json::from_slice::<Report>(&v)?,
                None => continue,
            };

            let category = category_map
                .get(&report.report_type)
                .cloned()
                .unwrap_or_else(|| report.report_type.clone());

            let prefix = format!("{}:", rid);
            for entry in idx_ri.scan_prefix(prefix.as_bytes()) {
                let (k, _) = entry?;
                let key_str = String::from_utf8_lossy(&k);
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() >= 2 {
                    if let Some(val) = items_tree.get(parts[1].as_bytes())? {
                        let item: TestItem = serde_json::from_slice(&val)?;
                        let effective_name = if item.canonical_name.is_empty() {
                            normalize_trend_item_name(&item.name)
                        } else {
                            normalize_trend_item_name(&item.canonical_name)
                        };
                        let trend_date = if report.sample_date.is_empty() {
                            report.report_date.clone()
                        } else {
                            report.sample_date.clone()
                        };
                        let key = (category.clone(), trend_date, effective_name);
                        if seen.insert(key.clone()) {
                            data_points.push(key);
                        }
                    }
                }
            }
        }

        // Count unique dates per (category, item_name) using exact matching.
        // Previous prefix-based "truncation detection" was too aggressive —
        // it incorrectly merged legitimate short items (e.g. "钙") with
        // unrelated longer items (e.g. "钙磷乘积").
        let mut counts: std::collections::HashMap<
            (String, String),
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();

        for (cat, date, item_name) in &data_points {
            counts
                .entry((cat.clone(), item_name.clone()))
                .or_default()
                .insert(date.clone());
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
    }

    // --- Report Interpretation Cache ---

    pub fn save_interpretation(&self, report_id: &str, content: &str) -> Result<(), AppError> {
        let tree = self.db.open_tree("report_interpretations")?;
        let data = serde_json::json!({
            "content": content,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        tree.insert(report_id.as_bytes(), serde_json::to_vec(&data)?)?;
        Ok(())
    }

    pub fn get_interpretation(
        &self,
        report_id: &str,
    ) -> Result<Option<(String, String)>, AppError> {
        let tree = self.db.open_tree("report_interpretations")?;
        match tree.get(report_id.as_bytes())? {
            Some(val) => {
                let data: serde_json::Value = serde_json::from_slice(&val)?;
                let content = data
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let created_at = data
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(Some((content, created_at)))
            }
            None => Ok(None),
        }
    }

    pub fn delete_interpretation(&self, report_id: &str) -> Result<(), AppError> {
        let tree = self.db.open_tree("report_interpretations")?;
        tree.remove(report_id.as_bytes())?;
        Ok(())
    }
}

/// Fallback keyword rules for report type categorization.
/// Used only when the authoritative taxonomy (`report_types.json`) has no match.
/// Category names are aligned with the taxonomy for consistency.
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
/// 1. Try the authoritative taxonomy dictionary (`report_types.json`).
/// 2. Fall back to keyword rules for types not covered by taxonomy.
/// 3. For still-unmatched types, use common-prefix grouping (min 3 Chinese chars).
fn compute_report_categories(report_types: &[String]) -> std::collections::HashMap<String, String> {
    let mut mapping = std::collections::HashMap::new();
    let mut unmatched: Vec<String> = Vec::new();

    // Deduplicate input
    let mut unique: Vec<&String> = report_types.iter().collect();
    unique.sort();
    unique.dedup();

    // Phase 1: authoritative taxonomy lookup (report_types.json)
    // Phase 2: fallback keyword rules
    for rt in &unique {
        // Try taxonomy first
        if let Some(cat) = crate::algorithm_engine::report_taxonomy::lookup_category_pub(rt) {
            mapping.insert((*rt).clone(), cat);
            continue;
        }
        // Fallback: keyword rules
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

    // Phase 2: prefix-based grouping for unmatched types (min 3 chars)
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
                // Only shrink prefix if new prefix is still ≥ 3 chars
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

impl Database {
    // --- Daily Expense CRUD ---

    pub fn create_expense(
        &self,
        expense: &DailyExpense,
        items: &[ExpenseItem],
    ) -> Result<(), AppError> {
        let exp_tree = self.db.open_tree("daily_expenses")?;
        let item_tree = self.db.open_tree("expense_items")?;
        let idx_pe = self.db.open_tree("idx_patient_expenses")?;
        let idx_ei = self.db.open_tree("idx_expense_items")?;

        let exp_val = serde_json::to_vec(expense)?;
        let exp_id_bytes = expense.id.as_bytes().to_vec();
        let idx_key = format!("{}:{}:{}", expense.patient_id, expense.expense_date, expense.id);
        let idx_key_bytes = idx_key.into_bytes();

        let mut item_vals: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::with_capacity(items.len());
        for item in items {
            let iv = serde_json::to_vec(item)?;
            let iid = item.id.as_bytes().to_vec();
            let iidx = format!("{}:{}", expense.id, item.id).into_bytes();
            item_vals.push((iid, iv, iidx));
        }

        vec![exp_tree, item_tree, idx_pe, idx_ei]
            .transaction(|tx_trees| {
                tx_trees[0].insert(exp_id_bytes.as_slice(), exp_val.as_slice())?;
                tx_trees[2].insert(idx_key_bytes.as_slice(), b"" as &[u8])?;
                for (iid, iv, iidx) in &item_vals {
                    tx_trees[1].insert(iid.as_slice(), iv.as_slice())?;
                    tx_trees[3].insert(iidx.as_slice(), b"" as &[u8])?;
                }
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("创建消费记录事务失败: {}", e))
            })?;
        Ok(())
    }

    pub fn list_expenses_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<DailyExpenseSummary>, AppError> {
        let idx = self.db.open_tree("idx_patient_expenses")?;
        let exp_tree = self.db.open_tree("daily_expenses")?;
        let idx_ei = self.db.open_tree("idx_expense_items")?;
        let item_tree = self.db.open_tree("expense_items")?;
        let prefix = format!("{}:", patient_id);
        let mut summaries = Vec::new();

        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                let expense_id = parts[2];
                if let Some(val) = exp_tree.get(expense_id.as_bytes())? {
                    let expense: DailyExpense = serde_json::from_slice(&val)?;

                    let item_prefix = format!("{}:", expense_id);
                    let mut item_count = 0usize;
                    let mut drug_count = 0usize;
                    let mut test_count = 0usize;
                    let mut treatment_count = 0usize;

                    for ie in idx_ei.scan_prefix(item_prefix.as_bytes()) {
                        let (ik, _) = ie?;
                        let ik_str = String::from_utf8_lossy(&ik);
                        if let Some((_, item_id)) = ik_str.split_once(':') {
                            if let Some(iv) = item_tree.get(item_id.as_bytes())? {
                                let item: ExpenseItem = serde_json::from_slice(&iv)?;
                                item_count += 1;
                                match item.category {
                                    ExpenseCategory::Drug => drug_count += 1,
                                    ExpenseCategory::Test => test_count += 1,
                                    ExpenseCategory::Treatment => treatment_count += 1,
                                    _ => {}
                                }
                            }
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
            }
        }

        summaries.sort_by(|a, b| b.expense.expense_date.cmp(&a.expense.expense_date));
        Ok(summaries)
    }

    pub fn get_expense_detail(&self, id: &str) -> Result<Option<DailyExpenseDetail>, AppError> {
        let exp_tree = self.db.open_tree("daily_expenses")?;
        let idx_ei = self.db.open_tree("idx_expense_items")?;
        let item_tree = self.db.open_tree("expense_items")?;

        let expense: DailyExpense = match exp_tree.get(id.as_bytes())? {
            Some(val) => serde_json::from_slice(&val)?,
            None => return Ok(None),
        };

        let prefix = format!("{}:", id);
        let mut items = Vec::new();
        for entry in idx_ei.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            if let Some((_, item_id)) = key_str.split_once(':') {
                if let Some(iv) = item_tree.get(item_id.as_bytes())? {
                    let item: ExpenseItem = serde_json::from_slice(&iv)?;
                    items.push(item);
                }
            }
        }

        Ok(Some(DailyExpenseDetail { expense, items }))
    }

    pub fn delete_expense(&self, id: &str) -> Result<(), AppError> {
        let exp_tree = self.db.open_tree("daily_expenses")?;
        let item_tree = self.db.open_tree("expense_items")?;
        let idx_pe = self.db.open_tree("idx_patient_expenses")?;
        let idx_ei = self.db.open_tree("idx_expense_items")?;

        // Find patient_id for index cleanup
        let pe_idx_key = if let Some(val) = exp_tree.get(id.as_bytes())? {
            let expense: DailyExpense = serde_json::from_slice(&val)?;
            let key = format!(
                "{}:{}:{}",
                expense.patient_id, expense.expense_date, expense.id
            );
            Some(key.into_bytes())
        } else {
            return Err(AppError::NotFound("消费记录不存在".to_string()));
        };

        // Collect expense item keys
        let prefix = format!("{}:", id);
        let mut item_ids: Vec<Vec<u8>> = Vec::new();
        let mut ei_idx_keys: Vec<Vec<u8>> = Vec::new();
        for entry in idx_ei.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            if let Some((_, item_id)) = key_str.split_once(':') {
                item_ids.push(item_id.as_bytes().to_vec());
            }
            ei_idx_keys.push(k.to_vec());
        }

        let id_bytes = id.as_bytes().to_vec();

        vec![exp_tree, item_tree, idx_pe, idx_ei]
            .transaction(|tx_trees| {
                tx_trees[0].remove(id_bytes.as_slice())?;
                if let Some(ref pk) = pe_idx_key {
                    tx_trees[2].remove(pk.as_slice())?;
                }
                for iid in &item_ids {
                    tx_trees[1].remove(iid.as_slice())?;
                }
                for eik in &ei_idx_keys {
                    tx_trees[3].remove(eik.as_slice())?;
                }
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError| {
                AppError::Internal(format!("删除消费记录事务失败: {}", e))
            })?;
        Ok(())
    }

    fn delete_expenses_by_patient(&self, patient_id: &str) -> Result<(), AppError> {
        let idx = self.db.open_tree("idx_patient_expenses")?;
        let prefix = format!("{}:", patient_id);
        let mut expense_ids: Vec<String> = Vec::new();
        for entry in idx.scan_prefix(prefix.as_bytes()) {
            let (k, _) = entry?;
            let key_str = String::from_utf8_lossy(&k);
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                expense_ids.push(parts[2].to_string());
            }
        }
        for eid in expense_ids {
            self.delete_expense(&eid)?;
        }
        Ok(())
    }
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
        // All 脑脊液* should share one category via taxonomy
        assert_eq!(map["脑脊液常规"], "脑脊液检查");
        assert_eq!(map["脑脊液生化"], "脑脊液检查");
        assert_eq!(map["脑脊液免疫球蛋白"], "脑脊液检查");
        // 血常规 maps to its own category
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
        // All thyroid function variants should map to the same category
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
        // Unknown types with ≥ 3 char common prefix should still be grouped
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
        // All hs-CRP variants are clinical synonyms — dictionary maps them to "超敏C反应蛋白"
        assert_eq!(normalize_trend_item_name("超敏C反应蛋白"), "超敏C反应蛋白");
        assert_eq!(normalize_trend_item_name("高敏C反应蛋白"), "超敏C反应蛋白");
        assert_eq!(normalize_trend_item_name("超高敏C反应蛋白"), "超敏C反应蛋白");
        // Regular CRP stays distinct
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
        // After stripping "尿液", "白细胞" is resolved via synonym dictionary to canonical form
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

    // --- 实战混乱分类测试 ---

    /// 模拟真实场景：多家医院、多种命名风格的报告类型 → 分类引擎能否正确归组
    #[test]
    fn chaos_report_type_classification() {
        let types = vec![
            // 肝功能变体（应归为同一组）
            "肝功能".to_string(),
            "肝功十一项".to_string(),
            "肝功八项".to_string(),
            "肝功全套".to_string(),
            "肝功能检测".to_string(),
            // 血常规变体（应归为同一组）
            "血常规".to_string(),
            "血常规五分类".to_string(),
            "血细胞分析".to_string(),
            "全血细胞计数".to_string(),
            "血液分析".to_string(),
            // 甲功变体（应归为同一组）
            "甲功三项".to_string(),
            "甲功五项".to_string(),
            "甲状腺功能".to_string(),
            "甲状腺功能全套".to_string(),
            // 凝血变体（应归为同一组）
            "凝血四项".to_string(),
            "凝血功能".to_string(),
            "凝血全套".to_string(),
            "凝血七项".to_string(),
            // 乙肝变体（应归为同一组）
            "乙肝五项".to_string(),
            "乙肝两对半".to_string(),
            "乙肝病毒DNA".to_string(),
            // 混淆项：不应被错误合并
            "肾功能".to_string(),
            "血脂四项".to_string(),
            "尿常规".to_string(),
            "生化全套".to_string(),
        ];

        let map = compute_report_categories(&types);

        // 打印分类结果方便调试
        let mut sorted: Vec<_> = map.iter().collect();
        sorted.sort_by_key(|(k, _)| k.clone());
        eprintln!("\n===== 报告类型分类结果 =====");
        for (rt, cat) in &sorted {
            eprintln!("  {:20} → {}", rt, cat);
        }

        // 验证：同组报告归到同一分类
        let liver = &map["肝功能"];
        assert_eq!(&map["肝功十一项"], liver, "肝功十一项 应与 肝功能 同组");
        assert_eq!(&map["肝功八项"], liver, "肝功八项 应与 肝功能 同组");
        assert_eq!(&map["肝功全套"], liver, "肝功全套 应与 肝功能 同组");
        assert_eq!(&map["肝功能检测"], liver, "肝功能检测 应与 肝功能 同组");

        let blood = &map["血常规"];
        assert_eq!(&map["血常规五分类"], blood, "血常规五分类 应与 血常规 同组");
        assert_eq!(&map["血细胞分析"], blood, "血细胞分析 应与 血常规 同组");
        assert_eq!(&map["全血细胞计数"], blood, "全血细胞计数 应与 血常规 同组");
        assert_eq!(&map["血液分析"], blood, "血液分析 应与 血常规 同组");

        let thyroid = &map["甲功三项"];
        assert_eq!(&map["甲功五项"], thyroid, "甲功五项 应与 甲功三项 同组");
        assert_eq!(&map["甲状腺功能"], thyroid, "甲状腺功能 应与 甲功三项 同组");
        assert_eq!(&map["甲状腺功能全套"], thyroid, "甲状腺功能全套 应与 甲功三项 同组");

        let coag = &map["凝血四项"];
        assert_eq!(&map["凝血功能"], coag, "凝血功能 应与 凝血四项 同组");
        assert_eq!(&map["凝血全套"], coag, "凝血全套 应与 凝血四项 同组");
        assert_eq!(&map["凝血七项"], coag, "凝血七项 应与 凝血四项 同组");

        let hbv = &map["乙肝五项"];
        assert_eq!(&map["乙肝两对半"], hbv, "乙肝两对半 应与 乙肝五项 同组");
        assert_eq!(&map["乙肝病毒DNA"], hbv, "乙肝病毒DNA 应与 乙肝五项 同组");

        // 验证：不同类别不应合并
        assert_ne!(&map["肝功能"], &map["肾功能"], "肝功能 ≠ 肾功能");
        assert_ne!(&map["血常规"], &map["血脂四项"], "血常规 ≠ 血脂四项");
        assert_ne!(&map["血常规"], &map["生化全套"], "血常规 ≠ 生化全套");
        assert_ne!(&map["尿常规"], &map["血常规"], "尿常规 ≠ 血常规");
        assert_ne!(&map["肝功能"], &map["生化全套"], "肝功能 ≠ 生化全套");

        // 统计：计算分类后的组数
        let mut categories: Vec<&String> = map.values().collect();
        categories.sort();
        categories.dedup();
        eprintln!("  共 {} 个报告类型 → {} 个分类组", types.len(), categories.len());
        // 预期：肝功能(5) + 血常规(5) + 甲功(4) + 凝血(4) + 乙肝(3) + 肾功(1) + 血脂(1) + 尿常规(1) + 生化(1) = 9 组
        assert_eq!(categories.len(), 9, "应分为 9 个不同的分类组");
    }

    /// 模拟真实场景：多家医院的混乱检验项目名称 → 趋势归一化能否正确统一
    #[test]
    fn chaos_item_name_normalization() {
        // 每组内的名称应归一化为同一个标准名
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
                vec!["hs-CRP", "超敏C反应蛋白", "高敏C反应蛋白", "超高敏C反应蛋白", "hsCRP"],
            ),
            (
                "C反应蛋白",
                vec!["CRP", "C反应蛋白", "C-反应蛋白", "常规C反应蛋白"],
            ),
            (
                "乙肝病毒DNA",  // trend_post_process 去掉 "定量"
                vec!["HBV-DNA", "HBV_DNA", "乙型肝炎病毒DNA", "乙肝病毒DNA"],
            ),
            (
                "乙肝表面抗原",
                vec!["HBsAg", "乙肝表面抗原", "乙型肝炎表面抗原"],
            ),
            (
                "甘油三酯",
                vec!["TG", "甘油三脂", "三酰甘油", "甘油三酯", "三酰甘油酯"],
            ),
            (
                "γ-谷氨酰转移酶",
                vec!["GGT", "γ-谷氨酰转肽酶", "谷氨酰转肽酶", "谷氨酰转移酶", "r-谷氨酰转移酶"],
            ),
            (
                "高敏心肌肌钙蛋白I",
                vec!["hs-cTnI", "高敏心肌肌钙蛋白I", "超敏肌钙蛋白I", "高敏肌钙蛋白I", "超敏肌钙蛋白", "hs-TnI", "hsTnI"],
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

        // 验证不应被合并的项目确实不同
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
            name_lower, pinyin_full, pinyin_init,
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
        // blob is lowercase, so querying lowercase matches
        assert!(blob.contains("zs"));
        // uppercase query needs .to_lowercase() at call site
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
        // Mixed Chinese and non-Chinese characters
        assert_eq!(to_pinyin_string("C反应蛋白"), "cfanyingdanbai");
        assert_eq!(to_pinyin_initials("C反应蛋白"), "cfydb");
    }
}
