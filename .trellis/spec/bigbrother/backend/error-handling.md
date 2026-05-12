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
    Dependency(String),      // External service failures (retryable)
    RuleRejected(String),    // Business rule violations (not retryable)
    Runtime(String),         // Unexpected runtime errors (retryable)
    Internal(String),        // Catch-all internal errors (retryable)
}
```

`AppErrorKind` mirrors the variants without message — use for pattern matching when you don't need the message text.

---

## RequestError (Infrastructure)

Defined in `infrastructure/client/mod.rs`. Fine-grained error types for HTTP clients:

```rust
pub enum RequestError {
    AlreadyExists,
    ShareAuditNotPass,
    ShareCancelled(String),
    Unauthorized,          // HTTP 401
    NotFound(String),      // HTTP 404
    TooManyRequests,       // HTTP 429
    BadRequest(String),    // HTTP 4xx (excl. 401/404/429)
    ConnectError(String),  // Network connection failure
    Timeout(String),       // Request timeout
    ServerError(String),   // HTTP 5xx
    Other(String),         // Serialization, parse, business logic errors
}
```

**Do not use** `Other` as a catch-all for HTTP errors — use the appropriate variant.

### RequestError → AppError Mapping

| RequestError | AppError | Retryable |
|---|---|---|
| `ShareAuditNotPass` | `RuleRejected` | No |
| `ShareCancelled` | `NotFound` | No |
| `BadRequest` | `InvalidParameter` | No |
| `ServerError` | `Dependency` | Yes |
| `ConnectError` | `Dependency` | Yes |
| `Timeout` | `Dependency` | Yes |
| `Unauthorized` | `Dependency` | Yes |
| `NotFound` | `Dependency` | Yes |
| `TooManyRequests` | `Dependency` | Yes |
| `AlreadyExists` | `Internal` | No |
| `Other` | `Internal` | Yes* |

*`Internal` is classified as retryable by `is_permanent_index_source_error`.

---

## Conversion Rules

`AppError` implements `From` for standard error types:
- `io::Error` → `Internal`
- `serde_json::Error` → `Internal`
- `RequestError` → varies (see table above)
- `sea_orm::DbErr` → `Dependency`

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

## Retry Classification

`application/file_index.rs` has `is_permanent_index_source_error`:

- **Permanent** (not retryable): `InvalidParameter`, `NotFound`, `RuleRejected`
- **Transient** (retryable): `Dependency`, `Runtime`, `Internal`

---

## Domain-Specific Errors

Complex modules may define their own error types (e.g., `DownloadUrlError`) that convert to `AppError` at the service boundary. Keep these in the application layer as port types.

```rust
// application/ports.rs
pub enum DownloadUrlError {
    Unauthorized,
    NotFound(String),
    Error(String),
}
```

Conversion happens manually in the application service (e.g., `resolve_download_url.rs`), not via `From` impls.

---

## Rules

- **Never** use `Box<dyn Error>` — always use typed `AppError`
- **Never** `.unwrap()` in production code (tests only)
- **Never** `panic!` in business logic
- User-facing messages should be in **Chinese** (e.g., "未找到匹配文件")
- Prefer `AppError::NotFound("资源名称".into())` over generic messages
- Use `RequestError::ServerError` for HTTP 5xx, `RequestError::Other` for non-HTTP errors
