use tracing::{error, info};

use crate::common::state::AppState;

mod alist;
mod push;
mod tmdb;
mod tv;

pub async fn run_tasks(state: &AppState) {
    for task in state.config.get_tasks() {
        info!("running task: {:?}", task);

        let processor = match tv::TvProcessor::new(state, task) {
            Ok(processor) => processor,
            Err(e) => {
                error!("failed to create tv processor: {}", e);
                return;
            }
        };

        if let Err(e) = processor.run().await {
            error!("failed to run task: {}", e);
        }
    }
}
