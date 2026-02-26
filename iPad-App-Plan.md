# iPad 原生应用 — 完整移植方案

将现有医疗报告管理系统的全部功能以 SwiftUI 原生 iPad App 的形式实现，复用已部署的 Rust 后端，共享账号与数据。

---

## 1. 整体架构

```
┌──────────────────────┐        HTTPS/JSON        ┌──────────────────────┐
│   iPad App (SwiftUI) │  ◄──── Bearer JWT ────►  │  Rust Backend (Axum) │
│                      │                           │  SQLite / 已部署     │
│  · URLSession        │                           │  /api/*              │
│  · Keychain (JWT)    │                           │                      │
│  · Swift Concurrency │                           │  (无需任何改动)       │
└──────────────────────┘                           └──────────────────────┘
```

**后端零改动** — 所有 API 已经是标准 RESTful JSON + JWT Bearer Token，iPad 客户端直接对接。

---

## 2. 技术选型

| 项目 | 选择 | 理由 |
|------|------|------|
| UI 框架 | **SwiftUI** | 原生、iPad 优化、NavigationSplitView 适配大屏 |
| 网络层 | **URLSession + async/await** | 原生、零依赖 |
| Token 存储 | **Keychain** | 安全持久存储 JWT |
| 数据流 | **@Observable (Observation)** | iOS 17+ 新范式，简洁高效 |
| 图表 | **Swift Charts** | 趋势分析图表，系统原生 |
| 图片选择 | **PhotosUI (PhotosPicker)** | OCR 上传图片 |
| 最低版本 | **iOS 17.0** | 支持 @Observable、NavigationSplitView 成熟 |

---

## 3. 项目结构

```
ios/MedReport/
├── MedReport.xcodeproj
├── MedReport/
│   ├── App/
│   │   ├── MedReportApp.swift          // @main 入口
│   │   └── ContentView.swift           // 根路由（登录/主界面切换）
│   ├── Models/                         // 与后端 JSON 对应的 Codable 模型
│   │   ├── Patient.swift
│   │   ├── Report.swift
│   │   ├── TestItem.swift
│   │   ├── Temperature.swift
│   │   ├── Expense.swift
│   │   ├── EditLog.swift
│   │   ├── Auth.swift
│   │   └── Common.swift                // ApiResponse<T>, PaginatedList<T>, etc.
│   ├── Services/
│   │   ├── APIClient.swift             // 核心网络层：baseURL、JWT 注入、错误处理
│   │   ├── AuthService.swift           // login/register/me + Keychain
│   │   ├── PatientService.swift
│   │   ├── ReportService.swift
│   │   ├── TemperatureService.swift
│   │   ├── ExpenseService.swift
│   │   ├── OCRService.swift
│   │   ├── TrendService.swift
│   │   └── KeychainHelper.swift
│   ├── ViewModels/                     // @Observable 视图模型
│   │   ├── AuthViewModel.swift
│   │   ├── PatientListViewModel.swift
│   │   ├── PatientDetailViewModel.swift
│   │   ├── ReportDetailViewModel.swift
│   │   ├── TrendViewModel.swift
│   │   ├── TemperatureViewModel.swift
│   │   ├── ExpenseViewModel.swift
│   │   ├── OCRViewModel.swift
│   │   └── SettingsViewModel.swift
│   ├── Views/
│   │   ├── Auth/
│   │   │   ├── LoginView.swift
│   │   │   └── RegisterView.swift
│   │   ├── Patient/
│   │   │   ├── PatientListView.swift   // Sidebar
│   │   │   ├── PatientDetailView.swift
│   │   │   └── PatientFormView.swift   // Create/Edit 复用
│   │   ├── Report/
│   │   │   ├── ReportListView.swift
│   │   │   ├── ReportDetailView.swift
│   │   │   ├── TestItemRow.swift
│   │   │   └── InterpretView.swift     // AI 解读（SSE 流式）
│   │   ├── OCR/
│   │   │   ├── OCRUploadView.swift
│   │   │   ├── OCRResultView.swift
│   │   │   └── BatchConfirmView.swift
│   │   ├── Trend/
│   │   │   └── TrendChartView.swift    // Swift Charts
│   │   ├── Temperature/
│   │   │   ├── TemperatureListView.swift
│   │   │   └── TemperatureFormView.swift
│   │   ├── Expense/
│   │   │   ├── ExpenseListView.swift
│   │   │   ├── ExpenseDetailView.swift
│   │   │   └── ExpenseParseView.swift
│   │   ├── EditLog/
│   │   │   └── EditLogListView.swift
│   │   ├── Settings/
│   │   │   ├── SettingsView.swift
│   │   │   └── ServerConfigView.swift  // 服务器地址配置
│   │   └── Components/
│   │       ├── StatusBadge.swift
│   │       ├── LoadingView.swift
│   │       └── EmptyStateView.swift
│   └── Utilities/
│       ├── Extensions.swift
│       └── Constants.swift
└── MedReportTests/
```

---

## 4. 关键设计

### 4.1 服务器地址配置

用户首次打开 App 时输入已部署的后端 URL（如 `https://your-server.com`），存入 `UserDefaults`。APIClient 使用此 baseURL 拼接所有请求路径。

### 4.2 网络层 (APIClient)

```swift
// 核心设计思路
@Observable class APIClient {
    var baseURL: String  // 用户配置的服务器地址
    
    func request<T: Decodable>(_ path: String, method: String = "GET", 
                                body: Encodable? = nil) async throws -> T
    // - 自动注入 Bearer token
    // - 统一解析 ApiResponse<T> 包装
    // - 401 自动清除 token 并触发登录
    // - 超时、网络错误统一处理
}
```

### 4.3 iPad 布局优化

利用 `NavigationSplitView` 实现三栏布局：
- **Sidebar**: 患者列表（搜索、拼音首字母索引）
- **Content**: 患者详情 / 报告列表
- **Detail**: 报告详情 / 趋势图 / 费用详情

### 4.4 AI 解读（SSE 流式）

后端 `/api/reports/:id/interpret` 等接口返回 SSE 流。在 Swift 中使用 `URLSession.bytes(for:)` 逐行读取 `data:` 行，实时渲染 Markdown。

### 4.5 OCR 图片上传

使用 `PhotosPicker` 选择图片 → 压缩 → `multipart/form-data` 上传到 `/api/ocr/parse`。

---

## 5. API 映射清单

全部 API 端点及 iPad App 中对应的功能：

| Web API | iPad 功能 |
|---------|-----------|
| `POST /api/auth/login` | 登录页 |
| `POST /api/auth/register` | 注册页 |
| `GET /api/auth/me` | 启动时验证 token |
| `GET/POST /api/patients` | 患者列表 + 新建 |
| `GET/PUT/DELETE /api/patients/:id` | 患者详情 / 编辑 / 删除 |
| `GET/POST /api/patients/:pid/reports` | 报告列表 + 新建 |
| `GET/PUT/DELETE /api/reports/:id` | 报告详情 / 编辑 / 删除 |
| `GET/POST/PUT/DELETE /api/test-items` | 检验项 CRUD |
| `POST /api/ocr/parse` | OCR 识别 |
| `POST /api/ocr/suggest-groups` | 智能分组 |
| `POST .../merge-check` | 合并检查 |
| `POST .../confirm` | 批量确认 |
| `GET .../interpret` | AI 解读（SSE） |
| `GET .../trends` | 趋势数据 + Swift Charts |
| `GET/POST/DELETE .../temperatures` | 体温记录 |
| `POST .../expenses/parse` | 费用 OCR |
| `POST .../expenses/confirm` | 费用确认 |
| `GET /api/expenses/:id` | 费用详情 |
| `GET/PUT /api/user/settings` | API Key 设置 |
| `GET /api/edit-logs` | 编辑日志 |

---

## 6. 实施步骤

### Phase 1: 基础框架 + 认证
1. 创建 Xcode 项目（iPad target, iOS 17+）
2. 实现 `APIClient`（baseURL 配置、JWT 注入、统一错误处理）
3. 实现 `KeychainHelper` + `AuthService`
4. 登录/注册页 + 服务器地址配置页
5. 主框架 `NavigationSplitView` 骨架

### Phase 2: 患者与报告
6. 患者列表（搜索、分页）
7. 患者 CRUD 表单
8. 报告列表 + 报告详情（检验项表格、异常高亮）
9. 检验项 CRUD

### Phase 3: OCR + AI 解读
10. 图片选择 + OCR 上传
11. OCR 结果预览 + 智能分组 + 合并检查
12. 批量确认流程
13. AI 解读（SSE 流式渲染）

### Phase 4: 趋势 + 体温 + 费用
14. 趋势分析（Swift Charts 折线图）
15. 体温记录（CRUD + 图表）
16. 费用管理（解析、确认、详情）

### Phase 5: 辅助功能
17. 编辑日志
18. 用户设置（API Key）
19. 管理员功能（按角色显示）
20. 细节打磨（Pull-to-refresh、空状态、加载动画、iPad 横竖屏适配）

---

## 7. 注意事项

- **后端无需改动** — 当前 CORS 配置（tower-http）对 native App 不影响，因为 App 不受浏览器同源策略限制
- **Token 过期** — 当前 24h 过期，App 侧需处理 401 自动跳转登录
- **大文件上传** — OCR/费用解析的图片上传需设合理超时（已有 Web 端参考值）
- **SSE 流** — AI 解读是流式返回，需用 `URLSession.bytes` 而非常规 `data(for:)`
- **无第三方依赖** — 全部使用 Apple 原生框架，零 CocoaPods/SPM 依赖
