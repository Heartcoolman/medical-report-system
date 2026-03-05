use crate::error::AppError;
use rusqlite::{params, OptionalExtension};

use super::Database;

impl Database {
    pub fn save_risk_prediction(&self, patient_id: &str, content: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO risk_predictions (patient_id, content, created_at)
                 VALUES (?1, ?2, datetime('now'))",
                params![patient_id, content],
            )?;
            Ok(())
        })
    }

    pub fn get_risk_prediction(
        &self,
        patient_id: &str,
    ) -> Result<Option<(String, String)>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT content, created_at FROM risk_predictions WHERE patient_id = ?1",
                [patient_id],
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

    pub fn delete_risk_prediction(&self, patient_id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM risk_predictions WHERE patient_id = ?1",
                [patient_id],
            )?;
            Ok(())
        })
    }

    pub fn update_patient_risk_level(
        &self,
        patient_id: &str,
        risk_level: &str,
    ) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE patients SET risk_level = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![risk_level, patient_id],
            )?;
            Ok(())
        })
    }
}
