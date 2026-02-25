use axum::{
    extract::{FromRequestParts, State},
    http::{header, request::Parts, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppError;
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

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
}

// --- JWT helpers ---

fn get_jwt_secret() -> String {
    std::env::var("JWT_SECRET").expect("环境变量 JWT_SECRET 未设置")
}

const TOKEN_EXPIRY_HOURS: i64 = 24;

pub fn create_token(user_id: &str, username: &str, role: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        iat: now.timestamp() as usize,
        exp: (now + chrono::Duration::hours(TOKEN_EXPIRY_HOURS)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(get_jwt_secret().as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("JWT 生成失败: {}", e)))
}

pub fn verify_token(token: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(get_jwt_secret().as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| AppError::Internal(format!("JWT 验证失败: {}", e)))
}

// --- Password hashing ---

pub fn hash_password(password: &str) -> Result<String, AppError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::Internal(format!("密码哈希失败: {}", e)))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    bcrypt::verify(password, hash)
        .map_err(|e| AppError::Internal(format!("密码验证失败: {}", e)))
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
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "success": false,
                        "data": null,
                        "message": "缺少认证令牌"
                    })),
                )
                    .into_response());
            }
        };

        match verify_token(token) {
            Ok(claims) => Ok(AuthUser(claims)),
            Err(_) => Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "data": null,
                    "message": "认证令牌无效或已过期"
                })),
            )
                .into_response()),
        }
    }
}

// --- Auth handlers ---

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<impl IntoResponse, AppError> {
    // Validate input
    if req.username.trim().is_empty() || req.username.len() < 3 {
        return Err(AppError::BadRequest("用户名至少 3 个字符".to_string()));
    }
    if req.password.len() < 6 {
        return Err(AppError::BadRequest("密码至少 6 个字符".to_string()));
    }
    let role = req.role.to_lowercase();
    if !["admin", "doctor", "nurse", "readonly"].contains(&role.as_str()) {
        return Err(AppError::BadRequest(
            "角色必须是 admin, doctor, nurse, readonly 之一".to_string(),
        ));
    }

    let password_hash = hash_password(&req.password)?;
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let username = req.username.trim().to_string();

    let db = state.db.clone();
    let uid = user_id.clone();
    let uname = username.clone();
    let r = role.clone();
    crate::error::run_blocking(move || {
        db.with_conn(|conn| {
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
                return Err(AppError::Conflict("用户名已存在".to_string()));
            }
            conn.execute(
                "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![uid, uname, password_hash, r, now],
            )?;
            Ok(())
        })
    })
    .await?;

    let token = create_token(&user_id, &username, &role)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": {
                "token": token,
                "user": {
                    "id": user_id,
                    "username": username,
                    "role": role,
                }
            },
            "message": "注册成功"
        })),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<impl IntoResponse, AppError> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(AppError::BadRequest("用户名和密码不能为空".to_string()));
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
            .map_err(|_| AppError::BadRequest("用户名或密码错误".to_string()))
        })
    })
    .await?;

    if !verify_password(&password, &stored_hash)? {
        return Err(AppError::BadRequest("用户名或密码错误".to_string()));
    }

    let token = create_token(&user_id, &req.username.trim(), &role)?;

    Ok(Json(json!({
        "success": true,
        "data": {
            "token": token,
            "user": {
                "id": user_id,
                "username": req.username.trim(),
                "role": role,
            }
        },
        "message": "登录成功"
    })))
}

pub async fn get_me(auth: AuthUser) -> impl IntoResponse {
    Json(json!({
        "success": true,
        "data": {
            "id": auth.0.sub,
            "username": auth.0.username,
            "role": auth.0.role,
        },
        "message": "ok"
    }))
}

// --- JWT auth middleware (layer-based) ---

/// Middleware that enforces JWT authentication on all requests.
/// Skips /api/auth/* and /api/health.
/// On success, injects Claims into request extensions for downstream handlers.
pub async fn jwt_auth_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Skip auth for public endpoints
    if path.starts_with("/api/auth") || path == "/api/health" {
        return next.run(request).await;
    }

    // Skip auth for non-API routes (static files, SPA, uploads)
    if !path.starts_with("/api/") {
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
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "data": null,
                    "message": "缺少认证令牌"
                })),
            )
                .into_response();
        }
    };

    match verify_token(token) {
        Ok(claims) => {
            let mut request = request;
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "success": false,
                "data": null,
                "message": "认证令牌无效或已过期"
            })),
        )
            .into_response(),
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
                        (
                            StatusCode::FORBIDDEN,
                            Json(json!({
                                "success": false,
                                "data": null,
                                "message": format!(
                                    "权限不足: 需要 {} 或更高角色",
                                    minimum_role.as_str()
                                )
                            })),
                        )
                            .into_response()
                    }
                }
                None => {
                    // No claims = not authenticated (shouldn't happen if jwt_auth_middleware ran)
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({
                            "success": false,
                            "data": null,
                            "message": "未认证"
                        })),
                    )
                        .into_response()
                }
            }
        })
    }
}
