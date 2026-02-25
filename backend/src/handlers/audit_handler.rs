use axum::{
    extract::{Query, State},
    Json,
};

use crate::audit::{self, AuditLogQuery};
use crate::error::{run_blocking, AppError};
use crate::AppState;

pub async fn list_audit_logs(
    State(state): State<AppState>,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let db = state.db.clone();
    let result = run_blocking(move || audit::query_audit_logs(&db, &query)).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "data": result,
        "message": "查询成功"
    })))
}
