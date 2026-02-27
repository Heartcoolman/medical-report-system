# API 接口参考文档（现状）

> 面向多端开发者的接口参考。基于当前代码 (2026-02-27) 整理。

---

## 基本信息

| 项目 | 值 |
|------|-----|
| 基础路径 | `/api/` |
| 认证方式 | JWT Bearer Token |
| 响应格式 | JSON |
| 字符编码 | UTF-8 |
| Token 有效期 | 24 小时 |

### 通用请求头

```
Authorization: Bearer <token>          # 除公开接口外必需
Content-Type: application/json         # JSON 请求体
Content-Type: multipart/form-data      # 文件上传
X-Client-Platform: web|ios|android|miniprogram  # 客户端平台标识
X-Client-Version: 1.0.0               # 客户端版本号
```

### 通用响应格式

```json
// 成功
{
  "success": true,
  "data": { ... },
  "message": "操作成功"
}

// 失败
{
  "success": false,
  "data": null,
  "message": "错误描述"
}

// 分页
{
  "success": true,
  "data": {
    "items": [ ... ],
    "total": 100,
    "page": 1,
    "page_size": 20
  },
  "message": "查询成功"
}
```

### HTTP 状态码

| 状态码 | 含义 |
|--------|------|
| 200 | 成功 |
| 201 | 创建成功 |
| 400 | 请求参数错误 |
| 401 | 未认证 / Token 过期 |
| 403 | 权限不足 |
| 404 | 资源不存在 |
| 409 | 资源冲突（如用户名已存在）|
| 429 | 请求频率超限 |
| 500 | 服务器内部错误 |

### 角色权限等级

```
Admin > Doctor > Nurse > ReadOnly
```

每个接口标注了最低角色要求，拥有更高角色的用户自动具备低角色的所有权限。

### 速率限制

| 范围 | 限制 |
|------|------|
| 全局 | 100 次/分钟/IP |
| 认证接口 | 5 次/分钟/IP |
| 上传接口 | 10 次/分钟/IP |

---

## 1. 认证 (Auth)

### POST /api/auth/register
注册新用户。

- **权限**：公开
- **请求体**：
```json
{
  "username": "doctor1",       // 必填，≥3 字符
  "password": "abc123",        // 必填，≥6 字符
  "role": "readonly"           // 可选，默认 "readonly"
}
```
- **响应** (201)：
```json
{
  "success": true,
  "data": {
    "token": "eyJ0eXAi...",
    "user": {
      "id": "uuid",
      "username": "doctor1",
      "role": "readonly"
    }
  },
  "message": "注册成功"
}
```

### POST /api/auth/login
用户登录。

- **权限**：公开
- **请求体**：
```json
{
  "username": "doctor1",
  "password": "abc123"
}
```
- **响应** (200)：
```json
{
  "success": true,
  "data": {
    "token": "eyJ0eXAi...",
    "user": {
      "id": "uuid",
      "username": "doctor1",
      "role": "doctor"
    }
  },
  "message": "登录成功",
  "update_notice": null
}
```
> `update_notice`：非 null 时表示客户端应展示更新提醒。

### GET /api/auth/me
获取当前用户信息。

- **权限**：已认证
- **响应**：
```json
{
  "success": true,
  "data": {
    "id": "uuid",
    "username": "doctor1",
    "role": "doctor"
  },
  "message": "ok",
  "update_notice": null
}
```

---

## 2. 患者 (Patients)

### GET /api/patients
获取患者列表（分页）。

- **权限**：ReadOnly+
- **查询参数**：
  - `search` (string) — 搜索关键词（姓名/手机号/身份证）
  - `page` (int) — 页码，默认 1
  - `page_size` (int) — 每页数量，默认 20
- **响应**：
```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": "uuid",
        "name": "张三",
        "gender": "男",
        "dob": "1990-01-15",
        "phone": "138****8000",
        "id_number": "1101****1234",
        "notes": "",
        "created_at": "2026-01-01 10:00:00",
        "updated_at": "2026-01-01 10:00:00",
        "report_count": 5,
        "last_report_date": "2026-02-20",
        "total_abnormal": 3
      }
    ],
    "total": 50,
    "page": 1,
    "page_size": 20
  }
}
```

### GET /api/patients/:id
获取患者详情。

- **权限**：ReadOnly+
- **响应**：Patient 对象

### POST /api/patients
创建患者。

- **权限**：Doctor+
- **请求体**：
```json
{
  "name": "张三",               // 必填
  "gender": "男",               // 必填，"男" 或 "女"
  "dob": "1990-01-15",          // 可选
  "phone": "13800138000",       // 必填（AES-256 加密存储）
  "id_number": "110101...",     // 必填（AES-256 加密存储）
  "notes": "备注"              // 可选
}
```
- **响应** (201)：创建的 Patient 对象

### PUT /api/patients/:id
更新患者信息。

- **权限**：Doctor+
- **请求体**：同创建，所有字段可选
- **响应**：更新后的 Patient 对象

### DELETE /api/patients/:id
删除患者及其所有关联数据。

- **权限**：Doctor+
- **响应**：`{ "success": true, "data": null, "message": "删除成功" }`

---

## 3. 检验报告 (Reports)

### GET /api/patients/:patient_id/reports
获取患者的报告列表。

- **权限**：ReadOnly+
- **响应**：`ReportSummary[]`
```json
{
  "success": true,
  "data": [
    {
      "id": "uuid",
      "patient_id": "uuid",
      "report_type": "血常规",
      "hospital": "协和医院",
      "report_date": "2026-02-20",
      "sample_date": "2026-02-19",
      "file_path": "/uploads/xxx.jpg",
      "created_at": "2026-02-20 10:00:00",
      "item_count": 20,
      "abnormal_count": 3,
      "abnormal_names": ["白细胞计数", "血红蛋白"]
    }
  ]
}
```

### GET /api/reports/:report_id
获取报告详情（含检验项目）。

- **权限**：ReadOnly+
- **响应**：`ReportDetail`（含 `test_items[]`）

### POST /api/patients/:patient_id/reports
创建报告。

- **权限**：Doctor+
- **请求体**：
```json
{
  "report_type": "血常规",
  "hospital": "协和医院",
  "report_date": "2026-02-20",
  "sample_date": "2026-02-19",
  "file_path": "/uploads/xxx.jpg"
}
```

### PUT /api/reports/:report_id
更新报告。

- **权限**：Doctor+
- **请求体**：字段均可选

### DELETE /api/reports/:report_id
删除报告及其检验项目。

- **权限**：Doctor+

---

## 4. 检验项目 (Test Items)

### GET /api/reports/:report_id/test-items
获取报告的检验项目列表。

- **权限**：ReadOnly+
- **响应**：`TestItem[]`
```json
[
  {
    "id": "uuid",
    "report_id": "uuid",
    "name": "白细胞计数",
    "value": "12.5",
    "unit": "10^9/L",
    "reference_range": "3.5-9.5",
    "status": "high",
    "canonical_name": "白细胞计数"
  }
]
```
> `status` 枚举值：`critical_high` | `high` | `normal` | `low` | `critical_low`

### POST /api/test-items
创建检验项目。

- **权限**：Doctor+
- **请求体**：
```json
{
  "report_id": "uuid",
  "name": "白细胞计数",
  "value": "12.5",
  "unit": "10^9/L",
  "reference_range": "3.5-9.5",
  "status": "high"
}
```

### PUT /api/test-items/:id
更新检验项目（字段均可选）。

- **权限**：Doctor+

### DELETE /api/test-items/:id
删除检验项目。

- **权限**：Doctor+

---

## 5. 趋势分析 (Trends)

### GET /api/patients/:patient_id/trend-items
获取可追踪的检验项目列表。

- **权限**：ReadOnly+
- **响应**：
```json
[
  { "report_type": "血常规", "item_name": "白细胞计数", "count": 5 }
]
```

### GET /api/patients/:patient_id/trends
获取趋势数据。

- **权限**：ReadOnly+
- **查询参数**：
  - `item_name` (string) — 检验项目名
  - `report_type` (string, 可选) — 报告类型筛选
- **响应**：`TrendPoint[]`
```json
[
  {
    "report_date": "2026-01-15",
    "sample_date": "2026-01-14",
    "value": "6.5",
    "unit": "10^9/L",
    "status": "normal",
    "reference_range": "3.5-9.5"
  }
]
```

---

## 6. 体温记录 (Temperatures)

### GET /api/patients/:patient_id/temperatures
获取体温记录列表。

- **权限**：ReadOnly+
- **响应**：`TemperatureRecord[]`
```json
[
  {
    "id": "uuid",
    "patient_id": "uuid",
    "recorded_at": "2026-02-20T08:30:00",
    "value": 36.5,
    "location": "腋下",
    "note": "",
    "created_at": "2026-02-20 08:30:00"
  }
]
```

### POST /api/patients/:patient_id/temperatures
创建体温记录。

- **权限**：Nurse+
- **请求体**：
```json
{
  "recorded_at": "2026-02-20T08:30:00",
  "value": 36.5,
  "location": "腋下",
  "note": "餐前"
}
```

### DELETE /api/temperatures/:id
删除体温记录。

- **权限**：Nurse+

---

## 7. OCR 与报告处理

### POST /api/upload
上传文件（图片/PDF）。

- **权限**：Doctor+
- **请求**：`multipart/form-data`，字段名 `file`
- **限制**：最大 10MB，允许 jpg/jpeg/png/gif/webp/pdf
- **响应**：文件路径字符串 `"/uploads/uuid.jpg"`

### POST /api/ocr/parse
OCR 识别报告图片（预览，不入库）。

- **权限**：Doctor+
- **请求**：`multipart/form-data`，字段名 `file`
- **超时**：建议 90s
- **响应**：
```json
{
  "file_path": "/uploads/xxx.jpg",
  "file_name": "report.jpg",
  "parsed": {
    "report_type": "血常规",
    "hospital": "协和医院",
    "report_date": "2026-02-20",
    "sample_date": "2026-02-19",
    "items": [
      {
        "name": "白细胞计数",
        "value": "12.5",
        "unit": "10^9/L",
        "reference_range": "3.5-9.5",
        "status": "high"
      }
    ]
  }
}
```

### POST /api/ocr/suggest-groups
建议报告分组（多张图片可能属于同一份报告）。

- **权限**：Doctor+
- **请求体**：`SuggestGroupsReq`
- **响应**：`SuggestGroupsResult`

### POST /api/patients/:patient_id/reports/merge-check
检查报告是否可与已有报告合并。

- **权限**：Doctor+

### POST /api/patients/:patient_id/reports/prefetch-normalize
预获取检验项目标准化名称（LLM）。

- **权限**：Doctor+
- **超时**：建议 90s

### POST /api/patients/:patient_id/reports/confirm
批量确认并入库报告。

- **权限**：Doctor+
- **请求体**：`BatchConfirmReq`
- **响应**：`ReportDetail[]`

---

## 8. AI 解读 (Interpret) — SSE 流式

> 所有解读接口返回 `text/event-stream`，不是标准 JSON 响应。
> 客户端需要以 SSE 方式消费，逐段拼接 `delta.content`。

### GET /api/reports/:report_id/interpret
单份报告 AI 解读。

- **权限**：Doctor+
- **响应**：SSE 流
```
data: {"delta":{"content":"根据报告..."}}
data: {"delta":{"content":"白细胞偏高..."}}
data: [DONE]
```

### GET /api/patients/:patient_id/interpret-multi
多份报告综合解读。

- **权限**：Doctor+
- **查询参数**：`report_ids` — 逗号分隔的报告 ID

### GET /api/patients/:patient_id/interpret-all
全部报告综合解读。

- **权限**：Doctor+

### GET /api/patients/:patient_id/trends/:item_name/interpret
趋势分析解读。

- **权限**：Doctor+

### GET /api/patients/:patient_id/trends/:item_name/interpret-time
趋势时间线解读。

- **权限**：Doctor+

### GET /api/reports/:report_id/interpret-cache
获取缓存的解读结果。

- **权限**：ReadOnly+
- **响应**：标准 JSON
```json
{
  "content": "解读内容...",
  "created_at": "2026-02-20 10:00:00"
}
```
> `content` 可能是 string、`{ points: string[] }` 或 `string[]` 格式。

---

## 9. 费用管理 (Expenses)

### GET /api/patients/:patient_id/expenses
获取费用列表。

- **权限**：ReadOnly+
- **响应**：`DailyExpenseSummary[]`
```json
[
  {
    "id": "uuid",
    "patient_id": "uuid",
    "expense_date": "2026-02-20",
    "total_amount": 1500.00,
    "drug_analysis": "药物分析...",
    "treatment_analysis": "治疗分析...",
    "created_at": "2026-02-20 10:00:00",
    "item_count": 15,
    "drug_count": 8,
    "test_count": 4,
    "treatment_count": 3
  }
]
```

### GET /api/expenses/:id
获取费用详情（含明细项）。

- **权限**：ReadOnly+
- **响应**：`DailyExpenseDetail`（含 `items[]`）

### POST /api/patients/:patient_id/expenses/parse
上传费用清单图片进行 OCR 解析。

- **权限**：Doctor+
- **请求**：`multipart/form-data`
- **超时**：建议 600s

### POST /api/patients/:patient_id/expenses/confirm
确认单日费用。

- **权限**：Doctor+

### POST /api/patients/:patient_id/expenses/batch-confirm
批量确认多日费用。

- **权限**：Doctor+

### POST /api/expenses/parse-chunk
分段解析费用图片。

- **权限**：Doctor+
- **超时**：建议 300s

### POST /api/expenses/merge-chunks
合并分段解析结果。

- **权限**：Doctor+

### POST /api/expenses/analyze
AI 分析费用明细（药物/治疗分析）。

- **权限**：Doctor+

### DELETE /api/expenses/:id
删除费用记录。

- **权限**：Doctor+

---

## 10. 用药管理 (Medications)

### GET /api/patients/:patient_id/medications
获取用药列表。

- **权限**：ReadOnly+
- **响应**：`Medication[]`
```json
[
  {
    "id": "uuid",
    "patient_id": "uuid",
    "name": "阿莫西林",
    "dosage": "500mg",
    "frequency": "每日3次",
    "start_date": "2026-02-01",
    "end_date": "2026-02-14",
    "note": "",
    "active": true,
    "created_at": "2026-02-01 10:00:00"
  }
]
```

### GET /api/patients/:patient_id/detected-drugs
从费用记录中检测到的药物。

- **权限**：ReadOnly+
- **响应**：`DetectedDrug[]`

### POST /api/patients/:patient_id/medications
创建用药记录。

- **权限**：Doctor+

### PUT /api/medications/:id
更新用药记录。

- **权限**：Doctor+

### DELETE /api/medications/:id
删除用药记录。

- **权限**：Doctor+

---

## 11. 健康评估 (Health Assessment)

### GET /api/patients/:patient_id/health-assessment
生成健康评估（SSE 流式）。

- **权限**：Doctor+
- **响应**：SSE 流，最终结果为 `HealthAssessment` 对象

### GET /api/patients/:patient_id/health-assessment-cache
获取缓存的健康评估。

- **权限**：ReadOnly+
- **响应**：
```json
{
  "content": {
    "overall_status": "需要关注",
    "risk_level": "中",
    "summary": "...",
    "findings": ["..."],
    "recommendations": ["..."],
    "follow_up_suggestions": ["..."],
    "disclaimer": "..."
  },
  "created_at": "2026-02-20 10:00:00"
}
```

---

## 12. 时间线与统计

### GET /api/patients/:patient_id/timeline
获取健康时间线。

- **权限**：ReadOnly+
- **响应**：`TimelineEvent[]`
```json
[
  {
    "event_type": "report",
    "event_date": "2026-02-20",
    "title": "血常规检查",
    "description": "3项异常",
    "related_id": "report-uuid",
    "created_at": "2026-02-20 10:00:00"
  }
]
```

### GET /api/stats/critical-alerts
获取危急值警报。

- **权限**：ReadOnly+
- **响应**：`CriticalAlert[]`

---

## 13. 编辑日志 (Edit Logs)

### GET /api/edit-logs
获取全局编辑日志（分页）。

- **权限**：ReadOnly+
- **查询参数**：`page`, `page_size`
- **响应**：`PaginatedList<EditLog>`

### GET /api/reports/:report_id/edit-logs
获取报告的编辑日志。

- **权限**：ReadOnly+
- **响应**：`EditLog[]`

---

## 14. 用户设置

### GET /api/user/settings
获取用户 API Key 设置。

- **权限**：ReadOnly+
- **响应**：
```json
{
  "llm_api_key": "sk-***",
  "interpret_api_key": "sk-***",
  "siliconflow_api_key": "sk-***"
}
```

### PUT /api/user/settings
更新用户 API Key 设置。

- **权限**：ReadOnly+

---

## 15. 管理员 (Admin)

### GET /api/admin/users
获取所有用户列表。

- **权限**：Admin
- **响应**：`UserInfo[]`

### PUT /api/admin/users/:id/role
更新用户角色。

- **权限**：Admin
- **请求体**：`{ "role": "doctor" }`

### DELETE /api/admin/users/:id
删除用户。

- **权限**：Admin

### GET /api/admin/backup
下载数据库备份。

- **权限**：Admin
- **响应**：SQLite 数据库文件（`application/octet-stream`）

### POST /api/admin/restore
恢复数据库。

- **权限**：Admin
- **请求**：`multipart/form-data`，字段名 `file`
- **限制**：最大 100MB

### POST /api/admin/backfill-canonical-names
回填标准化检验项目名称。

- **权限**：Admin

### GET /api/admin/audit-logs
获取审计日志。

- **权限**：Admin

---

## 16. 健康检查

### GET /api/health
服务健康检查。

- **权限**：公开（无需认证）
- **响应**：`{ "status": "ok" }`

---

## 数据模型速查

### Patient
| 字段 | 类型 | 说明 |
|------|------|------|
| id | string (UUID) | 主键 |
| name | string | 姓名 |
| gender | "男" \| "女" | 性别 |
| dob | string | 出生日期 (YYYY-MM-DD) |
| phone | string | 手机号（加密存储）|
| id_number | string | 身份证号（加密存储）|
| notes | string | 备注 |
| created_at | string | 创建时间 |
| updated_at | string | 更新时间 |

### Report
| 字段 | 类型 | 说明 |
|------|------|------|
| id | string (UUID) | 主键 |
| patient_id | string | 关联患者 ID |
| report_type | string | 报告类型（血常规、生化等）|
| hospital | string | 医院名称 |
| report_date | string | 报告日期 |
| sample_date | string | 采样日期 |
| file_path | string | 原始图片路径 |
| created_at | string | 创建时间 |

### TestItem
| 字段 | 类型 | 说明 |
|------|------|------|
| id | string (UUID) | 主键 |
| report_id | string | 关联报告 ID |
| name | string | 项目名称 |
| value | string | 检测值 |
| unit | string | 单位 |
| reference_range | string | 参考范围 |
| status | ItemStatus | 状态 |
| canonical_name | string | 标准化名称（LLM 生成）|

### ItemStatus 枚举
`critical_high` | `high` | `normal` | `low` | `critical_low`

### ExpenseCategory 枚举
`drug` | `test` | `treatment` | `material` | `nursing` | `other`

### 角色枚举
`admin` | `doctor` | `nurse` | `readonly`
