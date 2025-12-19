当前的实现中在处理 telegram 消息中会发送 telegram 消息
如果发送失败（由于网络原因，出现的可能性较大）会导致消息处理失败
希望将发送消息改成持久化的异步任务，发送失败自动重试，发送消息是否成功不能影响对分享的处理

下一步希望将 channel 的消息处理也改成异步的，收到 channel 消息后持久化，之后异步处理

通过 event bus 来实现，先持久化再通过信号通知 handler

```rust
pub struct EventBus {
    db: DatabaseConnection,
    handlers: Arc<RwLock<HashMap<String, Arc<dyn TaskHandler>>>>,
}

impl EventBus {
    /// 创建新的 Event Bus
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发布任务（先持久化，再异步执行）
    pub async fn pub<T: serde::Serialize>( &self, event: &str, payload: T,) -> Result<()> {
      // 1. save to db
      // 2. notify handler
    }

    pub fn sub(&self, event: &str, handler: Func)
    where
    Func: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Output> + Send + 'static,
    T: serde::Deserialize,
    {
      // add to handlers
      // start async loop to handle event
      //   in loop:
      //     wait for notify
      //     load event from db
      //     call handler
    }
}
```

```sql
CREATE TABLE event (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event varchar NOT NULL,
    payload TEXT NOT NULL,                -- JSON 格式的任务数据
    ack boolean NOT NULL,                 -- pending, processing, success, failed
    create_time timestamp NOT NULL,
    update_time timestamp NOT NULL
);
```
