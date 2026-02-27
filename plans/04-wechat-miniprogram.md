# 第四步：微信小程序开发

> 目标：基于独立化后的 API，开发微信小程序客户端，提供轻量级入口，面向患者/家属自助查看。

---

## 1. 定位 & 功能范围

小程序定位为**轻量查看端**，不做全量管理功能。

### 功能对比

| 功能 | Web | iOS/Android | 小程序 |
|------|-----|------------|--------|
| 患者管理 CRUD | ✅ | ✅ | ⚠️ 仅查看 + 搜索 |
| 报告查看 | ✅ | ✅ | ✅ |
| OCR 上传 | ✅ | ✅ | ✅（wx.chooseImage） |
| AI 解读 | ✅ | ✅ | ✅ |
| 体温记录 | ✅ | ✅ | ✅（录入 + 查看） |
| 费用清单 | ✅ | ✅ | ✅（查看） |
| 用药管理 | ✅ | ✅ | ⚠️ 仅查看 |
| 健康时间线 | ✅ | ✅ | ✅ |
| 数据导出 | ✅ | ✅ | ❌ |
| 管理员功能 | ✅ | ❌ | ❌ |
| 用户管理 | ✅ | ❌ | ❌ |
| 数据备份 | ✅ | ❌ | ❌ |

---

## 2. 技术选型

### 方案对比

| 方案 | 优点 | 缺点 |
|------|------|------|
| **微信原生** | 性能最好、API 最全 | 语法老旧（WXML/WXSS） |
| **Taro (React)** | React 语法、可跨端 | 包体稍大、部分 API 需适配 |
| **uni-app (Vue)** | Vue 语法、生态大 | 性能略差、微信特性支持慢 |

### 推荐：Taro 3 + React + TypeScript

理由：
- 你的 Web 前端是 SolidJS（类 React），开发体验接近
- TypeScript 类型安全
- 如果未来要做支付宝/抖音小程序，Taro 跨端成本低
- 可以复用 Web 端的 API 类型定义

---

## 3. 项目结构

```
medical-report-miniprogram/
├── src/
│   ├── app.ts                    # 入口
│   ├── app.config.ts             # 全局配置
│   ├── app.scss
│   │
│   ├── api/
│   │   ├── client.ts             # 请求封装（wx.request）
│   │   ├── types.ts              # API 类型定义
│   │   ├── auth.ts               # 认证接口
│   │   ├── patient.ts            # 患者接口
│   │   ├── report.ts             # 报告接口
│   │   ├── temperature.ts        # 体温接口
│   │   ├── expense.ts            # 费用接口
│   │   └── medication.ts         # 用药接口
│   │
│   ├── stores/
│   │   ├── auth.ts               # 登录状态
│   │   └── patient.ts            # 当前患者
│   │
│   ├── pages/
│   │   ├── index/                # 首页（患者列表）
│   │   │   ├── index.tsx
│   │   │   ├── index.config.ts
│   │   │   └── index.scss
│   │   ├── login/                # 登录页
│   │   ├── patient/              # 患者详情
│   │   ├── report-list/          # 报告列表
│   │   ├── report-detail/        # 报告详情
│   │   ├── interpret/            # AI 解读
│   │   ├── ocr-upload/           # OCR 上传
│   │   ├── temperature/          # 体温记录
│   │   ├── expense/              # 费用清单
│   │   ├── medication/           # 用药查看
│   │   ├── timeline/             # 健康时间线
│   │   └── settings/             # 设置
│   │
│   ├── components/
│   │   ├── StatusBadge/          # 检验状态标签
│   │   ├── PatientCard/          # 患者卡片
│   │   ├── ReportCard/           # 报告卡片
│   │   ├── TempChart/            # 体温图表（echarts-for-weixin）
│   │   ├── TimelineItem/         # 时间线项
│   │   ├── Empty/                # 空状态
│   │   └── Loading/              # 加载状态
│   │
│   └── utils/
│       ├── date.ts
│       ├── format.ts
│       └── storage.ts            # wx.setStorageSync 封装
│
├── project.config.json           # 微信开发者工具配置
├── tsconfig.json
├── package.json
└── README.md
```

---

## 4. 核心模块设计

### 4.1 登录流程

小程序登录和 Web/移动端不同，需要对接微信登录体系：

```
用户点击登录
    ↓
wx.login() 获取 code
    ↓
POST /api/v1/auth/login/wechat { code, userInfo }
    ↓
后端用 code 换 openid → 查找/创建用户 → 返回 token pair
    ↓
存储 token 到 wx.setStorageSync
```

**首次使用流程：**
1. 微信授权登录
2. 绑定已有账号（输入用户名密码）或注册新账号
3. 绑定后，微信和账号关联，下次自动登录

```typescript
// api/auth.ts
export async function wechatLogin() {
  const { code } = await Taro.login()
  const userInfo = await Taro.getUserProfile({ desc: '用于登录' })
  
  const res = await apiClient.post('/auth/login/wechat', {
    code,
    nickname: userInfo.userInfo.nickName,
    avatar: userInfo.userInfo.avatarUrl,
  })
  
  if (res.data.need_bindind) {
    // 跳转绑定页
    Taro.navigateTo({ url: '/pages/bind-account/index' })
  } else {
    tokenStore.save(res.data)
    Taro.switchTab({ url: '/pages/index/index' })
  }
}
```

### 4.2 请求封装

```typescript
// api/client.ts
const API_BASE = 'https://api.example.com/api/v1'

class ApiClient {
  async request<T>(path: string, options: Taro.request.Option = {}): Promise<ApiResponse<T>> {
    const token = Taro.getStorageSync('access_token')
    
    const res = await Taro.request({
      url: `${API_BASE}${path}`,
      header: {
        'Content-Type': 'application/json',
        ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
        ...options.header,
      },
      ...options,
    })

    // 401 → 刷新 token
    if (res.statusCode === 401) {
      const refreshed = await this.tryRefresh()
      if (refreshed) return this.request(path, options)
      Taro.redirectTo({ url: '/pages/login/index' })
      throw new Error('AUTH_EXPIRED')
    }

    return res.data as ApiResponse<T>
  }

  async get<T>(path: string, params?: Record<string, any>) {
    return this.request<T>(path, { method: 'GET', data: params })
  }

  async post<T>(path: string, data?: any) {
    return this.request<T>(path, { method: 'POST', data })
  }

  private async tryRefresh(): Promise<boolean> {
    const refreshToken = Taro.getStorageSync('refresh_token')
    if (!refreshToken) return false
    try {
      const res = await Taro.request({
        url: `${API_BASE}/auth/refresh`,
        method: 'POST',
        data: { refresh_token: refreshToken },
      })
      if (res.data.success) {
        Taro.setStorageSync('access_token', res.data.data.access_token)
        Taro.setStorageSync('refresh_token', res.data.data.refresh_token)
        return true
      }
      return false
    } catch { return false }
  }
}

export const apiClient = new ApiClient()
```

### 4.3 OCR 上传

```typescript
// pages/ocr-upload/index.tsx
export default function OcrUpload() {
  const [result, setResult] = useState<OcrResult | null>(null)
  const [loading, setLoading] = useState(false)

  const handleChooseImage = async () => {
    const { tempFilePaths } = await Taro.chooseImage({
      count: 1,
      sizeType: ['compressed'],
      sourceType: ['album', 'camera'],
    })

    setLoading(true)
    const res = await Taro.uploadFile({
      url: `${API_BASE}/ocr/parse`,
      filePath: tempFilePaths[0],
      name: 'image',
      header: { 'Authorization': `Bearer ${Taro.getStorageSync('access_token')}` },
    })
    
    const data = JSON.parse(res.data)
    setResult(data.data)
    setLoading(false)
  }

  return (
    <View className="ocr-upload">
      {loading ? <Loading /> : (
        <>
          {result ? (
            <OcrResultEditor result={result} onConfirm={handleSave} />
          ) : (
            <Button onClick={handleChooseImage}>📷 拍照/选图识别</Button>
          )}
        </>
      )}
    </View>
  )
}
```

### 4.4 AI 解读

微信小程序不支持原生 SSE/EventSource，处理方式：

**方案 A：轮询（简单）**
```
POST /api/v1/reports/:id/interpret → 触发解读
GET  /api/v1/reports/:id/interpret/result?since=0 → 轮询获取增量文本
```

**方案 B：wx.request 分块接收**
小程序 `wx.request` 在 enableChunkedTransfer 模式下可接收流式数据（基础库 2.24+）

**方案 C：WebSocket**
```
ws://api.example.com/ws → 建立连接
send { type: "interpret", report_id: "xxx" }
receive { type: "chunk", text: "..." } × N
receive { type: "done" }
```

建议后端同时支持 SSE（给 Web/移动端）和 WebSocket/轮询（给小程序），推荐 **方案 B 或 C**。

### 4.5 体温图表

使用 `echarts-for-weixin`（微信官方合作的 ECharts 小程序版）：

```typescript
// components/TempChart/index.tsx
import * as echarts from '../../utils/ec-canvas/echarts'

function initChart(canvas, width, height, records) {
  const chart = echarts.init(canvas, null, { width, height })
  
  chart.setOption({
    xAxis: {
      type: 'category',
      data: records.map(r => formatTime(r.recorded_at)),
    },
    yAxis: {
      type: 'value',
      min: 35,
      max: 42,
    },
    series: [{
      type: 'line',
      data: records.map(r => r.value),
      smooth: true,
      markLine: {
        data: [{ yAxis: 37.3, name: '发热线' }],
      },
    }],
  })
  
  return chart
}
```

---

## 5. 微信后台配置

### 5.1 需要配置的内容

| 配置项 | 值 |
|--------|---|
| 服务器域名 - request | `https://api.example.com` |
| 服务器域名 - uploadFile | `https://api.example.com` |
| 服务器域名 - downloadFile | `https://api.example.com` |
| 服务器域名 - socket | `wss://api.example.com`（如果用 WebSocket） |

### 5.2 注意事项
- 域名必须 HTTPS + 已备案
- 开发阶段可在开发者工具勾选"不校验合法域名"
- 上线审核时会检查域名配置

---

## 6. 后端需要新增的支持

| 新增 | 说明 |
|------|------|
| `POST /api/v1/auth/login/wechat` | 微信登录接口，用 code 换 openid |
| `POST /api/v1/auth/bind` | 微信 openid 绑定已有账号 |
| `users` 表新增 `wechat_openid` 字段 | 存储微信身份 |
| WebSocket / 轮询接口 | AI 解读的小程序适配 |
| 微信小程序 session_key 管理 | 解密用户信息用 |

后端需要引入微信 SDK 或手动对接：
```
GET https://api.weixin.qq.com/sns/jscode2session
  ?appid=APPID
  &secret=SECRET
  &js_code=CODE
  &grant_type=authorization_code
```

新增环境变量：
```env
WECHAT_APPID=wx1234567890
WECHAT_SECRET=your_wechat_secret
```

---

## 7. UI 设计

### 7.1 页面导航

```
TabBar:
├── 🏠 首页（患者列表/搜索）
├── 🌡️ 体温（快捷记录入口）
├── 📷 扫描（OCR 快捷入口）
└── 👤 我的（设置/关于）

页面栈:
首页 → 患者详情 → 报告列表 → 报告详情 → AI 解读
                            → 体温记录
                            → 费用清单
                            → 用药列表
                            → 时间线
```

### 7.2 设计原则
- 遵循微信小程序设计规范
- WeUI 组件库风格
- 大字体、高对比度（医疗场景，考虑中老年用户）
- 关键操作有确认提示

---

## 8. 开发阶段

### Phase 1 — 基础框架 + 核心（4天）

| 天 | 任务 |
|----|------|
| D1 | Taro 项目初始化、请求封装、Token 管理、登录页 |
| D2 | 微信登录对接 + 账号绑定流程 |
| D3 | 患者列表/搜索 + 患者详情页 |
| D4 | 报告列表/详情 + 检验项目展示 |

### Phase 2 — 功能完善（4天）

| 天 | 任务 |
|----|------|
| D5 | OCR 上传（拍照/相册） |
| D6 | AI 解读（适配小程序流式方案） |
| D7 | 体温记录 + ECharts 图表 |
| D8 | 费用清单 + 用药查看 + 时间线 |

### Phase 3 — 打磨发布（2天）

| 天 | 任务 |
|----|------|
| D9 | 设置页、错误处理、loading 状态、空状态 |
| D10 | 全流程测试、体验优化、提交审核 |

**总预估：约 10 天**

---

## 9. 发布流程

1. 注册微信小程序账号（如已有则跳过）
2. 微信后台配置服务器域名
3. 后端部署新增微信登录接口
4. 开发者工具上传代码
5. 提交审核（通常 1-3 天）
6. 审核通过后发布

### 审核注意事项
- 医疗类小程序可能需要**《互联网药品信息服务资格证》**或相关资质
- 如果只是个人/内部使用，可以不公开发布，用体验版
- 涉及用户健康数据，需要隐私协议

---

## 任务清单

| # | 任务 | 优先级 | 预估 |
|---|------|--------|------|
| 1 | Taro 项目初始化 + 配置 | P0 | 2h |
| 2 | 请求封装 + Token 管理 | P0 | 3h |
| 3 | 微信登录 + 绑定流程 | P0 | 4h |
| 4 | 后端新增微信登录接口 | P0 | 4h |
| 5 | 患者列表/搜索 | P0 | 3h |
| 6 | 患者详情页 | P0 | 2h |
| 7 | 报告列表/详情 | P0 | 4h |
| 8 | OCR 上传 | P0 | 3h |
| 9 | AI 解读（流式适配） | P1 | 5h |
| 10 | 体温记录 + 图表 | P1 | 5h |
| 11 | 费用清单查看 | P2 | 2h |
| 12 | 用药查看 | P2 | 2h |
| 13 | 健康时间线 | P2 | 3h |
| 14 | 设置页 | P2 | 2h |
| 15 | UI 打磨 + 测试 | P0 | 5h |
| 16 | 提交审核 | P0 | 1h |

**总预估：约 50 小时（1 人 ~10 天）**

---

## 完成标准

- [ ] 微信登录 + 账号绑定正常
- [ ] 患者列表/搜索/详情
- [ ] 报告查看 + OCR 上传
- [ ] AI 智能解读（小程序适配方案）
- [ ] 体温记录 + 图表
- [ ] 费用/用药查看
- [ ] 时间线
- [ ] 符合微信小程序设计规范
- [ ] 提交审核通过（或体验版可用）
