use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;

/// 统一错误码。前端根据此字段做业务判断，不再依赖 message 文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    // === 认证 ===
    AuthMissingToken,
    AuthInvalidToken,
    AuthInvalidCredentials,
    AuthUsernameConflict,

    // === 授权 ===
    AuthzInsufficientRole,

    // === 验证 ===
    ValidationError,
    MissingParameter,
    InvalidRole,

    // === 资源不存在 ===
    PatientNotFound,
    ReportNotFound,
    TestItemNotFound,
    MedicationNotFound,
    ExpenseNotFound,

    // === 数据不足 ===
    NoData,
    InsufficientData,

    // === 文件上传 ===
    UploadEmpty,
    UploadReadFailed,
    FileTooLarge,
    FileTypeNotAllowed,

    // === OCR / AI ===
    OcrFailed,
    LlmApiFailed,

    // === 备份恢复 ===
    BackupFailed,
    InvalidBackupFile,
    RestoreFailed,

    // === 速率限制 ===
    RateLimited,

    // === 系统内部 ===
    DatabaseError,
    InternalError,
}

impl ErrorCode {
    pub fn status_code(&self) -> StatusCode {
        match self {
            // 401
            ErrorCode::AuthMissingToken
            | ErrorCode::AuthInvalidToken
            | ErrorCode::AuthInvalidCredentials => StatusCode::UNAUTHORIZED,

            // 403
            ErrorCode::AuthzInsufficientRole => StatusCode::FORBIDDEN,

            // 409
            ErrorCode::AuthUsernameConflict => StatusCode::CONFLICT,

            // 400
            ErrorCode::ValidationError
            | ErrorCode::MissingParameter
            | ErrorCode::InvalidRole
            | ErrorCode::UploadEmpty
            | ErrorCode::UploadReadFailed
            | ErrorCode::FileTooLarge
            | ErrorCode::FileTypeNotAllowed
            | ErrorCode::InsufficientData
            | ErrorCode::InvalidBackupFile => StatusCode::BAD_REQUEST,

            // 404
            ErrorCode::PatientNotFound
            | ErrorCode::ReportNotFound
            | ErrorCode::TestItemNotFound
            | ErrorCode::MedicationNotFound
            | ErrorCode::ExpenseNotFound
            | ErrorCode::NoData => StatusCode::NOT_FOUND,

            // 429
            ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,

            // 502
            ErrorCode::OcrFailed | ErrorCode::LlmApiFailed => StatusCode::BAD_GATEWAY,

            // 500
            ErrorCode::BackupFailed
            | ErrorCode::RestoreFailed
            | ErrorCode::DatabaseError
            | ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    // --- 通用快捷方法 ---
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationError, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, msg)
    }

    // --- 资源不存在 ---
    pub fn patient_not_found() -> Self {
        Self::new(ErrorCode::PatientNotFound, "患者不存在")
    }
    pub fn report_not_found() -> Self {
        Self::new(ErrorCode::ReportNotFound, "报告不存在")
    }
    pub fn test_item_not_found() -> Self {
        Self::new(ErrorCode::TestItemNotFound, "检验项目不存在")
    }
    pub fn medication_not_found() -> Self {
        Self::new(ErrorCode::MedicationNotFound, "用药记录不存在")
    }
    pub fn expense_not_found() -> Self {
        Self::new(ErrorCode::ExpenseNotFound, "消费记录不存在")
    }

    // --- 认证 ---
    pub fn missing_token() -> Self {
        Self::new(ErrorCode::AuthMissingToken, "缺少认证令牌")
    }
    pub fn invalid_token() -> Self {
        Self::new(ErrorCode::AuthInvalidToken, "认证令牌无效或已过期")
    }
    pub fn invalid_credentials() -> Self {
        Self::new(ErrorCode::AuthInvalidCredentials, "用户名或密码错误")
    }
    pub fn insufficient_role(minimum: &str) -> Self {
        Self::new(
            ErrorCode::AuthzInsufficientRole,
            format!("权限不足: 需要 {} 或更高角色", minimum),
        )
    }
    pub fn rate_limited() -> Self {
        Self::new(ErrorCode::RateLimited, "请求过于频繁，请稍后再试")
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.code.status_code();
        let body = json!({
            "success": false,
            "data": null,
            "error_code": self.code,
            "message": self.message,
        });
        (status, Json(body)).into_response()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::new(ErrorCode::DatabaseError, format!("数据库错误: {}", e))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new(ErrorCode::InternalError, format!("序列化错误: {}", e))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new(ErrorCode::InternalError, format!("IO 错误: {}", e))
    }
}

/// Run a blocking closure on the tokio blocking thread pool.
pub async fn run_blocking<F, T>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::new(ErrorCode::InternalError, format!("任务执行失败: {}", e)))?
}
