use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::crypto;
use crate::error::{run_blocking, AppError};
use crate::models::ApiResponse;
use crate::AppState;

#[derive(Serialize)]
pub struct UserApiKeysResponse {
    pub llm_api_key: String,
    pub interpret_api_key: String,
    pub siliconflow_api_key: String,
}

#[derive(Deserialize)]
pub struct UpdateApiKeysRequest {
    pub llm_api_key: Option<String>,
    pub interpret_api_key: Option<String>,
    pub siliconflow_api_key: Option<String>,
}

/// Mask a key: show first 4 chars + "****" if non-empty.
fn mask_key(val: &str) -> String {
    if val.is_empty() {
        return String::new();
    }
    if val.len() <= 4 {
        return format!("{}****", val);
    }
    format!("{}****", &val[..4])
}

/// Decrypt an encrypted value and return its masked form.
fn decrypt_and_mask(val: Option<String>) -> String {
    match val {
        Some(v) if !v.is_empty() => {
            let decrypted = crypto::decrypt_field(&v).unwrap_or(v);
            mask_key(&decrypted)
        }
        _ => String::new(),
    }
}

/// GET /api/user/settings
pub async fn get_settings(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<UserApiKeysResponse>>, AppError> {
    let user_id = auth.0.sub.clone();
    let db = state.db.clone();

    let keys = run_blocking(move || {
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT llm_api_key, interpret_api_key, siliconflow_api_key FROM user_api_keys WHERE user_id = ?1",
            )?;
            let result = stmt
                .query_row(rusqlite::params![user_id], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .optional()?;
            Ok(result)
        })
    })
    .await?;

    let (llm, interpret, zhipu) = keys.unwrap_or((None, None, None));

    Ok(Json(ApiResponse::ok(
        UserApiKeysResponse {
            llm_api_key: decrypt_and_mask(llm),
            interpret_api_key: decrypt_and_mask(interpret),
            siliconflow_api_key: decrypt_and_mask(zhipu),
        },
        "获取成功",
    )))
}

/// PUT /api/user/settings
pub async fn update_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UpdateApiKeysRequest>,
) -> Result<Json<ApiResponse<UserApiKeysResponse>>, AppError> {
    let user_id = auth.0.sub.clone();
    let db = state.db.clone();

    let masked = run_blocking(move || {
        db.with_conn(|conn| {
            // Read current values
            let current: Option<(Option<String>, Option<String>, Option<String>)> = conn
                .prepare(
                    "SELECT llm_api_key, interpret_api_key, siliconflow_api_key FROM user_api_keys WHERE user_id = ?1",
                )?
                .query_row(rusqlite::params![user_id], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .optional()?;

            let (cur_llm, cur_interpret, cur_siliconflow) =
                current.unwrap_or((None, None, None));

            let process_key =
                |new_val: Option<String>, current_val: Option<String>| -> Result<Option<String>, AppError> {
                    match new_val {
                        None => Ok(current_val), // field not provided, keep current
                        Some(v) if v.contains("****") => Ok(current_val), // masked value, skip
                        Some(v) if v.is_empty() => Ok(None), // empty string, clear
                        Some(v) => {
                            let encrypted = crypto::encrypt_field(&v)
                                .map_err(|e| AppError::internal(format!("加密失败: {}", e)))?;
                            Ok(Some(encrypted))
                        }
                    }
                };

            let new_llm = process_key(req.llm_api_key, cur_llm)?;
            let new_interpret = process_key(req.interpret_api_key, cur_interpret)?;
            let new_siliconflow = process_key(req.siliconflow_api_key, cur_siliconflow)?;

            conn.execute(
                "INSERT INTO user_api_keys (user_id, llm_api_key, interpret_api_key, siliconflow_api_key, updated_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(user_id) DO UPDATE SET
                   llm_api_key = excluded.llm_api_key,
                   interpret_api_key = excluded.interpret_api_key,
                   siliconflow_api_key = excluded.siliconflow_api_key,
                   updated_at = excluded.updated_at",
                rusqlite::params![user_id, new_llm, new_interpret, new_siliconflow],
            )?;

            // Return masked keys
            Ok(UserApiKeysResponse {
                llm_api_key: decrypt_and_mask(new_llm),
                interpret_api_key: decrypt_and_mask(new_interpret),
                siliconflow_api_key: decrypt_and_mask(new_siliconflow),
            })
        })
    })
    .await?;

    Ok(Json(ApiResponse::ok(masked, "设置已保存")))
}

use rusqlite::OptionalExtension;

/// Get a user's API key by type, decrypted. Returns None if not set.
pub fn get_user_api_key(
    db: &crate::db::Database,
    user_id: &str,
    key_type: &str,
) -> Option<String> {
    let column = match key_type {
        "llm" => "llm_api_key",
        "interpret" => "interpret_api_key",
        "siliconflow" => "siliconflow_api_key",
        _ => return None,
    };

    let uid = user_id.to_string();
    let query = format!(
        "SELECT {} FROM user_api_keys WHERE user_id = ?1",
        column
    );

    db.with_conn(|conn| {
        let val: Option<Option<String>> = conn
            .prepare(&query)
            .ok()
            .and_then(|mut stmt| {
                stmt.query_row(rusqlite::params![uid], |row| row.get::<_, Option<String>>(0))
                    .optional()
                    .ok()
                    .flatten()
            });
        Ok(val.flatten())
    })
    .ok()
    .flatten()
    .and_then(|encrypted| {
        if encrypted.is_empty() {
            None
        } else {
            crypto::decrypt_field(&encrypted).ok()
        }
    })
}
