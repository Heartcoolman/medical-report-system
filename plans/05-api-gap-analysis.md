# API 多端对接差距分析 & 调整方案

> 对比现状与多端对接需求，明确要改什么、优先级、影响范围。

---

## 一、现状总结

| 维度 | 现状 | 多端就绪？ |
|------|------|-----------|
| 响应格式 | `{ success, data, message }` 基本统一 | 基本可用，缺 error_code |
| 认证 | 单 JWT Token，24h 有效 | 不够，需要 Refresh Token |
| 分页 | 仅 patients、edit-logs 支持分页 | 需要统一 |
| 文件上传 | 返回路径字符串，不是结构化对象 | 需要调整 |
| SSE 流式 | 使用 GET + EventSource | 移动端支持困难 |
| CORS | 仅配置了 localhost | 需要扩展 |
| API 版本 | 无版本前缀 | 需要加 |
| 错误码 | 无业务错误码 | 需要加 |
| 文档 | 无 OpenAPI | 需要加 |

---

## 二、必须调整的项（P0）

### 2.1 认证流程改造

**现状问题**：
- 单 Token 24h 有效期，过期后直接踢到登录页
- 移动端用户体验差：频繁重新登录
- 无设备管理能力：不知道哪些设备登录了
- 无法踢出特定设备

**调整方案**：

```
现有流程：
  login → token (24h) → 过期 → 重新登录

目标流程：
  login → access_token (15min) + refresh_token (30天)
       → access_token 过期 → POST /api/auth/refresh → 新 access_token
       → refresh_token 过期 → 重新登录
```

**新增接口**：
| 接口 | 说明 |
|------|------|
| `POST /api/auth/refresh` | 用 refresh_token 换 access_token |
| `POST /api/auth/logout` | 注销当前设备（清除 refresh_token）|
| `GET /api/auth/devices` | 查看已登录设备 |
| `DELETE /api/auth/devices/:id` | 踢出特定设备 |

**新增数据库表**：
```sql
CREATE TABLE refresh_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    device_name TEXT,
    device_type TEXT,      -- ios / android / web / miniprogram
    ip_address TEXT,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_used_at DATETIME
);
```

**客户端适配要点**：
- 每次请求如收到 401，先尝试用 refresh_token 刷新，刷新失败再跳登录
- login 请求增加 `device_name` 和 `device_type` 参数
- 存储 refresh_token（iOS: Keychain，Android: EncryptedSharedPreferences，Web: httpOnly cookie 或 localStorage）

**影响范围**：`auth.rs`，前端 `client.ts` 的 401 处理逻辑，新增 `refresh_tokens` 表

---

### 2.2 统一业务错误码

**现状问题**：
- 错误只有 `message` 字符串，客户端只能做字符串匹配
- 国际化困难，多端处理不一致
- 无法程序化地区分具体错误类型

**调整方案**：

在 `AppError` 枚举中增加 `error_code` 字段：

```rust
// 现有
{ "success": false, "data": null, "message": "用户名或密码错误" }

// 目标
{ "success": false, "data": null, "message": "用户名或密码错误", "error_code": "AUTH_INVALID_CREDENTIALS" }
```

**错误码清单**：
| error_code | HTTP | 说明 |
|------------|------|------|
| `AUTH_INVALID_CREDENTIALS` | 401 | 用户名或密码错误 |
| `AUTH_TOKEN_EXPIRED` | 401 | Token 过期 |
| `AUTH_TOKEN_INVALID` | 401 | Token 无效 |
| `AUTH_PERMISSION_DENIED` | 403 | 权限不足 |
| `AUTH_USERNAME_EXISTS` | 409 | 用户名已存在 |
| `VALIDATION_ERROR` | 400 | 参数校验失败 |
| `PATIENT_NOT_FOUND` | 404 | 患者不存在 |
| `REPORT_NOT_FOUND` | 404 | 报告不存在 |
| `RESOURCE_NOT_FOUND` | 404 | 通用资源不存在 |
| `OCR_PARSE_FAILED` | 500 | OCR 识别失败 |
| `LLM_SERVICE_ERROR` | 503 | AI 服务不可用 |
| `RATE_LIMITED` | 429 | 请求频率超限 |
| `FILE_TOO_LARGE` | 413 | 文件超过大小限制 |
| `FILE_TYPE_NOT_ALLOWED` | 415 | 不支持的文件类型 |
| `INTERNAL_ERROR` | 500 | 内部错误 |

**影响范围**：`error.rs` 中的 `AppError` 枚举及 `IntoResponse` 实现，各 handler 的错误返回

---

### 2.3 文件上传接口规范化

**现状问题**：
- `POST /api/upload` 返回的是裸字符串 `"/uploads/xxx.jpg"`
- 移动端无法获取文件 URL（不知道 base URL）
- 无缩略图支持，移动端加载大图消耗流量

**调整方案**：

```json
// 现有响应
{ "success": true, "data": "/uploads/xxx.jpg", "message": "上传成功" }

// 目标响应
{
  "success": true,
  "data": {
    "file_id": "uuid",
    "url": "/api/files/uuid",
    "original_name": "report.jpg",
    "mime_type": "image/jpeg",
    "size": 1024000
  }
}
```

**影响范围**：`handlers/ocr.rs` 的 upload_file，前端所有使用 file_path 的地方

---

## 三、应该调整的项（P1）

### 3.1 API 版本前缀

```
现有: /api/patients
目标: /api/v1/patients
兼容: /api/* 作为 v1 别名保留一段时间
```

**实现方式**：Axum `nest("/api/v1", routes)` + 保留旧路径的 fallback

**影响范围**：`routes.rs`，所有客户端 base URL

---

### 3.2 SSE 流式接口适配

**现状问题**：
- AI 解读使用 GET + `text/event-stream`
- iOS/Android 原生对 GET SSE 支持不如 Web 方便
- 部分移动端 HTTP 库不支持 GET SSE

**可选方案**：

| 方案 | 优点 | 缺点 |
|------|------|------|
| A: 保持 GET SSE | Web 端无需改动 | 移动端需要额外库 |
| B: 改 POST SSE | 能带 body | 不标准 |
| C: 增加轮询接口 | 全平台兼容 | 实时性差 |
| D: 同时提供 SSE + 轮询 | 各端按需选择 | 维护两套 |

**建议**：采用 **方案 A**，移动端可用 OkHttp/URLSession 的 SSE 支持（目前 iOS 端已实现）。如确实有移动端不支持，再加轮询兜底。

---

### 3.3 分页标准化

**现状问题**：
- 只有 patients 和 edit-logs 支持分页
- 其他列表接口直接返回全量数组
- 数据量大时性能问题

**需要加分页的接口**：
| 接口 | 当前 | 建议 |
|------|------|------|
| `GET /patients/:id/reports` | 全量返回 | 分页 |
| `GET /patients/:id/temperatures` | 全量返回 | 分页或按时间范围 |
| `GET /patients/:id/expenses` | 全量返回 | 分页 |
| `GET /patients/:id/medications` | 全量返回 | 分页（数据量小，可后做）|
| `GET /patients/:id/timeline` | 全量返回 | 分页 |
| `GET /admin/users` | 全量返回 | 分页（数据量小，可后做）|

**统一分页参数**：`?page=1&page_size=20&sort=created_at&order=desc`

---

### 3.4 CORS 扩展

当前仅配置了开发地址，需要支持：
- Web 生产域名
- iOS WKWebView（如有 hybrid 场景）
- Android WebView
- 本地调试地址

**实现**：通过 `ALLOWED_ORIGINS` 环境变量配置，已有基础设施，只需扩展默认值。

---

---

## 四、可以后做的项（P2）

| 项目 | 说明 | 为什么可以后做 |
|------|------|--------------|
| OpenAPI 文档 | 用 `utoipa` 自动生成 Swagger | 有了本文档可先手动维护 |
| 多认证方式 | 微信登录、Apple Sign-In | 各平台上线后再做 |
| 图片缩略图 | 上传时生成多尺寸缩略图 | 当前图片不多，可后加 |
| 存储抽象层 | 支持 OSS/MinIO | 当前本地存储够用 |
| WebSocket | 替代 SSE 的实时通信 | SSE 当前够用 |

---

## 五、调整路线图

### 第一批：核心基础（约 2 天）

```
1. [4h] 统一错误码 → error.rs 改造 + 各 handler 适配
2. [4h] Refresh Token → 新表 + auth.rs 改造 + /refresh 接口
3. [2h] 文件上传响应规范化
4. [1h] CORS 扩展
5. [2h] API 版本前缀 /api/v1/
```

完成后交付物：
- 后端 API 可被多端安全对接
- 客户端可通过 error_code 做错误处理
- Token 自动续期，用户不会频繁掉线

### 第二批：体验优化（约 1.5 天）

```
6. [3h] 分页标准化（reports/temperatures/expenses）
7. [2h] 设备管理接口
8. [2h] logout 接口
```

### 第三批：文档与工具（约 1 天）

```
10. [4h] OpenAPI 文档（utoipa）
11. [2h] 健康检查接口增强
```

---

## 六、各端对接注意事项

### iOS 端
- 已有项目 [medical-report-ios](https://github.com/Heartcoolman/medical-report-ios)
- Token 存 Keychain
- SSE 用 URLSession 的 `bytes` API
- 注意 `X-Client-Platform: ios` + `X-Client-Version` 版本号

### Android 端
- Token 存 EncryptedSharedPreferences
- SSE 用 OkHttp EventSource
- 注意 `X-Client-Platform: android`

### 微信小程序
- 不走浏览器 CORS，需在微信后台配置合法域名
- Token 存 `wx.setStorageSync`
- SSE 不可用，需要轮询或分块传输方案
- `X-Client-Platform: miniprogram`

### Web 端
- 保持现状，改动最小
- 适配 Refresh Token 的 401 重试逻辑
- `X-Client-Platform: web`（已有）

---

## 七、与现有计划的关系

本文档是 `plans/01-backend-independence.md` 的实施细化版：
- **01 是战略规划**（做什么）
- **本文档是战术方案**（怎么做、先做什么、影响什么）

两者目标一致，本文档按优先级重新排列，并补充了差距分析和各端适配要点。
