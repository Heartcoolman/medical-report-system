use crate::crypto;
use crate::error::AppError;
use crate::models::{PaginatedList, Patient, PatientWithStats};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

use super::helpers::*;
use super::Database;

impl Database {
    /// Migrate unencrypted patient sensitive fields (phone, id_number) to encrypted form.
    /// Called at startup when DB_ENCRYPTION_KEY is configured.
    pub fn migrate_encrypt_sensitive_fields(&self) -> Result<usize, AppError> {
        if !crypto::encryption_enabled() {
            return Ok(0);
        }
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, phone, id_number FROM patients")?;
            let rows: Vec<(String, String, String)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let mut migrated = 0usize;
            for (id, phone, id_number) in &rows {
                let phone_needs = !phone.is_empty() && !crypto::is_encrypted(phone);
                let id_needs = !id_number.is_empty() && !crypto::is_encrypted(id_number);
                if !phone_needs && !id_needs {
                    continue;
                }
                let enc_phone = if phone_needs {
                    crypto::encrypt_field(phone).map_err(|e| AppError::internal(e))?
                } else {
                    phone.clone()
                };
                let enc_id = if id_needs {
                    crypto::encrypt_field(id_number).map_err(|e| AppError::internal(e))?
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
            upsert_search_index(conn, patient)?;
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
                return Err(AppError::patient_not_found());
            }
            upsert_search_index(conn, patient)?;
            Ok(())
        })
    }

    pub fn delete_patient(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let affected = tx.execute("DELETE FROM patients WHERE id = ?1", params![id])?;
            if affected == 0 {
                return Err(AppError::patient_not_found());
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
                .query_row("SELECT COUNT(*) FROM patients", [], |row| {
                    row.get::<_, i64>(0)
                })?
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
    /// Optimized: uses 2 batch queries with IN clause instead of N+1 per-patient queries.
    fn enrich_patients_with_stats(
        &self,
        patients: Vec<Patient>,
    ) -> Result<Vec<PatientWithStats>, AppError> {
        if patients.is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            let ids: Vec<&str> = patients.iter().map(|p| p.id.as_str()).collect();
            let placeholders: String = (1..=ids.len()).map(|i| format!("?{}", i)).collect::<Vec<_>>().join(",");

            // Batch query 1: report_count and last_report_date per patient
            let sql_reports = format!(
                "SELECT patient_id, COUNT(*), MAX(report_date) FROM reports WHERE patient_id IN ({}) GROUP BY patient_id",
                placeholders
            );
            let mut stmt = conn.prepare(&sql_reports)?;
            let mut report_stats: HashMap<String, (i64, Option<String>)> = HashMap::new();
            {
                let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?;
                for row in rows {
                    let (pid, count, last_date) = row?;
                    report_stats.insert(pid, (count, last_date));
                }
            }

            // Batch query 2: total abnormal count per patient
            let sql_abnormal = format!(
                "SELECT r.patient_id, COUNT(*)
                 FROM test_items ti
                 JOIN reports r ON ti.report_id = r.id
                 WHERE r.patient_id IN ({}) AND LOWER(ti.status) <> 'normal'
                 GROUP BY r.patient_id",
                placeholders
            );
            let mut stmt2 = conn.prepare(&sql_abnormal)?;
            let mut abnormal_stats: HashMap<String, i64> = HashMap::new();
            {
                let rows = stmt2.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?;
                for row in rows {
                    let (pid, count) = row?;
                    abnormal_stats.insert(pid, count);
                }
            }

            // Assemble results
            let result = patients
                .into_iter()
                .map(|patient| {
                    let (report_count, last_report_date) = report_stats
                        .get(&patient.id)
                        .cloned()
                        .unwrap_or((0, None));
                    let total_abnormal = abnormal_stats.get(&patient.id).copied().unwrap_or(0);
                    PatientWithStats {
                        patient,
                        report_count: report_count as usize,
                        last_report_date: last_report_date.unwrap_or_default(),
                        total_abnormal: total_abnormal as usize,
                    }
                })
                .collect();

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

    pub fn search_patients_with_stats(
        &self,
        query: &str,
    ) -> Result<Vec<PatientWithStats>, AppError> {
        let patients = self.search_patients(query)?;
        self.enrich_patients_with_stats(patients)
    }
}
