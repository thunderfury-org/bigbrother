use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use sea_orm::DatabaseConnection;
use serde::{Serialize, de::DeserializeOwned};
use tokio::{
    sync::{
        RwLock,
        watch::{self, Receiver, Sender},
    },
    time::sleep,
};
use tracing::{error, info};

use crate::{
    entity::event,
    error::{AppError, AppResult},
};

pub trait Event: Serialize + DeserializeOwned + Send + 'static {
    const NAME: &'static str;
}

#[derive(Default)]
pub struct EventBus {
    db: DatabaseConnection,
    notifiers: Arc<RwLock<HashMap<String, watch::Sender<()>>>>,
}

impl EventBus {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            notifiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn publish<E: Event>(&self, payload: &E) -> AppResult<()> {
        let payload_json = serde_json::to_string(payload)?;
        // save to database
        event::add_record(&self.db, E::NAME, &payload_json).await?;

        // notify subscriber
        let notifiers = self.notifiers.read().await;
        if let Some(tx) = notifiers.get(E::NAME) {
            let _ = tx.send(());
        }

        Ok(())
    }

    pub async fn subscribe<S, E, Func, Fut>(&self, state: S, handler: Func) -> AppResult<()>
    where
        S: Clone + Send + Sync + 'static,
        E: Event,
        Func: Fn(S, E) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let (tx, mut rx) = self.create_channel(E::NAME).await?;

        let db = self.db.clone();
        let event_name = E::NAME.to_owned();
        tokio::spawn(async move {
            // Initial trigger to process existing events
            tx.send_replace(());

            loop {
                if rx.changed().await.is_err() {
                    // Channel closed
                    return;
                }

                let records = match event::list_next_records(&db, &event_name, 10).await {
                    Ok(records) => records,
                    Err(e) => {
                        error!("Failed to list event records for '{}', {}", event_name, e);
                        // retry later
                        continue;
                    }
                };

                // Process records one by one
                for record in records {
                    loop {
                        // Deserialize payload
                        let payload: E = match serde_json::from_str(&record.payload) {
                            Ok(p) => p,
                            Err(e) => {
                                error!(
                                    "Failed to deserialize event '{}' payload `{}`: {}",
                                    event_name, record.payload, e
                                );
                                // Mark as acknowledged to prevent infinite retry of malformed data
                                if let Err(ack_err) =
                                    event::mark_as_acknowledged(&db, record.id).await
                                {
                                    error!(
                                        "Failed to mark malformed event as acknowledged, {}",
                                        ack_err
                                    );
                                }
                                break;
                            }
                        };

                        // Call handler
                        match handler(state.clone(), payload).await {
                            Ok(_) => {
                                // Success: mark as acknowledged
                                if let Err(e) = event::mark_as_acknowledged(&db, record.id).await {
                                    error!(
                                        "Failed to mark event as acknowledged after processing, {}",
                                        e
                                    );
                                }
                                break;
                            }
                            Err(e) => {
                                error!(
                                    "Error processing event '{}', id {}, will retry after 5 seconds, {}",
                                    event_name, record.id, e
                                );
                                // Retry after delay
                                sleep(Duration::from_secs(5)).await;
                            }
                        }
                    }
                }
            }
        });

        info!("Subscribed to event '{}'", E::NAME);
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

        let (tx, rx) = watch::channel(());
        notifiers.insert(event.to_owned(), tx.clone());
        Ok((tx, rx))
    }
}
