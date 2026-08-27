use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tracing::{error, info};

use crate::{
    application::sync_strm::{SyncReport, SyncStrmService},
    error::AppError,
};

#[derive(Debug, Clone)]
pub(crate) enum LibrarySyncState {
    Idle,
    Running {
        started_at: DateTime<Utc>,
    },
    Succeeded {
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        report: SyncReport,
    },
    Failed {
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        error: AppError,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum StartSync {
    Started(LibrarySyncState),
    AlreadyRunning(LibrarySyncState),
}

#[derive(Clone)]
pub(crate) struct LibrarySyncController {
    service: Arc<SyncStrmService>,
    state: Arc<Mutex<LibrarySyncState>>,
}

impl LibrarySyncController {
    pub(crate) fn new(service: SyncStrmService) -> Self {
        Self {
            service: Arc::new(service),
            state: Arc::new(Mutex::new(LibrarySyncState::Idle)),
        }
    }

    pub(crate) fn snapshot(&self) -> LibrarySyncState {
        self.state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub(crate) fn try_start(&self) -> StartSync {
        let started_at = Utc::now();
        let running = LibrarySyncState::Running { started_at };
        {
            let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            if matches!(*state, LibrarySyncState::Running { .. }) {
                return StartSync::AlreadyRunning(state.clone());
            }
            *state = running.clone();
        }

        let service = self.service.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            info!("Starting library strm sync");
            let result = service.execute().await;
            let finished_at = Utc::now();
            let mut guard = state.lock().unwrap_or_else(|err| err.into_inner());
            *guard = match result {
                Ok(report) => {
                    info!(
                        created = report.created,
                        modified = report.modified,
                        deleted = report.deleted,
                        unchanged = report.unchanged,
                        "Library strm sync completed"
                    );
                    LibrarySyncState::Succeeded {
                        started_at,
                        finished_at,
                        report,
                    }
                }
                Err(error) => {
                    error!(error = %error, "Library strm sync failed");
                    LibrarySyncState::Failed {
                        started_at,
                        finished_at,
                        error,
                    }
                }
            };
        });
        StartSync::Started(running)
    }
}
