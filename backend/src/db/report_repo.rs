use crate::error::AppError;
use crate::models::{PaginatedList, Report, ReportSummary};
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};

use super::helpers::*;
use super::Database;

const ABNORMAL_NAME_SEPARATOR: &str = "||";

fn abnormal_names_from_blob(blob: String) -> Vec<String> {
    if blob.is_empty() {
        return Vec::new();
    }
    blob.split(ABNORMAL_NAME_SEPARATOR)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn report_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReportSummary> {
    let item_count = row.get::<_, i64>(8)?.max(0) as usize;
    let abnormal_count = row.get::<_, i64>(9)?.max(0) as usize;
    let abnormal_names_blob: String = row.get(10)?;

    Ok(ReportSummary {
        report: Report {
            id: row.get(0)?,
            patient_id: row.get(1)?,
            report_type: row.get(2)?,
            hospital: row.get(3)?,
            report_date: row.get(4)?,
            sample_date: row.get(5)?,
            file_path: row.get(6)?,
            created_at: row.get(7)?,
        },
        item_count,
        abnormal_count,
        abnormal_names: abnormal_names_from_blob(abnormal_names_blob),
    })
}

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
            if affected == 0 { return Err(AppError::report_not_found()); }
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
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT r.id, r.patient_id, r.report_type, r.hospital, r.report_date, r.sample_date, r.file_path, r.created_at,
                        COUNT(ti.id) AS item_count,
                        COALESCE(SUM(CASE WHEN LOWER(ti.status) <> 'normal' THEN 1 ELSE 0 END), 0) AS abnormal_count,
                        COALESCE(GROUP_CONCAT(CASE WHEN LOWER(ti.status) <> 'normal' THEN ti.name END, '{sep}'), '') AS abnormal_names
                 FROM reports r
                 LEFT JOIN test_items ti ON ti.report_id = r.id
                 WHERE r.patient_id = ?1
                 GROUP BY r.id, r.patient_id, r.report_type, r.hospital, r.report_date, r.sample_date, r.file_path, r.created_at
                 ORDER BY r.report_date ASC, r.id ASC",
                sep = ABNORMAL_NAME_SEPARATOR,
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([patient_id], report_summary_from_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn list_reports_with_summary_by_patient_paginated(
        &self,
        patient_id: &str,
        page: usize,
        page_size: usize,
    ) -> Result<PaginatedList<ReportSummary>, AppError> {
        self.with_conn(|conn| {
            let total: usize = conn
                .query_row(
                    "SELECT COUNT(*) FROM reports WHERE patient_id = ?1",
                    [patient_id],
                    |row| row.get::<_, i64>(0),
                )?
                .try_into()
                .unwrap_or(0);

            let offset = (page.max(1) - 1) * page_size;
            let sql = format!(
                "WITH paged_reports AS (
                    SELECT id, patient_id, report_type, hospital, report_date, sample_date, file_path, created_at
                    FROM reports
                    WHERE patient_id = ?1
                    ORDER BY report_date ASC, id ASC
                    LIMIT ?2 OFFSET ?3
                 )
                 SELECT pr.id, pr.patient_id, pr.report_type, pr.hospital, pr.report_date, pr.sample_date, pr.file_path, pr.created_at,
                        COUNT(ti.id) AS item_count,
                        COALESCE(SUM(CASE WHEN LOWER(ti.status) <> 'normal' THEN 1 ELSE 0 END), 0) AS abnormal_count,
                        COALESCE(GROUP_CONCAT(CASE WHEN LOWER(ti.status) <> 'normal' THEN ti.name END, '{sep}'), '') AS abnormal_names
                 FROM paged_reports pr
                 LEFT JOIN test_items ti ON ti.report_id = pr.id
                 GROUP BY pr.id, pr.patient_id, pr.report_type, pr.hospital, pr.report_date, pr.sample_date, pr.file_path, pr.created_at
                 ORDER BY pr.report_date ASC, pr.id ASC",
                sep = ABNORMAL_NAME_SEPARATOR,
            );
            let mut stmt = conn.prepare(&sql)?;
            let summaries = stmt
                .query_map(
                    params![patient_id, page_size as i64, offset as i64],
                    report_summary_from_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(PaginatedList {
                items: summaries,
                total,
                page: page.max(1),
                page_size,
            })
        })
    }

    pub fn list_reports_with_item_names_by_patient(
        &self,
        patient_id: &str,
    ) -> Result<Vec<(Report, Vec<String>)>, AppError> {
        let reports = self.list_reports_by_patient(patient_id)?;
        if reports.is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            let report_ids: Vec<&str> = reports.iter().map(|report| report.id.as_str()).collect();
            let placeholders = (1..=report_ids.len())
                .map(|idx| format!("?{}", idx))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT report_id, name
                 FROM test_items
                 WHERE report_id IN ({})
                 ORDER BY report_id ASC, id ASC",
                placeholders,
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut names_by_report: HashMap<String, Vec<String>> = HashMap::new();
            let rows = stmt.query_map(rusqlite::params_from_iter(report_ids.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (report_id, name) = row?;
                names_by_report.entry(report_id).or_default().push(name);
            }

            Ok(reports
                .into_iter()
                .map(|report| {
                    let item_names = names_by_report.remove(&report.id).unwrap_or_default();
                    (report, item_names)
                })
                .collect())
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

    pub fn delete_reports_with_cleanup(&self, ids: &[String]) -> Result<Vec<String>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            let placeholders = (1..=ids.len())
                .map(|idx| format!("?{}", idx))
                .collect::<Vec<_>>()
                .join(",");
            let select_sql = format!(
                "SELECT DISTINCT patient_id FROM reports WHERE id IN ({})",
                placeholders,
            );
            let delete_interpret_sql = format!(
                "DELETE FROM report_interpretations WHERE report_id IN ({})",
                placeholders,
            );
            let delete_report_sql = format!(
                "DELETE FROM reports WHERE id IN ({})",
                placeholders,
            );

            let tx = conn.transaction()?;
            let patient_ids: Vec<String> = {
                let mut stmt = tx.prepare(&select_sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| row.get(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            tx.execute(&delete_interpret_sql, rusqlite::params_from_iter(ids.iter()))?;
            tx.execute(&delete_report_sql, rusqlite::params_from_iter(ids.iter()))?;
            tx.commit()?;
            Ok(patient_ids)
        })
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
                    let report = report.ok_or_else(|| AppError::report_not_found())?;

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
