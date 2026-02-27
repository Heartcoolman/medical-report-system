# 第一步：后端独立化

> 目标：将后端从 mono repo 拆出，独立部署为纯 API 服务，为多端对接打好地基。

---

## 1. API 版本化

### 1.1 路由前缀迁移
- 所有现有路由从 `/api/` 迁移到 `/api/v1/`
- 保留 `/api/` 作为 v1 的别名（向后兼容），后续版本走 `/api/v2/`
- 在 `routes.rs` 中用 Axum 的 `nest` 统一管理

### 1.2 版本协商
- Response Header 中返回 `X-API-Version: v1`
- 客户端通过 `Accept: application/vnd.medical.v1+json` 或 URL 前缀指定版本

---

## 2. API 规范化

### 2.1 统一响应格式
```json
{
  "success": true,
  "data": { ... },
  "message": "操作成功",
  "error_code": null,
  "timestamp": "2026-02-26T10:00:00Z",
  "request_id": "uuid"
}
```

错误响应：
```json
{
  "success": false,
  "data": null,
  "message": "患者不存在",
  "error_code": "PATIENT_NOT_FOUND",
  "timestamp": "2026-02-26T10:00:00Z",
  "request_id": "uuid"
}
```

### 2.2 业务错误码体系
| 错误码 | HTTP 状态码 | 说明 |
|--------|-----------|------|
| `AUTH_INVALID_CREDENTIALS` | 401 | 用户名或密码错误 |
| `AUTH_TOKEN_EXPIRED` | 401 | Token 已过期 |
| `AUTH_TOKEN_INVALID` | 401 | Token 无效 |
| `AUTH_PERMISSION_DENIED` | 403 | 权限不足 |
| `PATIENT_NOT_FOUND` | 404 | 患者不存在 |
| `REPORT_NOT_FOUND` | 404 | 报告不存在 |
| `VALIDATION_ERROR` | 422 | 参数校验失败 |
| `OCR_PARSE_FAILED` | 500 | OCR 识别失败 |
| `LLM_SERVICE_UNAVAILABLE` | 503 | AI 服务不可用 |
| `RATE_LIMITED` | 429 | 请求过于频繁 |

### 2.3 分页统一
```json
{
  "success": true,
  "data": {
    "items": [...],
    "total": 150,
    "page": 1,
    "page_size": 20,
    "has_next": true
  }
}
```

所有列表接口统一支持 `?page=1&page_size=20&sort=created_at&order=desc`

---

## 3. OpenAPI 文档

### 3.1 方案
使用 `utoipa` crate 为 Axum 自动生成 OpenAPI 3.0 文档

### 3.2 实施
- 为每个 handler 添加 `#[utoipa::path(...)]` 宏
- 为所有请求/响应模型派生 `ToSchema`
- 暴露 `/api/docs` (Swagger UI) 和 `/api/openapi.json`

### 3.3 文档内容要求
- 每个接口标注：认证要求、角色权限、请求/响应示例
- 按模块分组：Auth、Patients、Reports、OCR、Interpret、Temperature、Expenses、Medications、Admin

---

## 4. 认证增强

### 4.1 Refresh Token 机制
```
登录 → 返回 access_token (15min) + refresh_token (30天)
请求 → 带 access_token
过期 → POST /api/v1/auth/refresh 用 refresh_token 换新 access_token
refresh_token 过期 → 重新登录
```

数据库新增 `refresh_tokens` 表：
```sql
CREATE TABLE refresh_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    device_name TEXT,          -- "iPhone 15", "Pixel 8", "微信小程序"
    device_type TEXT,          -- ios / android / web / miniprogram
    ip_address TEXT,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_used_at DATETIME
);
```

### 4.2 设备管理
- `GET /api/v1/auth/devices` — 查看已登录设备列表
- `DELETE /api/v1/auth/devices/:id` — 踢出指定设备
- `DELETE /api/v1/auth/devices` — 踢出所有设备（除当前）

### 4.3 多认证方式预留
```rust
enum AuthMethod {
    Password,          // 用户名+密码
    WechatOAuth,       // 微信登录（小程序用）
    AppleSignIn,       // Apple 登录（iOS 用）
    // 未来可扩展
}
```

登录接口改为：
```
POST /api/v1/auth/login          — 密码登录
POST /api/v1/auth/login/wechat   — 微信登录
POST /api/v1/auth/login/apple    — Apple 登录
```

---

## 5. 文件服务

### 5.1 统一上传接口
```
POST /api/v1/files/upload
Content-Type: multipart/form-data

返回：
{
  "success": true,
  "data": {
    "file_id": "uuid",
    "url": "/api/v1/files/uuid",
    "thumbnail_url": "/api/v1/files/uuid?w=200",
    "mime_type": "image/jpeg",
    "size": 1024000
  }
}
```

### 5.2 图片处理
- 上传时自动生成缩略图（200px、400px）
- 支持 URL 参数按需裁剪：`?w=300&h=300&fit=cover`
- EXIF 方向自动修正

### 5.3 存储抽象
```rust
trait FileStorage: Send + Sync {
    async fn upload(&self, data: &[u8], mime: &str) -> Result<FileInfo>;
    async fn get(&self, file_id: &str) -> Result<Vec<u8>>;
    async fn delete(&self, file_id: &str) -> Result<()>;
}

// 实现
struct LocalFileStorage { ... }      // 当前：本地 uploads/ 目录
struct OssFileStorage { ... }        // 未来：阿里云 OSS
struct MinioFileStorage { ... }      // 未来：MinIO
```

配置文件选择：
```env
FILE_STORAGE=local              # local / oss / minio
FILE_STORAGE_PATH=./uploads     # local 模式路径
OSS_ENDPOINT=...                # OSS 配置
OSS_BUCKET=...
```

---

## 6. CORS 配置

后端独立部署后，各端域名不同，需要完善 CORS：

```env
ALLOWED_ORIGINS=https://web.example.com,capacitor://localhost,http://localhost:5173
```

- Web 生产域名
- iOS/Android WebView（如果有）
- 本地开发地址
- 小程序不走浏览器 CORS，但需要在微信后台配置合法域名

---

## 7. 独立部署

### 7.1 代码拆分
```
medical-report-system/       # 当前 mono repo
├── backend/                 → 拆为独立 repo: medical-report-api
├── frontend/                → 拆为独立 repo: medical-report-web
└── ...

或者保持 mono repo，但部署完全独立：
├── backend/    → 独立部署为 API 服务
├── frontend/   → 独立部署为静态站
└── ...
```

建议先保持 mono repo（管理方便），部署上做独立。

### 7.2 Docker 化
```dockerfile
# backend/Dockerfile
FROM rust:1.75-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/backend /usr/local/bin/
COPY --from=builder /app/static /app/static
EXPOSE 3001
CMD ["backend"]
```

### 7.3 docker-compose.yml
```yaml
version: '3.8'
services:
  api:
    build: ./backend
    ports:
      - "3001:3001"
    volumes:
      - ./data:/app/data
      - ./uploads:/app/uploads
    env_file: .env
    restart: unless-stopped
```

### 7.4 后端不再托管前端
- 移除 backend 中 serve static files 的逻辑
- 只保留 `/api/` 路由
- 前端由 Nginx / Vercel / Zeabur 独立部署

---

## 8. 健康检查 & 监控

新增接口：
```
GET /api/v1/health          — 基础健康检查（无需认证）
GET /api/v1/health/detail   — 详细状态（需 Admin）
```

返回：
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 86400,
  "database": "ok",
  "llm_service": "ok",
  "ocr_service": "degraded"
}
```

---

## 任务清单

| # | 任务 | 优先级 | 预估 |
|---|------|--------|------|
| 1 | API 路由迁移到 `/api/v1/` | P0 | 2h |
| 2 | 统一响应格式 + 错误码 | P0 | 4h |
| 3 | 分页标准化 | P0 | 2h |
| 4 | Refresh Token 机制 | P0 | 4h |
| 5 | 设备管理接口 | P1 | 3h |
| 6 | OpenAPI 文档 (utoipa) | P1 | 4h |
| 7 | 文件上传接口统一 | P1 | 3h |
| 8 | 图片缩略图生成 | P2 | 3h |
| 9 | 存储抽象层 | P2 | 3h |
| 10 | Docker 化 | P1 | 2h |
| 11 | 移除前端托管逻辑 | P1 | 1h |
| 12 | CORS 完善 | P0 | 1h |
| 13 | 健康检查接口 | P2 | 1h |
| 14 | 多认证方式预留 | P2 | 2h |

**总预估：约 35 小时（1 人 4-5 天）**

---

## 完成标准

- [ ] 所有 API 走 `/api/v1/` 前缀
- [ ] 统一响应格式，所有错误有业务错误码
- [ ] Refresh Token 机制可用，支持设备管理
- [ ] OpenAPI 文档可访问，覆盖所有接口
- [ ] Docker 一键部署
- [ ] 后端不再托管前端静态文件
- [ ] 现有 Web 端 + iOS 端对接新 API 正常
