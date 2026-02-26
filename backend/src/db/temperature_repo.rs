use crate::error::AppError;
use crate::models::TemperatureRecord;
use rusqlite::params;

use super::Database;

impl Database {
    pub fn create_temperature(&self, record: &TemperatureRecord) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO temperature_records (id, patient_id, recorded_at, value, location, note, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    record.id,
                    record.patient_id,
                    record.recorded_at,
                    record.value,
                    record.location,
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
                "SELECT id, patient_id, recorded_at, value, location, note, created_at
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
                        location: row.get(4)?,
                        note: row.get(5)?,
                        created_at: row.get(6)?,
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
}
