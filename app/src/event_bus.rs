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
    entity::model::event as event_model,
    error::{AppError, AppResult},
};

pub trait Event: Serialize + DeserializeOwned + Send + 'static {
    const NAME: &'static str;
}

#[derive(Clone)]
pub struct EventBus {
    store: EventStore,
    notifiers: Arc<RwLock<HashMap<String, watch::Sender<()>>>>,
}

impl EventBus {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            store: EventStore::new(db),
            notifiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn publish<E: Event>(&self, payload: &E) -> AppResult<()> {
        let payload_json = serde_json::to_string(payload)?;
        self.store.append(E::NAME, &payload_json).await?;

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
        let worker = EventWorker::new(self.store.clone(), E::NAME);
        tokio::spawn(async move {
            tx.send_replace(());

            loop {
                if rx.changed().await.is_err() {
                    return;
                }

                if let Err(err) = worker
                    .drain_with_handler::<S, E, Func, Fut>(state.clone(), &handler)
                    .await
                {
                    error!("Failed to drain event worker for '{}', {}", E::NAME, err);
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

#[derive(Clone)]
struct EventStore {
    db: DatabaseConnection,
}

impl EventStore {
    fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn append(&self, name: &str, payload: &str) -> AppResult<()> {
        event::add_record(&self.db, name, payload).await?;
        Ok(())
    }

    async fn list_pending(&self, name: &str, limit: u64) -> AppResult<Vec<event_model::Model>> {
        Ok(event::list_next_records(&self.db, name, limit).await?)
    }

    async fn ack(&self, id: i64) -> AppResult<()> {
        event::mark_as_acknowledged(&self.db, id).await?;
        Ok(())
    }
}

#[derive(Clone)]
struct EventWorker {
    store: EventStore,
    event_name: &'static str,
    batch_size: u64,
    retry_delay: Duration,
}

impl EventWorker {
    fn new(store: EventStore, event_name: &'static str) -> Self {
        Self {
            store,
            event_name,
            batch_size: 10,
            retry_delay: Duration::from_secs(5),
        }
    }

    async fn drain_with_handler<S, E, Func, Fut>(&self, state: S, handler: &Func) -> AppResult<()>
    where
        S: Clone + Send + Sync + 'static,
        E: Event,
        Func: Fn(S, E) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let records = self
            .store
            .list_pending(self.event_name, self.batch_size)
            .await?;

        for record in records {
            loop {
                let payload: E = match serde_json::from_str(&record.payload) {
                    Ok(payload) => payload,
                    Err(err) => {
                        error!(
                            "Failed to deserialize event '{}' payload `{}`: {}",
                            self.event_name, record.payload, err
                        );
                        if let Err(ack_err) = self.store.ack(record.id).await {
                            error!("Failed to ack malformed event, {}", ack_err);
                        }
                        break;
                    }
                };

                match handler(state.clone(), payload).await {
                    Ok(_) => {
                        if let Err(err) = self.store.ack(record.id).await {
                            error!(
                                "Failed to mark event as acknowledged after processing, {}",
                                err
                            );
                        }
                        break;
                    }
                    Err(err) => {
                        error!(
                            "Error processing event '{}', id {}, will retry after 5 seconds, {}",
                            self.event_name, record.id, err
                        );
                        sleep(self.retry_delay).await;
                    }
                }
            }
        }

        Ok(())
    }
}
