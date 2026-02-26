use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use crate::error::{run_blocking, AppError};
use crate::models::ApiResponse;
use crate::AppState;

const BACKUP_DIR: &str = "data/backups";

/// GET /api/admin/backup — Download a consistent SQLite backup.
pub async fn download_backup(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // Generate backup filename with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_filename = format!("yiliao_backup_{}.db", timestamp);
    let backup_path = format!("{}/{}", BACKUP_DIR, backup_filename);

    // Ensure backup directory exists
    std::fs::create_dir_all(BACKUP_DIR)?;

    let bp = backup_path.clone();
    let db = state.db.clone();

    // Use VACUUM INTO for a consistent snapshot
    run_blocking(move || {
        db.with_conn(|conn| {
            conn.execute_batch(&format!("VACUUM INTO '{}'", bp))
                .map_err(|e| AppError::Internal(format!("备份失败: {}", e)))?;
            Ok(())
        })
    })
    .await?;

    // Read the backup file
    let data = tokio::fs::read(&backup_path).await.map_err(|e| {
        AppError::Internal(format!("读取备份文件失败: {}", e))
    })?;

    // Clean up the temporary backup file
    let _ = tokio::fs::remove_file(&backup_path).await;

    let headers = [
        (
            header::CONTENT_TYPE,
            "application/x-sqlite3".to_string(),
        ),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", backup_filename),
        ),
    ];

    Ok((StatusCode::OK, headers, data))
}

/// POST /api/admin/restore — Restore database from uploaded .db file.
pub async fn restore_backup(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<ApiResponse<()>>, AppError> {
    // Read the uploaded file
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("读取上传文件失败: {}", e)))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("读取文件数据失败: {}", e)))?;
            file_data = Some(data.to_vec());
            break;
        }
    }

    let data = file_data.ok_or_else(|| AppError::BadRequest("未找到上传文件".to_string()))?;

    // Validate it's a SQLite database (magic bytes: "SQLite format 3\0")
    if data.len() < 16 || &data[0..16] != b"SQLite format 3\0" {
        return Err(AppError::BadRequest(
            "上传的文件不是有效的 SQLite 数据库".to_string(),
        ));
    }

    // Create a pre-restore backup of the current database
    std::fs::create_dir_all(BACKUP_DIR)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let pre_restore_path = format!("{}/pre_restore_{}.db", BACKUP_DIR, timestamp);

    let prp = pre_restore_path.clone();
    let db = state.db.clone();
    run_blocking(move || {
        db.with_conn(|conn| {
            conn.execute_batch(&format!("VACUUM INTO '{}'", prp))
                .map_err(|e| AppError::Internal(format!("恢复前备份失败: {}", e)))?;
            Ok(())
        })
    })
    .await?;

    // Write the uploaded data to a temporary file first
    let temp_path = format!("{}/restore_temp_{}.db", BACKUP_DIR, timestamp);
    tokio::fs::write(&temp_path, &data).await.map_err(|e| {
        AppError::Internal(format!("写入临时文件失败: {}", e))
    })?;

    // Verify the uploaded database can be opened and has expected tables
    let tp = temp_path.clone();
    run_blocking(move || {
        let conn = rusqlite::Connection::open(&tp)
            .map_err(|e| AppError::BadRequest(format!("无法打开上传的数据库: {}", e)))?;
        // Check for essential tables
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .map_err(|e| AppError::Internal(format!("查询表结构失败: {}", e)))?
            .query_map([], |row| row.get(0))
            .map_err(|e| AppError::Internal(format!("查询表结构失败: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();
        let required = ["patients", "reports", "test_items", "users"];
        for t in &required {
            if !tables.iter().any(|name| name == *t) {
                return Err(AppError::BadRequest(format!(
                    "数据库缺少必要的表: {}",
                    t
                )));
            }
        }
        Ok(())
    })
    .await?;

    // Replace the current database: close current connection, copy file, reopen
    // We replace the DB file by loading from the uploaded one via SQLite backup API
    let tp2 = temp_path.clone();
    let db2 = state.db.clone();
    run_blocking(move || {
        db2.with_conn(|conn| {
            // Attach the uploaded database and copy all data
            conn.execute_batch(&format!(
                "ATTACH DATABASE '{}' AS restore_src",
                tp2
            ))
            .map_err(|e| AppError::Internal(format!("附加数据库失败: {}", e)))?;

            // Get all tables from the restore source
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM restore_src.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
                .map_err(|e| AppError::Internal(format!("查询恢复数据库表失败: {}", e)))?
                .query_map([], |row| row.get(0))
                .map_err(|e| AppError::Internal(format!("查询恢复数据库表失败: {}", e)))?
                .filter_map(|r| r.ok())
                .collect();

            // Delete existing data and insert from restore source
            for table in &tables {
                // Skip tables that might not exist in current schema
                let exists: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                        rusqlite::params![table],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|c| c > 0)
                    .unwrap_or(false);

                if exists {
                    conn.execute_batch(&format!("DELETE FROM \"{}\"", table))
                        .map_err(|e| {
                            AppError::Internal(format!("清空表 {} 失败: {}", table, e))
                        })?;
                    conn.execute_batch(&format!(
                        "INSERT INTO \"{}\" SELECT * FROM restore_src.\"{}\"",
                        table, table
                    ))
                    .map_err(|e| {
                        AppError::Internal(format!("恢复表 {} 失败: {}", table, e))
                    })?;
                }
            }

            conn.execute_batch("DETACH DATABASE restore_src")
                .map_err(|e| AppError::Internal(format!("分离数据库失败: {}", e)))?;

            Ok(())
        })
    })
    .await?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&temp_path).await;

    tracing::info!("数据库恢复成功，恢复前备份保存在: {}", pre_restore_path);

    Ok(Json(ApiResponse::ok_msg("数据库恢复成功")))
}
