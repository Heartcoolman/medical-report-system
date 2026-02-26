# Learnings

## Project Conventions
- Rust backend (Axum 0.7 + Tokio + SQLite via rusqlite)
- Solid.js frontend (Vite + TypeScript + Tailwind)
- Timing pattern: `let t0 = std::time::Instant::now(); ... tracing::info!("耗时 {:.1}s", t0.elapsed().as_secs_f64());`
- Error type: `AppError::BadRequest(String)` / `AppError::Internal(String)`
- Multipart in-memory pattern: `field.bytes().await` → `Vec<u8>`
- SSE pattern: `axum::response::sse::Sse` + `async_stream::stream!` (see interpret.rs)
- spawn_blocking for CPU-heavy work (image compression, base64 encoding)
- All handlers use `State(state): State<AppState>` extractor
- Auth: `auth: crate::auth::AuthUser` extractor (first param)
- API key helpers: `get_siliconflow_api_key()`, `get_llm_api_key()`

## File Locations
- Expense handler: backend/src/handlers/expense.rs
- OCR handler: backend/src/handlers/ocr.rs
- Vision module: backend/src/ocr/vision.rs
- Routes: backend/src/routes.rs
- Middleware: backend/src/middleware.rs
- AppState: backend/src/main.rs:26-37
- Existing tests: backend/src/algorithm_engine/integration_tests.rs, backend/src/db/test_item_repo.rs

## Key Functions
- `compress_image_to_webp()`: expense.rs:565-593 — resize to 1500px Lanczos3 + WebP encode
- `read_upload_bytes()`: expense.rs:527-561 — in-memory multipart reading pattern
- `recognize_expense_bytes()`: expense.rs:823-946 — main expense parse pipeline
- `save_upload_file()`: ocr.rs:27-69 — disk-based upload (to be replaced)
- `ocr_parse()`: ocr.rs:85-147 — main report parse pipeline
- `recognize_file_with_client()`: vision.rs:105-163 — Vision API call (takes file path)
- `llm_post_with_retry()`: handlers/mod.rs:162-199 — shared LLM HTTP call with retry

## Timing Pattern (from ocr.rs:650)
```rust
let t0 = std::time::Instant::now();
// ... work ...
tracing::info!("操作耗时 {:.1}s", t0.elapsed().as_secs_f64());
```
