# 第二步：Web 端独立对接

> 目标：将 Web 前端从后端分离，独立部署，对接独立化后的 API 服务，验证多端架构可行性。

---

## 1. 前端工程调整

### 1.1 API 客户端重构

当前前端的 API 调用可能直接请求相对路径（同源），独立部署后需要：

```typescript
// api/client.ts
const API_BASE = import.meta.env.VITE_API_BASE || 'https://api.example.com/api/v1'

export const apiClient = {
  baseUrl: API_BASE,
  
  async request<T>(path: string, options?: RequestInit): Promise<ApiResponse<T>> {
    const url = `${this.baseUrl}${path}`
    const token = tokenStore.getAccessToken()
    
    const res = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
        ...options?.headers,
      },
    })
    
    // 401 → 自动 refresh
    if (res.status === 401) {
      const refreshed = await this.tryRefresh()
      if (refreshed) return this.request(path, options)
      else { tokenStore.clear(); goto('/login'); throw new AuthError() }
    }
    
    return res.json()
  }
}
```

### 1.2 Token 管理适配 Refresh Token

```typescript
// stores/auth.ts
interface TokenPair {
  access_token: string
  refresh_token: string
  expires_at: number
}

const tokenStore = {
  save(pair: TokenPair) {
    localStorage.setItem('access_token', pair.access_token)
    localStorage.setItem('refresh_token', pair.refresh_token)
    localStorage.setItem('expires_at', String(pair.expires_at))
  },

  isExpired(): boolean {
    const exp = Number(localStorage.getItem('expires_at') || 0)
    return Date.now() > exp * 1000 - 60000  // 提前 1 分钟刷新
  },

  async refresh(): Promise<boolean> {
    const rt = localStorage.getItem('refresh_token')
    if (!rt) return false
    try {
      const res = await fetch(`${API_BASE}/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: rt }),
      })
      if (!res.ok) return false
      const { data } = await res.json()
      this.save(data)
      return true
    } catch { return false }
  },

  clear() {
    localStorage.removeItem('access_token')
    localStorage.removeItem('refresh_token')
    localStorage.removeItem('expires_at')
  }
}
```

### 1.3 环境变量

```env
# .env.development
VITE_API_BASE=http://localhost:3001/api/v1

# .env.production
VITE_API_BASE=https://api.yourservice.com/api/v1
```

### 1.4 API 路径批量更新

所有现有 API 调用路径加上 `/v1/` 前缀：

```
旧: /api/patients          → 新: /api/v1/patients
旧: /api/auth/login        → 新: /api/v1/auth/login
旧: /api/ocr/parse         → 新: /api/v1/ocr/parse
...
```

建议用全局搜索替换，统一走 `apiClient.request()` 而不是裸 fetch。

---

## 2. 适配新响应格式

### 2.1 统一响应处理

```typescript
// api/types.ts
interface ApiResponse<T = any> {
  success: boolean
  data: T | null
  message: string
  error_code: string | null
  timestamp: string
  request_id: string
}

interface PaginatedData<T> {
  items: T[]
  total: number
  page: number
  page_size: number
  has_next: boolean
}
```

### 2.2 错误处理统一

```typescript
// api/error-handler.ts
const ERROR_MESSAGES: Record<string, string> = {
  AUTH_INVALID_CREDENTIALS: '用户名或密码错误',
  AUTH_TOKEN_EXPIRED: '登录已过期，请重新登录',
  AUTH_PERMISSION_DENIED: '没有权限执行此操作',
  PATIENT_NOT_FOUND: '患者不存在',
  VALIDATION_ERROR: '输入信息有误',
  RATE_LIMITED: '操作过于频繁，请稍后再试',
  OCR_PARSE_FAILED: 'OCR 识别失败，请重试',
  LLM_SERVICE_UNAVAILABLE: 'AI 服务暂时不可用',
}

function handleApiError(response: ApiResponse) {
  const msg = ERROR_MESSAGES[response.error_code!] || response.message || '未知错误'
  showToast(msg, 'error')
}
```

---

## 3. 独立部署

### 3.1 部署方案选择

| 平台 | 优点 | 适合场景 |
|------|------|---------|
| **Zeabur** | 你已经在用，一键部署 | 推荐 |
| **Vercel** | SolidJS 支持好，免费额度 | 备选 |
| **Nginx 自托管** | 完全可控 | 有服务器时 |
| **Cloudflare Pages** | 免费，全球 CDN | 海外访问快 |

### 3.2 Nginx 配置参考

```nginx
server {
    listen 80;
    server_name web.example.com;
    root /var/www/medical-web/dist;
    index index.html;

    # SPA 路由
    location / {
        try_files $uri $uri/ /index.html;
    }

    # API 反代（可选，也可以前端直连 API）
    location /api/ {
        proxy_pass http://api.example.com;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # 静态资源缓存
    location ~* \.(js|css|png|jpg|gif|ico|svg|woff2)$ {
        expires 30d;
        add_header Cache-Control "public, immutable";
    }

    # gzip
    gzip on;
    gzip_types text/css application/javascript application/json;
}
```

### 3.3 构建 & 部署脚本

```bash
#!/bin/bash
# deploy-web.sh

set -e

echo "📦 构建前端..."
cd frontend
npm ci
npm run build

echo "🚀 部署..."
# 方案 A: rsync 到服务器
rsync -avz --delete dist/ user@server:/var/www/medical-web/dist/

# 方案 B: Zeabur（自动，push 即部署）
# git push zeabur main

echo "✅ 部署完成"
```

---

## 4. PWA 更新

### 4.1 Service Worker 调整
- API 请求不走 SW 缓存（域名不同了）
- 只缓存前端静态资源
- 更新 `manifest.json` 中的 `start_url` 和 `scope`

### 4.2 manifest.json
```json
{
  "name": "医疗报告管理系统",
  "short_name": "医疗报告",
  "start_url": "/",
  "scope": "/",
  "display": "standalone",
  "theme_color": "#0a0a0f",
  "background_color": "#0a0a0f"
}
```

---

## 5. 测试验证

### 5.1 验证清单

| # | 验证项 | 方法 |
|---|--------|------|
| 1 | 登录/注册正常 | 手动测试 |
| 2 | Token 自动刷新 | 设置 access_token 1分钟过期，等待刷新 |
| 3 | 患者 CRUD 正常 | 增删改查全流程 |
| 4 | OCR 上传正常 | 拍照+相册上传 |
| 5 | AI 解读流式返回 | 检查打字机效果 |
| 6 | 体温记录+图表 | 录入+查看日/周视图 |
| 7 | 费用/用药正常 | 全流程 |
| 8 | CORS 无报错 | 浏览器 DevTools Network 检查 |
| 9 | PWA 可安装 | 手机浏览器"添加到主屏幕" |
| 10 | 离线模式 | 断网后能显示缓存数据 |

### 5.2 多环境验证
- 本地开发（localhost:5173 → localhost:3001）
- 生产环境（web.example.com → api.example.com）
- 验证跨域、HTTPS、Cookie 等问题

---

## 任务清单

| # | 任务 | 优先级 | 预估 |
|---|------|--------|------|
| 1 | API 客户端重构（baseUrl + 统一请求） | P0 | 3h |
| 2 | Token 管理适配 Refresh Token | P0 | 2h |
| 3 | API 路径批量更新到 v1 | P0 | 2h |
| 4 | 响应格式 + 错误码适配 | P0 | 3h |
| 5 | 环境变量配置 | P0 | 0.5h |
| 6 | 移除对后端静态托管的依赖 | P1 | 1h |
| 7 | 部署脚本编写 | P1 | 1h |
| 8 | Nginx / Zeabur 部署配置 | P1 | 1h |
| 9 | PWA / SW 调整 | P2 | 1h |
| 10 | 全流程测试验证 | P0 | 3h |

**总预估：约 17.5 小时（1 人 2-3 天）**

---

## 完成标准

- [ ] Web 前端独立部署，不依赖后端托管
- [ ] 所有 API 调用走 `/api/v1/` + 可配置的 baseUrl
- [ ] Refresh Token 自动刷新正常
- [ ] 统一错误处理，用户体验不降级
- [ ] 全流程功能验证通过
- [ ] PWA 安装和离线功能正常
