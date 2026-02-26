use crate::error::AppError;
use crate::models::{EditLog, PaginatedList};
use rusqlite::params;

use super::helpers::DEFAULT_PAGE_SIZE;
use super::Database;

impl Database {
    pub fn create_edit_log(&self, log: &EditLog) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO edit_logs (id, report_id, patient_id, action, target_type, target_id, summary, changes, created_at, operator_id, operator_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    log.id,
                    log.report_id,
                    log.patient_id,
                    log.action,
                    log.target_type,
                    log.target_id,
                    log.summary,
                    serde_json::to_string(&log.changes)?,
                    log.created_at,
                    log.operator_id,
                    log.operator_name
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_edit_logs_by_report(&self, report_id: &str) -> Result<Vec<EditLog>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, report_id, patient_id, action, target_type, target_id,
                        summary, changes, created_at, operator_id, operator_name
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
                    operator_id: row.get(9)?,
                    operator_name: row.get(10)?,
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
                .query_row("SELECT COUNT(*) FROM edit_logs", [], |row| {
                    row.get::<_, i64>(0)
                })?
                .try_into()
                .unwrap_or(0);

            let mut stmt = conn.prepare(
                "SELECT id, report_id, patient_id, action, target_type, target_id,
                        summary, changes, created_at, operator_id, operator_name
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
                    operator_id: row.get(9)?,
                    operator_name: row.get(10)?,
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
}
