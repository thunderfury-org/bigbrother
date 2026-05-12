# Error Handling

> Error types, conversion strategies, and handling patterns.

---

## Core Types

Defined in `app/src/error.rs`:

```rust
pub type AppResult<T> = Result<T, AppError>;

pub enum AppError {
    InvalidParameter(String),              // Always not retryable
    NotFound(String),                      // Always not retryable
    Database(String, bool),                // (message, retryable)
    ExternalService(String, bool),         // (message, retryable)
    Network(String, bool),                 // (message, retryable)
    Internal(String),                      // Catch-all, always not retryable
}
```

`is_retryable()` method returns the retryable flag for Database/ExternalService/Network variants; returns `false` for InvalidParameter/NotFound/Internal.

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
| `ShareAuditNotPass` | `ExternalService` | No |
| `ShareCancelled` | `ExternalService` | No |
| `Unauthorized` | `ExternalService` | No |
| `NotFound` | `ExternalService` | No |
| `BadRequest` | `InvalidParameter` | No |
| `TooManyRequests` | `ExternalService` | Yes |
| `ServerError` | `ExternalService` | Yes |
| `ConnectError` | `Network` | Yes |
| `Timeout` | `Network` | Yes |
| `AlreadyExists` | `Internal` | No |
| `Other` | `Internal` | No |

---

## Conversion Rules

All `From` impls are in `infrastructure/error_conversions.rs`. `error.rs` has zero external dependencies.

Supported conversions:
- `sea_orm::DbErr` → `Database` (ConnectionAcquire/Conn/Exec/Query → retryable, others → not retryable)
- `RequestError` → varies (see table above)
- `std::io::Error` → `Internal`
- `serde_json::Error` → `Internal`
- `teloxide::RequestError` → `Network` (retryable) for Network/RetryAfter variants, `ExternalService` (not retryable) for Api/InvalidJson/Io/MigrateToChatId
- `teloxide::DownloadError` → `Network` (retryable) for Network variant, `Internal` for Io

Use `?` operator at service/repo boundaries — the `From` impls handle conversion automatically.

---

## HTTP Mapping

`interface/http/media.rs` has `map_app_error_to_response` (matches directly on variant, no AppErrorKind):

| AppError variant | HTTP Status |
|---|---|
| `InvalidParameter` | 400 Bad Request |
| `NotFound` | 404 Not Found |
| `Database` / `Network` / `ExternalService` | 502 Bad Gateway |
| `Internal` | 500 Internal Server Error |

---

## Retry Classification

`is_retryable()` method on AppError:

- **Not retryable**: `InvalidParameter`, `NotFound`, `Internal`, `Database(_, false)`, `ExternalService(_, false)`, `Network(_, false)`
- **Retryable**: `Database(_, true)`, `ExternalService(_, true)`, `Network(_, true)`

Event worker (`infrastructure/event_bus/worker.rs`) checks `is_retryable()`:
- Retryable errors → retry after delay
- Non-retryable errors → ack event and skip

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
- `error_conversions.rs` is the single source of truth for all `From` impls
