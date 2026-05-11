# Error Handling

> Error types, conversion strategies, and handling patterns.

---

## Core Types

Defined in `app/src/error.rs`:

```rust
pub type AppResult<T> = Result<T, AppError>;

pub enum AppError {
    InvalidParameter(String),
    NotFound(String),
    Dependency(String),      // External service failures
    RuleRejected(String),    // Business rule violations
    Runtime(String),         // Unexpected runtime errors
    Internal(String),        // Catch-all internal errors
}
```

`AppErrorKind` mirrors the variants without message — use for pattern matching when you don't need the message text.

---

## Conversion Rules

`AppError` implements `From` for standard error types:
- `io::Error` → `Runtime`
- `serde_json::Error` → `Runtime`
- `reqwest::Error` (as `RequestError`) → `Dependency`
- `sea_orm::DbErr` → `Runtime`

Use `?` operator at service/repo boundaries — the `From` impls handle conversion automatically.

---

## HTTP Mapping

`interface/http/media.rs` has `map_app_error_to_response`:

| AppError variant | HTTP Status |
|---|---|
| `InvalidParameter` | 400 Bad Request |
| `NotFound` | 404 Not Found |
| `Dependency` | 502 Bad Gateway |
| `RuleRejected` | 422 Unprocessable Entity |
| `Runtime` / `Internal` | 500 Internal Server Error |

---

## Domain-Specific Errors

Complex modules may define their own error types (e.g., `DownloadUrlError`) that convert to `AppError` at the service boundary. Keep these in the domain or application layer.

```rust
// application/resolve_download_url.rs
enum DownloadUrlError {
    Expired,
    QuotaExceeded,
}

impl From<DownloadUrlError> for AppError {
    fn from(e: DownloadUrlError) -> Self {
        match e {
            DownloadUrlError::Expired => AppError::RuleRejected("链接已过期".into()),
            DownloadUrlError::QuotaExceeded => AppError::Dependency("配额已满".into()),
        }
    }
}
```

---

## Rules

- **Never** use `Box<dyn Error>` — always use typed `AppError`
- **Never** `.unwrap()` in production code (tests only)
- **Never** `panic!` in business logic
- User-facing messages should be in **Chinese** (e.g., "未找到匹配文件")
- Prefer `AppError::NotFound("资源名称".into())` over generic messages
