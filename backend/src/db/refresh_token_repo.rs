use crate::error::AppError;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Database;

#[derive(Debug, Clone)]
pub struct RefreshTokenRow {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub device_name: String,
    pub device_type: String,
    pub ip_address: String,
    pub user_agent: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_used_at: String,
    pub revoked: bool,
    pub replaced_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceSession {
    pub id: String,
    pub device_name: String,
    pub device_type: String,
    pub ip_address: String,
    pub created_at: String,
    pub last_used_at: String,
}

impl Database {
    pub fn create_refresh_token(
        &self,
        user_id: &str,
        token_hash: &str,
        device_name: &str,
        device_type: &str,
        ip_address: &str,
        user_agent: &str,
        expires_at: &str,
    ) -> Result<String, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let id_clone = id.clone();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO refresh_tokens (id, user_id, token_hash, device_name, device_type, ip_address, user_agent, created_at, expires_at, last_used_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?8)",
                params![id_clone, user_id, token_hash, device_name, device_type, ip_address, user_agent, now, expires_at],
            )?;
            Ok(id_clone)
        })
    }

    pub fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<RefreshTokenRow>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, user_id, token_hash, device_name, device_type, ip_address, user_agent, created_at, expires_at, last_used_at, revoked, replaced_by
                 FROM refresh_tokens WHERE token_hash = ?1",
                params![token_hash],
                |row| {
                    Ok(RefreshTokenRow {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        token_hash: row.get(2)?,
                        device_name: row.get(3)?,
                        device_type: row.get(4)?,
                        ip_address: row.get(5)?,
                        user_agent: row.get(6)?,
                        created_at: row.get(7)?,
                        expires_at: row.get(8)?,
                        last_used_at: row.get(9)?,
                        revoked: row.get::<_, i32>(10)? != 0,
                        replaced_by: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn revoke_token(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE refresh_tokens SET revoked = 1 WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
    }

    pub fn revoke_and_replace(&self, old_id: &str, new_id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE refresh_tokens SET revoked = 1, replaced_by = ?2 WHERE id = ?1",
                params![old_id, new_id],
            )?;
            Ok(())
        })
    }

    /// Follow the replaced_by chain from start_id and revoke all tokens in the family.
    pub fn revoke_token_family(&self, start_id: &str) -> Result<u32, AppError> {
        self.with_conn(|conn| {
            let mut count = 0u32;
            let mut current_id = Some(start_id.to_string());

            while let Some(ref id) = current_id {
                // Revoke this token
                let updated = conn.execute(
                    "UPDATE refresh_tokens SET revoked = 1 WHERE id = ?1 AND revoked = 0",
                    params![id],
                )?;
                if updated > 0 {
                    count += 1;
                }

                // Follow the replaced_by chain
                let next: Option<String> = conn
                    .query_row(
                        "SELECT replaced_by FROM refresh_tokens WHERE id = ?1",
                        params![id],
                        |row| row.get(0),
                    )
                    .optional()?
                    .flatten();
                current_id = next;
            }

            Ok(count)
        })
    }

    pub fn list_active_sessions(&self, user_id: &str) -> Result<Vec<DeviceSession>, AppError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, device_name, device_type, ip_address, created_at, last_used_at
                 FROM refresh_tokens
                 WHERE user_id = ?1 AND revoked = 0 AND expires_at > datetime('now')
                 ORDER BY last_used_at DESC",
            )?;
            let rows = stmt
                .query_map(params![user_id], |row| {
                    Ok(DeviceSession {
                        id: row.get(0)?,
                        device_name: row.get(1)?,
                        device_type: row.get(2)?,
                        ip_address: row.get(3)?,
                        created_at: row.get(4)?,
                        last_used_at: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn revoke_all_user_tokens(&self, user_id: &str) -> Result<u32, AppError> {
        self.with_conn(|conn| {
            let count = conn.execute(
                "UPDATE refresh_tokens SET revoked = 1 WHERE user_id = ?1 AND revoked = 0",
                params![user_id],
            )?;
            Ok(count as u32)
        })
    }

    pub fn cleanup_expired_refresh_tokens(&self) -> Result<u32, AppError> {
        self.with_conn(|conn| {
            let count = conn.execute(
                "DELETE FROM refresh_tokens WHERE expires_at < datetime('now')",
                [],
            )?;
            Ok(count as u32)
        })
    }

    pub fn update_refresh_token_last_used(&self, id: &str) -> Result<(), AppError> {
        self.with_conn(|conn| {
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE refresh_tokens SET last_used_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            Ok(())
        })
    }

    /// Find a refresh token by its ID, ensuring it belongs to the given user.
    pub fn find_refresh_token_by_id_and_user(
        &self,
        id: &str,
        user_id: &str,
    ) -> Result<Option<RefreshTokenRow>, AppError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, user_id, token_hash, device_name, device_type, ip_address, user_agent, created_at, expires_at, last_used_at, revoked, replaced_by
                 FROM refresh_tokens WHERE id = ?1 AND user_id = ?2",
                params![id, user_id],
                |row| {
                    Ok(RefreshTokenRow {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        token_hash: row.get(2)?,
                        device_name: row.get(3)?,
                        device_type: row.get(4)?,
                        ip_address: row.get(5)?,
                        user_agent: row.get(6)?,
                        created_at: row.get(7)?,
                        expires_at: row.get(8)?,
                        last_used_at: row.get(9)?,
                        revoked: row.get::<_, i32>(10)? != 0,
                        replaced_by: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(AppError::from)
        })
    }
}
