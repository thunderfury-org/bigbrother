use chrono::Utc;
use dashmap::DashMap;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc::{self, UnboundedSender};

pub mod entity;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to serialize message: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("failed to send notification")]
    Notification,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct EventBus {
    db: DatabaseConnection,
    subscribers: Arc<DashMap<String, UnboundedSender<String>>>,
}

impl EventBus {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            subscribers: Arc::new(DashMap::new()),
        }
    }

    pub async fn publish<T: Serialize + Send + Sync>(
        &self,
        topic: String,
        message: T,
    ) -> Result<()> {
        let message_json = serde_json::to_string(&message)?;
        let message_model = entity::ActiveModel {
            topic: Set(topic.clone()),
            message: Set(message_json.clone()),
            created_at: Set(Utc::now()),
            ..Default::default()
        };

        message_model.insert(&self.db).await?;

        if let Some(subscriber) = self.subscribers.get(&topic) {
            subscriber
                .send(message_json)
                .map_err(|_| Error::Notification)?;
        }

        Ok(())
    }

    pub fn subscribe<H, M>(&self, topic: String, handler: H)
    where
        H: Fn(M) -> Result<()> + Send + Sync + 'static,
        M: DeserializeOwned + Send,
    {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        let handler = Arc::new(handler);

        tokio::spawn(async move {
            while let Some(message_json) = rx.recv().await {
                let message: M = match serde_json::from_str(&message_json) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to deserialize message: {}", e);
                        continue;
                    }
                };
                let handler = handler.clone();
                if let Err(e) = handler(message) {
                    tracing::error!("Failed to handle message: {}", e);
                }
            }
        });

        self.subscribers.insert(topic, tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::{Migrator, MigratorTrait};
    use sea_orm::{ColumnTrait, Database, DbErr, EntityTrait, QueryFilter};
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;

    async fn setup_in_memory_db() -> std::result::Result<DatabaseConnection, DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await.unwrap();
        Ok(db)
    }

    #[tokio::test]
    async fn test_publish_message_persistence() {
        let db = setup_in_memory_db().await.unwrap();
        let event_bus = EventBus::new(db.clone());

        let topic = "test_topic".to_string();
        let message = "hello_world".to_string();

        event_bus
            .publish(topic.clone(), message.clone())
            .await
            .unwrap();

        let stored_message = entity::Entity::find()
            .filter(entity::Column::Topic.eq(topic.clone()))
            .one(&db)
            .await
            .unwrap();

        assert!(stored_message.is_some());
        let stored_message = stored_message.unwrap();
        assert_eq!(stored_message.topic, topic);

        let stored_message_value: String =
            serde_json::from_str(&stored_message.message).unwrap();
        assert_eq!(stored_message_value, message);
    }

    #[tokio::test]
    async fn test_pub_sub_workflow() {
        let db = setup_in_memory_db().await.unwrap();
        let event_bus = EventBus::new(db);
        let topic = "test_topic".to_string();
        let message = "hello_world".to_string();

        let (tx, rx) = std_mpsc::channel();

        event_bus.subscribe(topic.clone(), move |msg: String| {
            tx.send(msg).unwrap();
            Ok(())
        });

        let event_bus_clone = event_bus.clone();
        let topic_clone = topic.clone();
        let message_clone = message.clone();
        let handle = tokio::spawn(async move {
            // Short delay to ensure subscription is registered
            tokio::time::sleep(Duration::from_millis(100)).await;
            event_bus_clone
                .publish(topic_clone, message_clone)
                .await
                .unwrap();
        });

        handle.await.unwrap();

        let received_message = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(received_message, message);
    }
}