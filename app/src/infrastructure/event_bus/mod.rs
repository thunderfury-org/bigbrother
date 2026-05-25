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
}

impl EventBus {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            store: SeaOrmEventStore::new(db),
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
        let worker = worker::EventWorker::new(self.store.clone(), E::NAME);
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

    use crate::{
        application::notify::{Message, MessageSender},
        infrastructure::event::publisher::EventBusPublisher,
    };
    use migration::{Migrator, MigratorTrait};

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

        bus.subscribe::<Arc<Mutex<Vec<String>>>, crate::application::notify::Message, _, _>(
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
}
