use std::sync::Arc;

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryMediaUpdateKind {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryMediaUpdate {
    pub path: String,
    pub kind: LibraryMediaUpdateKind,
}

#[async_trait::async_trait]
pub trait LibraryUpdateNotifier: Send + Sync {
    async fn notify(&self, updates: &[LibraryMediaUpdate]) -> AppResult<()>;
}

pub type LibraryUpdateNotifierHandle = Arc<dyn LibraryUpdateNotifier>;

#[derive(Debug, Clone, Default)]
pub struct NoopLibraryUpdateNotifier;

#[async_trait::async_trait]
impl LibraryUpdateNotifier for NoopLibraryUpdateNotifier {
    async fn notify(&self, _updates: &[LibraryMediaUpdate]) -> AppResult<()> {
        Ok(())
    }
}

pub async fn notify_library_updates(
    notifier: &dyn LibraryUpdateNotifier,
    updates: &[LibraryMediaUpdate],
) {
    if updates.is_empty() {
        return;
    }
    if let Err(error) = notifier.notify(updates).await {
        tracing::warn!(
            error = %error,
            count = updates.len(),
            "failed to notify media server of library updates"
        );
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::error::AppError;

    #[derive(Clone, Default)]
    pub struct RecordingLibraryUpdateNotifier {
        pub batches: Arc<Mutex<Vec<Vec<LibraryMediaUpdate>>>>,
        pub fail: bool,
    }

    impl RecordingLibraryUpdateNotifier {
        pub fn failing() -> Self {
            Self {
                batches: Arc::new(Mutex::new(Vec::new())),
                fail: true,
            }
        }

        pub fn batches(&self) -> Vec<Vec<LibraryMediaUpdate>> {
            self.batches.lock().unwrap().clone()
        }

        pub fn flat_updates(&self) -> Vec<LibraryMediaUpdate> {
            self.batches().into_iter().flatten().collect()
        }
    }

    #[async_trait::async_trait]
    impl LibraryUpdateNotifier for RecordingLibraryUpdateNotifier {
        async fn notify(&self, updates: &[LibraryMediaUpdate]) -> AppResult<()> {
            self.batches.lock().unwrap().push(updates.to_vec());
            if self.fail {
                return Err(AppError::ExternalService("notify failed".to_string(), true));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use test_support::RecordingLibraryUpdateNotifier;

    #[tokio::test]
    async fn noop_notifier_accepts_updates() {
        NoopLibraryUpdateNotifier
            .notify(&[LibraryMediaUpdate {
                path: "/local/a.strm".to_string(),
                kind: LibraryMediaUpdateKind::Created,
            }])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn notify_library_updates_skips_empty_list() {
        let notifier = RecordingLibraryUpdateNotifier::default();

        notify_library_updates(&notifier, &[]).await;

        assert!(notifier.batches().is_empty());
    }

    #[tokio::test]
    async fn notify_library_updates_swallows_notifier_error() {
        let notifier = RecordingLibraryUpdateNotifier::failing();
        let updates = [LibraryMediaUpdate {
            path: "/local/a.strm".to_string(),
            kind: LibraryMediaUpdateKind::Deleted,
        }];

        notify_library_updates(&notifier, &updates).await;

        assert_eq!(notifier.flat_updates(), updates);
        let error = notifier.notify(&updates).await.unwrap_err();
        assert!(
            matches!(error, AppError::ExternalService(message, _) if message.contains("notify failed"))
        );
    }
}
