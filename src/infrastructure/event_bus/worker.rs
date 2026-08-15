use std::{future::Future, time::Duration};

use tokio::time::sleep;
use tracing::error;

use crate::{
    error::AppResult,
    infrastructure::{event::store::SeaOrmEventStore, event_bus::Event},
};

#[derive(Clone)]
pub struct EventWorker {
    store: SeaOrmEventStore,
    event_name: &'static str,
    batch_size: u64,
    retry_delay: Duration,
}

impl EventWorker {
    pub fn new(store: SeaOrmEventStore, event_name: &'static str) -> Self {
        Self {
            store,
            event_name,
            batch_size: 10,
            retry_delay: Duration::from_secs(5),
        }
    }

    #[cfg(test)]
    fn with_retry_delay(mut self, retry_delay: Duration) -> Self {
        self.retry_delay = retry_delay;
        self
    }

    pub async fn drain_with_handler<S, E, Func, Fut>(
        &self,
        state: S,
        handler: &Func,
    ) -> AppResult<()>
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
                    Err(err) if err.is_retryable() => {
                        error!(
                            "Error processing event '{}', id {}, will retry after 5 seconds, {}",
                            self.event_name, record.id, err
                        );
                        sleep(self.retry_delay).await;
                    }
                    Err(err) => {
                        error!(
                            "Non-retryable error processing event '{}', id {}, acking: {}",
                            self.event_name, record.id, err
                        );
                        if let Err(ack_err) = self.store.ack(record.id).await {
                            error!("Failed to ack non-retryable event, {}", ack_err);
                        }
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use sea_orm::{ConnectOptions, Database};
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::migration::{Migrator, MigratorTrait};
    use crate::{error::AppError, infrastructure::event::store::SeaOrmEventStore};

    #[derive(Clone, Serialize, Deserialize)]
    struct SampleEvent {
        value: u32,
    }

    impl Event for SampleEvent {
        const NAME: &'static str = "SampleEventWorker";
    }

    async fn test_store() -> SeaOrmEventStore {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        SeaOrmEventStore::new(db)
    }

    #[tokio::test]
    async fn drain_retries_failed_handler_until_success_and_acks() {
        let store = test_store().await;
        let payload = serde_json::to_string(&SampleEvent { value: 7 }).unwrap();
        store.append(SampleEvent::NAME, &payload).await.unwrap();

        let attempts = Arc::new(AtomicUsize::new(0));
        let worker =
            EventWorker::new(store.clone(), SampleEvent::NAME).with_retry_delay(Duration::ZERO);

        worker
            .drain_with_handler::<Arc<AtomicUsize>, SampleEvent, _, _>(
                attempts.clone(),
                &|attempts, _event| async move {
                    let current = attempts.fetch_add(1, Ordering::SeqCst);
                    if current == 0 {
                        Err(AppError::Network("retry once".into(), true))
                    } else {
                        Ok(())
                    }
                },
            )
            .await
            .unwrap();

        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert!(
            store
                .list_pending(SampleEvent::NAME, 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn drain_acks_malformed_events_without_calling_handler() {
        let store = test_store().await;
        store.append(SampleEvent::NAME, "{not-json").await.unwrap();

        let calls = Arc::new(AtomicUsize::new(0));
        let worker =
            EventWorker::new(store.clone(), SampleEvent::NAME).with_retry_delay(Duration::ZERO);

        worker
            .drain_with_handler::<Arc<AtomicUsize>, SampleEvent, _, _>(
                calls.clone(),
                &|calls, _event| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(
            store
                .list_pending(SampleEvent::NAME, 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn drain_acks_non_retryable_error_without_retry() {
        let store = test_store().await;
        let payload = serde_json::to_string(&SampleEvent { value: 42 }).unwrap();
        store.append(SampleEvent::NAME, &payload).await.unwrap();

        let attempts = Arc::new(AtomicUsize::new(0));
        let worker =
            EventWorker::new(store.clone(), SampleEvent::NAME).with_retry_delay(Duration::ZERO);

        worker
            .drain_with_handler::<Arc<AtomicUsize>, SampleEvent, _, _>(
                attempts.clone(),
                &|attempts, _event| async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err(AppError::InvalidParameter("bad input".into()))
                },
            )
            .await
            .unwrap();

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(
            store
                .list_pending(SampleEvent::NAME, 10)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
