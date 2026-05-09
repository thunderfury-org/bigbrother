---
title: "Unified Media Source Event: Single Crawl for Index and Import"
module: app/src/application
date: "2026-05-09"
problem_type: architecture_pattern
component: service_object
severity: medium
applies_when:
  - 多个模块对同一外部资源发起重复的昂贵调用（API 请求、大文件下载、复杂计算）
  - 同步阻塞操作影响用户交互体验，需要拆分为异步事件驱动
  - 一个组件承担了过多职责，需要按关注点拆分
  - 泛型参数的 trait bound 通过其他方式满足，需要保留类型参数但不再需要字段
tags:
  - event-bus
  - share-crawler
  - telegram-bot
  - refactoring
  - api-deduplication
  - phantom-data
related_components:
  - app/src/application/import_media.rs
  - app/src/application/import/share.rs
  - app/src/application/import/factory.rs
  - app/src/application/file_index.rs
  - app/src/bootstrap/mod.rs
  - app/src/interface/telegram/file_index.rs
  - app/src/interface/telegram/mod.rs
  - app/src/main.rs
---

# Unified Media Source Event: Single Crawl for Index and Import

## Context

Telegram bot 收到消息后，file index 和 import 两个模块各自独立查询同一 share URL 的文件列表。`publish_file_index_event()` 通过 EventBus 触发 handler 调用 provider API 获取文件列表记录索引，同时 `MsgProcessor.process()` 同步阻塞地再次调用同一 provider API 获取文件列表执行导入。

这导致两个问题：
1. **重复 API 调用** — 同一 share URL 被 BFS 遍历两次，给云盘 API 造成 rate limit 压力，处理耗时翻倍。
2. **Bot 阻塞** — import 是同步阻塞的，用户发送消息后必须等待整个导入流程完成才能得到响应。

此外，BotServices 持有 `import_service`、`file_index_events`、`file_index_ingest_dir` 等多个与消息处理无关的字段，`MsgProcessor` 同时承担了 URL 提取、文件下载、导入执行、消息通知等多项职责，耦合严重。

## Guidance

### 1. 提取只读关注点为独立组件

将 BFS 遍历（从 share URL / fslink / JSON 获取 `Vec<RawFile>`）从 import 模块中提取为独立的 `ShareCrawler<S>`。它只依赖 `ShareSource` trait，职责是"给定来源，返回原始文件列表"，不涉及导入逻辑。

```rust
// app/src/application/share_crawler.rs (NEW)
#[derive(Clone)]
pub struct ShareCrawler<S> {
    share_source: S,
}

impl<S: ShareSource> ShareCrawler<S> {
    pub async fn raw_files_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<RawFile>> { ... }
    pub fn raw_files_from_fslink(&self, fslink: &str) -> AppResult<Vec<RawFile>> { ... }
    pub fn raw_files_from_json(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>> { ... }
}
```

`ImportMediaService` 持有 `ShareCrawler<S>` 和 `ImportUseCaseFactory`，对外暴露 `raw_files_from_*` 方法和 `import_with_raw_files` 方法。调用方只需获取一次 raw_files，即可同时用于 index 和 import。

### 2. 单源事件模式

用 `ProcessMediaSources` 事件替代 `IndexFilesFromSource`。每个事件携带一个 `MediaSource`，而非多个 source 的列表。一个消息若有 N 个 source，就发布 N 个事件，每个事件独立处理、独立重试。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaSource {
    ShareUrl(String),
    Fslink(String),
    TgDocument { file_id: String, file_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMediaSources {
    pub source: MediaSource,
    pub description: Option<String>,
    pub channel_post: bool,
    pub reply_to_message_id: Option<i32>,
}

impl Event for ProcessMediaSources {
    const NAME: &'static str = "ProcessMediaSources";
}
```

事件 handler 的统一流程：fetch raw_files（按 source 类型分派） → index → import + 通知。

### 3. Bot handler 简化

Bot handler 不再持有 import 逻辑。`handle_channel_post` 和 `handle_message` 只做三件事：提取 source → 发事件 → （仅 DM）发即时确认。

`MsgProcessor`（`msg.rs`，222 行）被整体删除。BotServices 不再持有 `import`、`file_index_events`、`file_index_ingest_dir` 字段。

### 4. PhantomData 模式

`ShareImportUseCase` 和 `ImportUseCaseFactory` 不再持有 `ShareSource` 实例（由 `ShareCrawler` 持有），但泛型参数 `S` 仍需要保留以维持类型层级。使用 `PhantomData<S>` 占位：

```rust
pub(crate) struct ImportUseCaseFactory<L, S, M, F> {
    library_gateway: L,
    metadata_catalog: M,
    local: F,
    _phantom: std::marker::PhantomData<S>,
}
```

### 5. 生产路径 / 测试路径分离

`import_from_share_url`、`import_from_fslink`、`import_from_json` 这些组合了 BFS + import 的便利方法标记为 `#[cfg(test)]`，只在测试中使用。生产代码一律走 `raw_files_from_*` + `import_with_raw_files` 的拆分路径，确保 BFS 只发生一次。

```rust
impl<L, S: ShareSource, M, F> ImportMediaService<L, S, M, F> {
    #[cfg(test)]
    pub async fn import_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>> {
        let raw_files = self.share_crawler.raw_files_from_share_url(url).await?;
        self.import_use_cases.share_import().import_from_raw_files(raw_files).await
    }

    // 生产路径：拆分为两步
    pub async fn raw_files_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<RawFile>> {
        self.share_crawler.raw_files_from_share_url(url).await
    }
    pub async fn import_with_raw_files(&self, raw_files: Vec<RawFile>) -> AppResult<Vec<ImportedMedia>> {
        self.import_use_cases.share_import().import_from_raw_files(raw_files).await
    }
}
```

### 6. 错误分类

复用 `is_permanent_index_source_error` 对错误分类。瞬态错误（`Dependency`、`Runtime`）返回 `Err` 让 EventBus 自动重试；永久错误（`InvalidParameter`、`NotFound`、`RuleRejected`）记录 warning 并通知用户后 ack 事件。

```rust
match handler.import_service.import_with_raw_files(raw_files).await {
    Ok(imported) => send_import_results(&handler.notify_service, reply_to, &imported).await,
    Err(err) if is_permanent_index_source_error(&err) => {
        send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
    }
    Err(err) => return Err(err),  // transient → EventBus retry
}
```

## Why This Matters

- **消除重复 API 调用** — 同一 share URL 只 BFS 遍历一次，raw_files 在 index 和 import 之间复用。对 rate limit 敏感的云盘 API（Pan123、Pan189、Pan115、Quark）而言，直接将 API 调用量减半。
- **Bot 响应不再阻塞** — handler 只做 source 提取和事件发布（毫秒级），import 在 EventBus worker 中异步执行。用户发送消息后立即收到确认。
- **职责清晰** — `ShareCrawler` 只管遍历，event handler 只管编排（index → import → notify），bot handler 只管消息解析和事件发布。每个组件可独立测试。
- **单源事件简化重试** — 一个 source 一个事件，失败时只重试该 source，不影响同一消息中的其他 source。
- **减少 BotServices 依赖** — bot handler 不再需要 `ImportService`、`EventBus`（for index）、`file_index_ingest_dir`，启动时的依赖注入更简单。

## When to Apply

- 当多个模块对同一外部资源发起重复的昂贵调用（API 请求、大文件下载、复杂计算）时。
- 当同步阻塞操作影响用户交互体验，需要拆分为异步事件驱动时。
- 当一个组件承担了过多职责（消息解析 + 导入执行 + 文件下载 + 通知发送），需要按关注点拆分时。
- 当泛型参数的 trait bound 在运行时通过其他方式满足，需要保留类型参数但不再需要字段时（PhantomData 模式）。

## Examples

### CLI 同样受益：一次 BFS 复用

CLI 的 `import-share-url` 命令也采用相同的模式。Before：index 和 import 各自调用 provider API，两次 BFS。After：一次 BFS，结果传给 index 和 import。

```rust
// Before: 两次独立调用
ingest_service.ingest_sources(vec![FileIndexSource::ShareUrl(url.to_string())], description).await?;
let imported = import_service.import_from_share_url(&share_url).await?;

// After: 一次 BFS + 复用
let raw_files = import_service.raw_files_from_share_url(&share_url).await?;
let seen: Vec<SeenFile> = raw_files.iter().map(SeenFile::from_raw_file).collect();
file_index_service.record_seen_files(seen, description).await?;
let imported = import_service.import_with_raw_files(raw_files).await?;
```

### TgDocument 处理：不再落盘

Before：`download_document_index_source` 将 Telegram 文档下载到 `file_index_ingest_dir` 的临时文件，index handler 再从磁盘读取。After：event handler 直接通过 `bot.download_file()` 获取字节，传给 `raw_files_from_json`，无需中间文件。

```rust
// Before
let local_path = format!("{}/{}-{}-{}", ingest_dir, msg.id.0, timestamp, sanitize(name));
bot.download_file(&file.path, &mut content).await?;
tokio::fs::write(&local_path, content).await?;

// After
bot.download_file(&file.path, &mut content).await?;
handler.import_service.raw_files_from_json(content)
```

## Related

- GitHub issue #52: Fix `AppErrorKind::Internal` classification — 某些被标记为 `Internal` 的错误实际应归类为 `Dependency` 或 `Runtime`，影响 `is_permanent_index_source_error` 的瞬态/永久判断准确性。
- 设计文档：`docs/superpowers/specs/2026-05-09-unified-media-source-event-design.md`
- 关键提交：`bcde00a feat: unify file index and import into async event handler` — net -309 行，17 文件变更。
