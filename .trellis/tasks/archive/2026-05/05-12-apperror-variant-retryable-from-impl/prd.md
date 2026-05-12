# PRD: 重构 AppError

## 背景

当前 AppError 存在以下问题：
1. `error.rs` 硬依赖 sea_orm、serde_json、reqwest 等外部库（通过 `From` impl）
2. `Dependency` variant 过于粗放，数据库、网络、外部服务错误混在一起
3. `RuleRejected` 和 `Runtime` 语义模糊，与 `InvalidParameter`、`ExternalService` 等重叠
4. retry 判断通过硬编码函数 `is_permanent_index_source_error` 实现，而非内嵌在 error 定义中
5. `AppErrorKind` 与 AppError variant 一一对应，冗余

## 设计决策

### 1. 新 AppError 定义

```rust
pub enum AppError {
    InvalidParameter(String),          // 不可重试
    NotFound(String),                  // 不可重试
    Database(String, bool),            // (message, retryable)
    ExternalService(String, bool),     // (message, retryable)
    Network(String, bool),             // (message, retryable)
    Internal(String),                  // 兜底，不可重试
}
```

- 消除 `RuleRejected` 和 `Runtime`
- 消除 `AppErrorKind`，直接 match variant
- 新增 `is_retryable()` 方法

### 2. RequestError → AppError 映射

| RequestError | AppError |
|---|---|
| ShareAuditNotPass, ShareCancelled | ExternalService(_, false) |
| Unauthorized, NotFound | ExternalService(_, false) |
| TooManyRequests | ExternalService(_, true) |
| BadRequest | InvalidParameter |
| ServerError | ExternalService(_, true) |
| ConnectError, Timeout | Network(_, true) |
| Other | Internal |

### 3. From impl 下沉

所有 From impl 移到 `infrastructure/error_conversions.rs`：
- `From<sea_orm::error::DbErr>`
- `From<RequestError>`
- `From<std::io::Error>`
- `From<serde_json::Error>`

`error.rs` 只保留 AppError 纯定义，零外部依赖。

### 4. 删除 `is_permanent_index_source_error`

改用 `app_error.is_retryable()`。

### 5. Event Worker 适配

worker 中 handler 返回 error 时，检查 `is_retryable()`，false 则直接 ack 而非无限重试。

## 影响范围

- `app/src/error.rs` — 核心改动
- `app/src/infrastructure/error_conversions.rs` — 新文件
- `app/src/infrastructure/client/mod.rs` — RequestError 定义不变
- `app/src/application/file_index.rs` — 删除 `is_permanent_index_source_error`
- `app/src/infrastructure/event_bus/worker.rs` — 重试逻辑适配
- `app/src/interface/http/media.rs` — HTTP 状态码映射直接 match variant
- `app/src/interface/telegram/file_index.rs` — 错误消息映射适配
- ~24 个引用 AppError 的文件 — variant 名称和字段变更
