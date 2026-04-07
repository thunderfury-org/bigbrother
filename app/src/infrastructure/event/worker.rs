use std::{future::Future, time::Duration};

use tokio::time::sleep;
use tracing::error;

use crate::{error::AppResult, event_bus::Event, infrastructure::event::store::SeaOrmEventStore};

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
