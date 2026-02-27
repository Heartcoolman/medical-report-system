use crate::error::{AppError, ErrorCode};
use chrono::Utc;
use rusqlite::params;

pub struct UploadedFileRow {
    pub id: String,
    pub original_name: String,
    pub safe_name: String,
    pub mime_type: String,
    pub size: i64,
    pub is_temporary: bool,
}

impl super::Database {
    pub fn insert_uploaded_file(
        &self,
        id: &str,
        original_name: &str,
        safe_name: &str,
        mime_type: &str,
        size: usize,
        is_temporary: bool,
    ) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO uploaded_files (id, original_name, safe_name, mime_type, size, created_at, is_temporary)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    original_name,
                    safe_name,
                    mime_type,
                    size as i64,
                    Utc::now().to_rfc3339(),
                    is_temporary as i32,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_uploaded_file(&self, file_id: &str) -> Result<UploadedFileRow, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, original_name, safe_name, mime_type, size, is_temporary
                 FROM uploaded_files WHERE id = ?1",
                params![file_id],
                |row| {
                    Ok(UploadedFileRow {
                        id: row.get(0)?,
                        original_name: row.get(1)?,
                        safe_name: row.get(2)?,
                        mime_type: row.get(3)?,
                        size: row.get(4)?,
                        is_temporary: row.get::<_, i32>(5)? != 0,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::new(ErrorCode::PatientNotFound, "文件不存在")
                }
                other => AppError::from(other),
            })
        })
    }

    pub fn cleanup_temporary_files(&self, max_age_hours: i64) -> Result<Vec<String>, AppError> {
        self.with_conn(|conn| {
            let cutoff = (Utc::now() - chrono::Duration::hours(max_age_hours)).to_rfc3339();
            let mut stmt = conn.prepare(
                "SELECT safe_name FROM uploaded_files
                 WHERE is_temporary = 1 AND created_at < ?1",
            )?;
            let names: Vec<String> = stmt
                .query_map(params![cutoff], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            conn.execute(
                "DELETE FROM uploaded_files
                 WHERE is_temporary = 1 AND created_at < ?1",
                params![cutoff],
            )?;

            Ok(names)
        })
    }
}
