use axum::{
    extract::{FromRequestParts, Path, State},
    http::{header, request::Parts, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Sha256, Digest};
use utoipa::ToSchema;

use crate::error::{AppError, ErrorCode};
use crate::AppState;

// --- Role enum for RBAC ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Doctor,
    Nurse,
    #[serde(rename = "readonly")]
    ReadOnly,
}

impl Role {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "doctor" => Some(Role::Doctor),
            "nurse" => Some(Role::Nurse),
            "readonly" => Some(Role::ReadOnly),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Doctor => "doctor",
            Role::Nurse => "nurse",
            Role::ReadOnly => "readonly",
        }
    }

    /// Check if this role has at least the given permission level.
    /// Admin > Doctor > Nurse > ReadOnly
    fn level(&self) -> u8 {
        match self {
            Role::Admin => 4,
            Role::Doctor => 3,
            Role::Nurse => 2,
            Role::ReadOnly => 1,
        }
    }

    pub fn has_at_least(&self, minimum: Role) -> bool {
        self.level() >= minimum.level()
    }
}

// --- JWT Claims ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,       // user id
    pub username: String,
    pub role: String,
    pub exp: usize,        // expiration timestamp
    pub iat: usize,        // issued at
}

// --- Request / Response DTOs ---

#[derive(Deserialize)]
pub struct RegisterReq {
    pub username: String,
    pub password: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "readonly".to_string()
}

#[derive(Deserialize, ToSchema)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub device_type: String,
}

#[derive(Serialize, ToSchema)]
pub struct AuthResponse {
    pub token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: UserInfo,
}

#[derive(Serialize, ToSchema)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
}

// --- JWT helpers ---

fn get_jwt_secret() -> String {
    std::env::var("JWT_SECRET").expect("环境变量 JWT_SECRET 未设置")
}

const ACCESS_TOKEN_EXPIRY_MINUTES: i64 = 15;
const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 30;
const REFRESH_GRACE_PERIOD_SECONDS: i64 = 5;

pub fn create_token(user_id: &str, username: &str, role: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        iat: now.timestamp() as usize,
        exp: (now + chrono::Duration::minutes(ACCESS_TOKEN_EXPIRY_MINUTES)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(get_jwt_secret().as_bytes()),
    )
    .map_err(|e| AppError::internal(format!("JWT 生成失败: {}", e)))
}

/// Generate a cryptographically secure random refresh token (base64url-encoded, 43 chars).
fn generate_refresh_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    base64_url_encode(&bytes)
}

/// Base64url encode without padding.
fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Hash a refresh token with SHA-256 for database storage.
fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extract client IP from headers (X-Forwarded-For, X-Real-IP).
fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Infer device type from headers if not explicitly provided.
fn infer_device_type(headers: &HeaderMap, explicit: &str) -> String {
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    headers
        .get("x-client-platform")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}

pub fn verify_token(token: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(get_jwt_secret().as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| AppError::internal(format!("JWT 验证失败: {}", e)))
}

// --- Password hashing ---

pub fn hash_password(password: &str) -> Result<String, AppError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::internal(format!("密码哈希失败: {}", e)))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    bcrypt::verify(password, hash)
        .map_err(|e| AppError::internal(format!("密码验证失败: {}", e)))
}

// --- Axum extractor for JWT auth ---

/// Extract authenticated user claims from the Authorization header.
/// Use this as an extractor in handler functions: `claims: AuthUser`
#[derive(Debug, Clone)]
pub struct AuthUser(pub Claims);

#[axum::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());

        let token = match auth_header {
            Some(h) if h.starts_with("Bearer ") => &h[7..],
            _ => {
                return Err(AppError::missing_token().into_response());
            }
        };

        match verify_token(token) {
            Ok(claims) => Ok(AuthUser(claims)),
            Err(_) => Err(AppError::invalid_token().into_response()),
        }
    }
}

// --- Auth handlers ---

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterReq>,
) -> Result<impl IntoResponse, AppError> {
    // Validate input
    if req.username.trim().is_empty() || req.username.len() < 3 {
        return Err(AppError::validation("用户名至少 3 个字符"));
    }
    if req.password.len() < 6 {
        return Err(AppError::validation("密码至少 6 个字符"));
    }
    let requested_role = req.role.to_lowercase();
    if !["admin", "doctor", "nurse", "readonly"].contains(&requested_role.as_str()) {
        return Err(AppError::new(
            ErrorCode::InvalidRole,
            "角色必须是 admin, doctor, nurse, readonly 之一",
        ));
    }

    let password_hash = hash_password(&req.password)?;
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let username = req.username.trim().to_string();

    let db = state.db.clone();
    let uid = user_id.clone();
    let uname = username.clone();
    let requested_role_clone = requested_role.clone();
    let (assigned_role, bootstrap_admin) = crate::error::run_blocking(move || {
        db.with_conn(|conn| {
            let user_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM users",
                [],
                |row| row.get(0),
            )?;

            let assigned_role = if user_count == 0 {
                if requested_role_clone == "readonly" {
                    "admin".to_string()
                } else {
                    requested_role_clone.clone()
                }
            } else if requested_role_clone == "readonly" {
                requested_role_clone.clone()
            } else {
                return Err(AppError::new(
                    ErrorCode::AuthzInsufficientRole,
                    "公开注册仅允许创建 readonly 账号；其他角色请由管理员分配",
                ));
            };

            // Check if username already exists
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM users WHERE username = ?1",
                    rusqlite::params![uname],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false);
            if exists {
                return Err(AppError::new(ErrorCode::AuthUsernameConflict, "用户名已存在"));
            }
            conn.execute(
                "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![uid, uname, password_hash, assigned_role, now],
            )?;
            Ok((assigned_role, user_count == 0))
        })
    })
    .await?;

    let access_token = create_token(&user_id, &username, &assigned_role)?;

    // Generate refresh token
    let raw_refresh = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw_refresh);
    let ip = extract_client_ip(&headers);
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let device_type = infer_device_type(&headers, "");
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let db2 = state.db.clone();
    let uid2 = user_id.clone();
    crate::error::run_blocking(move || {
        db2.create_refresh_token(&uid2, &token_hash, "", &device_type, &ip, &ua, &expires_at)
    })
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": {
                "token": access_token,
                "access_token": access_token,
                "refresh_token": raw_refresh,
                "expires_in": ACCESS_TOKEN_EXPIRY_MINUTES * 60,
                "user": {
                    "id": user_id,
                    "username": username,
                    "role": assigned_role,
                }
            },
            "message": if bootstrap_admin {
                "注册成功，首个账户已授予管理员权限"
            } else {
                "注册成功"
            }
        })),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> Result<impl IntoResponse, AppError> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(AppError::validation("用户名和密码不能为空"));
    }

    let username = req.username.trim().to_string();
    let password = req.password.clone();

    let db = state.db.clone();
    let (user_id, stored_hash, role) = crate::error::run_blocking(move || {
        db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, password_hash, role FROM users WHERE username = ?1",
                rusqlite::params![username],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(|_| AppError::invalid_credentials())
        })
    })
    .await?;

    if stored_hash.is_empty() || !verify_password(&password, &stored_hash)? {
        return Err(AppError::invalid_credentials());
    }

    let access_token = create_token(&user_id, &req.username.trim(), &role)?;
    let notice = check_update_notice(&headers);

    // Generate refresh token
    let raw_refresh = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw_refresh);
    let ip = extract_client_ip(&headers);
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let device_name = req.device_name.clone();
    let device_type = infer_device_type(&headers, &req.device_type);
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let db2 = state.db.clone();
    let uid2 = user_id.clone();
    crate::error::run_blocking(move || {
        db2.create_refresh_token(&uid2, &token_hash, &device_name, &device_type, &ip, &ua, &expires_at)
    })
    .await?;

    Ok(Json(json!({
        "success": true,
        "data": {
            "token": access_token,
            "access_token": access_token,
            "refresh_token": raw_refresh,
            "expires_in": ACCESS_TOKEN_EXPIRY_MINUTES * 60,
            "user": {
                "id": user_id,
                "username": req.username.trim(),
                "role": role,
            }
        },
        "message": "登录成功",
        "update_notice": notice
    })))
}

pub async fn get_me(headers: HeaderMap, auth: AuthUser) -> impl IntoResponse {
    let notice = check_update_notice(&headers);
    Json(json!({
        "success": true,
        "data": {
            "id": auth.0.sub,
            "username": auth.0.username,
            "role": auth.0.role,
        },
        "message": "ok",
        "update_notice": notice
    }))
}

// --- Refresh Token handlers ---

#[derive(Deserialize)]
pub struct RefreshReq {
    pub refresh_token: String,
}

#[derive(Deserialize)]
pub struct LogoutReq {
    pub refresh_token: String,
}

/// POST /api/auth/refresh — exchange a valid refresh token for new access + refresh tokens.
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RefreshReq>,
) -> Result<impl IntoResponse, AppError> {
    let token_hash = hash_refresh_token(&req.refresh_token);

    let db = state.db.clone();
    let hash_clone = token_hash.clone();
    let token_row = crate::error::run_blocking(move || {
        db.find_by_token_hash(&hash_clone)
    })
    .await?;

    let token_row = match token_row {
        Some(row) => row,
        None => return Err(AppError::new(ErrorCode::AuthInvalidToken, "Refresh Token 无效或已过期")),
    };

    // Check if token is expired
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if token_row.expires_at < now {
        return Err(AppError::new(ErrorCode::AuthInvalidToken, "Refresh Token 已过期"));
    }

    // Check if token has been revoked (potential replay attack)
    if token_row.revoked {
        // Check grace period for concurrent refresh requests
        let revoke_time = chrono::NaiveDateTime::parse_from_str(&token_row.last_used_at, "%Y-%m-%d %H:%M:%S")
            .unwrap_or_else(|_| chrono::Utc::now().naive_utc());
        let elapsed = chrono::Utc::now().naive_utc().signed_duration_since(revoke_time).num_seconds();

        if elapsed <= REFRESH_GRACE_PERIOD_SECONDS {
            // Within grace period — likely concurrent refresh, not replay attack.
            // Return a fresh access token using the user info from this token row.
            let db3 = state.db.clone();
            let uid = token_row.user_id.clone();
            let user_info = crate::error::run_blocking(move || {
                db3.with_conn(|conn| {
                    conn.query_row(
                        "SELECT username, role FROM users WHERE id = ?1",
                        rusqlite::params![uid],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .map_err(|_| AppError::new(ErrorCode::AuthInvalidToken, "用户不存在"))
                })
            })
            .await?;

            let new_access = create_token(&token_row.user_id, &user_info.0, &user_info.1)?;
            // Generate new refresh token
            let new_raw_refresh = generate_refresh_token();
            let new_hash = hash_refresh_token(&new_raw_refresh);
            let ip = extract_client_ip(&headers);
            let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            let expires_at = (chrono::Utc::now() + chrono::Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            let db4 = state.db.clone();
            let uid2 = token_row.user_id.clone();
            let dn = token_row.device_name.clone();
            let dt = token_row.device_type.clone();
            crate::error::run_blocking(move || {
                db4.create_refresh_token(&uid2, &new_hash, &dn, &dt, &ip, &ua, &expires_at)
            })
            .await?;

            return Ok(Json(json!({
                "success": true,
                "data": {
                    "access_token": new_access,
                    "refresh_token": new_raw_refresh,
                    "token": new_access,
                    "expires_in": ACCESS_TOKEN_EXPIRY_MINUTES * 60,
                },
                "message": "刷新成功"
            })));
        }

        // Beyond grace period — replay attack detected. Revoke entire token family.
        let db_revoke = state.db.clone();
        let start_id = token_row.id.clone();
        crate::error::run_blocking(move || {
            db_revoke.revoke_token_family(&start_id)
        })
        .await?;

        tracing::warn!(
            "Replay attack detected: revoked refresh token {} for user {}",
            token_row.id,
            token_row.user_id
        );

        return Err(AppError::new(ErrorCode::AuthInvalidToken, "Refresh Token 已被使用，为安全起见已撤销所有相关会话"));
    }

    // Token is valid — fetch user info
    let db2 = state.db.clone();
    let uid = token_row.user_id.clone();
    let (username, role) = crate::error::run_blocking(move || {
        db2.with_conn(|conn| {
            conn.query_row(
                "SELECT username, role FROM users WHERE id = ?1",
                rusqlite::params![uid],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(|_| AppError::new(ErrorCode::AuthInvalidToken, "用户不存在"))
        })
    })
    .await?;

    // Generate new tokens
    let new_access = create_token(&token_row.user_id, &username, &role)?;
    let new_raw_refresh = generate_refresh_token();
    let new_hash = hash_refresh_token(&new_raw_refresh);
    let ip = extract_client_ip(&headers);
    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let db3 = state.db.clone();
    let old_id = token_row.id.clone();
    let uid2 = token_row.user_id.clone();
    let dn = token_row.device_name.clone();
    let dt = token_row.device_type.clone();
    let new_token_id = crate::error::run_blocking(move || {
        // Create new refresh token first
        let new_id = db3.create_refresh_token(&uid2, &new_hash, &dn, &dt, &ip, &ua, &expires_at)?;
        // Revoke old and link to new
        db3.revoke_and_replace(&old_id, &new_id)?;
        // Update last_used_at on the old token (for grace period tracking)
        db3.update_refresh_token_last_used(&old_id)?;
        Ok(new_id)
    })
    .await?;

    let _ = new_token_id; // used only inside the blocking closure

    Ok(Json(json!({
        "success": true,
        "data": {
            "access_token": new_access,
            "refresh_token": new_raw_refresh,
            "token": new_access,
            "expires_in": ACCESS_TOKEN_EXPIRY_MINUTES * 60,
        },
        "message": "刷新成功"
    })))
}

/// POST /api/auth/logout — revoke the given refresh token.
pub async fn logout(
    State(state): State<AppState>,
    Json(req): Json<LogoutReq>,
) -> Result<impl IntoResponse, AppError> {
    let token_hash = hash_refresh_token(&req.refresh_token);

    let db = state.db.clone();
    crate::error::run_blocking(move || {
        if let Some(row) = db.find_by_token_hash(&token_hash)? {
            db.revoke_token(&row.id)?;
        }
        // Always return success (idempotent)
        Ok(())
    })
    .await?;

    Ok(Json(json!({
        "success": true,
        "data": null,
        "message": "已登出"
    })))
}

/// GET /api/auth/devices — list active sessions for the current user.
/// Uses AuthUser extractor for authentication (independent of middleware skip).
pub async fn list_devices(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let user_id = auth.0.sub.clone();

    let db = state.db.clone();
    let sessions = crate::error::run_blocking(move || {
        db.list_active_sessions(&user_id)
    })
    .await?;

    Ok(Json(json!({
        "success": true,
        "data": sessions,
        "message": "ok"
    })))
}

/// DELETE /api/auth/devices/:id — revoke a specific device session.
/// Uses AuthUser extractor for authentication.
pub async fn revoke_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = auth.0.sub.clone();

    let db = state.db.clone();
    crate::error::run_blocking(move || {
        let row = db.find_refresh_token_by_id_and_user(&session_id, &user_id)?;
        match row {
            Some(r) if !r.revoked => {
                db.revoke_token(&r.id)?;
                Ok(())
            }
            _ => Err(AppError::new(ErrorCode::AuthInvalidToken, "设备会话不存在或已失效")),
        }
    })
    .await?;

    Ok(Json(json!({
        "success": true,
        "data": null,
        "message": "设备已登出"
    })))
}

// --- JWT auth middleware (layer-based) ---

/// Middleware that enforces JWT authentication on all requests.
/// Runs inside `nest("/api/v1", ...)` and `nest("/api", ...)`, so the path
/// seen here is already stripped of the nest prefix.
/// Skips /auth/* and /health (public endpoints).
/// On success, injects Claims into request extensions for downstream handlers.
pub async fn jwt_auth_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Normalize path: strip /api/v1 or /api prefix if still present
    // (safety net for both nested and non-nested scenarios)
    let api_path = path
        .strip_prefix("/api/v1")
        .or_else(|| path.strip_prefix("/api"))
        .unwrap_or(&path);

    // Skip auth for public endpoints
    if api_path.starts_with("/auth") || api_path == "/health" {
        return next.run(request).await;
    }

    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let token = match auth_header.as_deref() {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return AppError::missing_token().into_response();
        }
    };

    match verify_token(token) {
        Ok(claims) => {
            let mut request = request;
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        Err(_) => AppError::invalid_token().into_response(),
    }
}

// --- RBAC middleware ---

/// Create a middleware that requires the user to have at least the given role.
/// Must be applied AFTER jwt_auth_middleware (which injects Claims into extensions).
///
/// Usage in routes:
///   .layer(axum::middleware::from_fn(require_role(Role::Doctor)))
pub fn require_role(
    minimum_role: Role,
) -> impl Fn(
    Request<axum::body::Body>,
    Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone
       + Send {
    move |request: Request<axum::body::Body>, next: Next| {
        let minimum_role = minimum_role;
        Box::pin(async move {
            // Get claims from extensions (injected by jwt_auth_middleware)
            let claims = request.extensions().get::<Claims>().cloned();

            match claims {
                Some(ref c) => {
                    let user_role = Role::from_str(&c.role).unwrap_or(Role::ReadOnly);
                    if user_role.has_at_least(minimum_role) {
                        next.run(request).await
                    } else {
                        AppError::insufficient_role(minimum_role.as_str()).into_response()
                    }
                }
                None => {
                    AppError::missing_token().into_response()
                }
            }
        })
    }
}

// --- WeChat Login ---

#[derive(Deserialize)]
pub struct WechatLoginReq {
    pub code: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub device_type: String,
    #[serde(default)]
    pub nickname: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct WechatCode2SessionResp {
    #[serde(default)]
    openid: String,
    #[serde(default)]
    session_key: String,
    #[serde(default)]
    unionid: String,
    #[serde(default)]
    errcode: i64,
    #[serde(default)]
    errmsg: String,
}

async fn wechat_code2session(
    client: &reqwest::Client,
    appid: &str,
    secret: &str,
    code: &str,
) -> Result<WechatCode2SessionResp, AppError> {
    let wx_resp = client
        .get("https://api.weixin.qq.com/sns/jscode2session")
        .query(&[
            ("appid", appid),
            ("secret", secret),
            ("js_code", code),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| AppError::internal(format!("微信 API 请求失败: {}", e)))?;

    let wx_data: WechatCode2SessionResp = wx_resp
        .json()
        .await
        .map_err(|e| AppError::internal(format!("微信 API 响应解析失败: {}", e)))?;

    if wx_data.errcode != 0 {
        return Err(AppError::new(
            ErrorCode::AuthInvalidCredentials,
            format!("微信登录失败: {} ({})", wx_data.errmsg, wx_data.errcode),
        ));
    }

    if wx_data.openid.is_empty() {
        return Err(AppError::internal("微信 API 未返回 openid"));
    }

    Ok(wx_data)
}

/// POST /api/auth/wechat-login — authenticate via WeChat Mini Program code.
pub async fn wechat_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WechatLoginReq>,
) -> Result<impl IntoResponse, AppError> {
    if req.code.trim().is_empty() {
        return Err(AppError::validation("微信登录 code 不能为空"));
    }

    let appid = std::env::var("WECHAT_APPID")
        .map_err(|_| AppError::internal("环境变量 WECHAT_APPID 未设置"))?;
    let secret = std::env::var("WECHAT_SECRET")
        .map_err(|_| AppError::internal("环境变量 WECHAT_SECRET 未设置"))?;

    let wx_data = wechat_code2session(&state.http_client, &appid, &secret, &req.code).await?;
    let openid = wx_data.openid;

    let db = state.db.clone();
    let oid = openid.clone();
    let existing = crate::error::run_blocking(move || db.find_user_by_wechat_openid(&oid)).await?;

    let (user_id, username, role) = if let Some((uid, uname, urole)) = existing {
        (uid, uname, urole)
    } else {
        let uid = uuid::Uuid::new_v4().to_string();
        let nickname = req.nickname.as_deref().unwrap_or("");
        let uname = if !nickname.is_empty() {
            nickname.to_string()
        } else {
            format!("wx_{}", &openid[..8.min(openid.len())])
        };
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let default_role = "readonly".to_string();

        let db = state.db.clone();
        let uid_c = uid.clone();
        let uname_c = uname.clone();
        let oid_c = openid.clone();
        let role_c = default_role.clone();
        crate::error::run_blocking(move || {
            db.create_wechat_user(&uid_c, &uname_c, &oid_c, &role_c, &now)
        })
        .await?;

        (uid, uname, default_role)
    };

    let access_token = create_token(&user_id, &username, &role)?;
    let notice = check_update_notice(&headers);

    let raw_refresh = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw_refresh);
    let ip = extract_client_ip(&headers);
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let device_name = req.device_name.clone();
    let device_type = infer_device_type(&headers, &req.device_type);
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let db2 = state.db.clone();
    let uid2 = user_id.clone();
    crate::error::run_blocking(move || {
        db2.create_refresh_token(&uid2, &token_hash, &device_name, &device_type, &ip, &ua, &expires_at)
    })
    .await?;

    Ok(Json(json!({
        "success": true,
        "data": {
            "token": access_token,
            "access_token": access_token,
            "refresh_token": raw_refresh,
            "expires_in": ACCESS_TOKEN_EXPIRY_MINUTES * 60,
            "user": {
                "id": user_id,
                "username": username,
                "role": role,
            }
        },
        "message": "微信登录成功",
        "update_notice": notice
    })))
}

// --- Bind WeChat ---

#[derive(Deserialize)]
pub struct BindWechatReq {
    pub code: String,
}

/// POST /api/auth/bind-wechat — bind WeChat openid to current account.
pub async fn bind_wechat(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<BindWechatReq>,
) -> Result<impl IntoResponse, AppError> {
    if req.code.trim().is_empty() {
        return Err(AppError::validation("微信 code 不能为空"));
    }

    let appid = std::env::var("WECHAT_APPID")
        .map_err(|_| AppError::internal("环境变量 WECHAT_APPID 未设置"))?;
    let secret = std::env::var("WECHAT_SECRET")
        .map_err(|_| AppError::internal("环境变量 WECHAT_SECRET 未设置"))?;

    let wx_data = wechat_code2session(&state.http_client, &appid, &secret, &req.code).await?;

    let db = state.db.clone();
    let uid = auth.0.sub.clone();
    let openid = wx_data.openid;
    crate::error::run_blocking(move || db.bind_wechat_openid(&uid, &openid)).await?;

    Ok(Json(json!({
        "success": true,
        "data": null,
        "message": "微信账号绑定成功"
    })))
}

// --- Update notice helpers ---

/// Decide whether to return an update notice for this request.
///
/// Rules (checked in order):
/// 1. `data/update_notice.txt` non-empty  → return its content (manual ops broadcast, all clients)
/// 2. Client platform is "web"            → skip version check (web is always in sync with backend)
/// 3. `data/min_client_version.txt` set   → compare X-Client-Version; if below minimum, return
///    the standard data-safety warning
fn check_update_notice(headers: &HeaderMap) -> Option<String> {
    // 1. Manual broadcast overrides everything
    if let Some(msg) = std::fs::read_to_string("data/update_notice.txt")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return Some(msg);
    }

    // 2. Web clients are always current — skip version check
    let platform = headers
        .get("x-client-platform")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if platform == "web" {
        return None;
    }

    // 3. Version-based check for non-web clients (e.g. iOS)
    let min_ver = std::fs::read_to_string("data/min_client_version.txt")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;

    let client_ver = headers
        .get("x-client-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("0.0.0");

    if version_is_less(client_ver, &min_ver) {
        Some("检测到新版本，请尽快更新 App，否则可能出现数据丢失或功能异常。".to_string())
    } else {
        None
    }
}

/// Return true if version string `a` is strictly less than `b`.
/// Compares dot-separated numeric segments (e.g. "1.0" < "1.1").
fn version_is_less(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let av = parse(a);
    let bv = parse(b);
    let len = av.len().max(bv.len());
    for i in 0..len {
        let ai = av.get(i).copied().unwrap_or(0);
        let bi = bv.get(i).copied().unwrap_or(0);
        match ai.cmp(&bi) {
            std::cmp::Ordering::Less => return true,
            std::cmp::Ordering::Greater => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    false
}
