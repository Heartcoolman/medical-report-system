# Refresh Token 认证改造详细规划

> 将现有单 JWT(24h) 认证升级为 Access Token + Refresh Token 双 Token 机制，
> 支持多设备管理、Token 轮换、安全登出。

---

## 一、现状分析

### 1.1 现有认证流程

```
login → JWT access_token (24h) → 过期 → 重新登录
```

**关键代码位置**：

| 文件 | 内容 |
|------|------|
| `backend/src/auth.rs:113` | `TOKEN_EXPIRY_HOURS = 24` |
| `backend/src/auth.rs:65-72` | `Claims { sub, username, role, exp, iat }` |
| `backend/src/auth.rs:115-130` | `create_token()` — 用 `jsonwebtoken` 生成 JWT |
| `backend/src/auth.rs:132-140` | `verify_token()` — 验证 JWT 签名+过期 |
| `backend/src/auth.rs:158-202` | `AuthUser` extractor — 从 `Authorization: Bearer` 提取 Claims |
| `backend/src/auth.rs:347-400` | `jwt_auth_middleware` — 跳过 `/api/auth/*` 和 `/api/health` |
| `backend/src/auth.rs:275-326` | `login()` handler — 返回 `{ token, user }` |
| `frontend/src/api/client.ts:45` | `TOKEN_KEY = 'auth_token'` — localStorage 存储 |
| `frontend/src/api/client.ts:58-61` | 请求时自动注入 `Authorization: Bearer` |
| `frontend/src/api/client.ts:74-80` | 401 处理：清 token、跳转 `/login` |

### 1.2 现有问题

1. **24h 有效期**：移动端用户每天至少重新登录一次，体验差
2. **无法续期**：token 过期后只能重新登录，无静默续期能力
3. **无设备管理**：不知道哪些设备/浏览器登录了
4. **无法踢出设备**：管理员无法远程登出特定设备
5. **长有效期安全风险**：24h 的 token 若泄露，攻击窗口太长

---

## 二、目标流程

```
login → access_token (15min) + refresh_token (30天)
     → access_token 过期
     → POST /api/auth/refresh (带 refresh_token)
     → 新 access_token + 新 refresh_token（旧 refresh_token 失效）
     → refresh_token 过期 → 重新登录
```

### 2.1 Token 类型对比

| 属性 | Access Token | Refresh Token |
|------|-------------|---------------|
| 格式 | JWT (无状态) | 不透明随机字符串 (有状态) |
| 有效期 | 15 分钟 | 30 天 |
| 存储位置(后端) | 不存储 | 数据库 `refresh_tokens` 表 |
| 存储位置(前端) | `localStorage` / 内存 | `localStorage` |
| 携带方式 | `Authorization: Bearer <token>` | 仅在 `/api/auth/refresh` 请求体中 |
| 撤销机制 | 过期自动失效 | 数据库标记 `revoked = 1` |

---

## 三、数据库设计

### 3.1 新建 `refresh_tokens` 表

```sql
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id TEXT PRIMARY KEY,                    -- UUID v4
    user_id TEXT NOT NULL,                  -- 关联 users.id
    token_hash TEXT NOT NULL UNIQUE,        -- SHA-256(refresh_token)，不存明文
    device_name TEXT NOT NULL DEFAULT '',   -- 设备名称，如 "iPhone 15 Pro"
    device_type TEXT NOT NULL DEFAULT '',   -- 设备类型: ios / android / web / unknown
    ip_address TEXT NOT NULL DEFAULT '',    -- 登录时 IP
    user_agent TEXT NOT NULL DEFAULT '',    -- User-Agent 头
    created_at TEXT NOT NULL,               -- 创建时间
    expires_at TEXT NOT NULL,               -- 过期时间
    last_used_at TEXT NOT NULL,             -- 最后使用时间（每次 refresh 更新）
    revoked INTEGER NOT NULL DEFAULT 0,     -- 0=有效, 1=已撤销
    replaced_by TEXT,                       -- 轮换时指向新 token 的 id
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user
    ON refresh_tokens(user_id, revoked, expires_at);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash
    ON refresh_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires
    ON refresh_tokens(expires_at);
```

### 3.2 设计说明

- **`token_hash`**：存储 `SHA-256(原始refresh_token)` 而非明文，即使数据库泄露也无法伪造 refresh_token
- **`device_name` / `device_type`**：前端登录时传入，用于"已登录设备"列表展示
- **`replaced_by`**：实现 Token 轮换追踪，检测 replay attack（详见 4.3）
- **`last_used_at`**：每次 refresh 时更新，用于展示"最后活跃时间"
- **`ip_address` / `user_agent`**：审计用途，帮助用户辨认陌生设备

### 3.3 迁移方式

沿用项目现有的迁移模式（在 `Database::new()` 中执行 `CREATE TABLE IF NOT EXISTS`），
新增表直接加在 `conn.execute_batch(...)` 最后即可。无需额外迁移框架。

---

## 四、Refresh Token 生成与安全策略

### 4.1 Token 生成算法

```rust
use rand::Rng;
use sha2::{Sha256, Digest};

/// 生成 256-bit (32 字节) 密码学安全随机数，
/// base64url 编码后作为 refresh_token 原文。
fn generate_refresh_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    base64_url_encode(&bytes) // 结果约 43 字符
}

/// 对 refresh_token 计算 SHA-256 哈希，用于数据库存储。
fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
```

依赖：项目已有 `rand = "0.8"` 和 `hex = "0.4"`，需新增 `sha2 = "0.10"`。
也可用已有的 `aes-gcm` 里自带的 SHA-256，但 `sha2` crate 更直观。

### 4.2 Access Token 调整

将 `TOKEN_EXPIRY_HOURS` 改为 `ACCESS_TOKEN_EXPIRY_MINUTES`：

```rust
// 改前
const TOKEN_EXPIRY_HOURS: i64 = 24;
// exp = now + Duration::hours(24)

// 改后
const ACCESS_TOKEN_EXPIRY_MINUTES: i64 = 15;
// exp = now + Duration::minutes(15)

const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 30;
```

### 4.3 Token 轮换策略（Rotation）

每次调用 `/api/auth/refresh` 时：

1. 验证请求中的 `refresh_token` — 计算 `SHA-256(token)` 查数据库
2. 检查：未过期 AND `revoked = 0`
3. **生成新的 refresh_token**
4. **将旧 refresh_token 标记为 `revoked = 1`，`replaced_by = 新token_id`**
5. 插入新 refresh_token 记录
6. 返回新的 `access_token` + 新的 `refresh_token`

**Replay Attack 检测**：
如果收到一个已经被 `revoked` 的 refresh_token（即有人在重放旧 token）：
- 通过 `replaced_by` 链追踪，**撤销该用户在该设备的整条 token 链**
- 这意味着攻击者和合法用户都需要重新登录——这是安全设计的正确行为
- 记录审计日志

```
正常流程：
  RT_1 (active) → refresh → RT_1 (revoked, replaced_by=RT_2), RT_2 (active)
                           → refresh → RT_2 (revoked, replaced_by=RT_3), RT_3 (active)

Replay Attack 检测：
  攻击者用 RT_1 (已revoked) 请求 refresh
  → 检测到 revoked token 被重用
  → 沿 replaced_by 链找到 RT_2, RT_3，全部 revoke
  → 攻击者和合法用户都被强制重新登录
```

### 4.4 并发刷新处理

场景：前端有多个并行请求同时发现 access_token 过期，同时触发 refresh。

**前端策略（推荐，主要防线）**：
- 在 `client.ts` 的 `request()` 函数中加入**刷新锁**
- 第一个收到 401 的请求触发 refresh，后续请求排队等待
- refresh 完成后，所有排队请求用新 access_token 重试

```typescript
let refreshPromise: Promise<string> | null = null;

async function refreshAccessToken(): Promise<string> {
  if (refreshPromise) return refreshPromise; // 复用正在进行的 refresh

  refreshPromise = doRefresh().finally(() => {
    refreshPromise = null;
  });
  return refreshPromise;
}
```

**后端容错（辅助防线）**：
- 旧 refresh_token 被 revoke 后，设置 **5秒宽限期**：
  如果在被 revoke 后 5 秒内又被使用，且 `replaced_by` 的 token 仍然有效，
  则视为并发请求而非 replay attack，返回与 `replaced_by` 相同的新 token
- 超过 5 秒则按 replay attack 处理

```rust
const REFRESH_GRACE_PERIOD_SECONDS: i64 = 5;
```

---

## 五、接口设计

### 5.1 POST /api/auth/login — 改造

**请求体**（向后兼容，新增字段均 optional）：

```json
{
  "username": "admin",
  "password": "123456",
  "device_name": "iPhone 15 Pro",     // 可选，默认 ""
  "device_type": "ios"                 // 可选: ios / android / web，默认从 X-Client-Platform 推断
}
```

**响应体**：

```json
{
  "success": true,
  "data": {
    "access_token": "eyJhbGciOiJI...",
    "refresh_token": "a1b2c3d4e5f6...",
    "expires_in": 900,
    "user": {
      "id": "uuid",
      "username": "admin",
      "role": "admin"
    }
  },
  "message": "登录成功",
  "update_notice": null
}
```

**向后兼容**：
- 响应中同时保留 `token` 字段（= `access_token`），供旧版客户端使用
- 即 `data.token = data.access_token`
- 旧版客户端不传 `device_name`/`device_type` 也能正常登录

**Rust 改动**：

```rust
#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_name: String,     // 新增
    #[serde(default)]
    pub device_type: String,     // 新增
}
```

### 5.2 POST /api/auth/refresh — 新增

**请求体**：

```json
{
  "refresh_token": "a1b2c3d4e5f6..."
}
```

**成功响应 (200)**：

```json
{
  "success": true,
  "data": {
    "access_token": "eyJhbGciOiJI...(新)",
    "refresh_token": "x9y8z7w6...(新，旧的已失效)",
    "token": "eyJhbGciOiJI...(同 access_token，兼容旧客户端)",
    "expires_in": 900
  },
  "message": "刷新成功"
}
```

**失败响应 (401)**：

```json
{
  "success": false,
  "data": null,
  "message": "Refresh Token 无效或已过期"
}
```

**此接口不需要 JWT 认证**（列入 `/api/auth/` 前缀，被 `jwt_auth_middleware` 跳过）。

### 5.3 POST /api/auth/logout — 新增

**请求头**：`Authorization: Bearer <access_token>`（可选，用于审计）

**请求体**：

```json
{
  "refresh_token": "a1b2c3d4e5f6..."
}
```

**响应**：

```json
{
  "success": true,
  "data": null,
  "message": "已登出"
}
```

**逻辑**：
1. 计算 `SHA-256(refresh_token)`，查找记录
2. 将该记录的 `revoked` 设为 1
3. 如果带了有效的 access_token，记录审计日志
4. 无论 refresh_token 是否有效，都返回成功（幂等）

**此接口放在 `/api/auth/` 下，可以不需要 JWT 认证**（因为 refresh_token 本身就是凭证）。

### 5.4 GET /api/auth/devices — 新增

**请求头**：`Authorization: Bearer <access_token>`（必须）

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": "session-uuid-1",
      "device_name": "iPhone 15 Pro",
      "device_type": "ios",
      "ip_address": "192.168.1.100",
      "created_at": "2026-02-20 10:00:00",
      "last_used_at": "2026-02-27 08:30:00",
      "is_current": true
    },
    {
      "id": "session-uuid-2",
      "device_name": "Chrome on MacOS",
      "device_type": "web",
      "ip_address": "192.168.1.101",
      "created_at": "2026-02-25 14:00:00",
      "last_used_at": "2026-02-26 20:15:00",
      "is_current": false
    }
  ],
  "message": "ok"
}
```

**逻辑**：
1. 从 access_token 中提取 `user_id`（通过 `AuthUser` extractor）
2. 查询 `refresh_tokens WHERE user_id = ? AND revoked = 0 AND expires_at > datetime('now')`
3. 当前 session 的 `is_current` 标记：请求头中带的 access_token 的 `iat` 时间匹配哪个 refresh_token 的 `created_at`

**注意**：此接口需要认证，但放在 `auth_routes()` 中。由于 `jwt_auth_middleware` 跳过所有 `/api/auth/*`，需要两种选择：
- **方案 A**：将此路由放到 `readonly_routes()` 而非 `auth_routes()`
- **方案 B**：在 handler 内部手动验证 JWT（使用 `AuthUser` extractor 即可，因为 extractor 有自己的验证逻辑）

**推荐方案 B**：handler 使用 `AuthUser` extractor，即使中间件跳过了 auth 检查，extractor 仍会独立验证 JWT。这已经是 `get_me` handler 的做法（它在 `auth_routes()` 中但使用了 `auth: AuthUser` 参数）。

### 5.5 DELETE /api/auth/devices/:id — 新增

**请求头**：`Authorization: Bearer <access_token>`（必须）

**响应**：

```json
{
  "success": true,
  "data": null,
  "message": "设备已登出"
}
```

**逻辑**：
1. 从 access_token 中提取 `user_id`
2. 查找 `refresh_tokens WHERE id = :id AND user_id = ?`（确保只能踢自己的设备）
3. 将 `revoked` 设为 1
4. 如果不存在或已经 revoked，返回 404

**Admin 增强（可选，Phase 2）**：Admin 可以踢任何用户的设备。

### 5.6 POST /api/auth/register — 改造

与 login 类似，注册成功后也返回 `access_token` + `refresh_token`：

```json
{
  "success": true,
  "data": {
    "access_token": "...",
    "refresh_token": "...",
    "token": "...(同 access_token，兼容)",
    "expires_in": 900,
    "user": { "id": "...", "username": "...", "role": "..." }
  },
  "message": "注册成功"
}
```

---

## 六、具体代码改动清单

### 6.1 backend/Cargo.toml

```diff
+ sha2 = "0.10"
```

已有的 `rand`, `hex`, `chrono`, `uuid`, `jsonwebtoken` 不需要新增。

### 6.2 backend/src/db/mod.rs

在 `Database::new()` 的 `conn.execute_batch(...)` 中追加：

```sql
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL DEFAULT '',
    device_type TEXT NOT NULL DEFAULT '',
    ip_address TEXT NOT NULL DEFAULT '',
    user_agent TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_used_at TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0,
    replaced_by TEXT,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user
    ON refresh_tokens(user_id, revoked, expires_at);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash
    ON refresh_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires
    ON refresh_tokens(expires_at);
```

在 `mod.rs` 顶部新增模块声明：

```rust
mod refresh_token_repo;
```

### 6.3 backend/src/db/refresh_token_repo.rs — 新建

```rust
// 函数清单：
pub fn create_refresh_token(conn, user_id, token_hash, device_name, device_type,
                            ip_address, user_agent, expires_at) -> Result<String, AppError>
pub fn find_by_token_hash(conn, token_hash) -> Result<RefreshTokenRow, AppError>
pub fn revoke_token(conn, id) -> Result<(), AppError>
pub fn revoke_and_replace(conn, old_id, new_id) -> Result<(), AppError>
pub fn revoke_token_family(conn, start_id) -> Result<u32, AppError>  // 沿 replaced_by 链全部 revoke
pub fn list_active_sessions(conn, user_id) -> Result<Vec<DeviceSession>, AppError>
pub fn revoke_all_user_tokens(conn, user_id) -> Result<u32, AppError>
pub fn cleanup_expired_tokens(conn) -> Result<u32, AppError>  // 定期清理
pub fn update_last_used(conn, id) -> Result<(), AppError>
```

数据结构：

```rust
pub struct RefreshTokenRow {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub device_name: String,
    pub device_type: String,
    pub ip_address: String,
    pub user_agent: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_used_at: String,
    pub revoked: bool,
    pub replaced_by: Option<String>,
}

pub struct DeviceSession {
    pub id: String,
    pub device_name: String,
    pub device_type: String,
    pub ip_address: String,
    pub created_at: String,
    pub last_used_at: String,
}
```

### 6.4 backend/src/auth.rs

**修改清单**：

| 行号 | 变更 | 说明 |
|------|------|------|
| L88-92 | 修改 `LoginReq` | 新增 `device_name`, `device_type` (可选字段) |
| L94-98 | 修改 `AuthResponse` | 新增 `access_token`, `refresh_token`, `expires_in`，保留 `token` 兼容 |
| L113 | 修改常量 | `TOKEN_EXPIRY_HOURS → ACCESS_TOKEN_EXPIRY_MINUTES = 15` |
| L113+ | 新增常量 | `REFRESH_TOKEN_EXPIRY_DAYS = 30`, `REFRESH_GRACE_PERIOD_SECONDS = 5` |
| L115-130 | 修改 `create_token()` | 使用 `Duration::minutes(15)` |
| L275-326 | 修改 `login()` | 生成 refresh_token 并存入 DB，响应包含两个 token |
| L206-273 | 修改 `register()` | 同上 |
| 新增 | `refresh()` handler | 处理 POST /api/auth/refresh |
| 新增 | `logout()` handler | 处理 POST /api/auth/logout |
| 新增 | `list_devices()` handler | 处理 GET /api/auth/devices |
| 新增 | `revoke_device()` handler | 处理 DELETE /api/auth/devices/:id |
| 新增 | `generate_refresh_token()` | 生成安全随机 token |
| 新增 | `hash_refresh_token()` | SHA-256 哈希 |

**新增辅助函数**：

```rust
fn generate_refresh_token() -> String { ... }
fn hash_refresh_token(token: &str) -> String { ... }

fn extract_client_ip(headers: &HeaderMap) -> String {
    // 优先 X-Forwarded-For, 其次 X-Real-IP, 否则 ""
    headers.get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(',').next().unwrap_or("")
        .trim().to_string()
}

fn infer_device_type(headers: &HeaderMap, explicit: &str) -> String {
    if !explicit.is_empty() { return explicit.to_string(); }
    headers.get("x-client-platform")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}
```

### 6.5 backend/src/routes.rs

修改 `auth_routes()` 函数：

```rust
fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/me", get(auth::get_me))
        // 新增
        .route("/api/auth/refresh", post(auth::refresh))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/devices", get(auth::list_devices))
        .route("/api/auth/devices/:id", axum::routing::delete(auth::revoke_device))
}
```

注意：`/api/auth/devices` 和 `/api/auth/devices/:id` 虽然在 `auth_routes()` 中（被全局 JWT 中间件跳过），但 handler 内部通过 `AuthUser` extractor 强制认证。`/api/auth/refresh` 和 `/api/auth/logout` 不需要 JWT 认证。

### 6.6 frontend/src/api/client.ts

**主要改动**：

#### a) Token 存储 — 双 Key

```typescript
const ACCESS_TOKEN_KEY = 'auth_token'        // 保持不变，兼容
const REFRESH_TOKEN_KEY = 'refresh_token'    // 新增
```

#### b) 登录响应处理

```typescript
login(username: string, password: string) {
  return request<AuthResponse>('/api/auth/login', jsonRequest('POST', {
    username,
    password,
    device_name: getDeviceName(),  // 新增
    device_type: 'web',            // 新增
  })).then(data => {
    // 存储两个 token
    localStorage.setItem(ACCESS_TOKEN_KEY, data.access_token);
    localStorage.setItem(REFRESH_TOKEN_KEY, data.refresh_token);
    return data;
  });
},
```

#### c) 401 处理 — 自动刷新

替换现有的硬跳转逻辑，改为尝试 refresh：

```typescript
let refreshPromise: Promise<string> | null = null;

async function tryRefreshToken(): Promise<string> {
  if (refreshPromise) return refreshPromise;

  const refreshToken = localStorage.getItem(REFRESH_TOKEN_KEY);
  if (!refreshToken) throw new Error('无 refresh token');

  refreshPromise = fetch('/api/auth/refresh', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: refreshToken }),
  })
    .then(async (res) => {
      if (!res.ok) throw new Error('refresh 失败');
      const json = await res.json();
      const newAccessToken = json.data.access_token;
      const newRefreshToken = json.data.refresh_token;
      localStorage.setItem(ACCESS_TOKEN_KEY, newAccessToken);
      localStorage.setItem(REFRESH_TOKEN_KEY, newRefreshToken);
      return newAccessToken;
    })
    .finally(() => { refreshPromise = null; });

  return refreshPromise;
}
```

在 `request()` 函数中：

```typescript
// 替换现有 401 处理
if (res.status === 401) {
  try {
    const newToken = await tryRefreshToken();
    // 用新 token 重试原请求
    headers.set('Authorization', `Bearer ${newToken}`);
    const retryRes = await fetch(url, { ...options, headers, signal: controller.signal });
    // ... 解析 retryRes
  } catch {
    localStorage.removeItem(ACCESS_TOKEN_KEY);
    localStorage.removeItem(REFRESH_TOKEN_KEY);
    if (window.location.pathname !== '/login') {
      window.location.href = '/login';
    }
    throw new Error('未授权，请重新登录');
  }
}
```

#### d) 新增 API 方法

```typescript
auth: {
  // ... 现有方法 ...
  refresh(refreshToken: string) {
    return request<AuthResponse>('/api/auth/refresh',
      jsonRequest('POST', { refresh_token: refreshToken }));
  },
  logout() {
    const rt = localStorage.getItem(REFRESH_TOKEN_KEY);
    localStorage.removeItem(ACCESS_TOKEN_KEY);
    localStorage.removeItem(REFRESH_TOKEN_KEY);
    if (rt) {
      // fire-and-forget，即使失败也不阻塞
      fetch('/api/auth/logout', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: rt }),
      }).catch(() => {});
    }
  },
  devices() {
    return request<DeviceSession[]>('/api/auth/devices');
  },
  revokeDevice(id: string) {
    return request<void>(`/api/auth/devices/${id}`, { method: 'DELETE' });
  },
},
```

#### e) AuthResponse 类型更新

```typescript
export interface AuthResponse {
  access_token: string
  refresh_token: string
  token: string          // 兼容旧版 = access_token
  expires_in: number
  user: { id: string; username: string; role: string }
}

export interface DeviceSession {
  id: string
  device_name: string
  device_type: string
  ip_address: string
  created_at: string
  last_used_at: string
  is_current: boolean
}
```

### 6.7 frontend/src/api/types.ts

新增 `DeviceSession` 类型定义（如果 types.ts 是集中类型定义文件）。

### 6.8 CORS 配置

在 `main.rs` 的 `allow_headers` 中确保已有 `CONTENT_TYPE`, `AUTHORIZATION`（已有，无需改动）。

---

## 七、过期 Token 清理

### 7.1 应用启动时清理

在 `main.rs` 的 `main()` 函数中，数据库初始化后：

```rust
// 清理过期的 refresh_tokens
match db.cleanup_expired_refresh_tokens() {
    Ok(0) => {},
    Ok(n) => tracing::info!("已清理 {} 条过期 refresh token", n),
    Err(e) => tracing::warn!("清理过期 refresh token 失败: {}", e),
}
```

### 7.2 后台定时清理（可选，Phase 2）

使用 `tokio::spawn` 启动后台任务，每小时清理一次：

```rust
let cleanup_db = db.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        if let Ok(n) = cleanup_db.cleanup_expired_refresh_tokens() {
            if n > 0 { tracing::info!("定时清理: 删除 {} 条过期 refresh token", n); }
        }
    }
});
```

---

## 八、安全考量

| 风险 | 对策 |
|------|------|
| Refresh Token 泄露 | 数据库只存 SHA-256 哈希，不存明文 |
| Replay Attack | Token 轮换 + revoked token 重用检测 → 全链 revoke |
| 并发刷新 race condition | 前端刷新锁 + 后端 5s 宽限期 |
| XSS 窃取 token | 存 localStorage（与现有方案一致）；如需更安全可考虑 HttpOnly Cookie（Phase 2）|
| CSRF | 使用 Authorization header（非 Cookie），不受 CSRF 影响 |
| 暴力猜测 refresh_token | 256-bit 随机数，熵足够；已有 rate limiter 限制 |
| 长期不活跃 session | 30 天自动过期；管理员可通过设备管理手动踢出 |

---

## 九、向后兼容策略

1. **响应字段兼容**：`data.token` 始终 = `data.access_token`
2. **请求字段兼容**：`LoginReq` 的 `device_name`, `device_type` 均为 `#[serde(default)]`
3. **旧客户端不感知 refresh_token**：旧客户端只用 `data.token`，过期后跳登录页（行为不变）
4. **中间件兼容**：`jwt_auth_middleware` 和 `AuthUser` extractor 不需要任何改动——它们只验证 JWT 签名和过期，与 refresh token 无关
5. **渐进式升级**：新客户端使用 `access_token + refresh_token`，旧客户端继续用 `token`（只是 15min 过期更频繁而已，推动升级）

---

## 十、实施分步

### Phase 1 — 核心改造（本次实施）

1. `Cargo.toml` 添加 `sha2` 依赖
2. `db/mod.rs` 添加 `refresh_tokens` 建表语句
3. 新建 `db/refresh_token_repo.rs`
4. `auth.rs` 修改常量、login/register 返回双 token
5. `auth.rs` 新增 `refresh()`, `logout()`, `list_devices()`, `revoke_device()` handlers
6. `routes.rs` 注册新路由
7. `frontend/src/api/client.ts` 添加 refresh 逻辑
8. 启动时过期 token 清理

### Phase 2 — 增强（后续迭代）

- 后台定时清理任务
- Admin 踢出任意用户设备
- 设备管理 UI 页面
- 考虑 HttpOnly Cookie 存储 refresh_token
- 可信设备记忆（受信设备 refresh_token 有效期更长）

---

## 十一、测试要点

| 场景 | 验证内容 |
|------|---------|
| 正常登录 | 返回 access_token + refresh_token，DB 中有记录 |
| access_token 过期后 refresh | 获得新 access_token + 新 refresh_token，旧 refresh_token 被 revoke |
| refresh_token 过期 | 返回 401，前端跳转登录页 |
| 重放已 revoke 的 refresh_token | 整条 token 链被 revoke |
| 并发 refresh | 只有一个成功，其余等待复用结果 |
| 登出 | refresh_token 被 revoke，再次 refresh 失败 |
| 设备列表 | 返回当前用户所有活跃 session |
| 踢出设备 | 目标 refresh_token 被 revoke |
| 旧客户端兼容 | 不传 device_name 也能登录，用 `token` 字段正常工作 |
| 删除用户 | CASCADE 删除所有 refresh_tokens |
