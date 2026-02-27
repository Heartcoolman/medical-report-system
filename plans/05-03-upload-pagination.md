# 05-03 文件上传规范化 & 分页标准化

## A. 文件上传规范化

### A.1 现状分析

**当前 upload_file handler** (`backend/src/handlers/ocr.rs:71-74`):
```rust
pub async fn upload_file(mut multipart: Multipart) -> Result<Json<ApiResponse<String>>, AppError> {
    let (path, _) = save_upload_file(&mut multipart).await?;
    Ok(Json(ApiResponse::ok(path, "上传成功")))
}
```
- 返回值是裸字符串 `"uploads/abc123.png"`，无结构化元数据
- 原始文件名被丢弃（`_`）
- 无 MIME type、文件大小等信息

**save_upload_file** (`ocr.rs:27-69`):
- 验证文件扩展名、magic bytes、文件大小
- 用 `generate_safe_filename(detected_ext)` 生成随机安全文件名
- 写入 `uploads/{safe_name}`，返回 `(path, original_filename)`

**file_path 在系统中的使用**:
| 位置 | 用法 |
|------|------|
| `Report` model | `file_path: String` 字段，存入 DB |
| `upload_file` handler | 返回裸路径字符串 |
| `ocr_parse` handler | 返回 `OcrParseResult.file_path`，然后**删除文件** |
| `batch_confirm` | 取 `file_paths[0]` 存入 Report 记录 |
| `main.rs:138` | `nest_service("/uploads", ServeDir::new(UPLOADS_DIR))` 静态访问 |
| 前端 `ReportUpload.tsx` | 存储 file_path，传给 batch_confirm |
| 前端 `types.ts` | `Report.file_path`, `OcrParseResult.file_path`, `BatchReportInput.file_paths` |

**关键问题**:
1. `ocr_parse` 在 OCR 完成后删除文件（line 137），file_path 指向不存在的文件
2. 上传返回裸路径，无法判断文件类型、大小、原始名称
3. 没有文件访问 API（仅靠静态文件服务 `/uploads/`）
4. file_path 是磁盘路径，暴露服务器内部结构

### A.2 设计方案

#### A.2.1 新的上传响应格式

```rust
#[derive(Serialize)]
pub struct FileUploadResult {
    pub file_id: String,           // UUID，作为文件唯一标识
    pub url: String,               // 访问 URL: "/api/files/{file_id}"
    pub original_name: String,     // 原始文件名
    pub mime_type: String,         // 检测到的 MIME type
    pub size: usize,               // 文件字节数
}
```

前端将使用 `file_id` 引用文件，不再直接使用文件系统路径。

#### A.2.2 文件元数据表

```sql
CREATE TABLE IF NOT EXISTS uploaded_files (
    id TEXT PRIMARY KEY,           -- UUID
    original_name TEXT NOT NULL,
    safe_name TEXT NOT NULL,       -- 磁盘上的随机文件名
    mime_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    is_temporary INTEGER NOT NULL DEFAULT 0  -- OCR 临时文件标记
);
```

#### A.2.3 文件访问接口

```
GET /api/files/:file_id
```
- 查 DB 获取 `safe_name` 和 `mime_type`
- 从 `uploads/` 目录读取文件，设置 `Content-Type` 和 `Content-Disposition`
- 权限：所有认证用户可访问（ReadOnly+）

#### A.2.4 迁移策略

1. Report 表的 `file_path` 字段保留，新增 `file_id TEXT` 字段
2. 新上传走 `file_id` 模式，旧数据 `file_path` 继续兼容
3. `ocr_parse` 不再删除文件，改为标记 `is_temporary=1`
4. 定期清理任务：删除 `is_temporary=1` 且超过 24 小时的文件

### A.3 需要改动的文件

**后端**:
| 文件 | 改动 |
|------|------|
| `backend/src/db/mod.rs` | 新增 `uploaded_files` 表建表语句 |
| `backend/src/db/` (新文件 `file_repo.rs`) | 文件元数据 CRUD |
| `backend/src/handlers/ocr.rs` | `upload_file` 返回 `FileUploadResult`；`save_upload_file` 同时写入元数据表；`ocr_parse` 不再删除文件 |
| `backend/src/routes.rs` | 新增 `GET /api/files/:file_id` 路由（readonly_routes） |
| `backend/src/handlers/` (新增或 ocr.rs 内) | `serve_file` handler |
| `backend/src/models.rs` | 新增 `FileUploadResult` struct（或放 ocr.rs） |

**前端**:
| 文件 | 改动 |
|------|------|
| `frontend/src/api/types.ts` | 新增 `FileUploadResult` 接口，`OcrParseResult.file_path` → `file_id` |
| `frontend/src/pages/ReportUpload.tsx` | 使用 `file_id` 代替 `file_path` |

---

## B. 分页标准化

### B.1 现状分析

**已有分页结构** (`backend/src/models.rs:515-521`):
```rust
pub struct PaginatedList<T: Serialize> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}
```

**已分页的 3 个接口**:
| 接口 | Handler | 返回类型 |
|------|---------|----------|
| `GET /api/patients?page=&page_size=&search=` | `list_patients` | `PaginatedList<PatientWithStats>` |
| `GET /api/edit-logs?page=&page_size=` | `list_edit_logs_global` | `PaginatedList<EditLog>` |
| `GET /api/admin/audit-logs?page=&page_size=&action=&resource_type=` | `list_audit_logs` | `PaginatedList<AuditLog>` |

**现有分页 DB 模式** (`patient_repo.rs`, `edit_log_repo.rs`, `audit.rs`):
```
默认 page_size = 20 (helpers::DEFAULT_PAGE_SIZE)
最大 page_size = 100
page 最小 = 1
两条查询: SELECT COUNT(*) + SELECT ... LIMIT ? OFFSET ?
```

**重复代码**: 每个分页方法都手动实现 page 参数校验、offset 计算、COUNT 查询 + 数据查询。

### B.2 未分页列表接口清单

| 接口 | Handler | 返回类型 | 是否需要分页 | 理由 |
|------|---------|----------|-------------|------|
| `GET /api/patients/:id/reports` | `list_reports_by_patient` | `Vec<ReportSummary>` | **是** | 报告随时间持续增长 |
| `GET /api/patients/:id/expenses` | `list_expenses` | `Vec<DailyExpenseSummary>` | **是** | 住院日均产生记录，可达数十上百条 |
| `GET /api/patients/:id/temperatures` | `list_temperatures` | `Vec<TemperatureRecord>` | **是** | 每日多次测温，增长快 |
| `GET /api/reports/:id/edit-logs` | `list_edit_logs_by_report` | `Vec<EditLog>` | 暂不需要 | 单报告编辑日志通常 <50 条 |
| `GET /api/stats/critical-alerts` | `get_critical_alerts` | `Vec<CriticalAlert>` | **是** | 跨全部患者，可能很大 |
| `GET /api/patients/:id/timeline` | `get_timeline` | `Vec<TimelineEvent>` | 可选 | 汇总数据，单患者通常不大 |
| `GET /api/reports/:id/test-items` | `get_test_items_by_report` | `Vec<TestItem>` | 否 | 单报告通常 5-50 项 |
| `GET /api/patients/:id/trends` | `get_trends` | `Vec<TrendPoint>` | 否 | 单项目趋势，受报告数限制 |
| `GET /api/patients/:id/trend-items` | `list_trend_items` | `Vec<TrendItemInfo>` | 否 | 去重后项目名列表，通常 <200 |
| `GET /api/patients/:id/medications` | `list_medications` | `Vec<Medication>` | 否 | 单患者用药记录通常 <30 |
| `GET /api/patients/:id/detected-drugs` | `list_detected_drugs` | `Vec<DetectedDrug>` | 否 | 从消费提取，去重后通常 <50 |
| `GET /api/admin/users` | `list_users` | `Vec<UserInfo>` | 否 | 用户数通常 <20 |

### B.3 设计方案

#### B.3.1 统一分页查询参数

```rust
/// 从 Query 中提取分页参数的通用结构
#[derive(Deserialize)]
pub struct PaginationParams {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

impl PaginationParams {
    pub fn normalize(&self) -> (usize, usize) {
        let page = self.page.unwrap_or(1).max(1);
        let page_size = self.page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, 100);
        (page, page_size)
    }

    pub fn offset(&self) -> usize {
        let (page, page_size) = self.normalize();
        (page - 1) * page_size
    }
}
```

放在 `backend/src/models.rs` 中，Handler 层通过 `Query<PaginationParams>` 提取。

#### B.3.2 DB 层分页辅助函数

在 `backend/src/db/helpers.rs` 中新增：

```rust
/// 执行分页查询的通用辅助函数
pub fn paginated_query<T, F>(
    conn: &Connection,
    count_sql: &str,
    data_sql: &str,
    count_params: &[&dyn rusqlite::types::ToSql],
    data_params: &[&dyn rusqlite::types::ToSql],
    page: usize,
    page_size: usize,
    row_mapper: F,
) -> Result<PaginatedList<T>, AppError>
where
    T: Serialize,
    F: FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
{
    let total: usize = conn
        .query_row(count_sql, count_params, |row| row.get::<_, i64>(0))?
        .try_into().unwrap_or(0);

    let offset = (page - 1) * page_size;
    let mut all_params: Vec<&dyn rusqlite::types::ToSql> = data_params.to_vec();
    all_params.push(&(page_size as i64));
    all_params.push(&(offset as i64));

    let mut stmt = conn.prepare(data_sql)?;
    let items = stmt
        .query_map(all_params.as_slice(), row_mapper)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(PaginatedList { items, total, page, page_size })
}
```

#### B.3.3 现有分页接口重构

用 `PaginationParams` + `paginated_query` 重构以下现有实现：
- `patient_repo.rs:list_patients_paginated` — 使用辅助函数替代手动拼装
- `edit_log_repo.rs:list_edit_logs_global` — 使用辅助函数替代手动拼装
- `audit.rs:query_audit_logs` — 使用辅助函数替代手动拼装（有 filter，稍微复杂）

#### B.3.4 新增分页的 4 个接口

按优先级：

**P0 — 必须分页**:

1. **`GET /api/patients/:id/reports?page=&page_size=`**
   - Handler: `reports.rs:list_reports_by_patient`
   - DB: `report_repo.rs` 新增 `list_reports_with_summary_by_patient_paginated`
   - SQL 改动: 原 `ORDER BY report_date ASC` 加 `LIMIT ? OFFSET ?`，新增 COUNT 查询
   - 返回类型: `PaginatedList<ReportSummary>`

2. **`GET /api/patients/:id/expenses?page=&page_size=`**
   - Handler: `expense.rs:list_expenses`
   - DB: `expense_repo.rs` 新增 `list_expenses_by_patient_paginated`
   - 返回类型: `PaginatedList<DailyExpenseSummary>`

3. **`GET /api/patients/:id/temperatures?page=&page_size=`**
   - Handler: `temperatures.rs:list_temperatures`
   - DB: `temperature_repo.rs` 新增 `list_temperatures_by_patient_paginated`
   - 返回类型: `PaginatedList<TemperatureRecord>`

4. **`GET /api/stats/critical-alerts?page=&page_size=`**
   - Handler: `stats.rs:get_critical_alerts`
   - 当前是内联 SQL，移至 DB repo 并分页
   - 返回类型: `PaginatedList<CriticalAlert>`

### B.4 需要改动的文件

**后端 — 新增/修改**:
| 文件 | 改动 |
|------|------|
| `backend/src/models.rs` | 新增 `PaginationParams` struct |
| `backend/src/db/helpers.rs` | 新增 `paginated_query` 辅助函数 |
| `backend/src/db/patient_repo.rs` | 重构 `list_patients_paginated` 使用辅助函数 |
| `backend/src/db/edit_log_repo.rs` | 重构 `list_edit_logs_global` 使用辅助函数 |
| `backend/src/audit.rs` | 重构 `query_audit_logs` 使用辅助函数 |
| `backend/src/db/report_repo.rs` | 新增 `list_reports_with_summary_by_patient_paginated` |
| `backend/src/db/expense_repo.rs` | 新增 `list_expenses_by_patient_paginated` |
| `backend/src/db/temperature_repo.rs` | 新增 `list_temperatures_by_patient_paginated` |
| `backend/src/handlers/reports.rs` | `list_reports_by_patient` 接受分页参数，返回 PaginatedList |
| `backend/src/handlers/expense.rs` | `list_expenses` 接受分页参数，返回 PaginatedList |
| `backend/src/handlers/temperatures.rs` | `list_temperatures` 接受分页参数，返回 PaginatedList |
| `backend/src/handlers/stats.rs` | `get_critical_alerts` 接受分页参数，返回 PaginatedList |

**前端**:
| 文件 | 改动 |
|------|------|
| 调用 reports list 的组件 | 传递 page/page_size 参数，处理 PaginatedList 返回 |
| 调用 expenses list 的组件 | 同上 |
| 调用 temperatures list 的组件 | 同上 |
| 调用 critical-alerts 的组件 | 同上 |

### B.5 向后兼容性

- 所有分页参数可选，默认 `page=1, page_size=20`
- 不传分页参数 = 返回第一页 20 条 + total 字段
- 返回类型从 `Vec<T>` 变为 `PaginatedList<T>` — **这是 breaking change**
- 前端需要同步更新：`response.data` 从数组变为 `{ items, total, page, page_size }` 对象
- 建议前后端同步上线，或在 API 版本化（05-04）后通过 v2 接口引入

---

## C. 实施顺序建议

1. **Phase 1**: 新增 `PaginationParams` 和 `paginated_query` 基础设施
2. **Phase 2**: 重构现有 3 个分页接口使用新基础设施（无功能变化，低风险）
3. **Phase 3**: 新增 4 个列表接口的分页支持 + 前端适配
4. **Phase 4**: 文件上传规范化（`uploaded_files` 表 + `FileUploadResult` + `/api/files/:id`）
5. **Phase 5**: 前端上传流程适配 + 清理临时文件机制
