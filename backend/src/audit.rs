use crate::db::Database;
use crate::error::AppError;
use crate::models::PaginatedList;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: i64,
    pub user_id: Option<i64>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub ip_address: Option<String>,
    pub details: Option<String>,
    pub created_at: String,
}

/// Log an audit event. Called from handlers on write operations.
/// user_id is None until authentication is integrated.
pub fn log_audit(
    db: &Database,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    ip_address: Option<&str>,
    details: Option<&str>,
) -> Result<(), AppError> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO audit_logs (user_id, action, resource_type, resource_id, ip_address, details)
             VALUES (NULL, ?1, ?2, ?3, ?4, ?5)",
            params![action, resource_type, resource_id, ip_address, details],
        )?;
        Ok(())
    })
}

#[derive(Deserialize)]
pub struct AuditLogQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
}

/// Query audit logs with pagination and optional filters.
pub fn query_audit_logs(
    db: &Database,
    query: &AuditLogQuery,
) -> Result<PaginatedList<AuditLog>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).min(100).max(1);
    let offset = (page - 1) * page_size;

    db.with_conn(|conn| {
        let mut where_clauses = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref action) = query.action {
            if !action.is_empty() {
                param_values.push(Box::new(action.clone()));
                where_clauses.push(format!("action = ?{}", param_values.len()));
            }
        }
        if let Some(ref resource_type) = query.resource_type {
            if !resource_type.is_empty() {
                param_values.push(Box::new(resource_type.clone()));
                where_clauses.push(format!("resource_type = ?{}", param_values.len()));
            }
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM audit_logs {}", where_sql);
        let mut count_stmt = conn.prepare(&count_sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let total: usize = count_stmt
            .query_row(params_ref.as_slice(), |row| row.get::<_, i64>(0))?
            .try_into()
            .unwrap_or(0);

        param_values.push(Box::new(page_size as i64));
        let limit_idx = param_values.len();
        param_values.push(Box::new(offset as i64));
        let offset_idx = param_values.len();

        let query_sql = format!(
            "SELECT id, user_id, action, resource_type, resource_id, ip_address, details, created_at
             FROM audit_logs {} ORDER BY id DESC LIMIT ?{} OFFSET ?{}",
            where_sql, limit_idx, offset_idx
        );

        let mut stmt = conn.prepare(&query_sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let items = stmt
            .query_map(params_ref.as_slice(), |row| {
                Ok(AuditLog {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    action: row.get(2)?,
                    resource_type: row.get(3)?,
                    resource_id: row.get(4)?,
                    ip_address: row.get(5)?,
                    details: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(PaginatedList {
            items,
            total,
            page,
            page_size,
        })
    })
}
