use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use sea_orm::*;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use crate::entity::model::event;

pub mod types;

type BoxFuture = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
type HandlerFn = Arc<dyn Fn(serde_json::Value) -> BoxFuture + Send + Sync>;

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
    pub async fn publish<T: Serialize>(
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
            let _ = tx.send(event_id as i32);
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

        // 将 handler 包装在 Arc 中以便在闭包间共享
        let handler = Arc::new(handler);

        // 包装 handler 为类型擦除的函数
        let wrapped_handler: HandlerFn = {
            let handler = handler.clone();
            Arc::new(move |value| {
                let handler = handler.clone();
                Box::pin(async move {
                    let payload: T = serde_json::from_value(value)?;
                    handler(payload).await
                })
            })
        };

        // 存储 notifier 和 handler (在当前线程)
        let notifiers = self.notifiers.clone();
        let handlers = self.handlers.clone();
        let event_clone1 = event_name.clone();
        let event_clone2 = event_name.clone();
        let tx_clone = tx.clone();
        let handler_clone = wrapped_handler.clone();

        // 使用 spawn_blocking 来避免类型推断问题
        tokio::spawn(async move {
            drop(tx); // drop 原始的 tx
            notifiers.write().await.insert(event_clone1, tx_clone);
            handlers.write().await.insert(event_clone2, handler_clone);
        });

        // 启动事件处理循环
        let db = self.db.clone();
        let handlers = self.handlers.clone();
        let event_name_for_loop = event_name.clone();

        tokio::spawn(async move {
            // 等待一下确保 handler 已注册
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            loop {
                // 等待通知
                let event_id = match rx.recv().await {
                    Some(id) => id as i64,
                    None => break,
                };

                // 加载事件
                let evt = match event::Entity::find_by_id(event_id).one(&db).await {
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
                    let h = handlers.read().await;
                    h.get(&event_name_for_loop).cloned()
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
                        // 失败：更新时间戳
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
