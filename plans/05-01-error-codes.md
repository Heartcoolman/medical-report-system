# 05-01 统一错误码体系详细规划

## 1. 现有错误处理分析

### 1.1 当前 `AppError` 枚举（`backend/src/error.rs`）

```rust
pub enum AppError {
    NotFound(String),    // → 404
    BadRequest(String),  // → 400
    Conflict(String),    // → 409
    Internal(String),    // → 500
}
```

当前响应格式：
```json
{ "success": false, "data": null, "message": "..." }
```

**问题：**
- 无 `error_code` 字段，前端只能靠 `message` 文本判断错误类型
- 中文错误消息直接嵌在代码里，修改困难，国际化不可能
- `auth.rs` 和 `middleware.rs` 中的认证/授权错误绕开了 `AppError`，直接手动构造 JSON 响应
- 相同语义的错误在不同 handler 中使用不同的消息文本
- 速率限制返回裸 `StatusCode::TOO_MANY_REQUESTS`，没有 JSON body

---

## 2. 全量错误场景梳理

### 2.1 认证与授权（`auth.rs`）

| 错误场景 | 现有实现 | HTTP Status | 现有消息 |
|---------|---------|-------------|---------|
| 缺少 Authorization header | 手动 JSON | 401 | "缺少认证令牌" |
| Bearer token 无效/过期 | 手动 JSON | 401 | "认证令牌无效或已过期" |
| JWT 生成失败 | `AppError::Internal` | 500 | "JWT 生成失败: {e}" |
| JWT 验证失败 | `AppError::Internal` | 500 | "JWT 验证失败: {e}" |
| 密码哈希失败 | `AppError::Internal` | 500 | "密码哈希失败: {e}" |
| 密码验证失败 | `AppError::Internal` | 500 | "密码验证失败: {e}" |
| 用户名太短（<3） | `AppError::BadRequest` | 400 | "用户名至少 3 个字符" |
| 密码太短（<6） | `AppError::BadRequest` | 400 | "密码至少 6 个字符" |
| 无效角色 | `AppError::BadRequest` | 400 | "角色必须是 admin, doctor, nurse, readonly 之一" |
| 用户名已存在 | `AppError::Conflict` | 409 | "用户名已存在" |
| 用户名或密码错误 | `AppError::BadRequest` | 400 | "用户名或密码错误" |
| 登录请求空字段 | `AppError::BadRequest` | 400 | "用户名和密码不能为空" |

### 2.2 RBAC 中间件（`auth.rs` `require_role`）

| 错误场景 | 现有实现 | HTTP Status | 现有消息 |
|---------|---------|-------------|---------|
| 权限不足 | 手动 JSON | 403 | "权限不足: 需要 {role} 或更高角色" |
| 未认证（无 Claims） | 手动 JSON | 401 | "未认证" |

### 2.3 JWT 中间件（`auth.rs` `jwt_auth_middleware`）

| 错误场景 | 现有实现 | HTTP Status | 现有消息 |
|---------|---------|-------------|---------|
| 缺少令牌 | 手动 JSON | 401 | "缺少认证令牌" |
| 令牌无效 | 手动 JSON | 401 | "认证令牌无效或已过期" |

### 2.4 速率限制中间件（`middleware.rs`）

| 错误场景 | 现有实现 | HTTP Status | 现有消息 |
|---------|---------|-------------|---------|
| Auth 端点限流 | 裸 StatusCode | 429 | (无 JSON body) |
| Upload 端点限流 | 裸 StatusCode | 429 | (无 JSON body) |
| 全局限流 | 裸 StatusCode | 429 | (无 JSON body) |

### 2.5 患者管理（`handlers/patients.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 患者数据验证失败 | `VALIDATION_ERROR` | `req.validate()` 返回的动态消息 |
| 患者不存在 | `PATIENT_NOT_FOUND` | "患者不存在" |

### 2.6 报告管理（`handlers/reports.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 报告数据验证失败 | `VALIDATION_ERROR` | `req.validate()` 返回的动态消息 |
| 患者不存在（创建报告时） | `PATIENT_NOT_FOUND` | "患者不存在" |
| 报告不存在 | `REPORT_NOT_FOUND` | "报告不存在" |
| 检验项目不存在 | `TEST_ITEM_NOT_FOUND` | "检验项目不存在" |
| 缺少 item_name 参数 | `MISSING_PARAMETER` | "缺少 item_name 参数" |

### 2.7 用药管理（`handlers/medications.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 药品名称为空 | `VALIDATION_ERROR` | "药品名称不能为空" |
| 用药记录不存在 | `MEDICATION_NOT_FOUND` | "用药记录不存在" |

### 2.8 体温记录（`handlers/temperatures.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 体温数据验证失败 | `VALIDATION_ERROR` | `req.validate()` 返回的动态消息 |
| 患者不存在 | `PATIENT_NOT_FOUND` | "患者不存在" |

### 2.9 管理员（`handlers/admin.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 无效角色 | `INVALID_ROLE` | "无效的角色: {role}，有效值: [...]" |

### 2.10 文件上传与 OCR（`handlers/ocr.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 文件扩展名不允许 | `FILE_TYPE_NOT_ALLOWED` | validate_file_extension 返回的消息 |
| 读取上传数据失败 | `UPLOAD_READ_FAILED` | "读取上传数据失败: {e}" |
| 文件大小超过限制 | `FILE_TOO_LARGE` | "文件大小 {n} 超过限制 {m}MB" |
| 文件魔数验证失败 | `FILE_TYPE_NOT_ALLOWED` | validate_file_magic_bytes 返回的消息 |
| 写入文件失败 | `FILE_WRITE_FAILED` | "写入文件失败: {e}" |
| 未找到上传文件 | `UPLOAD_EMPTY` | "未找到上传文件" |
| 读取上传字段失败 | `UPLOAD_READ_FAILED` | "读取上传字段失败: {e}" |
| 不支持的文件格式 | `FILE_TYPE_NOT_ALLOWED` | "不支持的文件格式，请上传 PDF 或图片文件" |
| PDF 识别失败 | `OCR_FAILED` | "PDF 识别失败: {e}" |
| OCR 也失败 | `OCR_FAILED` | "识别失败: {e}; OCR也失败: {e2}" |
| 报告类型为空 | `VALIDATION_ERROR` | "报告类型不能为空" |
| 报告日期为空 | `VALIDATION_ERROR` | "报告日期不能为空" |
| 日期格式错误 | `VALIDATION_ERROR` | validate_date 返回的消息 |

### 2.11 备份与恢复（`handlers/backup.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 备份失败 | `BACKUP_FAILED` | "备份失败: {e}" |
| 读取备份文件失败 | `BACKUP_FAILED` | "读取备份文件失败: {e}" |
| 读取上传文件失败 | `UPLOAD_READ_FAILED` | "读取上传文件失败: {e}" / "读取文件数据失败: {e}" |
| 未找到上传文件 | `UPLOAD_EMPTY` | "未找到上传文件" |
| 不是有效的 SQLite 数据库 | `INVALID_BACKUP_FILE` | "上传的文件不是有效的 SQLite 数据库" |
| 数据库缺少必要的表 | `INVALID_BACKUP_FILE` | "数据库缺少必要的表: {t}" |
| 恢复前备份失败 | `BACKUP_FAILED` | "恢复前备份失败: {e}" |
| 写入临时文件失败 | `FILE_WRITE_FAILED` | "写入临时文件失败: {e}" |
| 无法打开上传的数据库 | `INVALID_BACKUP_FILE` | "无法打开上传的数据库: {e}" |
| 附加/分离/清空/恢复失败 | `RESTORE_FAILED` | 各种 "...失败: {e}" |

### 2.12 消费清单（`handlers/expense.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 不支持的文件格式 | `FILE_TYPE_NOT_ALLOWED` | "不支持的文件格式..." |
| 消费清单识别失败 | `OCR_FAILED` | "消费清单识别失败: {e}" |
| 患者不存在 | `PATIENT_NOT_FOUND` | "患者不存在" |
| 消费记录不存在 | `EXPENSE_NOT_FOUND` | "消费记录不存在" |
| 条带识别失败 | `OCR_FAILED` | "条带识别失败: {e}" |

### 2.13 AI 解读（`handlers/interpret.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 报告不存在 | `REPORT_NOT_FOUND` | "报告不存在" |
| 缺少 report_ids | `MISSING_PARAMETER` | "缺少 report_ids 参数" |
| 患者不存在 | `PATIENT_NOT_FOUND` | "患者不存在" |
| 未找到指定报告 | `REPORT_NOT_FOUND` | "未找到指定报告" |
| 该患者暂无报告 | `REPORT_NOT_FOUND` | "该患者暂无报告" |
| 暂无趋势数据 | `NO_DATA` | "暂无趋势数据" |
| 数据点不足 | `INSUFFICIENT_DATA` | "至少需要2个数据点才能进行时间变化分析" |

### 2.14 健康评估（`handlers/health_assessment.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 患者不存在 | `PATIENT_NOT_FOUND` | "患者不存在" |

### 2.15 标准化回填（`handlers/normalize.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| LLM 标准化调用失败 | `LLM_API_FAILED` | "LLM 标准化调用失败，未获得任何映射结果" |

### 2.16 通用 / 自动转换的错误（`error.rs`）

| 错误场景 | 错误码建议 | 现有消息 |
|---------|-----------|---------|
| 数据库错误 (rusqlite) | `DATABASE_ERROR` | "数据库错误: {e}" |
| 序列化错误 (serde_json) | `SERIALIZATION_ERROR` | "序列化错误: {e}" |
| IO 错误 | `IO_ERROR` | "IO 错误: {e}" |
| spawn_blocking 失败 | `TASK_FAILED` | "任务执行失败: {e}" |

---

## 3. ErrorCode 枚举设计

```rust
/// 统一错误码。前端根据此字段做业务判断，不再依赖 message 文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    // === 认证 (AUTH_*) ===
    AuthMissingToken,         // 缺少认证令牌
    AuthInvalidToken,         // 令牌无效或已过期
    AuthInvalidCredentials,   // 用户名或密码错误
    AuthUsernameConflict,     // 用户名已存在

    // === 授权 (AUTHZ_*) ===
    AuthzInsufficientRole,    // 权限不足

    // === 验证 (VALIDATION_*) ===
    ValidationError,          // 通用验证失败（message 携带具体信息）
    MissingParameter,         // 缺少必要的请求参数
    InvalidRole,              // 无效的角色值

    // === 资源不存在 (NOT_FOUND_*) ===
    PatientNotFound,          // 患者不存在
    ReportNotFound,           // 报告不存在
    TestItemNotFound,         // 检验项目不存在
    MedicationNotFound,       // 用药记录不存在
    ExpenseNotFound,          // 消费记录不存在

    // === 数据不足 ===
    NoData,                   // 暂无数据
    InsufficientData,         // 数据不足（如趋势分析需要 >=2 个点）

    // === 文件上传 (UPLOAD_*) ===
    UploadEmpty,              // 未找到上传文件
    UploadReadFailed,         // 读取上传数据失败
    FileTooLarge,             // 文件大小超过限制
    FileTypeNotAllowed,       // 不支持的文件类型

    // === OCR / AI ===
    OcrFailed,                // OCR 识别失败
    LlmApiFailed,             // LLM API 调用失败

    // === 备份恢复 ===
    BackupFailed,             // 备份操作失败
    InvalidBackupFile,        // 上传的备份文件无效
    RestoreFailed,            // 恢复操作失败

    // === 速率限制 ===
    RateLimited,              // 请求过于频繁

    // === 系统内部 ===
    DatabaseError,            // 数据库错误
    InternalError,            // 其他内部错误
}
```

### 3.1 ErrorCode → HTTP Status 映射

```rust
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
            ErrorCode::OcrFailed
            | ErrorCode::LlmApiFailed => StatusCode::BAD_GATEWAY,

            // 500
            ErrorCode::BackupFailed
            | ErrorCode::RestoreFailed
            | ErrorCode::DatabaseError
            | ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
```

---

## 4. AppError 改造方案

### 4.1 新的 AppError 结构体

```rust
#[derive(Debug)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}
```

### 4.2 便捷构造宏/方法

```rust
impl AppError {
    // 通用快捷方法
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationError, msg)
    }
    pub fn not_found(code: ErrorCode, msg: impl Into<String>) -> Self {
        Self::new(code, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, msg)
    }

    // 特定错误快捷方法
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
        Self::new(ErrorCode::AuthzInsufficientRole, format!("权限不足: 需要 {} 或更高角色", minimum))
    }
    pub fn rate_limited() -> Self {
        Self::new(ErrorCode::RateLimited, "请求过于频繁，请稍后再试")
    }
}
```

### 4.3 新的 IntoResponse 实现

```rust
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.code.status_code();
        let body = json!({
            "success": false,
            "data": null,
            "error_code": self.code,    // 新增：机器可读的错误码
            "message": self.message,    // 保留：人类可读的消息
        });
        (status, Json(body)).into_response()
    }
}
```

### 4.4 From trait 保持不变（更新 code 字段）

```rust
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
```

### 4.5 run_blocking 更新

```rust
pub async fn run_blocking<F, T>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::new(ErrorCode::InternalError, format!("任务执行失败: {}", e)))?
}
```

---

## 5. 新响应格式对比

### 改造前
```json
{
  "success": false,
  "data": null,
  "message": "患者不存在"
}
```

### 改造后
```json
{
  "success": false,
  "data": null,
  "error_code": "PATIENT_NOT_FOUND",
  "message": "患者不存在"
}
```

速率限制改造前（无 body）→ 改造后：
```json
{
  "success": false,
  "data": null,
  "error_code": "RATE_LIMITED",
  "message": "请求过于频繁，请稍后再试"
}
```

---

## 6. 需要修改的文件及具体改动点

### 6.1 `backend/src/error.rs` — 核心改动

- 将 `AppError` 从枚举改为结构体 `{ code: ErrorCode, message: String }`
- 新增 `ErrorCode` 枚举（含 Serialize）
- 新增 `ErrorCode::status_code()` 方法
- 改写 `IntoResponse`，响应体增加 `error_code` 字段
- 更新 `From<rusqlite::Error>`、`From<serde_json::Error>`、`From<std::io::Error>`
- 更新 `run_blocking` 函数
- 新增便捷构造方法

### 6.2 `backend/src/auth.rs` — 认证/授权统一

**register 函数：**
- `AppError::BadRequest("用户名至少...")` → `AppError::validation("用户名至少...")`
- `AppError::BadRequest("密码至少...")` → `AppError::validation("密码至少...")`
- `AppError::BadRequest("角色必须是...")` → `AppError::new(ErrorCode::InvalidRole, "...")`
- `AppError::Conflict("用户名已存在")` → `AppError::new(ErrorCode::AuthUsernameConflict, "用户名已存在")`

**login 函数：**
- `AppError::BadRequest("用户名和密码不能为空")` → `AppError::validation("用户名和密码不能为空")`
- `AppError::BadRequest("用户名或密码错误")` → `AppError::invalid_credentials()`

**create_token / verify_token / hash_password / verify_password：**
- `AppError::Internal(...)` → `AppError::new(ErrorCode::InternalError, ...)`

**AuthUser::from_request_parts：**
- 手动构造的 `(StatusCode::UNAUTHORIZED, Json(...))` → 使用 `AppError::missing_token().into_response()` / `AppError::invalid_token().into_response()`

**jwt_auth_middleware：**
- 手动构造的 `(StatusCode::UNAUTHORIZED, Json(...))` → 使用 `AppError::missing_token().into_response()` / `AppError::invalid_token().into_response()`

**require_role：**
- 手动构造的 `(StatusCode::FORBIDDEN, Json(...))` → 使用 `AppError::insufficient_role(minimum_role.as_str()).into_response()`
- 手动构造的 `(StatusCode::UNAUTHORIZED, Json(...))` → 使用 `AppError::missing_token().into_response()`

### 6.3 `backend/src/middleware.rs` — 速率限制

**rate_limit 函数：**
- `Err(StatusCode::TOO_MANY_REQUESTS)` → `Err(AppError::rate_limited().into_response())`
- 需要修改函数返回类型以支持返回完整的 Response（或改用 `IntoResponse`）

### 6.4 `backend/src/handlers/patients.rs`

- `AppError::BadRequest(msg)` → `AppError::validation(msg)`
- `AppError::NotFound("患者不存在")` → `AppError::patient_not_found()`

### 6.5 `backend/src/handlers/reports.rs`

- `AppError::BadRequest(msg)` → `AppError::validation(msg)`
- `AppError::NotFound("患者不存在")` → `AppError::patient_not_found()`
- `AppError::NotFound("报告不存在")` → `AppError::report_not_found()`
- `AppError::NotFound("检验项目不存在")` → `AppError::test_item_not_found()`
- `AppError::BadRequest("缺少 item_name 参数")` → `AppError::new(ErrorCode::MissingParameter, "缺少 item_name 参数")`

### 6.6 `backend/src/handlers/medications.rs`

- `AppError::BadRequest("药品名称不能为空")` → `AppError::validation("药品名称不能为空")`
- `AppError::NotFound("用药记录不存在")` → `AppError::medication_not_found()`

### 6.7 `backend/src/handlers/temperatures.rs`

- `AppError::BadRequest(msg)` → `AppError::validation(msg)`
- `AppError::NotFound("患者不存在")` → `AppError::patient_not_found()`

### 6.8 `backend/src/handlers/admin.rs`

- `AppError::BadRequest(format!("无效的角色: {}..."))` → `AppError::new(ErrorCode::InvalidRole, format!(...))`

### 6.9 `backend/src/handlers/ocr.rs`

- `AppError::BadRequest` 与文件相关 → 使用 `ErrorCode::FileTypeNotAllowed` / `UploadEmpty` / `UploadReadFailed` / `FileTooLarge`
- `AppError::Internal("写入文件失败...")` → `AppError::new(ErrorCode::InternalError, ...)`
- `AppError::Internal("PDF 识别失败...")` → `AppError::new(ErrorCode::OcrFailed, ...)`
- `AppError::Internal("识别失败...; OCR也失败...")` → `AppError::new(ErrorCode::OcrFailed, ...)`
- `AppError::BadRequest("报告类型不能为空")` → `AppError::validation("报告类型不能为空")`
- `AppError::BadRequest("报告日期不能为空")` → `AppError::validation("报告日期不能为空")`
- `AppError::Internal("任务执行失败...")` → `AppError::new(ErrorCode::InternalError, ...)`

### 6.10 `backend/src/handlers/backup.rs`

- `AppError::Internal("备份失败: {e}")` → `AppError::new(ErrorCode::BackupFailed, ...)`
- `AppError::Internal("读取备份文件失败: {e}")` → `AppError::new(ErrorCode::BackupFailed, ...)`
- `AppError::BadRequest("读取上传文件/数据失败")` → `AppError::new(ErrorCode::UploadReadFailed, ...)`
- `AppError::BadRequest("未找到上传文件")` → `AppError::new(ErrorCode::UploadEmpty, ...)`
- `AppError::BadRequest("不是有效的 SQLite")` → `AppError::new(ErrorCode::InvalidBackupFile, ...)`
- `AppError::BadRequest("缺少必要的表")` → `AppError::new(ErrorCode::InvalidBackupFile, ...)`
- `AppError::BadRequest("无法打开数据库")` → `AppError::new(ErrorCode::InvalidBackupFile, ...)`
- `AppError::Internal("恢复前备份/附加/分离/清空/恢复失败")` → `AppError::new(ErrorCode::RestoreFailed, ...)`

### 6.11 `backend/src/handlers/expense.rs`

- 文件上传相关 → 同 ocr.rs 的处理
- `AppError::Internal("消费清单识别失败")` → `AppError::new(ErrorCode::OcrFailed, ...)`
- `AppError::NotFound("患者不存在")` → `AppError::patient_not_found()`
- `AppError::NotFound("消费记录不存在")` → `AppError::expense_not_found()`

### 6.12 `backend/src/handlers/interpret.rs`

- `AppError::NotFound("报告不存在")` → `AppError::report_not_found()`
- `AppError::BadRequest("缺少 report_ids 参数")` → `AppError::new(ErrorCode::MissingParameter, ...)`
- `AppError::NotFound("患者不存在")` → `AppError::patient_not_found()`
- `AppError::NotFound("未找到指定报告")` → `AppError::report_not_found()`
- `AppError::NotFound("该患者暂无报告")` → `AppError::new(ErrorCode::NoData, "该患者暂无报告")`
- `AppError::NotFound("暂无趋势数据")` → `AppError::new(ErrorCode::NoData, "暂无趋势数据")`
- `AppError::BadRequest("至少需要2个数据点...")` → `AppError::new(ErrorCode::InsufficientData, ...)`

### 6.13 `backend/src/handlers/health_assessment.rs`

- `AppError::NotFound("患者不存在")` → `AppError::patient_not_found()`

### 6.14 `backend/src/handlers/normalize.rs`

- `AppError::Internal("LLM 标准化调用失败...")` → `AppError::new(ErrorCode::LlmApiFailed, ...)`

### 6.15 `backend/src/handlers/stats.rs`

- `AppError::Internal("查询危急值失败: {e}")` → `AppError::new(ErrorCode::DatabaseError, ...)`

### 6.16 `backend/src/handlers/user_settings.rs`

- `AppError::Internal("加密失败: {e}")` → `AppError::new(ErrorCode::InternalError, ...)`

### 6.17 `backend/src/handlers/mod.rs`

- 无 AppError 使用，不需要修改

---

## 7. 迁移策略

### 7.1 实施步骤

1. **先改 `error.rs`**：将 AppError 从枚举改为 struct + ErrorCode 枚举，保证编译失败以找出所有使用点
2. **逐文件修改**：按编译错误提示依次修改每个文件
3. **统一 auth/middleware**：将手动构造的 JSON 响应替换为 `AppError::xxx().into_response()`
4. **测试验证**：确认所有端点返回正确的 `error_code` 字段

### 7.2 前端适配

- 错误响应新增 `error_code` 字段，`message` 保留不变
- 前端可以逐步从依赖 `message` 迁移到依赖 `error_code`
- 此改动向后兼容：`success`、`data`、`message` 字段格式不变

### 7.3 向后兼容

- 成功响应不受影响（`{ success: true, data: ..., message: "..." }`）
- 错误响应增加 `error_code` 字段，不删除任何现有字段
- HTTP 状态码保持语义一致，部分调整（如认证错误从 500 改为 401）反而是修复

---

## 8. 改动量估算

| 文件 | 改动规模 |
|------|---------|
| `error.rs` | 重写（~120 行） |
| `auth.rs` | 中等（~30 处替换） |
| `middleware.rs` | 小（~3 处替换） |
| `handlers/patients.rs` | 小（~4 处） |
| `handlers/reports.rs` | 小（~8 处） |
| `handlers/medications.rs` | 小（~3 处） |
| `handlers/temperatures.rs` | 小（~3 处） |
| `handlers/admin.rs` | 小（~1 处） |
| `handlers/ocr.rs` | 中等（~15 处） |
| `handlers/backup.rs` | 中等（~15 处） |
| `handlers/expense.rs` | 中等（~12 处） |
| `handlers/interpret.rs` | 小（~8 处） |
| `handlers/health_assessment.rs` | 小（~1 处） |
| `handlers/normalize.rs` | 小（~1 处） |
| `handlers/stats.rs` | 小（~1 处） |
| `handlers/user_settings.rs` | 小（~1 处） |
| **合计** | **~106 处改动** |
