use utoipa::OpenApi;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "医疗数据管理系统 API",
        version = "1.0.0",
        description = "支持 Web、iOS、Android、微信小程序、macOS 等多客户端的医疗数据管理后端 API。\n\n## 认证\n所有需要认证的接口都使用 Bearer Token 方式：\n```\nAuthorization: Bearer <access_token>\n```\n\n## 客户端标识\n所有请求建议携带以下 Header：\n- `X-Client-Platform`: 客户端平台（web / ios / android / wechat-mini / macos）\n- `X-Client-Version`: 客户端版本号（如 1.0.0）\n\n## 响应格式\n所有接口统一返回：\n```json\n{\n  \"success\": true/false,\n  \"data\": ...,\n  \"message\": \"...\",\n  \"error_code\": \"...\"  // 仅错误时\n}\n```\n\n## SSE vs 同步端点\nAI 解读和健康评估提供两种模式：\n- SSE 流式（GET）: 适用于支持 SSE 的客户端（Web、iOS、Android、macOS）\n- 同步（POST -sync）: 适用于不支持 SSE 的客户端（微信小程序）",
    ),
    servers(
        (url = "/api/v1", description = "API v1"),
        (url = "/api", description = "API v1 别名"),
    ),
    modifiers(&SecurityAddon),
    components(
        schemas(
            crate::models::Gender,
            crate::models::ItemStatus,
            crate::models::Patient,
            crate::models::Report,
            crate::models::TestItem,
            crate::models::ReportDetail,
            crate::models::ReportSummary,
            crate::models::PatientWithStats,
            crate::models::TemperatureRecord,
            crate::models::Medication,
            crate::models::FileUploadResult,
            crate::auth::LoginReq,
            crate::auth::AuthResponse,
            crate::auth::UserInfo,
            crate::error::ErrorCode,
        ),
    ),
    tags(
        (name = "认证", description = "用户注册、登录、微信登录、Token 刷新"),
        (name = "患者", description = "患者 CRUD 和列表"),
        (name = "报告", description = "检验报告管理"),
        (name = "AI 解读", description = "AI 智能解读（SSE 流式 + 同步）"),
        (name = "健康评估", description = "综合健康风险评估"),
        (name = "用药", description = "用药记录管理"),
        (name = "体温", description = "体温记录"),
        (name = "消费", description = "住院消费管理"),
        (name = "文件", description = "文件上传和下载"),
        (name = "管理", description = "管理员功能"),
    ),
    paths(
        auth_login, auth_register, auth_wechat_login, auth_me,
        auth_refresh, auth_logout, auth_bind_wechat, auth_devices,
        list_patients, get_patient, create_patient, update_patient, delete_patient,
        list_reports, get_report_detail,
        interpret_report, interpret_report_sync,
        interpret_all, interpret_all_sync,
        health_assessment, health_assessment_sync, health_assessment_cache,
        list_medications, create_medication,
        list_temperatures, create_temperature,
        upload_file, serve_file,
    ),
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some("JWT Access Token"))
                    .build(),
            ),
        );
    }
}

// ---- Path stubs for OpenAPI documentation ----

#[utoipa::path(
    post, path = "/auth/login", tag = "认证",
    request_body = crate::auth::LoginReq,
    responses(
        (status = 200, description = "登录成功"),
        (status = 401, description = "用户名或密码错误"),
    )
)]
async fn auth_login() {}

#[utoipa::path(post, path = "/auth/register", tag = "认证",
    responses((status = 201, description = "注册成功"))
)]
async fn auth_register() {}

#[utoipa::path(
    post, path = "/auth/wechat-login", tag = "认证",
    description = "微信小程序登录。客户端调用 wx.login() 获取 code 后传入。",
    responses(
        (status = 200, description = "微信登录成功"),
        (status = 401, description = "微信 code 无效"),
    )
)]
async fn auth_wechat_login() {}

#[utoipa::path(get, path = "/auth/me", tag = "认证",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "当前用户信息"))
)]
async fn auth_me() {}

#[utoipa::path(post, path = "/auth/refresh", tag = "认证",
    responses((status = 200, description = "Token 刷新成功"))
)]
async fn auth_refresh() {}

#[utoipa::path(post, path = "/auth/logout", tag = "认证",
    responses((status = 200, description = "已登出"))
)]
async fn auth_logout() {}

#[utoipa::path(post, path = "/auth/bind-wechat", tag = "认证",
    security(("bearer_auth" = [])),
    description = "已登录用户绑定微信账号",
    responses((status = 200, description = "绑定成功"))
)]
async fn auth_bind_wechat() {}

#[utoipa::path(get, path = "/auth/devices", tag = "认证",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "活跃设备列表"))
)]
async fn auth_devices() {}

#[utoipa::path(get, path = "/patients", tag = "患者",
    security(("bearer_auth" = [])),
    params(
        ("page" = Option<usize>, Query, description = "页码（从 1 开始）"),
        ("page_size" = Option<usize>, Query, description = "每页条数（默认 20，最大 100）"),
        ("q" = Option<String>, Query, description = "搜索关键词"),
    ),
    responses((status = 200, description = "患者列表"))
)]
async fn list_patients() {}

#[utoipa::path(get, path = "/patients/{id}", tag = "患者",
    security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "患者 ID")),
    responses(
        (status = 200, description = "患者详情"),
        (status = 404, description = "患者不存在"),
    )
)]
async fn get_patient() {}

#[utoipa::path(post, path = "/patients", tag = "患者",
    security(("bearer_auth" = [])),
    responses((status = 201, description = "创建成功"))
)]
async fn create_patient() {}

#[utoipa::path(put, path = "/patients/{id}", tag = "患者",
    security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "更新成功"))
)]
async fn update_patient() {}

#[utoipa::path(delete, path = "/patients/{id}", tag = "患者",
    security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "删除成功"))
)]
async fn delete_patient() {}

#[utoipa::path(get, path = "/patients/{patient_id}/reports", tag = "报告",
    security(("bearer_auth" = [])),
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "报告列表"))
)]
async fn list_reports() {}

#[utoipa::path(get, path = "/reports/{report_id}", tag = "报告",
    security(("bearer_auth" = [])),
    params(("report_id" = String, Path, description = "报告 ID")),
    responses(
        (status = 200, description = "报告详情"),
        (status = 404, description = "报告不存在"),
    )
)]
async fn get_report_detail() {}

#[utoipa::path(get, path = "/reports/{report_id}/interpret", tag = "AI 解读",
    security(("bearer_auth" = [])),
    description = "SSE 流式单份报告解读",
    params(("report_id" = String, Path, description = "报告 ID")),
    responses((status = 200, description = "SSE 事件流"))
)]
async fn interpret_report() {}

#[utoipa::path(post, path = "/reports/{report_id}/interpret-sync", tag = "AI 解读",
    security(("bearer_auth" = [])),
    description = "同步非流式报告解读（适用于微信小程序）",
    params(("report_id" = String, Path, description = "报告 ID")),
    responses(
        (status = 200, description = "解读结果 JSON"),
        (status = 404, description = "报告不存在"),
    )
)]
async fn interpret_report_sync() {}

#[utoipa::path(get, path = "/patients/{patient_id}/interpret-all", tag = "AI 解读",
    security(("bearer_auth" = [])),
    description = "SSE 流式全部报告综合解读",
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "SSE 事件流"))
)]
async fn interpret_all() {}

#[utoipa::path(post, path = "/patients/{patient_id}/interpret-all-sync", tag = "AI 解读",
    security(("bearer_auth" = [])),
    description = "同步非流式全部报告综合解读",
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "解读结果 JSON"))
)]
async fn interpret_all_sync() {}

#[utoipa::path(get, path = "/patients/{patient_id}/health-assessment", tag = "健康评估",
    security(("bearer_auth" = [])),
    description = "SSE 流式综合健康评估",
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "SSE 事件流"))
)]
async fn health_assessment() {}

#[utoipa::path(post, path = "/patients/{patient_id}/health-assessment-sync", tag = "健康评估",
    security(("bearer_auth" = [])),
    description = "同步非流式综合健康评估",
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "评估结果 JSON"))
)]
async fn health_assessment_sync() {}

#[utoipa::path(get, path = "/patients/{patient_id}/health-assessment-cache", tag = "健康评估",
    security(("bearer_auth" = [])),
    description = "获取已缓存的健康评估结果",
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "缓存的评估或空"))
)]
async fn health_assessment_cache() {}

#[utoipa::path(get, path = "/patients/{patient_id}/medications", tag = "用药",
    security(("bearer_auth" = [])),
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "用药记录列表"))
)]
async fn list_medications() {}

#[utoipa::path(post, path = "/patients/{patient_id}/medications", tag = "用药",
    security(("bearer_auth" = [])),
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 201, description = "创建成功"))
)]
async fn create_medication() {}

#[utoipa::path(get, path = "/patients/{patient_id}/temperatures", tag = "体温",
    security(("bearer_auth" = [])),
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 200, description = "体温记录列表"))
)]
async fn list_temperatures() {}

#[utoipa::path(post, path = "/patients/{patient_id}/temperatures", tag = "体温",
    security(("bearer_auth" = [])),
    params(("patient_id" = String, Path, description = "患者 ID")),
    responses((status = 201, description = "创建成功"))
)]
async fn create_temperature() {}

#[utoipa::path(post, path = "/upload", tag = "文件",
    security(("bearer_auth" = [])),
    description = "上传文件（multipart/form-data），支持 jpg/jpeg/png/gif/webp/pdf，最大 10MB",
    responses((status = 200, description = "上传成功"))
)]
async fn upload_file() {}

#[utoipa::path(get, path = "/files/{file_id}", tag = "文件",
    security(("bearer_auth" = [])),
    description = "获取已上传的文件。图片支持缩略图参数 ?w=300&q=80",
    params(
        ("file_id" = String, Path, description = "文件 ID"),
        ("w" = Option<u32>, Query, description = "缩略图宽度（仅图片）"),
        ("q" = Option<u8>, Query, description = "JPEG 压缩质量（1-100，默认 80）"),
    ),
    responses((status = 200, description = "文件内容"))
)]
async fn serve_file() {}
