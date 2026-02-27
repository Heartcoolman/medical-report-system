# API 版本前缀 & CORS 扩展 — 详细实施方案

> 本文档是 `05-api-gap-analysis.md` 中 3.1（API 版本前缀）和 3.4（CORS 扩展）的实施细化。
> 仅做规划，不修改代码。

---

## A. API 版本前缀

### A.1 现状分析

**路由定义** (`backend/src/routes.rs`):
- `build_router()` 返回一个扁平 Router，所有路由使用完整路径（如 `/api/patients`、`/api/auth/login`）
- 路由分为 5 个子组：`auth_routes()`、`readonly_routes()`、`nurse_routes()`、`doctor_routes()`、`admin_routes()`
- 每个子组内的路由都硬编码了 `/api/` 前缀
- JWT 中间件作为 layer 附加在整个 Router 上

**路由挂载** (`backend/src/main.rs:135-150`):
```rust
let app = routes::build_router()
    .route("/", axum::routing::get(serve_spa_index))
    .route("/index.html", axum::routing::get(serve_spa_index))
    .nest_service("/uploads", ServeDir::new(UPLOADS_DIR))
    .fallback_service(...)
    .layer(...)
    .with_state(state);
```
- 路由直接合并到顶层 Router，无嵌套

**JWT 路径判断** (`backend/src/auth.rs:354-361`):
```rust
if path.starts_with("/api/auth") || path == "/api/health" {
    return next.run(request).await;  // 跳过认证
}
if !path.starts_with("/api/") {
    return next.run(request).await;  // 非 API 路由跳过
}
```

**Rate Limiter 路径判断** (`backend/src/middleware.rs:157-167`):
```rust
if path.starts_with("/api/auth") { ... }
else if path == "/api/upload" || path == "/api/ocr/parse" { ... }
```

**前端 API 调用** (`frontend/src/api/client.ts`):
- 所有调用使用相对路径：`/api/patients`、`/api/auth/login` 等
- 无可配置的 base URL

---

### A.2 迁移方案

#### 核心思路

使用 Axum `nest` 将所有 API 路由嵌套到 `/api/v1` 下，同时保留 `/api` 作为 v1 的别名。

#### 步骤 1：重构 routes.rs — 去除路径前缀

将各子路由组中的 `/api/` 前缀全部去掉，只保留资源路径：

```rust
// 改造前
fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/me", get(auth::get_me))
}

// 改造后
fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/me", get(auth::get_me))
}
```

对所有 5 个子路由组同样处理，并将 health 路由也移入：

```rust
fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { Json(json!({ "status": "ok" })) }))
        .merge(auth_routes())
        .merge(readonly_routes())
        .merge(nurse_routes())
        .merge(doctor_routes())
        .merge(admin_routes())
        .layer(axum_mw::from_fn(auth::jwt_auth_middleware))
}
```

所有子路由组需去除的前缀清单（共 ~70 条路由）：

| 子组 | 路由数 | 示例变更 |
|------|--------|----------|
| health | 1 | `/api/health` -> `/health` |
| auth_routes | 3 | `/api/auth/login` -> `/auth/login` |
| readonly_routes | 17 | `/api/patients` -> `/patients` |
| nurse_routes | 2 | `/api/patients/:patient_id/temperatures` -> `/patients/:patient_id/temperatures` |
| doctor_routes | 24 | `/api/reports/:report_id` -> `/reports/:report_id` |
| admin_routes | 6 | `/api/admin/users` -> `/admin/users` |

#### 步骤 2：重构 build_router — 使用 nest

```rust
pub fn build_router() -> Router<AppState> {
    let api = api_routes();

    Router::new()
        // v1 正式路径
        .nest("/api/v1", api.clone())
        // /api 作为 v1 别名（向后兼容）
        .nest("/api", api)
}
```

**注意**：Axum 的 `Router` 实现了 `Clone`，可以挂载到两个前缀下。如果 `Clone` 有 trait bound 问题（因为 middleware layers），可以改用函数调用两次：

```rust
pub fn build_router() -> Router<AppState> {
    Router::new()
        .nest("/api/v1", api_routes())
        .nest("/api", api_routes())
}
```

#### 步骤 3：更新 JWT 中间件路径判断

JWT 中间件现在运行在 nest 内部，`request.uri().path()` 看到的是 **去掉前缀后的路径**。

Axum nest 的行为：当路由嵌套在 `/api/v1` 下时，中间件看到的 `path` 仍然是原始的完整路径（Axum layer 在 nest 之外时）。但由于 JWT 中间件是在 `api_routes()` 内部通过 `.layer()` 附加的，它看到的路径取决于 Axum 版本的行为。

**关键验证点**：需要确认 Axum 0.7 中，`nest` 内部的 `layer` 看到的 `uri().path()` 是否包含 nest 前缀。

**保守方案** — 统一匹配两种路径格式：

```rust
pub async fn jwt_auth_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // 跳过公开端点 — 兼容 /api/v1/ 和 /api/ 两种前缀
    let api_path = path
        .strip_prefix("/api/v1")
        .or_else(|| path.strip_prefix("/api"))
        .unwrap_or(&path);

    if api_path.starts_with("/auth") || api_path == "/health" {
        return next.run(request).await;
    }

    // 非 API 路由跳过
    if !path.starts_with("/api/") {
        return next.run(request).await;
    }

    // ... 后续 JWT 校验逻辑不变
}
```

**更优方案** — 将 JWT 中间件放在 nest 内部（推荐）：

如果 JWT 中间件在 `api_routes()` 内部作为 layer，Axum nest 会在调用内部路由前已经匹配了前缀。此时中间件看到的 path 可能是 stripped 过的。需要实际验证。

最安全的实现：**把 JWT 中间件保持在 api_routes() 内部**，路径判断改为匹配去前缀后的路径：

```rust
// 在 nest 内部，path 已是 /auth/login、/health 等
if api_path.starts_with("/auth") || api_path == "/health" {
    return next.run(request).await;
}
// 所有进入此 middleware 的请求都是 API 请求，无需检查 /api/ 前缀
```

#### 步骤 4：更新 Rate Limiter 路径判断

Rate limiter 在 `main.rs` 中作为全局 layer（在 nest 之外），它看到的是完整路径。需要同时匹配两种前缀：

```rust
// middleware.rs: rate_limit 函数
let path = request.uri().path().to_string();

// 归一化路径：去掉 /api/v1 或 /api 前缀，得到 API 相对路径
let api_path = path
    .strip_prefix("/api/v1")
    .or_else(|| path.strip_prefix("/api"))
    .map(|p| p.to_string());

if let Some(ref p) = api_path {
    if p.starts_with("/auth") {
        if limiter.auth.check_key(&client_ip).is_err() { ... }
    } else if p == "/upload" || p == "/ocr/parse" {
        if limiter.upload.check_key(&client_ip).is_err() { ... }
    }
}
```

#### 步骤 5：添加 X-API-Version 响应头

在 `middleware.rs` 的 `security_headers` 中添加（因为它已经处理所有响应的 header）：

```rust
pub async fn security_headers(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response<axum::body::Body> {
    let path = request.uri().path().to_string();
    // ... 现有逻辑 ...

    // API 版本标识
    if path.starts_with("/api/") {
        headers.insert(
            "X-API-Version".parse::<header::HeaderName>().unwrap(),
            "v1".parse().unwrap(),
        );
    }

    response
}
```

#### 步骤 6：前端适配

**方案 A — 环境变量配置 baseUrl（推荐）**：

在 `frontend/src/api/client.ts` 中添加 API 基础路径常量：

```typescript
// 通过 Vite 环境变量注入 API 基础路径
// 默认 /api（向后兼容），正式迁移后改为 /api/v1
const API_BASE = import.meta.env.VITE_API_BASE || '/api';

// request 函数中的调用改为：
// 原：request<T>('/api/patients')
// 新：request<T>(`${API_BASE}/patients`)
```

同时在 `frontend/.env` 中配置：
```env
VITE_API_BASE=/api/v1
```

**所有 api 对象中的路径需去掉 `/api` 前缀**，改为使用 `API_BASE`：

```typescript
// 改造前
login(username, password) {
    return request<AuthResponse>('/api/auth/login', ...);
}

// 改造后
login(username, password) {
    return request<AuthResponse>(`${API_BASE}/auth/login`, ...);
}
```

这涉及 `client.ts` 中约 60 处路径字符串的修改。建议用全局替换：
- 搜索 `'/api/` 替换为 `` `${API_BASE}/ ``
- 搜索 `"/api/` 替换为 `` `${API_BASE}/ ``
- 注意 template literal 的引号变更

**方案 B — 封装在 request 函数中（更简洁）**：

```typescript
const API_BASE = import.meta.env.VITE_API_BASE || '/api';

async function request<T>(path: string, options?: RequestInit, timeout = 12000): Promise<T> {
    // path 传入 '/patients'，自动拼接 base
    const url = `${API_BASE}${path}`;
    // ... 后续逻辑不变，使用 url 替代原来的 path
}
```

然后所有调用改为不带 `/api` 前缀：
```typescript
login(u, p) {
    return request<AuthResponse>('/auth/login', ...);
}
```

**推荐方案 B**：更干净，所有路径统一为相对于 API base 的路径。

**iOS 客户端**：在配置文件中将 `baseURL` 设为 `https://<domain>/api/v1`。

---

### A.3 需要修改的文件清单

| 文件 | 改动内容 | 行数估计 |
|------|----------|----------|
| `backend/src/routes.rs` | 所有路由去掉 `/api/` 前缀；新增 `api_routes()` 函数；`build_router()` 改用 `nest` | ~80 行 |
| `backend/src/auth.rs` | `jwt_auth_middleware` 路径判断更新 | ~10 行 |
| `backend/src/middleware.rs` | rate_limit 路径判断更新；security_headers 添加 X-API-Version | ~15 行 |
| `backend/src/main.rs` | 无需改动（build_router 内部已处理 nest） | 0 行 |
| `frontend/src/api/client.ts` | 添加 API_BASE 常量；所有路径替换 | ~60 处 |
| `frontend/.env` | 添加 `VITE_API_BASE=/api/v1` | 1 行 |

---

### A.4 迁移顺序与验证

```
1. [后端] routes.rs 重构 — 去前缀 + nest
2. [后端] auth.rs 更新路径判断
3. [后端] middleware.rs 更新路径判断 + 添加 X-API-Version
4. [后端] cargo build 验证编译通过
5. [测试] curl /api/v1/health → 200
6. [测试] curl /api/health → 200 (别名)
7. [测试] 验证 JWT 认证在两个前缀下均正常
8. [前端] client.ts 添加 API_BASE + 路径替换
9. [集成测试] 前后端联调
```

### A.5 向后兼容策略

- `/api/*` 与 `/api/v1/*` 完全等价，指向相同的处理函数
- 当未来推出 v2 时，`/api/` 别名可以切换指向 v2，或完全移除
- 建议在 v2 上线前至少保留 `/api/` 别名 6 个月
- 通过 `X-API-Version` 响应头，客户端可以检测实际使用的 API 版本

---

## B. CORS 扩展

### B.1 现状分析

**当前配置** (`backend/src/main.rs:84-115`):

```rust
let default_origins = "http://localhost:5173,http://127.0.0.1:5173,http://localhost:3001";
let origins_str = std::env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| default_origins.to_string());
let origins: Vec<HeaderValue> = origins_str.split(',')
    .filter_map(|s| { ... trimmed.parse::<HeaderValue>() ... })
    .collect();

let cors = CorsLayer::new()
    .allow_origin(origins)
    .allow_methods([GET, POST, PUT, DELETE])
    .allow_headers([CONTENT_TYPE, AUTHORIZATION, ACCEPT]);
```

- 通过 `ALLOWED_ORIGINS` 环境变量配置，逗号分隔
- 默认值仅包含开发地址
- 不支持通配符或正则
- 缺少部分常用 headers（如自定义的 `X-Client-Platform`、`X-Client-Version`）

### B.2 各端 CORS 需求分析

| 客户端 | 是否需要 CORS | 原因 |
|--------|---------------|------|
| Web 生产 | **需要** | 浏览器同源策略强制执行 |
| Web 开发 | **需要** | 前后端分离开发（不同端口） |
| iOS 原生 | **不需要** | URLSession 不受浏览器 CORS 限制 |
| Android 原生 | **不需要** | OkHttp/HttpURLConnection 不受 CORS 限制 |
| 微信小程序 | **不需要** | wx.request 不走浏览器 CORS，通过微信后台配置合法域名 |
| iOS WKWebView | **可能需要** | 如果 hybrid 场景加载远程页面，则需要 |
| Android WebView | **可能需要** | 同上 |

**结论**：CORS 主要服务于 Web 端（生产 + 开发）。移动原生端无需 CORS 配置。

### B.3 需要允许的额外 Headers

当前缺少的 request headers：

```rust
// 现有
allow_headers([CONTENT_TYPE, AUTHORIZATION, ACCEPT])

// 应添加
allow_headers([
    CONTENT_TYPE,
    AUTHORIZATION,
    ACCEPT,
    // 客户端标识头 — 前端已在发送，但 CORS 未允许
    "X-Client-Platform".parse().unwrap(),
    "X-Client-Version".parse().unwrap(),
])
```

需要暴露的 response headers（如果前端需要读取）：

```rust
.expose_headers([
    "X-API-Version".parse().unwrap(),  // 新增的版本标识
])
```

### B.4 灵活配置方案

#### 方案 A：保持当前模式 + 扩展默认值（推荐）

当前基于 `ALLOWED_ORIGINS` 环境变量的方案已经足够灵活。只需要：

1. 扩展默认值以包含常见开发地址
2. 确保生产部署时通过环境变量配置正式域名
3. 补全 allow_headers 和 expose_headers

```rust
// 扩展默认开发 origins
let default_origins = [
    "http://localhost:5173",    // Vite dev
    "http://127.0.0.1:5173",   // Vite dev (IP)
    "http://localhost:3001",    // 后端同域
    "http://localhost:3000",    // 备用端口
].join(",");

let origins_str = std::env::var("ALLOWED_ORIGINS")
    .unwrap_or_else(|_| default_origins);
```

生产环境 `.env` 配置示例：
```env
ALLOWED_ORIGINS=https://medical.example.com,https://www.medical.example.com
```

#### 方案 B：支持通配符模式

如果需要支持多个子域名（如 `*.medical.example.com`），tower-http 的 `CorsLayer` 支持 `AllowOrigin::predicate()`：

```rust
use tower_http::cors::AllowOrigin;

let origins_str = std::env::var("ALLOWED_ORIGINS")
    .unwrap_or_else(|_| default_origins);

// 检查是否包含通配符模式
let has_wildcard = origins_str.contains('*');

let cors = if has_wildcard {
    // 解析通配符模式为正则匹配
    let patterns: Vec<String> = origins_str.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            let origin_str = origin.to_str().unwrap_or("");
            patterns.iter().any(|pattern| {
                if pattern.contains('*') {
                    // 简单通配符匹配：*.example.com
                    let suffix = pattern.trim_start_matches("https://*")
                        .trim_start_matches("http://*");
                    origin_str.ends_with(suffix)
                } else {
                    origin_str == pattern
                }
            })
        }))
        // ... 其他配置
} else {
    // 精确匹配模式（现有逻辑）
    let origins: Vec<HeaderValue> = origins_str.split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    CorsLayer::new()
        .allow_origin(origins)
        // ... 其他配置
};
```

通配符配置示例：
```env
ALLOWED_ORIGINS=https://*.medical.example.com,http://localhost:5173
```

#### 方案 C：`CORS_MODE` 环境变量

对于开发环境，可以提供一键全开模式：

```rust
let cors_mode = std::env::var("CORS_MODE").unwrap_or_else(|_| "strict".to_string());

let cors = match cors_mode.as_str() {
    "permissive" => {
        tracing::warn!("CORS 模式: permissive — 仅用于开发环境!");
        CorsLayer::permissive()
    }
    _ => {
        // 正常的严格模式，从 ALLOWED_ORIGINS 读取
        CorsLayer::new()
            .allow_origin(origins)
            // ...
    }
};
```

### B.5 推荐实施方案

采用 **方案 A 为主，方案 B 为可选扩展**：

```rust
// main.rs CORS 配置（完整改造后）

let default_origins = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "http://localhost:3001",
    "http://localhost:3000",
].join(",");

let origins_str = std::env::var("ALLOWED_ORIGINS")
    .unwrap_or_else(|_| default_origins);

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
        "X-API-Version".parse().unwrap(),
    ]);
```

### B.6 需要改动的文件

| 文件 | 改动内容 | 行数估计 |
|------|----------|----------|
| `backend/src/main.rs` | allow_headers 补充 2 个自定义头；添加 expose_headers | ~5 行 |

（如果实现通配符方案 B，额外改动 ~25 行）

### B.7 部署配置参考

```env
# 开发环境 (.env)
ALLOWED_ORIGINS=http://localhost:5173,http://127.0.0.1:5173,http://localhost:3001

# 生产环境
ALLOWED_ORIGINS=https://medical.example.com

# 生产 + 预览环境
ALLOWED_ORIGINS=https://medical.example.com,https://preview.medical.example.com
```

---

## C. 综合实施路线

```
阶段 1: CORS 扩展（30 分钟）
  - 补全 allow_headers + expose_headers
  - 测试 preflight 请求

阶段 2: API 版本前缀 — 后端（1.5 小时）
  - routes.rs 重构
  - auth.rs 路径判断更新
  - middleware.rs 路径判断 + X-API-Version
  - 编译验证 + curl 测试

阶段 3: API 版本前缀 — 前端（1 小时）
  - client.ts API_BASE 改造
  - 联调验证

总计约 3 小时
```

---

## D. 风险与注意事项

1. **Axum nest 中间件路径行为**：Axum 0.7 中 `nest` 内部 layer 看到的 path 是否包含前缀，需要实际验证。建议先写一个简单测试确认。

2. **双挂载路由冲突**：`/api` 和 `/api/v1` 都 nest 同一组路由时，Axum 应该能正确路由。但 `/api/v1/health` 不应该被 `/api` 的 nest 错误匹配。需验证优先级。

3. **SSE 端点**：AI 解读等 SSE 端点（`/api/reports/:id/interpret` 等）路径更新后需特别测试流式响应是否正常。

4. **iOS 客户端同步**：如果 iOS 客户端已上线，需要确保 `/api/` 别名长期保留，避免破坏已安装的旧版本。

5. **CORS 自定义头**：当前前端已经发送 `X-Client-Platform` 和 `X-Client-Version`，但 CORS 的 `allow_headers` 中未包含。这在同域部署时不会报错（非 CORS 请求），但跨域部署会导致 preflight 失败。应尽快修复。
