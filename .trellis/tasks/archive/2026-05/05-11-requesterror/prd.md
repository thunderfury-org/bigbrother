# 细化 RequestError 错误分类

## Context

当前 `RequestError::Error(String)` 是一个垃圾桶 variant，混合了：
- HTTP transport 层错误（DNS/连接/超时）
- HTTP 4xx/5xx 响应错误
- 序列化/解析失败
- 各云盘 API 业务错误

这导致上层无法区分可重试错误（网络抖动、5xx）和不可重试错误（4xx、序列化），`is_permanent_index_source_error` 对所有 `Dependency` 一视同仁。

## 改动范围

### 1. `RequestError` variant 重构（`infrastructure/client/mod.rs`）

删除 `Error(String)`，新增 4 个明确 variant：

```rust
pub enum RequestError {
    // 保留现有
    AlreadyExists,
    ShareAuditNotPass,
    ShareCancelled(String),
    Unauthorized,          // HTTP 401
    NotFound(String),      // HTTP 404
    TooManyRequests,       // HTTP 429
    // 新增
    BadRequest(String),    // HTTP 4xx（不含 401/404/429）
    ConnectError(String),  // 网络连接失败（DNS/连接被拒）
    Timeout(String),       // 请求超时
    ServerError(String),   // HTTP 5xx
    Other(String),         // 序列化/解析/业务逻辑错误
}
```

### 2. `From<reqwest::Error>` 改造（`infrastructure/client/mod.rs`）

```rust
impl From<reqwest::Error> for RequestError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout(e.to_string())
        } else if e.is_connect() {
            Self::ConnectError(e.to_string())
        } else {
            Self::Other(format!("http request error: {e}"))
        }
    }
}
```

`From<reqwest_middleware::Error>`：解构后 `Reqwest(e)` 委托给上面，`Middleware(e)` → `Other`。

### 3. `process_response` 改造（`infrastructure/client/http.rs`）

```rust
match status {
    StatusCode::UNAUTHORIZED => Err(RequestError::Unauthorized),
    StatusCode::NOT_FOUND => Err(RequestError::NotFound(url)),
    StatusCode::TOO_MANY_REQUESTS => Err(RequestError::TooManyRequests),
    s if s.is_client_error() => Err(RequestError::BadRequest(...)),
    _ => Err(RequestError::ServerError(...)),  // 5xx
}
```

JSON 解析失败 → `RequestError::Other`。

### 4. `download_file` 改造（`infrastructure/client/http.rs`）

同 `process_response` 逻辑，4xx → `BadRequest`，5xx → `ServerError`，文件系统错误 → `Other`。

### 5. `From<RequestError> for AppError` 更新（`error.rs`）

```rust
impl From<RequestError> for AppError {
    fn from(e: RequestError) -> Self {
        match e {
            RequestError::ShareAuditNotPass => Self::RuleRejected(...),
            RequestError::ShareCancelled(msg) => Self::NotFound(...),
            RequestError::BadRequest(msg) => Self::InvalidParameter(msg),
            RequestError::ServerError(msg)
            | RequestError::ConnectError(msg)
            | RequestError::Timeout(msg) => Self::Dependency(msg),
            other => Self::Internal(format!("request error, {other}")),
        }
    }
}
```

### 6. 批量替换

`pan115.rs`、`pan189.rs`、`pan123.rs`、`quark.rs`、`tmdb.rs` 中所有 `RequestError::Error` → `RequestError::Other`。

`map_download_url_error` 新增 `ServerError` 和 `Other` 两个分支，统一映射到 `DownloadUrlError::Error`。

### 7. 测试

- `http.rs`：新增 400/403/500/503 mock 测试，验证 `BadRequest`/`ServerError`
- `error.rs`：新增 `BadRequest`/`ConnectError`/`Timeout`/`ServerError`/`Other` 映射测试
- `library_remote.rs`：`map_download_url_error` 新增 3 个 variant 映射测试

## 不改的部分

- `DownloadUrlError` 不新增 variant（端口层保持简单）
- `is_permanent_index_source_error` 不需要改（`BadRequest→InvalidParameter` 永久失败，`ServerError/ConnectError/Timeout→Dependency` 可重试，`Other→Internal` 也是可重试）
- `error.rs` 反向依赖 `infrastructure::client::RequestError` 后续单独处理

## 验证

```bash
cargo test -p bigbrother
cargo clippy -p bigbrother
```
