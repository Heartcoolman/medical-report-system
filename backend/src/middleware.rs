use axum::{
    extract::ConnectInfo,
    http::{header, Request, Response, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect},
};
use crate::error::AppError;
use governor::{clock::DefaultClock, state::keyed::DefaultKeyedStateStore, Quota, RateLimiter};
use std::{net::SocketAddr, num::NonZeroU32, sync::Arc};

// --- Security Response Headers Middleware ---

pub async fn security_headers(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response<axum::body::Body> {
    let is_sw = request.uri().path() == "/sw.js";
    let is_api = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // SW must not be cached so browsers pick up updates immediately
    if is_sw {
        headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    }

    // API version identifier (only for /api/ requests)
    if is_api {
        headers.insert(
            "X-API-Version".parse::<header::HeaderName>().unwrap(),
            "v1".parse().unwrap(),
        );
    }

    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'"
            .parse()
            .unwrap(),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().unwrap(),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        "DENY".parse().unwrap(),
    );
    headers.insert(
        "X-XSS-Protection".parse::<header::HeaderName>().unwrap(),
        "1; mode=block".parse().unwrap(),
    );
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains".parse().unwrap(),
    );
    headers.insert(
        header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );

    response
}

// --- HTTPS Redirect Middleware ---

pub async fn https_redirect(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response<axum::body::Body>, StatusCode> {
    let force_https = std::env::var("FORCE_HTTPS")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if force_https {
        // Check X-Forwarded-Proto header (common behind reverse proxies)
        let is_https = request
            .headers()
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "https")
            .unwrap_or(false);

        if !is_https {
            let host = request
                .headers()
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("localhost");
            let path = request.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
            let redirect_url = format!("https://{}{}", host, path);
            return Ok(Redirect::permanent(&redirect_url).into_response());
        }
    }

    Ok(next.run(request).await)
}

// --- Rate Limiting ---

type KeyedLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

#[derive(Clone)]
pub struct RateLimitState {
    /// Global: 100 req/min per IP
    pub global: Arc<KeyedLimiter>,
    /// Auth endpoints: 5 req/min per IP
    pub auth: Arc<KeyedLimiter>,
    /// Upload endpoints: 10 req/min per IP
    pub upload: Arc<KeyedLimiter>,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            global: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(100).unwrap()),
            )),
            auth: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(5).unwrap()),
            )),
            upload: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(10).unwrap()),
            )),
        }
    }
}

fn extract_client_ip(request: &Request<axum::body::Body>) -> String {
    // Check X-Forwarded-For first (behind reverse proxy)
    if let Some(forwarded) = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first_ip) = forwarded.split(',').next() {
            return first_ip.trim().to_string();
        }
    }

    // Check X-Real-IP
    if let Some(real_ip) = request
        .headers()
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
    {
        return real_ip.trim().to_string();
    }

    // Fallback to ConnectInfo if available via extensions
    if let Some(addr) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.0.ip().to_string();
    }

    "unknown".to_string()
}

pub async fn rate_limit(
    axum::extract::State(limiter): axum::extract::State<RateLimitState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response<axum::body::Body>, AppError> {
    let client_ip = extract_client_ip(&request);
    let path = request.uri().path().to_string();

    // Normalize path: strip /api/v1 or /api prefix to get the API-relative path
    let api_path = path
        .strip_prefix("/api/v1")
        .or_else(|| path.strip_prefix("/api"));

    // Check endpoint-specific limits first
    if let Some(p) = api_path {
        if p.starts_with("/auth") {
            if limiter.auth.check_key(&client_ip).is_err() {
                tracing::warn!("速率限制: auth 端点限流 IP={}", client_ip);
                return Err(AppError::rate_limited());
            }
        } else if p == "/upload" || p == "/ocr/parse" {
            if limiter.upload.check_key(&client_ip).is_err() {
                tracing::warn!("速率限制: upload 端点限流 IP={}", client_ip);
                return Err(AppError::rate_limited());
            }
        }
    }

    // Global limit
    if limiter.global.check_key(&client_ip).is_err() {
        tracing::warn!("速率限制: 全局限流 IP={}", client_ip);
        return Err(AppError::rate_limited());
    }

    Ok(next.run(request).await)
}

// --- File Upload Security ---

/// Allowed file extensions for upload
const ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "pdf"];

/// Maximum upload size: 10MB
pub const MAX_UPLOAD_SIZE: usize = 10 * 1024 * 1024;

/// Validate file type by checking magic bytes.
/// Returns Ok(detected_extension) or Err(reason).
pub fn validate_file_magic_bytes(data: &[u8]) -> Result<&'static str, String> {
    if data.len() < 4 {
        return Err("文件太小，无法识别类型".to_string());
    }

    // JPEG: FF D8 FF
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Ok("jpg");
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if data.len() >= 8
        && data[0] == 0x89
        && data[1] == 0x50
        && data[2] == 0x4E
        && data[3] == 0x47
        && data[4] == 0x0D
        && data[5] == 0x0A
        && data[6] == 0x1A
        && data[7] == 0x0A
    {
        return Ok("png");
    }

    // GIF: GIF87a or GIF89a
    if data.len() >= 6 && &data[0..3] == b"GIF" {
        return Ok("gif");
    }

    // WebP: RIFF....WEBP
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Ok("webp");
    }

    // PDF: %PDF
    if data.len() >= 4 && &data[0..4] == b"%PDF" {
        return Ok("pdf");
    }

    Err("不支持的文件类型，仅允许 jpg/jpeg/png/gif/webp/pdf".to_string())
}

/// Validate that the file extension is allowed.
pub fn validate_file_extension(filename: &str) -> Result<(), String> {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    if ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "不支持的文件扩展名 .{}，仅允许 {}",
            ext,
            ALLOWED_EXTENSIONS.join("/")
        ))
    }
}

/// Generate a safe random filename with the detected extension.
pub fn generate_safe_filename(detected_ext: &str) -> String {
    format!("{}.{}", uuid::Uuid::new_v4(), detected_ext)
}
