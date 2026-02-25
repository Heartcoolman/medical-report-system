use crate::error::AppError;
use rusqlite::{params, OptionalExtension};

use super::Database;

impl Database {
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
            conn.execute(
                "DELETE FROM report_interpretations WHERE report_id = ?1",
                [report_id],
            )?;
            Ok(())
        })
    }
}
