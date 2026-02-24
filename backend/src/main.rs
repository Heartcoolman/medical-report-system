mod algorithm_engine;
mod db;
mod error;
mod handlers;
mod models;
mod ocr;
mod routes;

use axum::extract::DefaultBodyLimit;
use axum::http::Method;
use db::Database;
use tower_http::cors::{Any, CorsLayer};

const DB_PATH: &str = "data/yiliao.db";
const UPLOADS_DIR: &str = "uploads";
const LISTEN_ADDR: &str = "0.0.0.0:3001";

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
}

#[tokio::main]
async fn main() {
    // Load .env file
    dotenvy::dotenv().ok();

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

    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:5173".parse().unwrap(),
            "http://127.0.0.1:5173".parse().unwrap(),
            "http://localhost:3001".parse().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    let state = AppState {
        db,
        http_client,
        normalize_prefetch_cache,
        normalize_prefetch_locks,
    };

    let app = routes::build_router()
        .layer(cors)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR)
        .await
        .unwrap_or_else(|e| {
            eprintln!("错误: 无法绑定监听地址 {}: {}", LISTEN_ADDR, e);
            std::process::exit(1);
        });
    tracing::info!("后端服务运行在 http://{}", LISTEN_ADDR);
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
