use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use sea_orm::DatabaseConnection;
use serde::{Serialize, de::DeserializeOwned};
use tokio::{
    sync::{
        RwLock,
        mpsc::{self, Receiver, Sender},
    },
    time::sleep,
};
use tracing::{error, info};

use crate::{
    entity::event,
    error::{AppError, AppResult},
};

#[derive(Default)]
pub struct EventBus {
    db: DatabaseConnection,
    notifiers: Arc<RwLock<HashMap<String, mpsc::Sender<()>>>>,
}

impl EventBus {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            notifiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn publish<T: Serialize>(&self, event: &str, payload: &T) -> AppResult<()> {
        let payload_json = serde_json::to_string(payload)?;
        // save to database
        event::add_record(&self.db, event, &payload_json).await?;

        // notify subscriber
        let notifiers = self.notifiers.read().await;
        if let Some(tx) = notifiers.get(event) {
            let _ = tx.try_send(());
        }

        Ok(())
    }

    pub async fn subscribe<T, Func, Fut>(&self, event: &str, handler: Func) -> AppResult<()>
    where
        T: DeserializeOwned + Send,
        Func: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let (tx, mut rx) = self.create_channel(event).await?;

        let db = self.db.clone();
        let event_owned = event.to_owned();
        tokio::spawn(async move {
            // Initial trigger to process existing events
            let _ = tx.try_send(());

            loop {
                if let None = rx.recv().await {
                    // Channel closed
                    return;
                }

                let records = match event::list_next_records(&db, &event_owned, 10).await {
                    Ok(records) => records,
                    Err(e) => {
                        error!("Failed to list event records for '{}', {}", event_owned, e);
                        // retry later
                        continue;
                    }
                };

                // Process records one by one
                for record in records {
                    // Retry loop for each record
                    loop {
                        // Check if channel is closed (shutdown signal)
                        if rx.is_closed() {
                            info!("Event bus shutting down for event '{}'", event_owned);
                            return;
                        }

                        // Deserialize payload
                        let payload: T = match serde_json::from_str(&record.payload) {
                            Ok(p) => p,
                            Err(e) => {
                                error!(
                                    "Failed to deserialize event '{}' payload `{}`: {}",
                                    event_owned, record.payload, e
                                );
                                // Mark as acknowledged to prevent infinite retry of malformed data
                                if let Err(ack_err) = event::mark_as_acknowledged(&db, record.id).await {
                                    error!("Failed to mark malformed event as acknowledged, {}", ack_err);
                                }
                                // Continue to next record
                                break;
                            }
                        };

                        // Call handler
                        match handler(payload).await {
                            Ok(_) => {
                                // Success: mark as acknowledged
                                if let Err(e) = event::mark_as_acknowledged(&db, record.id).await {
                                    error!("Failed to mark event as acknowledged after processing, {}", e);
                                }
                                // Continue to next record
                                break;
                            }
                            Err(e) => {
                                // Failure: log error and retry after delay
                                error!(
                                    "Event '{}' processing failed, retrying after delay, id {}, {}",
                                    event_owned, record.id, e
                                );

                                // Sleep to avoid rapid retry loop
                                sleep(Duration::from_secs(5)).await;

                                // Continue retry loop for same record
                            }
                        }
                    }
                }
            }
        });

        info!("Subscribed to event '{}'", event);
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
