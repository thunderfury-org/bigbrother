use std::{collections::HashMap, future::Future, sync::Arc};

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{RwLock, mpsc},
    time,
};

use crate::{entity::model::event, error::AppResult};

/// Event Bus 核心实现
pub struct EventBus {
    db: DatabaseConnection,
    notifiers: Arc<RwLock<HashMap<String, mpsc::Sender<()>>>>,
}

impl EventBus {
    /// 创建新的 Event Bus
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            notifiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发布事件
    pub async fn publish<T: Serialize>(&self, event: &str, payload: T) -> AppResult<()> {
        let now = Utc::now();
        let payload_json = serde_json::to_string(&payload)?;

        let new_event = event::ActiveModel {
            event: Set(event.to_owned()),
            payload: Set(payload_json),
            ack: Set(false),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        new_event.insert(&self.db).await?;

        let notifiers = self.notifiers.read().await;
        if let Some(tx) = notifiers.get(event) {
            // 发送通知，不阻塞
            let _ = tx.try_send(());
        }

        Ok(())
    }

    /// 订阅事件
    pub async fn subscribe<T, Func, Fut>(&self, event: &str, handler: Func)
    where
        T: for<'de> Deserialize<'de> + Send + 'static,
        Func: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        rx.close();

        // 存储 notifier 和 handler (在当前线程)
        let notifiers = self.notifiers.clone();
        let tx_clone = tx.clone();

        // 启动事件处理循环
        let db = self.db.clone();

        tokio::spawn(async move {
            rx.close();
            loop {
                match rx.recv().await {
                    Some(()) => (),
                    None => break,
                };

                let events = match event::Entity::find()
                    .filter(event::Column::Event.eq(event))
                    .filter(event::Column::Ack.eq(false))
                    .order_by_asc(event::Column::Id)
                    .limit(10)
                    .all(&db)
                    .await
                {
                    Ok(evts) => evts,
                    Err(e) => {
                        tracing::error!("Failed to fetch event '{}' records, {}, will retry later", event, e);
                        time::sleep(time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if events.is_empty() {
                    continue;
                }

                for e in events {
                    // 解析 payload
                    let payload = match serde_json::from_str::<T>(&e.payload) {
                        Ok(p) => p,
                        Err(err) => {
                            tracing::error!("Failed to parse payload '{}', {}", e.payload, err);
                            // need delete
                            continue;
                        }
                    };

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
            }
        });

        tracing::info!("Subscribed to event '{}'", event);
    }
}
