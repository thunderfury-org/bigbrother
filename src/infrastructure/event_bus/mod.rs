use std::{collections::HashMap, future::Future, sync::Arc};

use sea_orm::DatabaseConnection;
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{
    RwLock,
    watch::{self, Receiver, Sender},
};
use tracing::{error, info};

use crate::{
    error::{AppError, AppResult},
    infrastructure::event::store::SeaOrmEventStore,
};

pub(crate) mod worker;

pub trait Event: Serialize + DeserializeOwned + Send + 'static {
    const NAME: &'static str;
}

#[derive(Clone)]
pub struct EventBus {
    store: SeaOrmEventStore,
    notifiers: Arc<RwLock<HashMap<String, watch::Sender<()>>>>,
    error_retry_delay: std::time::Duration,
}

impl EventBus {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            store: SeaOrmEventStore::new(db),
            notifiers: Arc::new(RwLock::new(HashMap::new())),
            error_retry_delay: std::time::Duration::from_secs(10),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_error_retry_delay(mut self, delay: std::time::Duration) -> Self {
        self.error_retry_delay = delay;
        self
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
        let worker = worker::EventWorker::new(self.store.clone(), E::NAME);
        let error_retry_delay = self.error_retry_delay;
        tokio::spawn(async move {
            tx.send_replace(());
            let mut retry_backoff = false;

            loop {
                if retry_backoff {
                    tokio::select! {
                        res = rx.changed() => {
                            if res.is_err() {
                                return;
                            }
                        }
                        _ = tokio::time::sleep(error_retry_delay) => {}
                    }
                } else if rx.changed().await.is_err() {
                    return;
                }

                match worker
                    .drain_with_handler::<S, E, Func, Fut>(state.clone(), &handler)
                    .await
                {
                    Ok(()) => {
                        retry_backoff = false;
                    }
                    Err(err) => {
                        error!("Failed to drain event worker for '{}', {}", E::NAME, err);
                        retry_backoff = true;
                    }
                }
            }
        });

        info!("Subscribed to event '{}'", E::NAME);
        Ok(())
    }

    async fn create_channel(&self, event: &str) -> AppResult<(Sender<()>, Receiver<()>)> {
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use sea_orm::{ConnectOptions, Database};
    use serde::{Deserialize, Serialize};
    use tokio::time::{Duration, sleep};

    use crate::migration::{Migrator, MigratorTrait};
    use crate::{
        application::ports::{Message, MessageSender},
        infrastructure::event::publisher::EventBusPublisher,
    };

    #[derive(Clone, Serialize, Deserialize)]
    struct SampleEvent;

    impl Event for SampleEvent {
        const NAME: &'static str = "SampleEvent";
    }

    async fn test_bus() -> EventBus {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        EventBus::new(db)
    }

    #[tokio::test]
    async fn rejects_duplicate_subscription_for_same_event() {
        let bus = test_bus().await;

        bus.subscribe::<(), SampleEvent, _, _>((), |_state, _event| async { Ok(()) })
            .await
            .unwrap();

        let error = bus
            .subscribe::<(), SampleEvent, _, _>((), |_state, _event| async { Ok(()) })
            .await
            .unwrap_err();

        assert!(error.to_string().contains("already subscribed"));
    }

    #[tokio::test]
    async fn publish_service_reaches_subscribed_handler() {
        let bus = test_bus().await;
        let received = Arc::new(Mutex::new(Vec::new()));

        bus.subscribe::<Arc<Mutex<Vec<String>>>, crate::application::ports::Message, _, _>(
            received.clone(),
            |received, payload| async move {
                received.lock().unwrap().push(payload.message);
                Ok(())
            },
        )
        .await
        .unwrap();

        EventBusPublisher::new(bus.clone())
            .send(&Message::new("hello chain", Some(7)))
            .await
            .unwrap();

        for _ in 0..20 {
            if received.lock().unwrap().len() == 1 {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(received.lock().unwrap().as_slice(), ["hello chain"]);
    }

    #[tokio::test]
    async fn worker_recovers_after_drain_error_without_new_publish() {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();

        let bus = EventBus::new(db.clone()).with_error_retry_delay(Duration::from_millis(20));

        use std::sync::atomic::{AtomicUsize, Ordering};
        let processed = Arc::new(AtomicUsize::new(0));
        let processed_clone = processed.clone();
        let db_clone = db.clone();

        bus.subscribe::<Arc<AtomicUsize>, SampleEvent, _, _>(
            processed_clone,
            move |processed, _event| {
                let db = db_clone.clone();
                async move {
                    let current = processed.fetch_add(1, Ordering::SeqCst);
                    if current == 0 {
                        use sea_orm::ConnectionTrait;
                        db.execute_unprepared("ALTER TABLE event RENAME TO event_bak")
                            .await
                            .unwrap();
                    }
                    Ok(())
                }
            },
        )
        .await
        .unwrap();

        // Publish event
        bus.publish(&SampleEvent).await.unwrap();

        // Wait until first attempt fails and renames table
        for _ in 0..50 {
            if processed.load(Ordering::SeqCst) >= 1 {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(processed.load(Ordering::SeqCst), 1);

        // Wait for ack retries (100ms + 200ms + 400ms = 700ms) to fail and enter retry backoff
        sleep(Duration::from_millis(850)).await;

        // Restore table so subsequent drain can succeed
        use sea_orm::ConnectionTrait;
        db.execute_unprepared("ALTER TABLE event_bak RENAME TO event")
            .await
            .unwrap();

        // Without any new publish, the error timer (20ms) should self-heal and drain again
        for _ in 0..50 {
            if processed.load(Ordering::SeqCst) >= 2 {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }

        assert_eq!(processed.load(Ordering::SeqCst), 2);
    }
}
