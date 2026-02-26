# Upload Speed Optimization — 消费清单 & 报告

## TL;DR

> **Quick Summary**: 优化消费清单（重点）和报告上传的处理速度，通过去除冗余压缩、添加图片压缩、消除不必要的磁盘 I/O、修复超时配置、添加 SSE 实时进度反馈，并建立测试基础设施。
> 
> **Deliverables**:
> - 消费清单：去除后端重复压缩，减少处理时间
> - 报告：添加图片压缩 + 内存处理，修复超时配置
> - 两者：SSE 进度反馈，前端进度 UI
> - 测试基础设施 + 性能基线测量
> 
> **Estimated Effort**: Medium
> **Parallel Execution**: YES - 4 waves
> **Critical Path**: Task 1 → Task 2/3 → Task 6/7 → Task 9 → Task 10 → F1-F4

---

## Context

### Original Request
用户要求优化上传消费清单和上传报告的速度。消费清单是主要痛点，报告速度可接受但也慢。

### Interview Summary
**Key Discussions**:
- 消费清单是主要痛点，报告次要
- 不更换模型（报告保持 235B，消费清单保持 32B），准确度不妥协
- 进度反馈算优化的一部分，但实际处理时间也要缩短
- 需要添加自动化测试（tests-after 方式）

**Research Findings**:
- 消费清单存在双重压缩：前端 Canvas 压缩到 WebP/JPEG → 后端再次解码+重编码 WebP，完全冗余
- 报告上传完全没有图片压缩，原始大图直接 base64 发给 Vision API
- 报告先写磁盘再读回来，多了不必要的 I/O
- 报告使用共享 http_client 的 300s 超时，但用的是更大的 235B 模型，可能导致静默超时失败
- SSE 基础设施已存在于 interpret.rs，可复用
- 已有少量测试：algorithm_engine/integration_tests.rs, db/test_item_repo.rs

### Metis Review
**Identified Gaps** (addressed):
- 双重压缩修复方向：应保留前端压缩（减少网络传输），去除后端重复压缩（而非反过来）
- 报告压缩需测试 OCR 准确度影响：只做 resize，不做有损质量压缩
- 报告超时 300s vs 消费清单 600s 不一致：235B 模型更慢，需要更长超时
- SSE 进度粒度有限：Vision API 调用期间无法细分进度，只能报告阶段性状态
- 内存处理需注意大文件：50MB 文件 + base64 = 67MB 内存占用

---

## Work Objectives

### Core Objective
通过消除冗余处理步骤、优化 I/O 路径、修复超时配置来缩短实际处理时间，同时添加 SSE 进度反馈改善用户等待体验。

### Concrete Deliverables
- `backend/src/handlers/expense.rs`: 去除后端 `compress_image_to_webp` 调用，直接使用前端已压缩的数据
- `backend/src/ocr/vision.rs`: 添加图片 resize（max 2000px）+ 内存处理（去除磁盘 I/O）
- `backend/src/handlers/ocr.rs`: 改为内存处理，去除 `save_upload_file` 磁盘写入
- 新增 SSE 端点：`/api/patients/:id/expenses/parse-sse` 和 `/api/ocr/parse-sse`
- 前端 SSE 进度 UI：`ExpenseUpload.tsx` 和 `ReportUpload.tsx`
- 测试文件：`backend/src/handlers/expense_test.rs`, `backend/src/ocr/vision_test.rs` 等

### Definition of Done
- [ ] `cargo build` 成功，无编译错误
- [ ] `cargo test --workspace` 全部通过
- [ ] 消费清单上传：后端不再执行 `compress_image_to_webp`
- [ ] 报告上传：图片经过 resize 后再发送给 Vision API
- [ ] 报告上传：不再写入/读取磁盘临时文件
- [ ] 两个上传流程都有 SSE 进度反馈
- [ ] 前端显示实时进度状态

### Must Have
- 消费清单去除后端重复压缩
- 报告添加图片 resize（仅缩放，不降质量）
- 报告去除磁盘 I/O，改为内存处理
- 报告超时从 300s 提升到 600s
- SSE 进度反馈（至少 3 个阶段：压缩/处理中 → API 调用中 → 解析中）
- 前端进度 UI
- 自动化测试

### Must NOT Have (Guardrails)
- 不更换 Vision API 模型（报告保持 Qwen3-VL-235B，消费清单保持 Qwen3-VL-32B）
- 不修改 Vision API 参数（prompt、temperature、max_tokens）
- 不删除前端 Canvas 压缩（它减少了网络传输量）
- 不修改 confirm/save 流程
- 不修改 analysis LLM 调用
- 不修改 grouping/merge 逻辑
- 不添加有损质量压缩到报告（只做 resize）
- 不改变端点 HTTP 方法（POST 保持 POST）
- 不并行化 Vision API 调用（未确认 SiliconFlow 速率限制）
- 不添加过度抽象或不必要的中间层
- 不添加 JSDoc/注释膨胀

---

## Verification Strategy

> **ZERO HUMAN INTERVENTION** — ALL verification is agent-executed. No exceptions.

### Test Decision
- **Infrastructure exists**: Partial (algorithm_engine tests exist)
- **Automated tests**: Tests-after
- **Framework**: Rust built-in `#[tokio::test]` + axum test helpers
- **Pattern**: Follow `backend/src/algorithm_engine/integration_tests.rs`

### QA Policy
Every task MUST include agent-executed QA scenarios.
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **Backend**: Use Bash (cargo test, curl) — Build, run tests, send requests, assert responses
- **Frontend/UI**: Use Playwright — Navigate, interact, assert DOM, screenshot

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Foundation — timing baseline + test infra):
├── Task 1: Add timing instrumentation to both upload flows [quick]
├── Task 2: Set up test infrastructure + test helpers [unspecified-high]
└── Task 3: Fix report timeout configuration (300s → 600s) [quick]

Wave 2 (Core backend optimizations — MAX PARALLEL):
├── Task 4: Remove backend double compression for expenses (depends: 1) [deep]
├── Task 5: Add image resize + in-memory processing for reports (depends: 1) [deep]
└── Task 6: Implement SSE progress endpoints for both flows (depends: 1) [unspecified-high]

Wave 3 (Frontend + tests):
├── Task 7: Frontend SSE progress UI for expense upload (depends: 6) [visual-engineering]
├── Task 8: Frontend SSE progress UI for report upload (depends: 6) [visual-engineering]
└── Task 9: Write integration tests for optimized flows (depends: 2, 4, 5) [unspecified-high]

Wave 4 (Verification):
├── Task 10: Performance comparison — before vs after timing (depends: 4, 5, 6) [deep]

Wave FINAL (After ALL tasks — independent review, 4 parallel):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: Real manual QA (unspecified-high)
└── Task F4: Scope fidelity check (deep)

Critical Path: Task 1 → Task 4/5 → Task 6 → Task 7/8 → Task 10 → F1-F4
Parallel Speedup: ~60% faster than sequential
Max Concurrent: 3 (Wave 2)
```

### Dependency Matrix

| Task | Depends On | Blocks | Wave |
|------|-----------|--------|------|
| 1 | — | 4, 5, 6, 10 | 1 |
| 2 | — | 9 | 1 |
| 3 | — | — | 1 |
| 4 | 1 | 9, 10 | 2 |
| 5 | 1 | 9, 10 | 2 |
| 6 | 1 | 7, 8 | 2 |
| 7 | 6 | — | 3 |
| 8 | 6 | — | 3 |
| 9 | 2, 4, 5 | — | 3 |
| 10 | 4, 5, 6 | — | 4 |

### Agent Dispatch Summary

- **Wave 1**: 3 tasks — T1 → `quick`, T2 → `unspecified-high`, T3 → `quick`
- **Wave 2**: 3 tasks — T4 → `deep`, T5 → `deep`, T6 → `unspecified-high`
- **Wave 3**: 3 tasks — T7 → `visual-engineering`, T8 → `visual-engineering`, T9 → `unspecified-high`
- **Wave 4**: 1 task — T10 → `deep`
- **FINAL**: 4 tasks — F1 → `oracle`, F2 → `unspecified-high`, F3 → `unspecified-high`, F4 → `deep`

---

## TODOs

> Implementation + Test = ONE Task. Never separate.
> EVERY task MUST have: Recommended Agent Profile + Parallelization info + QA Scenarios.

---

- [x] 1. Add Timing Instrumentation to Both Upload Flows

  **What to do**:
  - Add `tracing::info!` timing spans at each stage of the expense parse flow in `expense.rs`:
    - Multipart read duration
    - Image compression duration (currently `compress_image_to_webp`)
    - Base64 encoding duration
    - Vision API call duration
    - JSON parsing duration
  - Add same timing spans to the report parse flow in `ocr.rs` + `vision.rs`:
    - File save-to-disk duration
    - File read-from-disk duration
    - Base64 encoding duration
    - Vision API call duration
    - JSON parsing duration
  - Use `std::time::Instant::now()` + `elapsed()` pattern (already used in `ocr.rs:650`)
  - Log total end-to-end time for each flow
  - This establishes the baseline for measuring optimization impact

  **Must NOT do**:
  - Do not change any processing logic, only add timing logs
  - Do not add external dependencies

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Simple instrumentation, adding log lines only
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2, 3)
  - **Blocks**: Tasks 4, 5, 6, 10
  - **Blocked By**: None (can start immediately)

  **References**:

  **Pattern References**:
  - `backend/src/handlers/ocr.rs:650` — Existing `t0.elapsed()` timing pattern used in merge_check
  - `backend/src/handlers/ocr.rs:887-893` — Existing timing in prefetch_normalize

  **Target Files**:
  - `backend/src/handlers/expense.rs:596-946` — `parse_expense()` and `recognize_expense_bytes()` functions
  - `backend/src/ocr/vision.rs:105-163` — `recognize_file_with_client()` function
  - `backend/src/handlers/ocr.rs:85-147` — `ocr_parse()` function

  **Acceptance Criteria**:
  - [ ] `cargo build` succeeds with no errors
  - [ ] Expense parse logs show timing for each stage (compression, base64, API call, parsing)
  - [ ] Report parse logs show timing for each stage (disk I/O, base64, API call, parsing)

  **QA Scenarios:**

  ```
  Scenario: Expense timing logs appear in output
    Tool: Bash (cargo build)
    Preconditions: Backend compiles successfully
    Steps:
      1. Run `cargo build 2>&1` — verify no errors
      2. Run `grep -c "Instant::now\|elapsed" backend/src/handlers/expense.rs` — count timing points
      3. Run `grep -c "Instant::now\|elapsed" backend/src/ocr/vision.rs` — count timing points
    Expected Result: Build succeeds; ≥4 timing points in expense.rs; ≥3 timing points in vision.rs
    Failure Indicators: Build fails; grep returns 0
    Evidence: .sisyphus/evidence/task-1-timing-instrumentation.txt
  ```

  **Commit**: YES
  - Message: `perf(upload): add timing instrumentation to upload flows`
  - Files: `backend/src/handlers/expense.rs`, `backend/src/ocr/vision.rs`, `backend/src/handlers/ocr.rs`
  - Pre-commit: `cargo build`

- [ ] 2. Set Up Test Infrastructure + Test Helpers

  **What to do**:
  - Create `backend/tests/` directory for integration tests
  - Create `backend/tests/common/mod.rs` with shared test utilities:
    - Helper to create an in-memory SQLite `Database` instance for test isolation
    - Helper to build a test `AppState` with mock `http_client`
    - Helper to build an Axum test router with `axum::Router` for handler testing
    - Helper to create a multipart request body from a file bytes + filename
  - Create `backend/tests/upload_test.rs` as a placeholder with one smoke test
  - Follow patterns from `backend/src/algorithm_engine/integration_tests.rs` and `backend/src/db/test_item_repo.rs`
  - Ensure `cargo test --workspace` passes including new test files

  **Must NOT do**:
  - Do not add external test dependencies (no wiremock, mockito) — use reqwest's built-in test server or simple mock
  - Do not write actual upload tests yet (that's Task 9)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Test infrastructure setup requires understanding of Axum test patterns and project structure
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 3)
  - **Blocks**: Task 9
  - **Blocked By**: None (can start immediately)

  **References**:

  **Pattern References**:
  - `backend/src/algorithm_engine/integration_tests.rs` — Existing `#[tokio::test]` pattern
  - `backend/src/db/test_item_repo.rs` — Existing `#[test]` pattern with Database
  - `backend/src/db/mod.rs` — `Database::new()` constructor (use `:memory:` for tests)
  - `backend/src/main.rs:117-122` — `AppState` construction pattern

  **API/Type References**:
  - `backend/src/main.rs:26-37` — `AppState` struct definition
  - `backend/src/routes.rs:13-28` — `build_router()` function

  **Acceptance Criteria**:
  - [ ] `backend/tests/common/mod.rs` exists with test helpers
  - [ ] `backend/tests/upload_test.rs` exists with at least one passing test
  - [ ] `cargo test --workspace` passes (all existing + new tests)

  **QA Scenarios:**

  ```
  Scenario: Test infrastructure works
    Tool: Bash (cargo test)
    Preconditions: Test files created
    Steps:
      1. Run `cargo test --workspace 2>&1` — verify all tests pass
      2. Run `ls backend/tests/common/mod.rs` — verify file exists
      3. Run `ls backend/tests/upload_test.rs` — verify file exists
    Expected Result: All tests pass; both files exist
    Failure Indicators: Test failures; files missing
    Evidence: .sisyphus/evidence/task-2-test-infra.txt

  Scenario: Test helper creates valid AppState
    Tool: Bash (cargo test)
    Preconditions: Test infrastructure created
    Steps:
      1. Run `cargo test --test upload_test 2>&1` — verify smoke test passes
    Expected Result: Test passes with in-memory database
    Failure Indicators: Test panics or fails
    Evidence: .sisyphus/evidence/task-2-test-helper-smoke.txt
  ```

  **Commit**: YES
  - Message: `test(infra): set up test helpers for handler integration tests`
  - Files: `backend/tests/common/mod.rs`, `backend/tests/upload_test.rs`
  - Pre-commit: `cargo test --workspace`

- [x] 3. Fix Report Vision API Timeout (300s → 600s)

  **What to do**:
  - In `backend/src/ocr/vision.rs`, the `recognize_file_with_client()` function calls `call_api()` which uses `llm_post_with_retry()` from `handlers/mod.rs`
  - `llm_post_with_retry()` uses the shared `http_client` which has a global 300s timeout (`main.rs:72`)
  - The expense flow explicitly sets `.timeout(Duration::from_secs(600))` per-request (`expense.rs:885`), overriding the client default
  - The report flow does NOT set a per-request timeout, so it inherits the 300s client default
  - The report uses the larger Qwen3-VL-235B model which is slower — it needs at least 600s
  - Fix: Add `.timeout(Duration::from_secs(600))` to the Vision API call in `call_api()` or `recognize_file_with_client()`
  - Alternative: Pass timeout as parameter to `call_api()` so it can be configured per-caller

  **Must NOT do**:
  - Do not change the global `http_client` timeout (it affects all other API calls)
  - Do not change the model or any other Vision API parameters

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single-line fix, clear location
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 2)
  - **Blocks**: None directly (but improves report reliability)
  - **Blocked By**: None (can start immediately)

  **References**:

  **Pattern References**:
  - `backend/src/handlers/expense.rs:882-885` — How expense sets per-request 600s timeout (the pattern to follow)

  **Target Files**:
  - `backend/src/ocr/vision.rs:165-177` — `call_api()` function where the request is sent via `llm_post_with_retry`
  - `backend/src/handlers/mod.rs:162-199` — `llm_post_with_retry()` function (may need timeout parameter)
  - `backend/src/main.rs:70-78` — Shared `http_client` with 300s default timeout

  **Acceptance Criteria**:
  - [ ] `cargo build` succeeds
  - [ ] Report Vision API call has explicit 600s timeout
  - [ ] Global `http_client` timeout unchanged at 300s

  **QA Scenarios:**

  ```
  Scenario: Report timeout is explicitly set to 600s
    Tool: Bash (grep)
    Preconditions: Code change applied
    Steps:
      1. Run `cargo build 2>&1` — verify no errors
      2. Run `grep -n "600" backend/src/ocr/vision.rs` — verify 600s timeout present
      3. Run `grep -n "timeout.*300" backend/src/main.rs` — verify global timeout unchanged
    Expected Result: Build succeeds; 600s timeout in vision.rs; 300s in main.rs
    Failure Indicators: Build fails; timeout not found
    Evidence: .sisyphus/evidence/task-3-timeout-fix.txt
  ```

  **Commit**: YES
  - Message: `fix(ocr): increase report vision API timeout to 600s`
  - Files: `backend/src/ocr/vision.rs` (and possibly `backend/src/handlers/mod.rs`)
  - Pre-commit: `cargo build`
- [ ] 4. Remove Backend Double Compression for Expense Upload
  **What to do**:
  - In `recognize_expense_bytes()` (`expense.rs:823-946`), the function currently:
    1. Takes raw bytes from multipart upload
    2. Calls `compress_image_to_webp()` via `spawn_blocking` (line 841-845) — resize to 1500px + WebP encode
    3. Base64 encodes the compressed result
  - The frontend (`ExpenseUpload.tsx:116-153`) ALREADY compresses: Canvas resize to 1500px → WebP/JPEG at 0.85 quality
  - This means the backend is decoding an already-compressed WebP, resizing it (no-op since it's already 1500px), and re-encoding to WebP — pure waste
  - **Fix**: Skip `compress_image_to_webp()` for images that are already compressed (WebP/JPEG). Only compress if the image is PNG or very large.
  - Specifically:
    - Check if the uploaded file is already WebP (magic bytes `RIFF....WEBP`) — if so, skip compression entirely
    - Check if the uploaded file is JPEG and < 500KB — if so, skip compression
    - Only run `compress_image_to_webp()` for PNG files or files > 1MB
    - PDF files already pass through unchanged (line 567-569)
  - This saves: image decode time + Lanczos3 resize time + WebP encode time (typically 200-800ms for large images)
  **Must NOT do**:
  - Do not remove the `compress_image_to_webp()` function entirely (it's still needed for PNG/large files)
  - Do not remove frontend Canvas compression
  - Do not change Vision API parameters
  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Requires careful understanding of the compression pipeline and edge cases
  - **Skills**: []
  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 5, 6)
  - **Blocks**: Tasks 9, 10
  - **Blocked By**: Task 1
  **References**:
  **Pattern References**:
  - `backend/src/handlers/expense.rs:565-593` — `compress_image_to_webp()` function (the compression being skipped)
  - `backend/src/handlers/expense.rs:838-845` — Where compression is called in `recognize_expense_bytes()`
  - `frontend/src/pages/ExpenseUpload.tsx:116-153` — Frontend `compressImage()` that already handles compression
  **API/Type References**:
  - `backend/src/middleware.rs:188-228` — `validate_file_magic_bytes()` for detecting file type from magic bytes
  **Acceptance Criteria**:
  - [ ] `cargo build` succeeds
  - [ ] WebP files from frontend skip backend compression (verified by timing logs from Task 1)
  - [ ] PNG files still get compressed
  - [ ] PDF files still pass through unchanged
  - [ ] Vision API still receives valid base64 data URL
  **QA Scenarios:**
  ```
  Scenario: WebP upload skips backend compression
    Tool: Bash (cargo build + grep)
    Preconditions: Task 1 timing instrumentation in place
    Steps:
      1. Run `cargo build 2>&1` — verify no errors
      2. Read `recognize_expense_bytes()` in expense.rs — verify WebP detection logic exists
      3. Verify the function checks magic bytes before deciding to compress
    Expected Result: Build succeeds; WebP files bypass compress_image_to_webp()
    Failure Indicators: Build fails; all files still go through compression
    Evidence: .sisyphus/evidence/task-4-skip-compression.txt
  Scenario: PNG files still get compressed
    Tool: Bash (grep)
    Preconditions: Code change applied
    Steps:
      1. Read the updated `recognize_expense_bytes()` — verify PNG files still call compress_image_to_webp()
    Expected Result: PNG path still includes compression
    Failure Indicators: PNG files skip compression
    Evidence: .sisyphus/evidence/task-4-png-still-compressed.txt
  ```
  **Commit**: YES
  - Message: `perf(expense): skip redundant backend compression for pre-compressed images`
  - Files: `backend/src/handlers/expense.rs`
  - Pre-commit: `cargo build`
- [ ] 5. Add Image Resize + In-Memory Processing for Report Upload
  **What to do**:
  - **Part A: In-memory processing** — Modify `ocr.rs` to process report uploads in memory instead of writing to disk:
    - Replace `save_upload_file()` call in `ocr_parse()` (line 91) with a new `read_upload_bytes()` function (similar to `expense.rs:527-561`)
    - Read multipart field directly into `Vec<u8>` in memory
    - Validate file extension + magic bytes + size (same checks as current)
    - Remove the `tokio::fs::write` and `tokio::fs::read` round-trip
    - Remove the `tokio::fs::remove_file` cleanup (no temp file to clean up)
    - Keep the `file_path` field in `OcrParseResult` as empty string (it was only used for temp file tracking)
  - **Part B: Image resize** — Add image resize before sending to Vision API:
    - In `vision.rs`, modify `recognize_file_with_client()` to accept `&[u8]` + filename instead of file path
    - Add a new function `compress_for_vision(raw_bytes: &[u8], file_name: &str, max_width: u32) -> (Vec<u8>, &'static str)`
    - For images (not PDF): resize to max 2000px width using Lanczos3, encode as WebP
    - For PDF: pass through unchanged
    - This reduces base64 payload size significantly (e.g., 8MB photo → ~200KB WebP)
    - Use `spawn_blocking` for the CPU-bound image processing (same pattern as expense.rs)
  - **Part C: Update callers** — Update `ocr_parse()` to pass bytes directly to the new vision function
  **Must NOT do**:
  - Do not add lossy quality compression (only resize, keep WebP default quality)
  - Do not change Vision API model or parameters
  - Do not change the `upload_file()` endpoint (it's a separate simple upload, not OCR)
  - Do not change the Tesseract OCR fallback path (it still needs a file path — write to temp file only for fallback)
  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Multi-file refactor touching vision.rs, ocr.rs, and their interface
  - **Skills**: []
  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4, 6)
  - **Blocks**: Tasks 9, 10
  - **Blocked By**: Task 1
  **References**:
  **Pattern References**:
  - `backend/src/handlers/expense.rs:527-561` — `read_upload_bytes()` in-memory multipart reading (the pattern to follow for Part A)
  - `backend/src/handlers/expense.rs:565-593` — `compress_image_to_webp()` image resize pattern (the pattern to follow for Part B)
  - `backend/src/handlers/expense.rs:838-853` — How expense does in-memory base64 encoding
  **Target Files**:
  - `backend/src/handlers/ocr.rs:27-69` — `save_upload_file()` to be replaced with in-memory version
  - `backend/src/handlers/ocr.rs:85-147` — `ocr_parse()` to be refactored
  - `backend/src/ocr/vision.rs:105-163` — `recognize_file_with_client()` to accept bytes instead of file path
  **API/Type References**:
  - `backend/src/ocr/vision.rs:80-92` — `detect_mime()` function (still needed)
  - `backend/src/handlers/ocr.rs:78-83` — `OcrParseResult` struct
  **Acceptance Criteria**:
  - [ ] `cargo build` succeeds
  - [ ] `ocr_parse()` no longer writes to disk (no `tokio::fs::write` in the happy path)
  - [ ] Images are resized to max 2000px before Vision API call
  - [ ] PDF files pass through without resize
  - [ ] Tesseract fallback still works (writes temp file only when needed)
  - [ ] `OcrParseResult.file_path` is empty string for in-memory processed files
  **QA Scenarios:**
  ```
  Scenario: Report upload processes in memory (no disk I/O)
    Tool: Bash (grep)
    Preconditions: Code changes applied
    Steps:
      1. Run `cargo build 2>&1` — verify no errors
      2. Run `grep -n "tokio::fs::write" backend/src/handlers/ocr.rs` — verify no disk write in ocr_parse path
      3. Read `ocr_parse()` function — verify it uses in-memory bytes
    Expected Result: Build succeeds; no disk write in main OCR path
    Failure Indicators: Build fails; tokio::fs::write still present in ocr_parse
    Evidence: .sisyphus/evidence/task-5-in-memory.txt
  Scenario: Image resize is applied before Vision API
    Tool: Bash (grep)
    Preconditions: Code changes applied
    Steps:
      1. Run `grep -n "resize\|max_width\|2000" backend/src/ocr/vision.rs` — verify resize logic
      2. Read the new compression function — verify max 2000px width
    Expected Result: Resize logic present with 2000px max width
    Failure Indicators: No resize logic found
    Evidence: .sisyphus/evidence/task-5-image-resize.txt
  ```
  **Commit**: YES
  - Message: `perf(ocr): add image resize and in-memory processing for reports`
  - Files: `backend/src/handlers/ocr.rs`, `backend/src/ocr/vision.rs`
  - Pre-commit: `cargo build`
- [ ] 6. Implement SSE Progress Endpoints for Both Upload Flows
  **What to do**:
  - Create two new SSE endpoints that wrap the existing parse logic with progress events:
    - `POST /api/patients/:patient_id/expenses/parse-sse` — SSE version of expense parse
    - `POST /api/ocr/parse-sse` — SSE version of report parse
  - Follow the existing SSE pattern from `interpret.rs` using `axum::response::sse::Sse` + `async_stream::stream!`
  - Progress event stages for expense:
    - `{"stage": "processing", "message": "图片处理中..."}` — after multipart read
    - `{"stage": "api_call", "message": "AI 识别中..."}` — before Vision API call
    - `{"stage": "parsing", "message": "解析结果中..."}` — after API response, during JSON parsing
    - `{"stage": "done", "data": <ExpenseParseResponse>}` — final result
    - `{"stage": "error", "message": "..."}` — on failure
  - Progress event stages for report:
    - `{"stage": "processing", "message": "图片处理中..."}` — after multipart read, during resize
    - `{"stage": "api_call", "message": "AI 识别中..."}` — before Vision API call
    - `{"stage": "parsing", "message": "解析结果中..."}` — after API response
    - `{"stage": "done", "data": <OcrParseResult>}` — final result
    - `{"stage": "error", "message": "..."}` — on failure
  - Keep the original non-SSE endpoints unchanged (backward compatibility)
  - Register new routes in `routes.rs` under doctor_routes
  - SSE response uses `Content-Type: text/event-stream`
  - Each event is a JSON object sent as SSE `data:` field
  **Must NOT do**:
  - Do not modify the existing non-SSE endpoints
  - Do not change the processing logic (only wrap it with SSE events)
  - Do not use WebSocket (SSE is simpler and sufficient)
  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: SSE implementation requires understanding of Axum streaming + async patterns
  - **Skills**: []
  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 4, 5)
  - **Blocks**: Tasks 7, 8
  - **Blocked By**: Task 1
  **References**:
  **Pattern References**:
  - `backend/src/handlers/interpret.rs` — Existing SSE implementation pattern (the primary reference)
  - Search for `axum::response::sse` and `async_stream::stream!` usage in interpret.rs
  **Target Files**:
  - `backend/src/handlers/expense.rs` — Add `parse_expense_sse()` handler
  - `backend/src/handlers/ocr.rs` — Add `ocr_parse_sse()` handler
  - `backend/src/routes.rs` — Register new SSE routes
  **External References**:
  - Axum SSE docs: `axum::response::sse::Sse` type
  - Dependencies already in Cargo.toml: `tokio-stream`, `async-stream`, `futures-util`
  **Acceptance Criteria**:
  - [ ] `cargo build` succeeds
  - [ ] Two new SSE endpoints registered in routes.rs
  - [ ] SSE endpoints send at least 3 progress stages before final result
  - [ ] Original non-SSE endpoints still work unchanged
  - [ ] SSE events are valid JSON in `data:` field
  **QA Scenarios:**
  ```
  Scenario: SSE endpoints are registered
    Tool: Bash (grep)
    Preconditions: Code changes applied
    Steps:
      1. Run `cargo build 2>&1` — verify no errors
      2. Run `grep -n "parse-sse" backend/src/routes.rs` — verify both SSE routes registered
    Expected Result: Build succeeds; 2 SSE route registrations found
    Failure Indicators: Build fails; routes not found
    Evidence: .sisyphus/evidence/task-6-sse-routes.txt
  Scenario: SSE handler sends progress events
    Tool: Bash (grep)
    Preconditions: Code changes applied
    Steps:
      1. Read the new `parse_expense_sse()` function — verify it yields SSE events at each stage
      2. Verify it uses `async_stream::stream!` pattern
      3. Verify it sends `processing`, `api_call`, `parsing`, `done` stages
    Expected Result: All 4 stages present in the SSE handler
    Failure Indicators: Missing stages; no stream! macro usage
    Evidence: .sisyphus/evidence/task-6-sse-stages.txt
  ```
  **Commit**: YES
  - Message: `feat(upload): add SSE progress endpoints for expense and report parsing`
  - Files: `backend/src/handlers/expense.rs`, `backend/src/handlers/ocr.rs`, `backend/src/routes.rs`
  - Pre-commit: `cargo build`
- [ ] 7. Frontend SSE Progress UI for Expense Upload
  **What to do**:
  - Modify `ExpenseUpload.tsx` to use the new SSE endpoint instead of the regular POST:
    - In `handleParse()` (line 155-173), replace `api.expenses.parse()` call with an EventSource connection to `/api/patients/:id/expenses/parse-sse`
    - Since SSE is typically GET-based but our endpoint is POST with multipart, use `fetch()` with streaming response instead of `EventSource`:
      - Send POST with FormData to the SSE endpoint
      - Read the response as a ReadableStream
      - Parse SSE events from the stream (split by `\n\n`, parse `data:` lines)
    - Update UI state based on progress events:
      - `processing` → Show "图片处理中..."
      - `api_call` → Show "AI 识别中..." with elapsed time counter
      - `parsing` → Show "解析结果中..."
      - `done` → Call `finishParse()` with the result data
      - `error` → Show error toast
    - Add a new signal `parseStage` to track current stage
    - Show elapsed time during `api_call` stage (the longest wait)
    - Keep the existing `parsing` signal for overall loading state
  - Add SSE parse method to `api/client.ts`:
    - New function `api.expenses.parseSse(patientId, file, onProgress, onDone, onError)`
    - Uses `fetch` + `ReadableStream` to handle POST SSE
  **Must NOT do**:
  - Do not remove the non-SSE `api.expenses.parse()` method (keep as fallback)
  - Do not change the preview/edit/confirm UI
  - Do not change the analysis flow (it's already separate and concurrent)
  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
    - Reason: Frontend UI work with progress indicators and streaming
  - **Skills**: [`playwright`]
    - `playwright`: For verifying the progress UI renders correctly
  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 8, 9)
  - **Blocks**: None
  - **Blocked By**: Task 6
  **References**:
  **Pattern References**:
  - `frontend/src/pages/ExpenseUpload.tsx:155-173` — Current `handleParse()` to be modified
  - `frontend/src/components/LlmInterpret.tsx` — Check if there's existing SSE/streaming handling in frontend (interpret uses SSE)
  **Target Files**:
  - `frontend/src/pages/ExpenseUpload.tsx` — Main file to modify
  - `frontend/src/api/client.ts` — Add SSE parse method
  **API/Type References**:
  - `frontend/src/api/types.ts` — `ExpenseParseResponse`, `DayParseResult` types
  - `frontend/src/api/client.ts:268-276` — Current `api.expenses.parse()` method
  **Acceptance Criteria**:
  - [ ] Expense upload shows progress stages during parsing
  - [ ] Elapsed time counter visible during AI recognition stage
  - [ ] Final result correctly populates the preview/edit UI
  - [ ] Error handling works (shows toast on failure)
  **QA Scenarios:**
  ```
  Scenario: Expense upload shows SSE progress stages
    Tool: Playwright
    Preconditions: Backend running with SSE endpoints; logged in as Doctor
    Steps:
      1. Navigate to a patient's expense page
      2. Open expense upload modal
      3. Select a test expense image file
      4. Click "开始识别"
      5. Observe progress text changes: "图片处理中..." → "AI 识别中..." → "解析结果中..."
      6. Wait for completion — verify preview UI shows parsed results
    Expected Result: At least 2 distinct progress messages visible before result; preview shows parsed expense items
    Failure Indicators: No progress messages; spinner only; parse fails
    Evidence: .sisyphus/evidence/task-7-expense-sse-progress.png
  Scenario: Expense upload error handling via SSE
    Tool: Playwright
    Preconditions: Backend running
    Steps:
      1. Upload an invalid file (e.g., a .txt file renamed to .png)
      2. Verify error toast appears
    Expected Result: Error toast with meaningful message
    Failure Indicators: Silent failure; no error feedback
    Evidence: .sisyphus/evidence/task-7-expense-sse-error.png
  ```
  **Commit**: YES
  - Message: `feat(ui): add SSE progress display for expense upload`
  - Files: `frontend/src/pages/ExpenseUpload.tsx`, `frontend/src/api/client.ts`
  - Pre-commit: `cd frontend && npm run build`
- [ ] 8. Frontend SSE Progress UI for Report Upload
  **What to do**:
  - Modify `ReportUpload.tsx` to use the new SSE endpoint for each file parse:
    - In `startParsing()` (line 142-175), replace `api.ocr.parse(file, timeout)` with SSE streaming version
    - Since reports support multi-file upload with `Promise.all`, each file gets its own SSE stream
    - Update progress display:
      - Currently shows `parseProgress` as a count of completed files (line 143)
      - Add per-file stage tracking: show which file is in which stage
      - Example: "File 1/3: AI 识别中... | File 2/3: 图片处理中... | File 3/3: 等待中"
    - Use the existing `Progress` component for overall progress bar
    - Add per-file status indicators
  - Add SSE parse method to `api/client.ts`:
    - New function `api.ocr.parseSse(file, onProgress, onDone, onError)`
  **Must NOT do**:
  - Do not change the grouping/merge/confirm flow
  - Do not change the file selection UI
  - Do not remove the non-SSE `api.ocr.parse()` method
  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
    - Reason: Frontend UI work with multi-file progress tracking
  - **Skills**: [`playwright`]
    - `playwright`: For verifying multi-file progress UI
  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 7, 9)
  - **Blocks**: None
  - **Blocked By**: Task 6
  **References**:
  **Pattern References**:
  - `frontend/src/pages/ReportUpload.tsx:142-175` — Current `startParsing()` with Promise.all
  - `frontend/src/pages/ReportUpload.tsx:43-44` — `parseProgress` signal
  - Task 7's SSE client implementation — Reuse the same SSE fetch pattern
  **Target Files**:
  - `frontend/src/pages/ReportUpload.tsx` — Main file to modify
  - `frontend/src/api/client.ts` — Add SSE parse method for OCR
  **API/Type References**:
  - `frontend/src/api/types.ts` — `OcrParseResult` type
  - `frontend/src/api/client.ts:233-241` — Current `api.ocr.parse()` method
  **Acceptance Criteria**:
  - [ ] Report upload shows per-file progress stages
  - [ ] Overall progress bar updates as files complete
  - [ ] All files parse successfully and populate preview UI
  **QA Scenarios:**
  ```
  Scenario: Multi-file report upload shows per-file progress
    Tool: Playwright
    Preconditions: Backend running with SSE endpoints; logged in as Doctor
    Steps:
      1. Navigate to a patient's report page
      2. Open report upload modal
      3. Select 2-3 test report images
      4. Click next to start parsing
      5. Observe per-file progress indicators
      6. Wait for all files to complete
    Expected Result: Each file shows its own progress stage; overall progress bar advances; all files parsed
    Failure Indicators: No per-file progress; files fail silently
    Evidence: .sisyphus/evidence/task-8-report-sse-progress.png
  ```
  **Commit**: YES
  - Message: `feat(ui): add SSE progress display for report upload`
  - Files: `frontend/src/pages/ReportUpload.tsx`, `frontend/src/api/client.ts`
  - Pre-commit: `cd frontend && npm run build`
- [ ] 9. Write Integration Tests for Optimized Upload Flows
  **What to do**:
  - Using the test infrastructure from Task 2, write integration tests:
  - **Expense tests** (`backend/tests/expense_upload_test.rs`):
    - Test that WebP file input skips backend compression (verify by checking that `compress_image_to_webp` is not called for WebP)
    - Test that PNG file input still gets compressed
    - Test that PDF file input passes through unchanged
    - Test multipart upload validation (invalid file type, oversized file)
  - **Report tests** (`backend/tests/report_upload_test.rs`):
    - Test that image files are resized (verify output dimensions ≤ 2000px)
    - Test that PDF files pass through without resize
    - Test in-memory processing (no temp files created)
    - Test multipart upload validation
  - **SSE tests** (`backend/tests/sse_test.rs`):
    - Test that SSE endpoints return `text/event-stream` content type
    - Test that SSE stream contains expected event stages
    - Test error handling (invalid file → error event)
  - Mock the Vision API calls (don't make real API calls in tests):
    - Create a mock HTTP server or use conditional compilation to return canned responses
    - Or: test only the pre-API-call pipeline (compression, validation) and post-API-call pipeline (parsing)
  **Must NOT do**:
  - Do not make real Vision API calls in tests
  - Do not test the Vision API response parsing (that's already working and not being changed)
  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Integration test writing requires understanding of Axum test patterns and mock strategies
  - **Skills**: []
  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 7, 8)
  - **Blocks**: None
  - **Blocked By**: Tasks 2, 4, 5
  **References**:
  **Pattern References**:
  - `backend/tests/common/mod.rs` — Test helpers from Task 2
  - `backend/src/algorithm_engine/integration_tests.rs` — Existing test patterns
  **Target Files**:
  - `backend/tests/expense_upload_test.rs` — Expense upload tests
  - `backend/tests/report_upload_test.rs` — Report upload tests
  - `backend/tests/sse_test.rs` — SSE endpoint tests
  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` passes with all new tests
  - [ ] At least 3 expense upload tests
  - [ ] At least 3 report upload tests
  - [ ] At least 2 SSE tests
  - [ ] No real API calls made during tests
  **QA Scenarios:**
  ```
  Scenario: All integration tests pass
    Tool: Bash (cargo test)
    Preconditions: All implementation tasks complete
    Steps:
      1. Run `cargo test --workspace 2>&1`
      2. Count test results
    Expected Result: All tests pass; ≥8 new tests total
    Failure Indicators: Any test failure
    Evidence: .sisyphus/evidence/task-9-integration-tests.txt
  ```
  **Commit**: YES
  - Message: `test(upload): add integration tests for optimized upload flows`
  - Files: `backend/tests/expense_upload_test.rs`, `backend/tests/report_upload_test.rs`, `backend/tests/sse_test.rs`
  - Pre-commit: `cargo test --workspace`
- [ ] 10. Performance Comparison — Before vs After Timing
  **What to do**:
  - Using the timing instrumentation from Task 1, document the performance improvement:
  - Create a test script that:
    1. Uploads a known test image to the expense parse endpoint
    2. Uploads a known test image to the report parse endpoint
    3. Records timing for each stage from the server logs
    4. Compares with baseline (if available) or documents current timings
  - Create `.sisyphus/evidence/performance-comparison.md` with:
    - Expense flow: time per stage (before optimization vs after)
    - Report flow: time per stage (before optimization vs after)
    - Total wall time comparison
    - Specific improvements noted (e.g., "backend compression eliminated: saved Xms")
  - Note: Actual Vision API latency won't change (it's external), but pre/post processing time should decrease
  **Must NOT do**:
  - Do not make this a blocking requirement (API latency varies)
  - Do not modify any code in this task (measurement only)
  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Requires careful analysis of timing data and meaningful comparison
  - **Skills**: []
  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 4 (sequential)
  - **Blocks**: None
  - **Blocked By**: Tasks 4, 5, 6
  **References**:
  - Task 1's timing instrumentation output
  - Server logs with timing data
  **Acceptance Criteria**:
  - [ ] Performance comparison document created
  - [ ] Expense pre-processing time decreased (compression step eliminated for WebP)
  - [ ] Report pre-processing time documented (resize added but disk I/O removed)
  **QA Scenarios:**
  ```
  Scenario: Performance comparison document exists with data
    Tool: Bash (ls + cat)
    Preconditions: All optimization tasks complete; backend running
    Steps:
      1. Run `ls .sisyphus/evidence/performance-comparison.md` — verify file exists
      2. Read the file — verify it contains timing data for both flows
    Expected Result: File exists with expense and report timing data
    Failure Indicators: File missing; no timing data
    Evidence: .sisyphus/evidence/task-10-perf-comparison.txt
  ```
  **Commit**: NO (documentation only, groups with final commit)

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Rejection → fix → re-run.

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, curl endpoint, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files exist in .sisyphus/evidence/. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo build` + `cargo test --workspace` + `cargo clippy`. Review all changed files for: `unwrap()` in production paths, empty error handling, unused imports, dead code. Check AI slop: excessive comments, over-abstraction, generic names.
  Output: `Build [PASS/FAIL] | Tests [N pass/N fail] | Clippy [PASS/FAIL] | Files [N clean/N issues] | VERDICT`

- [ ] F3. **Real Manual QA** — `unspecified-high` (+ `playwright` skill for frontend)
  Start from clean state. Test expense upload with a real image — verify SSE progress events fire, verify parse result is correct. Test report upload — verify compression applied, SSE works, parse result correct. Test edge cases: empty file, oversized file, PDF file. Save evidence to `.sisyphus/evidence/final-qa/`.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual diff (git log/diff). Verify 1:1 — everything in spec was built, nothing beyond spec was built. Check "Must NOT do" compliance: Vision API params unchanged, models unchanged, frontend compression preserved. Flag unaccounted changes.
  Output: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Unaccounted [CLEAN/N files] | VERDICT`

---

## Commit Strategy

- **Wave 1**: `perf(upload): add timing instrumentation to upload flows` — expense.rs, vision.rs
- **Wave 1**: `test(infra): set up test helpers for handler integration tests` — test files
- **Wave 1**: `fix(ocr): increase report vision API timeout to 600s` — vision.rs
- **Wave 2**: `perf(expense): remove redundant backend image compression` — expense.rs
- **Wave 2**: `perf(ocr): add image resize and in-memory processing for reports` — ocr.rs, vision.rs
- **Wave 2**: `feat(upload): add SSE progress endpoints for expense and report parsing` — expense.rs, ocr.rs, routes.rs
- **Wave 3**: `feat(ui): add SSE progress display for expense upload` — ExpenseUpload.tsx
- **Wave 3**: `feat(ui): add SSE progress display for report upload` — ReportUpload.tsx
- **Wave 3**: `test(upload): add integration tests for optimized upload flows` — test files

---

## Success Criteria

### Verification Commands
```bash
cargo build 2>&1 | tail -3          # Expected: Finished
cargo test --workspace 2>&1 | tail -5  # Expected: test result: ok
cargo clippy 2>&1 | tail -3         # Expected: no warnings/errors

# Verify no double compression:
grep -c "compress_image_to_webp" backend/src/handlers/expense.rs  # Expected: 0 (removed)

# Verify report compression added:
grep -c "resize\|compress" backend/src/ocr/vision.rs  # Expected: >0

# Verify report timeout fixed:
grep "600" backend/src/ocr/vision.rs  # Expected: timeout set to 600s

# Verify SSE endpoints exist:
grep "parse-sse" backend/src/routes.rs  # Expected: 2 matches
```

### Final Checklist
- [ ] All "Must Have" present
- [ ] All "Must NOT Have" absent
- [ ] All tests pass
- [ ] Timing comparison shows improvement
