use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use rusqlite::Connection;
use crate::error::{run_blocking, AppError, ErrorCode};
use crate::models::ApiResponse;
use crate::AppState;

const BACKUP_DIR: &str = "data/backups";

fn quote_sqlite_string(value: &str) -> String {
    value.replace('\'', "''")
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn list_table_names(conn: &Connection, schema: &str) -> Result<Vec<String>, AppError> {
    let sql = format!(
        "SELECT name FROM {}.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        quote_identifier(schema)
    );
    let mut stmt = conn.prepare(&sql)?;
    let tables = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tables)
}

fn list_table_columns(
    conn: &Connection,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, AppError> {
    let sql = format!(
        "PRAGMA {}.table_info({})",
        quote_identifier(schema),
        quote_identifier(table)
    );
    let mut stmt = conn.prepare(&sql)?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

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
                .map_err(|e| AppError::new(ErrorCode::BackupFailed, format!("备份失败: {}", e)))?;
            Ok(())
        })
    })
    .await?;

    // Read the backup file
    let data = tokio::fs::read(&backup_path).await.map_err(|e| {
        AppError::new(ErrorCode::BackupFailed, format!("读取备份文件失败: {}", e))
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
        .map_err(|e| AppError::new(ErrorCode::UploadReadFailed, format!("读取上传文件失败: {}", e)))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::new(ErrorCode::UploadReadFailed, format!("读取文件数据失败: {}", e)))?;
            file_data = Some(data.to_vec());
            break;
        }
    }

    let data = file_data.ok_or_else(|| AppError::new(ErrorCode::UploadEmpty, "未找到上传文件"))?;

    // Validate it's a SQLite database (magic bytes: "SQLite format 3\0")
    if data.len() < 16 || &data[0..16] != b"SQLite format 3\0" {
        return Err(AppError::new(ErrorCode::InvalidBackupFile,
            "上传的文件不是有效的 SQLite 数据库",
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
                .map_err(|e| AppError::new(ErrorCode::BackupFailed, format!("恢复前备份失败: {}", e)))?;
            Ok(())
        })
    })
    .await?;

    // Write the uploaded data to a temporary file first
    let temp_path = format!("{}/restore_temp_{}.db", BACKUP_DIR, timestamp);
    tokio::fs::write(&temp_path, &data).await.map_err(|e| {
        AppError::new(ErrorCode::RestoreFailed, format!("写入临时文件失败: {}", e))
    })?;

    // Verify the uploaded database can be opened and has expected tables
    let tp = temp_path.clone();
    run_blocking(move || {
        let conn = rusqlite::Connection::open(&tp)
            .map_err(|e| AppError::new(ErrorCode::InvalidBackupFile, format!("无法打开上传的数据库: {}", e)))?;
        // Check for essential tables
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("查询表结构失败: {}", e)))?
            .query_map([], |row| row.get(0))
            .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("查询表结构失败: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();
        let required = ["patients", "reports", "test_items", "users"];
        for t in &required {
            if !tables.iter().any(|name| name == *t) {
                return Err(AppError::new(ErrorCode::InvalidBackupFile, format!(
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
            let attach_sql = format!(
                "ATTACH DATABASE '{}' AS restore_src",
                quote_sqlite_string(&tp2)
            );
            conn.execute_batch(&attach_sql)
                .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("附加数据库失败: {}", e)))?;

            let restore_result = (|| -> Result<(), AppError> {
                conn.execute_batch("PRAGMA foreign_keys = OFF; BEGIN IMMEDIATE;")
                    .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("开始恢复事务失败: {}", e)))?;

                let target_tables = list_table_names(conn, "main")
                    .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("查询当前数据库表失败: {}", e)))?;
                let source_tables = list_table_names(conn, "restore_src")
                    .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("查询恢复数据库表失败: {}", e)))?;

                for table in target_tables.iter().rev() {
                    let delete_sql = format!("DELETE FROM {}", quote_identifier(table));
                    conn.execute_batch(&delete_sql).map_err(|e| {
                        AppError::new(ErrorCode::RestoreFailed, format!("清空表 {} 失败: {}", table, e))
                    })?;
                }

                for table in source_tables {
                    if !target_tables.iter().any(|name| name == &table) {
                        continue;
                    }

                    let target_columns = list_table_columns(conn, "main", &table).map_err(|e| {
                        AppError::new(ErrorCode::RestoreFailed, format!("读取当前表 {} 列失败: {}", table, e))
                    })?;
                    let source_columns = list_table_columns(conn, "restore_src", &table).map_err(|e| {
                        AppError::new(ErrorCode::RestoreFailed, format!("读取恢复表 {} 列失败: {}", table, e))
                    })?;

                    let columns: Vec<String> = target_columns
                        .into_iter()
                        .filter(|col| source_columns.iter().any(|src| src == col))
                        .collect();

                    if columns.is_empty() {
                        continue;
                    }

                    let column_list = columns
                        .iter()
                        .map(|col| quote_identifier(col))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let insert_sql = format!(
                        "INSERT INTO {table_name} ({columns}) SELECT {columns} FROM restore_src.{table_name}",
                        table_name = quote_identifier(&table),
                        columns = column_list,
                    );
                    conn.execute_batch(&insert_sql).map_err(|e| {
                        AppError::new(ErrorCode::RestoreFailed, format!("恢复表 {} 失败: {}", table, e))
                    })?;
                }

                conn.execute_batch("COMMIT;")
                    .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("提交恢复事务失败: {}", e)))?;
                Ok(())
            })();

            if restore_result.is_err() {
                let _ = conn.execute_batch("ROLLBACK;");
            }

            let detach_result = conn
                .execute_batch("DETACH DATABASE restore_src")
                .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("分离数据库失败: {}", e)));
            let fk_result = conn
                .execute_batch("PRAGMA foreign_keys = ON;")
                .map_err(|e| AppError::new(ErrorCode::RestoreFailed, format!("恢复外键约束失败: {}", e)));

            restore_result?;
            detach_result?;
            fk_result?;
            Ok(())
        })
    })
    .await?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&temp_path).await;

    tracing::info!("数据库恢复成功，恢复前备份保存在: {}", pre_restore_path);

    Ok(Json(ApiResponse::ok_msg("数据库恢复成功")))
}
