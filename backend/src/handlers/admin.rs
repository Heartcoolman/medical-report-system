use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

use crate::error::{run_blocking, AppError};
use crate::models::ApiResponse;
use crate::AppState;

#[derive(Deserialize)]
pub struct UpdateRoleReq {
    pub role: String,
}

pub async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<crate::db::medication_repo::UserInfo>>>, AppError> {
    let db = state.db.clone();
    let users = run_blocking(move || db.list_users()).await?;
    Ok(Json(ApiResponse::ok(users, "查询成功")))
}

pub async fn update_user_role(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateRoleReq>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let valid_roles = ["admin", "doctor", "nurse", "readonly"];
    if !valid_roles.contains(&req.role.as_str()) {
        return Err(AppError::BadRequest(format!(
            "无效的角色: {}，有效值: {:?}",
            req.role, valid_roles
        )));
    }
    let db = state.db.clone();
    run_blocking(move || db.update_user_role(&user_id, &req.role)).await?;
    Ok(Json(ApiResponse::ok_msg("角色更新成功")))
}

pub async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let db = state.db.clone();
    run_blocking(move || db.delete_user(&user_id)).await?;
    Ok(Json(ApiResponse::ok_msg("用户已删除")))
}
