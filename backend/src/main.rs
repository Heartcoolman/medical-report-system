mod algorithm_engine;
mod audit;
pub mod auth;
pub mod cache;
mod crypto;
pub mod db;
mod error;
mod handlers;
pub mod metrics;
pub mod middleware;
pub mod models;
mod ocr;
#[allow(dead_code)]
mod openapi;
mod routes;
pub mod search;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method};
use axum::middleware as axum_mw;
use axum::response::IntoResponse;
use db::Database;
use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

const DB_PATH: &str = "data/yiliao.db";
const UPLOADS_DIR: &str = "uploads";
const STATIC_DIR: &str = "static";

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub http_client: reqwest::Client,
    pub normalize_prefetch_cache: std::sync::Arc<
        tokio::sync::RwLock<indexmap::IndexMap<String, std::collections::HashMap<String, String>>>,
    >,
    pub normalize_prefetch_locks: std::sync::Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>,
        >,
    >,
    pub llm_cache: cache::LlmCache,
    pub metrics_handle: PrometheusHandle,
    pub patient_index: Option<std::sync::Arc<search::PatientIndex>>,
}

#[tokio::main]
async fn main() {
    // Load .env file
    dotenvy::dotenv().ok();

    // Check required API key environment variables at startup
    check_required_env_vars();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Ensure storage directories exist
    std::fs::create_dir_all("data").ok();

    // Create uploads directory
    std::fs::create_dir_all(UPLOADS_DIR).ok();

    let db = Database::new(DB_PATH).unwrap_or_else(|e| {
        eprintln!("错误: 无法初始化数据库: {}", e);
        std::process::exit(1);
    });

    // Migrate unencrypted sensitive fields if DB_ENCRYPTION_KEY is set
    match db.migrate_encrypt_sensitive_fields() {
        Ok(0) => {}
        Ok(n) => tracing::info!("已加密 {} 条患者敏感数据", n),
        Err(e) => {
            eprintln!("错误: 敏感数据加密迁移失败: {}", e);
            std::process::exit(1);
        }
    }

    // Clean up expired refresh tokens on startup
    match db.cleanup_expired_refresh_tokens() {
        Ok(0) => {}
        Ok(n) => tracing::info!("已清理 {} 条过期 refresh token", n),
        Err(e) => tracing::warn!("清理过期 refresh token 失败: {}", e),
    }
    let http_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300))
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|e| {
            eprintln!("错误: 无法创建 HTTP 客户端: {}", e);
            std::process::exit(1);
        });
    let normalize_prefetch_cache =
        std::sync::Arc::new(tokio::sync::RwLock::new(indexmap::IndexMap::new()));
    let normalize_prefetch_locks =
        std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));

    // Initialize LLM cache and Prometheus metrics
    let llm_cache = cache::LlmCache::new();
    let (prometheus_layer, metrics_handle) = metrics::setup_metrics();

    // Initialize Tantivy search index
    let patient_index = match search::PatientIndex::new("data/search_index") {
        Ok(idx) => {
            let idx = std::sync::Arc::new(idx);
            // Rebuild index from DB inside spawn_blocking to avoid
            // calling tokio::sync::Mutex::blocking_lock() in async context.
            match db.list_patients() {
                Ok(patients) => {
                    let idx_clone = idx.clone();
                    let count = patients.len();
                    let rebuild_result = tokio::task::spawn_blocking(move || {
                        for p in &patients {
                            if let Err(e) = idx_clone.add_or_update_sync(&p.id, &p.name, &p.phone, &p.notes) {
                                tracing::warn!("索引患者 {} 失败: {}", p.id, e);
                            }
                        }
                        idx_clone.commit_sync()
                    }).await;
                    match rebuild_result {
                        Ok(Ok(())) => tracing::info!("搜索索引已重建，共 {} 条患者记录", count),
                        Ok(Err(e)) => tracing::warn!("索引提交失败: {}", e),
                        Err(e) => tracing::warn!("索引重建任务失败: {}", e),
                    }
                }
                Err(e) => tracing::warn!("读取患者列表失败，跳过索引重建: {}", e),
            }
            Some(idx)
        }
        Err(e) => {
            tracing::warn!("搜索索引初始化失败，将降级使用 SQLite 搜索: {}", e);
            None
        }
    };

    // CORS: read allowed origins from ALLOWED_ORIGINS env var (comma-separated)
    // Default for development: localhost:5173, 127.0.0.1:5173, localhost:3001
    let default_origins = "http://localhost:5173,http://127.0.0.1:5173,http://localhost:3001";
    let origins_str = std::env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| default_origins.to_string());
    let origins: Vec<HeaderValue> = origins_str
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                match trimmed.parse::<HeaderValue>() {
                    Ok(v) => Some(v),
                    Err(e) => {
                        tracing::warn!("无效的 CORS origin '{}': {}", trimmed, e);
                        None
                    }
                }
            }
        })
        .collect();

    tracing::info!("CORS 允许的 origins: {:?}", origins);

    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            "X-Client-Platform".parse().unwrap(),
            "X-Client-Version".parse().unwrap(),
        ])
        .expose_headers([
            "X-API-Version".parse::<header::HeaderName>().unwrap(),
        ]);

    let state = AppState {
        db,
        http_client,
        normalize_prefetch_cache,
        normalize_prefetch_locks,
        llm_cache,
        metrics_handle,
        patient_index,
    };

    // Initialize rate limiter
    let rate_limit_state = middleware::RateLimitState::new();

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let listen_addr = format!("0.0.0.0:{}", port);

    // SPA fallback: serve index.html for non-API, non-uploads, non-static-file requests.
    // Also registered explicitly for "/" and "/index.html" so that ServeDir doesn't
    // serve the file directly (bypassing notice injection).
    let spa_fallback = axum::routing::get(serve_spa_index);

    let app = routes::build_router()
        .route("/", axum::routing::get(serve_spa_index))
        .route("/index.html", axum::routing::get(serve_spa_index))
        .fallback_service(
            ServeDir::new(STATIC_DIR).fallback(spa_fallback),
        )
        .layer(CompressionLayer::new())
        .layer(axum_mw::from_fn(middleware::security_headers))
        .layer(axum_mw::from_fn(middleware::https_redirect))
        .layer(axum_mw::from_fn_with_state(
            rate_limit_state,
            middleware::rate_limit,
        ))
        .layer(prometheus_layer)
        .layer(cors)
        .layer(DefaultBodyLimit::max(middleware::MAX_UPLOAD_SIZE))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("错误: 无法绑定监听地址 {}: {}", listen_addr, e);
            std::process::exit(1);
        });
    tracing::info!("后端服务运行在 http://{}", listen_addr);
    let shutdown = async {
        tokio::signal::ctrl_c()
            .await
            .expect("无法监听 Ctrl+C 信号");
        tracing::info!("收到关闭信号，正在优雅关闭...");
    };
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        eprintln!("错误: 服务启动失败: {}", e);
        std::process::exit(1);
    }
    tracing::info!("服务已关闭");
}

/// Serve index.html with optional update-notice banner injection.
/// Used for both the explicit "/" route and the SPA fallback.
async fn serve_spa_index() -> axum::response::Response {
    let index_path = format!("{}/index.html", STATIC_DIR);
    let notice_path = "data/update_notice.txt";

    match tokio::fs::read(&index_path).await {
        Ok(contents) => {
            let notice = tokio::fs::read_to_string(notice_path).await
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            if let Some(msg) = notice {
                let escaped = html_escape(&msg);
                let banner = format!(r#"
<style>
#__update_notice{{position:fixed;top:0;left:0;right:0;z-index:99999;
background:#f59e0b;color:#000;padding:10px 48px 10px 16px;
font-size:14px;font-family:system-ui,sans-serif;
box-shadow:0 2px 8px rgba(0,0,0,.2);line-height:1.4}}
#__update_notice button{{position:absolute;right:12px;top:50%;
transform:translateY(-50%);background:none;border:none;
cursor:pointer;font-size:20px;padding:0 4px}}
#__update_notice.hidden{{display:none}}
</style>
<div id="__update_notice">
⚠️ {escaped}
<button onclick="this.parentElement.className='hidden';sessionStorage.setItem('_un_d','1')">×</button>
</div>
<script>if(sessionStorage.getItem('_un_d')==='1'){{document.getElementById('__update_notice').className='hidden'}}</script>
"#);
                let html = String::from_utf8_lossy(&contents);
                let modified = html.replacen("</body>", &format!("{}</body>", banner), 1);
                axum::response::Html(modified).into_response()
            } else {
                axum::response::Html(contents).into_response()
            }
        }
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

/// Escape special HTML characters to prevent XSS in injected banners.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Check that required environment variables are set at startup.
/// Prints warnings for missing optional API keys.
fn check_required_env_vars() {
    // JWT_SECRET is required for authentication
    if std::env::var("JWT_SECRET").map(|v| v.is_empty()).unwrap_or(true) {
        eprintln!("错误: 环境变量 JWT_SECRET 未设置");
        std::process::exit(1);
    }

    let optional_keys = [
        ("LLM_API_KEY", "LLM 识别功能"),
        ("INTERPRET_API_KEY", "AI 智能解读功能"),
        ("SILICONFLOW_API_KEY", "视觉 OCR / 消费清单识别功能"),
    ];

    for (key, desc) in &optional_keys {
        match std::env::var(key) {
            Ok(val) if val.is_empty() => {
                eprintln!("警告: 环境变量 {} 为空，{} 将不可用", key, desc);
            }
            Err(_) => {
                eprintln!("警告: 环境变量 {} 未设置，{} 将不可用", key, desc);
            }
            Ok(_) => {
                eprintln!("信息: {} 已配置 ({})", key, mask_key(key));
            }
        }
    }

    if std::env::var("DB_ENCRYPTION_KEY").map(|v| v.is_empty()).unwrap_or(true) {
        eprintln!("警告: DB_ENCRYPTION_KEY 未设置，患者敏感数据将以明文存储");
    }

    if std::env::var("WECHAT_APPID").map(|v| v.is_empty()).unwrap_or(true) {
        eprintln!("信息: WECHAT_APPID 未设置，微信小程序登录将不可用");
    }
}

/// Mask an API key for safe logging: show first 4 chars + "***"
fn mask_key(env_key: &str) -> String {
    match std::env::var(env_key) {
        Ok(val) if val.len() > 4 => format!("{}***", &val[..4]),
        Ok(val) => format!("{}***", val),
        Err(_) => "未设置".to_string(),
    }
}
