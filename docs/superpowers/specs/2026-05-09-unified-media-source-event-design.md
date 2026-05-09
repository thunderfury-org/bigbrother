# 统一媒体源事件：合并 File Index 与 Import

## 问题

收到 TG 消息后，file index 和 import 各自独立查询同一 share URL 的文件列表：
- `publish_file_index_event()` → EventBus → handler 调用 provider API 获取文件列表 → 记录索引
- `processor.process()` → **同步阻塞** → 再次调用 provider API 获取文件列表 → 执行导入

导致：**重复 API 调用（rate limit 压力 + 处理耗时增加）** + Bot 阻塞于 import。

## 方案

用 `ProcessMediaSources` 事件替代 `IndexFilesFromSource` 事件 + 同步 import 调用。每个 source 独立一个事件，handler 中查询一次文件列表，同时完成 index 和 import。

关键词匹配逻辑从 bot handler 移入事件 handler，bot handler 只负责提 source 和发事件。

## 事件结构

```rust
pub struct ProcessMediaSources {
    pub source: MediaSource,             // 单个 source
    pub description: Option<String>,     // 消息文本（用于 file index 记录 + 关键词匹配）
    pub channel_post: bool,              // 是否来自频道消息（决定关键词匹配逻辑）
    pub reply_to_message_id: Option<i32>,// 回复的消息 ID（DM 用，频道为 None）
}

impl Event for ProcessMediaSources {
    const NAME: &'static str = "ProcessMediaSources";
}

pub enum MediaSource {
    ShareUrl(String),
    Fslink(String),
    TgDocument { file_id: String, file_name: String },
}
```

`MediaSource` 定义在 `interface/telegram/`，区别于 `FileIndexSource`（后者用于 CLI 等内部场景）：
- `TgDocument` 携带 Telegram file_id，handler 下载字节后直接使用，无需中间临时文件

一个消息有 N 个 source → 发布 N 个事件，每个事件独立处理、独立重试。

## Bot Handler 流程

### handle_channel_post（频道消息）

1. 提取 `MediaSource` 列表
2. 无 source → 返回
3. 对每个 source：发布 `ProcessMediaSources { channel_post: true, reply_to_message_id: None }`
4. 立即返回

关键词匹配不在 bot handler 做，移入事件 handler。

### handle_message（用户私聊）

1. 提取 `MediaSource` 列表
2. 无 source → 返回
3. 对每个 source：即时回复确认（"开始处理分享: url"）
4. 发布 `ProcessMediaSources { channel_post: false, reply_to_message_id: Some(msg.id) }`
5. 立即返回

### Source 提取

复用现有 `extract_index_sources(msg)` 的逻辑，但改为构建 `MediaSource` 列表：
- Share URL → `MediaSource::ShareUrl`
- Fslink → `MediaSource::Fslink`
- JSON/CAS 文档 → `MediaSource::TgDocument { file_id, file_name }`（不再预下载到磁盘）

## 统一事件 Handler

### 结构

```rust
struct ProcessMediaSourcesHandler {
    ingest_service: FileIndexIngestRuntimeService,
    import_service: ImportService,
    notify_service: NotifyService,
    keyword_service: KeywordService,
    bot: teloxide::Bot,
    ingest_dir: String,
}
```

### 处理流程

单个 source 顺序处理（一个事件 = 一个 source）。

**通用步骤（所有 source 类型）：**
1. 获取 raw_files（按 source 类型不同）
2. Index: `ingest_service.record_seen_files(raw_files → seen_files, description)`
3. 决定是否 import:
   - `channel_post == true` → 查询关键词 → 匹配 description → 有匹配才 import
   - `channel_post == false`（DM）→ 直接 import
4. if import: 执行导入 → 发送结果通知
5. if 不 import: 只完成 index，不发通知

**ShareUrl:**
1. `import_service.raw_files_from_share_url(&url)` → `Vec<RawFile>`（一次 BFS 查询）
2. Index（通用步骤 2）
3. Import 决定（通用步骤 3）
4. if import: `import_service.import_with_raw_files(&url, raw_files)` → 结果通知

**Fslink:**
1. Index: `ingest_service` 处理（raw_files_from_fslink，纯解析无 API 调用）
2. Import 决定（通用步骤 3）
3. if import: `import_service.import_from_fslink(&fslink)` → 结果通知

**TgDocument:**
1. `bot.download_file()` 获取字节
2. Index: `ingest_service` 通过 raw_files_from_json_bytes 处理
3. Import 决定（通用步骤 3）
4. if import: `import_service.import_from_json(bytes)` → 结果通知

### 错误处理

复用现有 `AppError` + `is_permanent_index_source_error` 分类：

| 错误类型 | AppError 变体 | 处理 |
|---------|-------------|------|
| 瞬态（网络超时、连接失败） | Dependency, Runtime, Internal | 返回 Err → EventBus 自动重试 |
| 永久（无效参数、不存在、规则拒绝） | InvalidParameter, NotFound, RuleRejected | log warning，通知用户错误，ack 事件 |

各步骤的错误处理：
- **获取 raw_files 失败**：按上表分类处理
- **Index 失败（永久）**：log warning，不阻塞 import
- **Import 失败**：通过 notify_service 发送错误消息
- **TgDocument 下载失败**：网络错误 → 重试（返回 Err）；其他 → 跳过（log error + notify）

### 通知行为

| 场景 | 即时确认 | 结果通知 | 错误通知 |
|------|---------|---------|---------|
| DM (channel_post=false) | Bot handler 发 | Handler 发（reply_to DM 消息） | Handler 发（reply_to DM 消息） |
| Channel post (channel_post=true) | 不发 | Handler 发（发到用户 DM，不 reply） | Handler 发（发到用户 DM，不 reply） |

所有通知都发到用户 DM（由 TelegramDeliveryContext.user_id 决定，handler 不需要关心 chat_id）。

## ImportService 新增方法

> `ImportService` 是 `ImportMediaService` 在 `bootstrap/services.rs` 中的类型别名。下文两个名字混用，指同一个类型。

为避免 ShareUrl 的 BFS 查询重复：

```rust
// ImportMediaService（即 ImportService）新增
pub async fn import_with_raw_files(
    &self,
    url: &ShareUrl<'_>,
    raw_files: Vec<RawFile>,
) -> AppResult<Vec<ImportedMedia>> {
    let mut use_case = self.import_use_cases.share_import();
    use_case.import_from_raw_files(raw_files).await
}

// ShareImportUseCase 新增
// 注意：伪代码，实际方法调用链需要参考现有 collect_media_files / import_from_share_url 实现
pub async fn import_from_raw_files(
    &mut self,
    raw_files: Vec<RawFile>,
) -> AppResult<Vec<ImportedMedia>> {
    let media_files = self.metadata_lookup_mut().build_media_files(raw_files);
    // TODO: 实现时确认 transfer 调用路径（self.transfer_mut().transfer_media_files 或等效方法）
    todo!()
}
```

`collect_media_files` 和 `raw_files_from_share_url` 保留，供 CLI 和其他场景使用。

## Bootstrap 接线

```rust
pub struct EventDeliveryRuntime {
    pub event_bus: EventBus,
    pub telegram_delivery: TelegramDeliveryContext,
    pub media_handler: ProcessMediaSourcesHandler,  // 替换 file_index_ingest
}

impl EventDeliveryRuntime {
    async fn run(self) -> AppResult<()> {
        self.event_bus
            .subscribe(self.telegram_delivery, on_send_telegram_message)
            .await?;
        self.event_bus
            .subscribe(self.media_handler, on_process_media_sources)
            .await?;
        // ...
    }
}
```

ImportService 在 bootstrap 中只需构建一次（clone 给 bot_runtime 和 media_handler）。`KeywordService` 已在 bootstrap 中为 `BotRuntime` 构建，同样 clone 给 handler 即可。

## CLI 优化

`run_import_share_url` 改为一次查询：

```rust
let raw_files = import_service.raw_files_from_share_url(&share_url).await?;
// index: 用 raw_files
let seen: Vec<SeenFile> = raw_files.iter().map(SeenFile::from_raw_file).collect();
ingest_service.record_seen_files(seen, description).await?;
// import: 复用 raw_files
let imported = import_service.import_with_raw_files(&share_url, raw_files).await?;
```

## 删除

- `IndexFilesFromSource` 事件类型
- `publish_file_index_event()` 函数
- `download_document_index_source()` 函数（下载逻辑移入 handler）
- `extract_index_sources()` → 改为返回 `Vec<MediaSource>`，重命名为 `extract_media_sources()`
- `MsgProcessor` 整体删除（`interface/telegram/msg.rs`）
  - `handle_share_url()` / `handle_fslink()` / `handle_document()` → import 逻辑移入 handler
  - `send_message()` → handler 直接用 notify_service
  - `extract_urls_from_msg()` / `extract_urls_from_text()` → 移入 `file_index.rs`（在删除 msg.rs 前迁移）
- `EventDeliveryRuntime.file_index_ingest` 字段
- `BotServices.file_index_events` 字段（bot 不再直接持有 event bus 用于 index 事件）
- `BotServices.file_index_ingest_dir` 字段（移入 handler）

## 影响范围

| 文件 | 变化 |
|------|------|
| `interface/telegram/mod.rs` | 简化 handle_channel_post/handle_message：只做 source 提取 + 发事件，删除 publish_file_index_event |
| `interface/telegram/file_index.rs` | 重写：MediaSource 定义、extract_media_sources、ProcessMediaSources 事件，从 msg.rs 迁入 extract_urls_from_msg/extract_urls_from_text |
| `interface/telegram/msg.rs` | 删除 |
| `application/import_media.rs` | 新增 import_with_raw_files |
| `application/import/share.rs` | 新增 import_from_raw_files |
| `application/file_index.rs` | is_permanent_index_source_error 复用不变 |
| `bootstrap/mod.rs` | 新增 ProcessMediaSourcesHandler，替换 file_index_ingest 订阅 |
| `bootstrap/services.rs` | 新增 handler 类型别名和构建函数 |
| `main.rs` | CLI import-share-url 优化 |
