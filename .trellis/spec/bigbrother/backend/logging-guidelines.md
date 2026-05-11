# Logging Guidelines

> Structured logging, log levels, and conventions.

---

## Logging Framework

Uses the **tracing** ecosystem (`tracing` + `tracing-subscriber`). Setup in `app/src/logger.rs`.

---

## Log Output

Two separate log files, both with daily rotation (max 3 files retained):

| File | Purpose | Filter |
|---|---|---|
| `bigbrother*.log` | Application logs | All targets |
| `access.http*.log` | HTTP access logs | Module target `bigbrother::interface::http::log` |

Console output via `init_console()` for CLI commands (stdout, not file).

---

## Log Levels

| Level | When to use |
|---|---|
| `error!` | Failures that need attention (external API errors, panics, DB errors) |
| `warn!` | Degraded but recoverable (retry succeeded, deprecated API used) |
| `info!` | Key operations (import started/completed, server started, config loaded) |
| `debug!` | Detailed flow (request/response bodies, state transitions) |
| `trace!` | Very low-level (loop iterations, cache hits/misses) |

---

## Structured Fields

Always use structured fields, not string interpolation:

```rust
// Good
tracing::info!(key = %keyword, "关键词添加成功");
tracing::error!(url = %share_url, error = %e, "分享链接解析失败");

// Bad
tracing::info!("关键词添加成功: {}", keyword);
```

---

## Span Usage

Use `#[instrument]` for service methods with meaningful parameters:

```rust
#[instrument(skip(self), fields(share_url = %input.share_url))]
pub async fn import(&self, input: ImportInput) -> AppResult<ImportResult> {
    // ...
}
```

---

## Panic Hook

`logger.rs` installs a custom panic hook that logs panics via `tracing::error!` with backtrace. Do not add separate panic handling.

---

## Rules

- **Never** use `println!` in library/application code — only in `interface/cli/` handlers
- **Never** log sensitive data (passwords, tokens, full URLs with credentials)
- Use Chinese for user-facing log messages
- Prefer structured fields over string formatting
