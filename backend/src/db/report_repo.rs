use crate::error::AppError;
use crate::models::{Report, ReportSummary};
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;

use super::helpers::*;
use super::Database;

impl Database {
    pub fn create_report(&self, report: &Report) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO reports (id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    report.id, report.patient_id, report.report_type, report.hospital,
                    report.report_date, report.sample_date, report.file_path, report.created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn find_duplicate_report(
        &self,
        patient_id: &str,
        report_type: &str,
        report_date: &str,
    ) -> Result<Option<Report>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                 FROM reports WHERE patient_id = ?1 AND report_date = ?2 AND report_type = ?3 LIMIT 1",
                params![patient_id, report_date, report_type],
                |row| Ok(Report {
                    id: row.get(0)?, patient_id: row.get(1)?, report_type: row.get(2)?,
                    hospital: row.get(3)?, report_date: row.get(4)?, sample_date: row.get(5)?,
                    file_path: row.get(6)?, created_at: row.get(7)?,
                }),
            ).optional().map_err(AppError::from)
        })
    }

    pub fn get_report(&self, id: &str) -> Result<Option<Report>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                 FROM reports WHERE id = ?1",
                [id],
                |row| Ok(Report {
                    id: row.get(0)?, patient_id: row.get(1)?, report_type: row.get(2)?,
                    hospital: row.get(3)?, report_date: row.get(4)?, sample_date: row.get(5)?,
                    file_path: row.get(6)?, created_at: row.get(7)?,
                }),
            ).optional().map_err(AppError::from)
        })
    }

    pub fn update_report(&self, report: &Report) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                "UPDATE reports SET patient_id = ?2, report_type = ?3, hospital = ?4, report_date = ?5, sample_date = ?6, file_path = ?7 WHERE id = ?1",
                params![report.id, report.patient_id, report.report_type, report.hospital, report.report_date, report.sample_date, report.file_path],
            )?;
            if affected == 0 { return Err(AppError::NotFound("报告不存在".to_string())); }
            Ok(())
        })
    }

    pub fn list_reports_by_patient(&self, patient_id: &str) -> Result<Vec<Report>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                 FROM reports WHERE patient_id = ?1 ORDER BY report_date ASC, id ASC",
            )?;
            let reports = stmt.query_map([patient_id], |row| Ok(Report {
                id: row.get(0)?, patient_id: row.get(1)?, report_type: row.get(2)?,
                hospital: row.get(3)?, report_date: row.get(4)?, sample_date: row.get(5)?,
                file_path: row.get(6)?, created_at: row.get(7)?,
            }))?.collect::<rusqlite::Result<Vec<_>>>()?;
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

    pub fn list_reports_with_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<(Report, Vec<String>)>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        self.with_conn(|conn| {
            let mut result = Vec::with_capacity(reports.len());
            let mut stmt =
                conn.prepare("SELECT name FROM test_items WHERE report_id = ?1 ORDER BY id")?;
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

    pub fn list_canonical_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<String>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT t.canonical_name FROM test_items t
                 INNER JOIN reports r ON t.report_id = r.id
                 WHERE r.patient_id = ?1 AND t.canonical_name <> ''
                 ORDER BY t.canonical_name ASC",
            )?;
            let rows = stmt.query_map([patient_id], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn list_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<(String, usize)>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT CASE WHEN canonical_name = '' THEN name ELSE canonical_name END AS effective_name,
                        COUNT(1) AS count
                 FROM test_items ti INNER JOIN reports r ON ti.report_id = r.id
                 WHERE r.patient_id = ?1
                 GROUP BY effective_name ORDER BY count DESC, effective_name ASC",
            )?;
            let rows = stmt.query_map([patient_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn list_report_ids_by_patient(&self, patient_id: &str) -> Result<Vec<String>, AppError> {
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

    pub fn batch_create_reports_and_items(
        &self,
        _patient_id: &str,
        inputs: Vec<super::BatchReportInput>,
    ) -> Result<Vec<(Report, Vec<crate::models::TestItem>)>, AppError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            let mut results = Vec::new();

            for input in &inputs {
                let (_report_id, report_obj, current_items) = if let Some(ref eid) = input.existing_report_id {
                    let report = tx.query_row(
                        "SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at FROM reports WHERE id = ?1",
                        [eid.as_str()],
                        |row| Ok(Report {
                            id: row.get(0)?, patient_id: row.get(1)?, report_type: row.get(2)?,
                            hospital: row.get(3)?, report_date: row.get(4)?, sample_date: row.get(5)?,
                            file_path: row.get(6)?, created_at: row.get(7)?,
                        }),
                    ).optional()?;
                    let report = report.ok_or_else(|| AppError::NotFound("报告不存在".to_string()))?;

                    let mut stmt = tx.prepare(
                        "SELECT id, report_id, name, value, unit, reference_range, status, canonical_name FROM test_items WHERE report_id = ?1 ORDER BY id",
                    )?;
                    let rows = stmt.query_map([&report.id], |row| {
                        Ok(crate::models::TestItem {
                            id: row.get(0)?, report_id: row.get(1)?, name: row.get(2)?,
                            value: row.get(3)?, unit: row.get(4)?, reference_range: row.get(5)?,
                            status: parse_status(&row.get::<_, String>(6)?), canonical_name: row.get(7)?,
                        })
                    })?;
                    let current_items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
                    (report.id.clone(), report, current_items)
                } else if let Some(ref report) = input.new_report {
                    tx.execute(
                        "INSERT INTO reports (id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![report.id, report.patient_id, report.report_type, report.hospital, report.report_date, report.sample_date, report.file_path, report.created_at],
                    )?;
                    (report.id.clone(), report.clone(), Vec::new())
                } else {
                    continue;
                };

                let mut existing_names: HashSet<String> = current_items.iter().map(|item| item.name.clone()).collect();
                let mut merged_items = current_items;

                for item in &input.items {
                    if existing_names.contains(&item.name) { continue; }
                    tx.execute(
                        "INSERT INTO test_items (id, report_id, name, value, unit, reference_range, status, canonical_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![item.id, item.report_id, item.name, item.value, item.unit, item.reference_range, status_to_db(&item.status), item.canonical_name],
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
}
