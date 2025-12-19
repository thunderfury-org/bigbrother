# BigBrother 异步任务系统设计方案（Event Bus 架构）

## 1. 背景与目标

### 1.1 当前问题

1. **Telegram 消息发送阻塞**
   - 消息处理流程中同步发送 Telegram 消息（见 [bot/msg.rs:182-190](app/src/bot/msg.rs#L182-L190)）
   - 网络故障导致发送失败会中断整个导入流程
   - 分享文件的处理成功与否不应该依赖于消息发送是否成功

2. **Channel 消息处理同步阻塞**
   - Channel 消息接收后立即同步处理（见 [bot/mod.rs:32-78](app/src/bot/mod.rs#L32-L78)）
   - 处理耗时长（导入文件、TMDB 查询等）会阻塞 Bot 响应
   - 处理失败无法自动重试

### 1.2 设计目标

1. **解耦性**：业务代码与任务系统完全解耦
2. **可靠性**：事件持久化，失败自动重试
3. **实时性**：事件驱动，立即响应
4. **简洁性**：API 极简，易于使用
5. **灵活性**：支持闭包 handler，类型安全

## 2. Event Bus 架构

### 2.1 核心组件

```
┌──────────────────────────────────────────────────────────────┐
│                        Event Bus                              │
│                                                                │
│  pub(event, payload) -> Result<()>                            │
│    │                                                           │
│    ├─► 1. 持久化到 DB (event 表)                              │
│    └─► 2. 通知 handler                                        │
│                                                                │
│  sub(event, handler)                                          │
│    ├─► 注册 handler（闭包）                                   │
│    └─► 启动异步循环处理事件                                   │
│           ├─► 等待通知                                        │
│           ├─► 从 DB 加载事件                                  │
│           └─► 调用 handler                                    │
└────────────────────────────────────────────────────────────────┘
        │                          │
        │ pub                      │ sub
        │                          │
┌───────▼─────────┐       ┌────────▼─────────┐
│  Event Source   │       │  Event Handler   │
├─────────────────┤       ├──────────────────┤
│                 │       │                  │
│ • Bot Handler   │       │ • send_message   │
│ • Channel Post  │       │ • process_post   │
│ • Command       │       │ • custom_tasks   │
│                 │       │                  │
└─────────────────┘       └──────────────────┘
                                   │
                                   │ 读取和更新事件
                                   ▼
                          ┌──────────────────┐
                          │   Event Table    │
                          │   (持久化队列)   │
                          └──────────────────┘
```

### 2.2 工作流程

#### 场景 1：发送消息任务

```
1. Bot Handler 处理分享
   │
   ├─► 导入文件到库 (业务逻辑)
   │
   └─► event_bus.pub("send_message", SendMessagePayload {
           chat_id: ...,
           text: "导入成功"
       }).await?
       │
       ├─► EventBus 内部:
       │   ├─► 1. 持久化到 event 表 (ack = false)
       │   └─► 2. 通知对应的 handler
       │
       └─► Handler 接收通知:
           ├─► 从 DB 加载 event
           ├─► 调用闭包: handler(payload).await
           │   ├─► 成功: 更新 ack = true
           │   └─► 失败: 记录错误，等待重试
           └─► 继续监听下一个事件
```

#### 场景 2：Channel 消息处理

```
1. 收到 Channel Post
   │
   └─► 检查关键词匹配
       │
       └─► event_bus.pub("process_channel_post", ProcessChannelPostPayload {
               channel_id: ...,
               message_id: ...,
               message: serde_json::to_value(&msg)
           }).await?
           │
           ├─► EventBus 内部:
           │   ├─► 1. 持久化到 event 表
           │   └─► 2. 通知对应的 handler
           │
           └─► Handler 执行:
               ├─► 下载文件
               ├─► 导入到库
               └─► 发送结果消息（再次调用 pub("send_message", ...)）
```

#### 场景 3：失败重试

```
1. EventBus 定期轮询 (每 10 秒)
   │
   └─► 查询 ack = false AND update_time < now - retry_delay
       │
       └─► 批量通知对应的 handlers
           └─► Handler 重新处理事件
```

## 3. 数据库设计

### 3.1 事件表结构

```sql
CREATE TABLE event (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- 事件基本信息
    event TEXT NOT NULL,                  -- 事件类型（如 "send_message"）
    payload TEXT NOT NULL,                -- JSON 格式的事件数据
    ack BOOLEAN NOT NULL DEFAULT 0,       -- 是否已确认（成功处理）

    -- 时间戳
    create_time INTEGER NOT NULL,         -- 创建时间
    update_time INTEGER NOT NULL          -- 更新时间（用于重试判断）
);

CREATE INDEX idx_event_ack
    ON event(event, ack, update_time)
    WHERE ack = 0;
```

### 3.2 与 task.md 设计的对应

task.md 中定义的 SQL 与实际需求的对应：
- `event`: 对应任务类型
- `payload`: JSON 格式的任务数据
- `ack`: 替代 `status`，简化为布尔值（false = 待处理/失败，true = 成功）
- `create_time`, `update_time`: 时间戳（使用 Unix timestamp）

## 4. 核心实现

### 4.1 目录结构

```
app/src/
├── event/                          # 新增：事件系统
│   ├── mod.rs                      # Event Bus 实现
│   └── types.rs                    # Payload 类型定义
├── bot/
│   ├── mod.rs                      # 修改：调用 event_bus.pub()
│   └── msg.rs                      # 修改：调用 event_bus.pub("send_message", ...)
└── state.rs                        # 修改：添加 EventBus
```

### 4.2 Event Bus 实现（event/mod.rs）

```rust
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use sea_orm::*;
use serde::{Serialize, Deserialize};
use crate::entity::event;

type HandlerFn = Arc<dyn Fn(serde_json::Value) -> BoxFuture + Send + Sync>;
type BoxFuture = std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

/// Event Bus 核心实现
pub struct EventBus {
    db: DatabaseConnection,
    handlers: Arc<RwLock<HashMap<String, HandlerFn>>>,
    notifiers: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<i32>>>>,
}

impl EventBus {
    /// 创建新的 Event Bus
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            notifiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发布事件
    pub async fn pub<T: Serialize>(
        &self,
        event_name: &str,
        payload: T,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        let payload_json = serde_json::to_string(&payload)?;

        // 1. 持久化到数据库
        let new_event = event::ActiveModel {
            event: Set(event_name.to_string()),
            payload: Set(payload_json),
            ack: Set(false),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };

        let result = new_event.insert(&self.db).await?;
        let event_id = result.id;

        tracing::info!(event_id, event = event_name, "Event published");

        // 2. 通知 handler
        let notifiers = self.notifiers.read().await;
        if let Some(tx) = notifiers.get(event_name) {
            let _ = tx.send(event_id);
        }

        Ok(())
    }

    /// 订阅事件
    pub fn sub<T, Func, Fut>(&self, event_name: &str, handler: Func)
    where
        T: for<'de> Deserialize<'de> + Send + 'static,
        Func: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let event_name = event_name.to_string();

        // 创建通知 channel
        let (tx, mut rx) = mpsc::unbounded_channel::<i32>();

        // 存储 notifier
        let notifiers = self.notifiers.clone();
        tokio::spawn(async move {
            notifiers.write().await.insert(event_name.clone(), tx);
        });

        // 包装 handler 为类型擦除的函数
        let wrapped_handler: HandlerFn = Arc::new(move |value| {
            Box::pin(async move {
                let payload: T = serde_json::from_value(value)?;
                handler(payload).await
            })
        });

        // 存储 handler
        let handlers = self.handlers.clone();
        let event_name_clone = event_name.clone();
        tokio::spawn(async move {
            handlers.write().await.insert(event_name_clone, wrapped_handler);
        });

        // 启动事件处理循环
        let db = self.db.clone();
        let handlers = self.handlers.clone();
        let event_name_for_loop = event_name.clone();

        tokio::spawn(async move {
            loop {
                // 等待通知
                let event_id = match rx.recv().await {
                    Some(id) => id,
                    None => break,
                };

                // 加载事件
                let evt = match event::Entity::find_by_id(event_id)
                    .one(&db)
                    .await
                {
                    Ok(Some(e)) if e.event == event_name_for_loop && !e.ack => e,
                    Ok(Some(_)) => continue, // 已处理或类型不匹配
                    Ok(None) => {
                        tracing::warn!(event_id, "Event not found");
                        continue;
                    }
                    Err(e) => {
                        tracing::error!(event_id, "Failed to load event: {}", e);
                        continue;
                    }
                };

                // 获取 handler
                let handler = {
                    let handlers_read = handlers.read().await;
                    handlers_read.get(&event_name_for_loop).cloned()
                };

                let handler = match handler {
                    Some(h) => h,
                    None => {
                        tracing::error!(event = %event_name_for_loop, "No handler registered");
                        continue;
                    }
                };

                // 解析 payload
                let payload: serde_json::Value = match serde_json::from_str(&evt.payload) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!(event_id, "Failed to parse payload: {}", e);
                        continue;
                    }
                };

                // 执行 handler
                tracing::info!(event_id, event = %evt.event, "Processing event");

                match handler(payload).await {
                    Ok(_) => {
                        // 成功：标记为已确认
                        let mut active: event::ActiveModel = evt.into();
                        active.ack = Set(true);
                        active.update_time = Set(chrono::Utc::now().timestamp());

                        if let Err(e) = active.update(&db).await {
                            tracing::error!(event_id, "Failed to update event: {}", e);
                        } else {
                            tracing::info!(event_id, "Event completed successfully");
                        }
                    }
                    Err(e) => {
                        // 失败：更新时间戳（用于重试判断）
                        let mut active: event::ActiveModel = evt.into();
                        active.update_time = Set(chrono::Utc::now().timestamp());

                        if let Err(update_err) = active.update(&db).await {
                            tracing::error!(event_id, "Failed to update event: {}", update_err);
                        }

                        tracing::warn!(event_id, "Event processing failed: {}", e);
                    }
                }
            }
        });

        tracing::info!(event = event_name, "Subscribed to event");
    }

    /// 启动定期轮询（处理重试）
    pub fn start_polling(self: Arc<Self>, interval_secs: u64, retry_delay_secs: i64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(interval_secs)
            );

            loop {
                interval.tick().await;

                if let Err(e) = self.poll_pending_events(retry_delay_secs).await {
                    tracing::error!("Error polling pending events: {}", e);
                }
            }
        });
    }

    async fn poll_pending_events(&self, retry_delay_secs: i64) -> anyhow::Result<()> {
        let retry_threshold = chrono::Utc::now().timestamp() - retry_delay_secs;

        let events = event::Entity::find()
            .filter(
                event::Column::Ack
                    .eq(false)
                    .and(event::Column::UpdateTime.lt(retry_threshold)),
            )
            .limit(20)
            .all(&self.db)
            .await?;

        let notifiers = self.notifiers.read().await;

        for evt in events {
            if let Some(tx) = notifiers.get(&evt.event) {
                let _ = tx.send(evt.id);
                tracing::info!(event_id = evt.id, event = %evt.event, "Retrying event");
            }
        }

        Ok(())
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            handlers: self.handlers.clone(),
            notifiers: self.notifiers.clone(),
        }
    }
}
```

### 4.3 Payload 类型定义（event/types.rs）

```rust
use serde::{Deserialize, Serialize};

/// 发送消息 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessagePayload {
    pub chat_id: i64,
    pub text: String,
    pub reply_to_message_id: Option<i32>,
}

/// Channel 消息处理 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessChannelPostPayload {
    pub channel_id: i64,
    pub message_id: i32,
    pub message: serde_json::Value,
}
```

## 5. 系统集成

### 5.1 修改 AppState（state.rs）

```rust
use std::sync::Arc;
use crate::event::EventBus;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<config::Manager>,
    pub pan123: Arc<pan123::Client>,
    pub pan189: Arc<pan189::Client>,
    pub tmdb: Arc<tmdb::Client>,
    pub event_bus: Arc<EventBus>,  // 新增
}

impl AppState {
    pub async fn new(data_dir: &str) -> AppResult<Self> {
        // ... 原有初始化代码 ...

        // 创建 Event Bus
        let event_bus = Arc::new(EventBus::new(db.clone()));

        let state = AppState {
            db,
            pan123: Arc::new(pan123::Client::new(/* ... */)),
            pan189: Arc::new(pan189::Client::new()),
            tmdb: Arc::new(tmdb::Client::new(/* ... */)),
            config: Arc::new(config),
            event_bus,
        };

        Ok(state)
    }
}
```

### 5.2 修改 Bot 启动（bot/mod.rs）

```rust
use std::sync::Arc;
use crate::event::types::{SendMessagePayload, ProcessChannelPostPayload};

pub async fn run(state: AppState) -> anyhow::Result<()> {
    let bot = Bot::new(state.config.get_telegram_config().bot_token.as_str());

    // 订阅 send_message 事件
    let bot_clone = bot.clone();
    state.event_bus.sub("send_message", move |payload: SendMessagePayload| {
        let bot = bot_clone.clone();
        async move {
            let mut request = bot.send_message(
                ChatId(payload.chat_id),
                payload.text,
            );

            if let Some(reply_to) = payload.reply_to_message_id {
                request = request.reply_to_message_id(MessageId(reply_to));
            }

            request.await?;
            Ok(())
        }
    });

    // 订阅 process_channel_post 事件
    let state_clone = Arc::new(state.clone());
    let bot_clone = bot.clone();
    state.event_bus.sub("process_channel_post", move |payload: ProcessChannelPostPayload| {
        let state = state_clone.clone();
        let bot = bot_clone.clone();
        async move {
            // 反序列化 Telegram Message
            let message: teloxide::types::Message =
                serde_json::from_value(payload.message)?;

            // 复用现有的 MsgProcessor 逻辑
            let processor = msg::MsgProcessor {
                state: &state,
                bot: &bot,
                msg: &message,
                from_monitor: true,
            };

            processor.process().await?;
            Ok(())
        }
    });

    // 启动定期轮询（处理重试任务）
    // 参数：interval_secs=10（每10秒轮询一次），retry_delay_secs=60（失败后60秒重试）
    state.event_bus.clone().start_polling(10, 60);

    // 启动 Bot
    cmd::create_commands_in_background(&bot);

    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
        .branch(
            Update::filter_message()
                .filter_command::<cmd::Command>()
                .endpoint(cmd::handle_command),
        )
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![state])
        .build()
        .dispatch()
        .await;

    Ok(())
}
```

### 5.3 修改消息处理（bot/msg.rs）

```rust
use crate::event::types::SendMessagePayload;

impl MsgProcessor<'_> {
    async fn send_message<T: Into<String>>(&self, text: T) -> ResponseResult<Message> {
        let text = text.into();
        let chat_id = self.get_chat_id().0;
        let reply_to = if self.msg.from.is_some() {
            Some(self.msg.id.0)
        } else {
            None
        };

        // 发布事件（非阻塞）
        let _ = self.state.event_bus.pub(
            "send_message",
            SendMessagePayload {
                chat_id,
                text,
                reply_to_message_id: reply_to,
            },
        ).await;

        // 返回 Ok（调用方不再依赖返回值）
        Ok(/* dummy */)
    }
}
```

### 5.4 修改 Channel 处理（bot/mod.rs）

```rust
use crate::event::types::ProcessChannelPostPayload;

async fn handle_channel_post(state: AppState, _bot: Bot, msg: Message) -> ResponseResult<()> {
    let keywords = match keyword::list_all_keywords(&state.db).await {
        Ok(keywords) => keywords,
        Err(e) => {
            tracing::error!("Failed to query keywords: {}", e);
            return Ok(());
        }
    };

    if keywords.is_empty() {
        return Ok(());
    }

    let filters: Vec<String> = keywords.into_iter().map(|k| k.value).collect();
    let text = msg.text().or(msg.caption()).unwrap_or_default();

    // 检查是否匹配关键词
    let mut matched = false;
    for keyword in &filters {
        if text.contains(keyword) {
            matched = true;
            break;
        }
    }

    if matched {
        // 序列化消息并发布事件
        let message_json = serde_json::to_value(&msg)?;

        let _ = state.event_bus.pub(
            "process_channel_post",
            ProcessChannelPostPayload {
                channel_id: msg.chat.id.0,
                message_id: msg.id.0,
                message: message_json,
            },
        ).await;
    }

    Ok(())
}
```

## 6. 数据库迁移

### 创建 migration

```bash
cd migration
cargo run -- generate create_event_table
```

### 编写 migration 代码（migration/src/mXXXXXXXXXX_create_event_table.rs）

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Event::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Event::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Event::Event).string().not_null())
                    .col(ColumnDef::new(Event::Payload).text().not_null())
                    .col(ColumnDef::new(Event::Ack).boolean().not_null().default(false))
                    .col(ColumnDef::new(Event::CreateTime).big_integer().not_null())
                    .col(ColumnDef::new(Event::UpdateTime).big_integer().not_null())
                    .to_owned(),
            )
            .await?;

        // 创建索引
        manager
            .create_index(
                Index::create()
                    .name("idx_event_ack")
                    .table(Event::Table)
                    .col(Event::Event)
                    .col(Event::Ack)
                    .col(Event::UpdateTime)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Event::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Event {
    Table,
    Id,
    Event,
    Payload,
    Ack,
    CreateTime,
    UpdateTime,
}
```

### 应用 migration

```bash
cd migration
cargo run
```

### 生成 Entity

```bash
./tools/generate_entity.sh
```

## 7. 实施计划

### Phase 1: Event Bus 基础框架（1.5 天）
- [ ] 实现 EventBus (pub/sub/start_polling 方法)
- [ ] 实现类型擦除的 handler 包装
- [ ] 实现通知机制（mpsc channel）
- [ ] 集成到 AppState
- [ ] 数据库 migration

### Phase 2: 消息发送异步化（0.5 天）
- [ ] 修改 bot/msg.rs 调用 event_bus.pub()
- [ ] 在 bot/mod.rs 中订阅 send_message 事件
- [ ] 测试消息发送失败重试
- [ ] 验证消息发送失败不影响导入

### Phase 3: Channel 消息异步化（0.5 天）
- [ ] 修改 handle_channel_post 调用 event_bus.pub()
- [ ] 在 bot/mod.rs 中订阅 process_channel_post 事件
- [ ] 测试 Channel 消息处理

### Phase 4: 测试和优化（0.5 天）
- [ ] 压力测试
- [ ] 错误处理优化
- [ ] 日志完善

**总计：约 3 天**

## 8. 监控与运维

### 8.1 日志监控

```bash
# 查看事件发布日志
tail -f data/log/bigbrother.*.log | grep "Event published"

# 查看事件处理日志
tail -f data/log/bigbrother.*.log | grep "Processing event"

# 查看重试日志
tail -f data/log/bigbrother.*.log | grep "Retrying event"

# 查看失败日志
tail -f data/log/bigbrother.*.log | grep "processing failed"
```

### 8.2 数据库查询

```sql
-- 查看待处理事件
SELECT * FROM event WHERE ack = 0 ORDER BY create_time;

-- 查看失败事件（长时间未确认）
SELECT * FROM event
WHERE ack = 0
AND update_time < unixepoch() - 3600  -- 1小时前
ORDER BY update_time;

-- 查看事件统计
SELECT
    event,
    COUNT(*) as total,
    SUM(CASE WHEN ack = 1 THEN 1 ELSE 0 END) as completed,
    SUM(CASE WHEN ack = 0 THEN 1 ELSE 0 END) as pending
FROM event
GROUP BY event;

-- 清理已完成事件（保留最近7天）
DELETE FROM event
WHERE ack = 1
AND create_time < unixepoch() - 7 * 24 * 3600;
```

## 9. 核心特性

### 9.1 API 设计

完全符合 task.md 中的设计：

```rust
// 发布事件
event_bus.pub("send_message", payload).await?;

// 订阅事件（闭包 handler）
event_bus.sub("send_message", |payload: SendMessagePayload| async move {
    // 处理逻辑
    Ok(())
});

// 启动轮询
event_bus.start_polling(interval_secs, retry_delay_secs);
```

### 9.2 核心优势

1. ✅ **闭包 handler**：直接使用闭包，无需定义 trait
2. ✅ **类型安全**：泛型约束确保 payload 类型正确
3. ✅ **自动重试**：基于 update_time 的重试机制
4. ✅ **简化状态**：使用 ack 布尔值替代复杂的状态机
5. ✅ **事件驱动**：通过 mpsc channel 实时通知
6. ✅ **持久化保证**：先持久化再通知，不会丢失事件

### 9.3 关键技术点

1. **类型擦除**：使用 `BoxFuture` 和 trait object 实现泛型 handler 存储
2. **通知机制**：每个事件类型一个 mpsc channel
3. **重试策略**：基于 `update_time` 判断是否需要重试
4. **并发安全**：使用 `RwLock` 保护共享状态

## 10. 总结

本设计完全遵循 task.md 中提出的接口规范：

| 需求 | 实现 |
|------|------|
| `pub(event, payload)` | ✅ 先持久化再通知 |
| `sub(event, handler)` | ✅ 支持闭包，启动异步循环 |
| 先持久化再通知 | ✅ pub 方法中实现 |
| 事件表结构 | ✅ event, payload, ack, create_time, update_time |
| 自动重试 | ✅ 基于 update_time 轮询 |

**推荐**：这个设计在保持极简 API 的同时，提供了灵活的闭包 handler 支持，非常适合 BigBrother 项目。
