# 医疗报告管理系统

一站式管理患者医疗报告、检验数据、费用清单与用药记录的 Web 应用，支持 AI 智能解读与趋势分析。

## 功能特性

- **患者管理** — CRUD、搜索（拼音/姓名/电话/身份证）
- **报告管理** — 图片 OCR 自动识别、手动录入、批量上传、智能合并
- **检验项目** — 自动状态判定（正常/偏高/偏低/危急值）、名称标准化
- **AI 智能解读** — 单报告/多报告/趋势解读、健康评估（基于 LLM）
- **趋势分析** — 检验指标多期趋势图表、报告对比
- **体温记录** — 日/周视图图表、5 分钟测量计时器
- **费用清单** — 图片识别消费清单、药品/检查/治疗分类、自动用药检测
- **用药管理** — 手动录入 + 费用自动识别药品
- **健康时间线** — 整合报告、体温、费用、用药事件
- **数据导出** — CSV / PDF 导出
- **数据备份** — 管理员一键备份/恢复数据库
- **权限控制** — RBAC 四级角色（Admin / Doctor / Nurse / ReadOnly）
- **安全加固** — JWT 认证、AES-256 数据库字段加密、速率限制、CSP 安全头
- **PWA 支持** — 可安装到手机桌面，离线基础支持

## 技术栈

| 层级 | 技术 |
|------|------|
| 后端 | Rust、Axum、SQLite (rusqlite)、JWT、AES-GCM |
| 前端 | SolidJS、TypeScript、TailwindCSS v4、Vite |
| AI | 通义千问 (DashScope)、Gemini (pucode.com)、SiliconFlow Vision |
| 部署 | 单二进制 + 静态文件，支持 Zeabur / 自托管 |

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) ≥ 18
- [Rust](https://rustup.rs/) ≥ 1.75
- (可选) OpenSSL — 用于自动生成密钥

### 一键部署

```bash
chmod +x deploy.sh
./deploy.sh
```

脚本会自动：
1. 检查 Node.js / Rust 环境
2. 生成 `.env` 配置文件（含随机 JWT_SECRET 和 DB_ENCRYPTION_KEY）
3. 构建前端 → 构建后端 → 启动服务

启动后访问 `http://localhost:3001`，首次使用请注册管理员账号。

### 手动构建

```bash
# 前端
cd frontend && npm install && npm run build && cd ..
cp -r frontend/dist/* static/

# 后端
cd backend && cargo build --release && cd ..

# 配置环境变量（参考 .env.example）
cp .env.example .env
# 编辑 .env 填入必要配置

# 启动
./backend/target/release/backend
```

## 项目结构

```
├── backend/              # Rust 后端
│   └── src/
│       ├── main.rs           # 入口、AppState、服务器启动
│       ├── routes.rs         # 路由定义（按角色分组）
│       ├── auth.rs           # JWT 认证 + RBAC
│       ├── crypto.rs         # AES-256-GCM 字段加密
│       ├── middleware.rs     # 安全头、HTTPS 重定向、速率限制
│       ├── models.rs         # 数据模型 + 验证
│       ├── error.rs          # 统一错误处理
│       ├── db/               # 数据库层（SQLite）
│       ├── handlers/         # API 处理函数
│       └── algorithm_engine/ # 报告分类 + 名称标准化引擎
├── frontend/             # SolidJS 前端
│   └── src/
│       ├── api/              # API 客户端 + 类型定义
│       ├── components/       # 通用 UI 组件
│       ├── pages/            # 页面组件
│       ├── layouts/          # 布局组件
│       ├── stores/           # 状态管理
│       └── lib/              # 工具函数（导出、拼音等）
├── data/                 # SQLite 数据库文件
├── uploads/              # 上传的图片文件
├── static/               # 前端构建产物
├── deploy.sh             # 一键部署脚本
├── .env.example          # 环境变量模板
└── zbpack.json           # Zeabur 部署配置
```

## 环境变量

| 变量 | 必填 | 说明 |
|------|------|------|
| `JWT_SECRET` | ✅ | JWT 签名密钥（至少 32 字符） |
| `DB_ENCRYPTION_KEY` | 推荐 | 数据库敏感字段加密密钥（64 位 hex） |
| `LLM_API_KEY` | 可选 | 通义千问 API Key（OCR + 名称标准化） |
| `INTERPRET_API_KEY` | 可选 | AI 智能解读 API Key |
| `SILICONFLOW_API_KEY` | 可选 | SiliconFlow Vision API Key（图片 OCR） |
| `PORT` | 可选 | 监听端口（默认 3001） |
| `ALLOWED_ORIGINS` | 可选 | CORS 允许的域名（逗号分隔） |
| `FORCE_HTTPS` | 可选 | 强制 HTTPS 重定向（true/false） |

> 用户也可以在"设置"页面配置自己的 API Key，无需修改环境变量。

## API 概览

所有 API 均以 `/api/` 为前缀，返回统一 JSON 格式：

```json
{ "success": true, "data": ..., "message": "操作成功" }
```

| 分组 | 路径 | 说明 |
|------|------|------|
| 认证 | `POST /api/auth/login, /register` | 登录、注册 |
| 患者 | `GET/POST /api/patients` | 患者列表、创建 |
| 报告 | `GET /api/patients/:id/reports` | 报告列表 |
| OCR | `POST /api/ocr/parse` | 图片 OCR 识别 |
| 解读 | `GET /api/reports/:id/interpret` | AI 智能解读 |
| 趋势 | `GET /api/patients/:id/trends` | 趋势数据 |
| 体温 | `GET/POST /api/patients/:id/temperatures` | 体温记录 |
| 费用 | `POST /api/patients/:id/expenses/parse` | 费用清单识别 |
| 用药 | `GET/POST /api/patients/:id/medications` | 用药管理 |
| 备份 | `GET /api/admin/backup` | 数据库备份下载 |
| 管理 | `GET /api/admin/users` | 用户管理 |

## 相关项目

- **iOS 客户端（前端）**：[medical-report-ios](https://github.com/Heartcoolman/medical-report-ios) — 本项目的 iOS 原生客户端，提供移动端报告查看与管理体验。

## 许可证

MIT
