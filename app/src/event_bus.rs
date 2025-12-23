use std::{collections::HashMap, future::Future, sync::Arc};

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{
    RwLock,
    mpsc::{self, Receiver, Sender},
};

use crate::{
    entity::model::event,
    error::{AppError, AppResult},
};

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
    pub async fn subscribe<T, Func, Fut>(&self, event: &str, handler: Func) -> AppResult<()>
    where
        T: DeserializeOwned,
        Func: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let (tx, mut rx) = self.create_channel(event).await?;

        tokio::spawn(async move {
            tx.try_send(());

            loop {
                if let None = rx.recv().await {
                    // Channel closed
                    break;
                }

                // check if event bus closed
            }
        });

        tracing::info!("Subscribed to event '{}'", event);
        Ok(())
    }

    async fn create_channel(&self, event: &str) -> AppResult<(Sender<()>, Receiver<()>)> {
        // check if already subscribed
        {
            let notifiers = self.notifiers.read().await;
            if notifiers.contains_key(event) {
                return Err(AppError::InvalidParameter(format!(
                    "Event '{}' is already subscribed",
                    event
                )));
            }
        }

        let mut notifiers = self.notifiers.write().await;
        if notifiers.contains_key(event) {
            return Err(AppError::InvalidParameter(format!(
                "Event '{}' is already subscribed",
                event
            )));
        }

        let (tx, rx) = mpsc::channel(1);
        notifiers.insert(event.to_owned(), tx.clone());
        Ok((tx, rx))
    }
}
