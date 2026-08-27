use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::application::sync_strm::{LibrarySyncState, SyncReport};

use super::console::{ConsoleContext, json_response};

#[derive(Debug, Serialize)]
struct LibrarySyncResponse {
    status: &'static str,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    created: u32,
    modified: u32,
    deleted: u32,
    unchanged: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub(super) async fn get_library_sync(State(ctx): State<ConsoleContext>) -> Response {
    let Some(sync) = ctx.library_sync.as_ref() else {
        return unavailable();
    };
    json_response(StatusCode::OK, &to_response(sync.snapshot()))
}

pub(super) async fn start_library_sync(State(ctx): State<ConsoleContext>) -> Response {
    let Some(sync) = ctx.library_sync.as_ref() else {
        return unavailable();
    };
    let started = sync.try_start();
    let status = if started {
        StatusCode::ACCEPTED
    } else {
        StatusCode::CONFLICT
    };
    json_response(status, &to_response(sync.snapshot()))
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "library sync service not available",
    )
        .into_response()
}

fn to_response(state: LibrarySyncState) -> LibrarySyncResponse {
    match state {
        LibrarySyncState::Idle => LibrarySyncResponse {
            status: "idle",
            started_at: None,
            finished_at: None,
            created: 0,
            modified: 0,
            deleted: 0,
            unchanged: 0,
            error: None,
        },
        LibrarySyncState::Running { started_at } => LibrarySyncResponse {
            status: "running",
            started_at: Some(started_at),
            finished_at: None,
            created: 0,
            modified: 0,
            deleted: 0,
            unchanged: 0,
            error: None,
        },
        LibrarySyncState::Succeeded {
            started_at,
            finished_at,
            report,
        } => with_report("succeeded", started_at, finished_at, report, None),
        LibrarySyncState::Failed {
            started_at,
            finished_at,
            message,
        } => with_report(
            "failed",
            started_at,
            finished_at,
            SyncReport::default(),
            Some(message),
        ),
    }
}

fn with_report(
    status: &'static str,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    report: SyncReport,
    error: Option<String>,
) -> LibrarySyncResponse {
    LibrarySyncResponse {
        status,
        started_at: Some(started_at),
        finished_at: Some(finished_at),
        created: report.created,
        modified: report.modified,
        deleted: report.deleted,
        unchanged: report.unchanged,
        error,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use axum::{Router, body::to_bytes, http::StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{
        application::{
            file_index::FileIndexService,
            ports::{FileStore, LibraryGateway, LocalEntry, MediaDirectoryRecord},
            sync_strm::{LibrarySyncController, SyncStrmConfig, SyncStrmService},
        },
        domain::{import::LibraryFile, share::FileHash},
        error::AppResult,
        infrastructure::repo::{
            file_index::SeaOrmFileIndexRepository, import_record::SeaOrmImportRecordRepository,
        },
        interface::http::console::{ConsoleContext, new_router},
        migration::{Migrator, MigratorTrait},
    };

    #[derive(Clone, Default)]
    struct FakeRemote {
        root_ids: Arc<Mutex<HashMap<String, i64>>>,
        dirs: Arc<Mutex<HashMap<i64, Vec<LibraryFile>>>>,
        list_hold: Option<Arc<tokio::sync::Mutex<()>>>,
    }

    #[async_trait::async_trait]
    impl LibraryGateway for FakeRemote {
        async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
            if let Some(hold) = &self.list_hold {
                let _guard = hold.lock().await;
            }
            Ok(self
                .dirs
                .lock()
                .unwrap()
                .get(&dir_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
            Ok(self.root_ids.lock().unwrap().get(path).copied())
        }

        async fn ensure_dir(&self, _path: &str) -> AppResult<i64> {
            unimplemented!()
        }

        async fn list_library_dir_ids(&self, _dir_id: i64) -> AppResult<HashMap<String, i64>> {
            unimplemented!()
        }

        async fn mkdir_library_dir(
            &self,
            _parent_dir_id: i64,
            _folder_name: &str,
        ) -> AppResult<i64> {
            unimplemented!()
        }

        async fn trash_library_files(&self, _file_ids: &[i64]) -> AppResult<()> {
            unimplemented!()
        }

        async fn upload(
            &self,
            _parent_dir_id: i64,
            _file_name: &str,
            _hash: &FileHash,
            _size: u64,
        ) -> AppResult<Option<i64>> {
            unimplemented!()
        }

        async fn download_library_file(&self, _file_id: i64, _local_path: &str) -> AppResult<()> {
            Ok(())
        }

        async fn search_media_dirs(&self, _keyword: &str) -> AppResult<Vec<MediaDirectoryRecord>> {
            unimplemented!()
        }
    }

    #[derive(Clone, Default)]
    struct FakeFileStore;

    #[async_trait::async_trait]
    impl FileStore for FakeFileStore {
        async fn read_to_string_if_exists(&self, _path: &str) -> AppResult<Option<String>> {
            Ok(None)
        }
        async fn metadata_len_if_exists(&self, _path: &str) -> AppResult<Option<u64>> {
            Ok(None)
        }
        async fn ensure_parent_dir(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn write(&self, _path: &str, _content: &[u8]) -> AppResult<()> {
            Ok(())
        }
        async fn read_dir(&self, _path: &str) -> AppResult<Vec<LocalEntry>> {
            Ok(Vec::new())
        }
        async fn remove_file(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn remove_dir_all(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn remove_file_if_exists(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
        async fn remove_dir_all_if_exists(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }
    }

    async fn fresh_db() -> sea_orm::DatabaseConnection {
        let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = sea_orm::Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    async fn router_without_sync() -> Router {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let files = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        new_router(ConsoleContext::new_without_import(repo, files))
    }

    async fn router_with_sync(remote: FakeRemote) -> Router {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let files = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        let controller = LibrarySyncController::new(SyncStrmService::new(
            remote,
            FakeFileStore,
            SyncStrmConfig {
                remote_path: "/remote".into(),
                local_path: "/local".into(),
                strm_download_url: "https://host/d".into(),
            },
            std::sync::Arc::new(crate::application::ports::NoopLibraryUpdateNotifier),
        ));
        new_router(ConsoleContext::new_with_library_sync(
            repo, files, controller,
        ))
    }

    fn movie_remote() -> FakeRemote {
        let remote = FakeRemote::default();
        remote.root_ids.lock().unwrap().insert("/remote".into(), 1);
        remote.dirs.lock().unwrap().insert(
            1,
            vec![LibraryFile {
                file_id: 3,
                file_name: "Movie.2024.1080p.WEB-DL.mkv".into(),
                is_dir: false,
                size: 100,
                hash: String::new(),
            }],
        );
        remote
    }

    fn request(method: &str, uri: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap()
    }

    async fn json_body(response: axum::http::Response<axum::body::Body>) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn wait_until_not_running(router: &Router) -> Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let response = router
                .clone()
                .oneshot(request("GET", "/api/library/sync"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = json_body(response).await;
            if body["status"] != "running" {
                return body;
            }
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for library sync");
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn library_sync_returns_503_without_service() {
        let router = router_without_sync().await;
        for method in ["GET", "POST"] {
            let response = router
                .clone()
                .oneshot(request(method, "/api/library/sync"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert_eq!(bytes.as_ref(), b"library sync service not available");
        }
    }

    #[tokio::test]
    async fn get_library_sync_returns_idle() {
        let router = router_with_sync(movie_remote()).await;
        let response = router
            .oneshot(request("GET", "/api/library/sync"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["status"], "idle");
        assert!(body["started_at"].is_null());
        assert_eq!(body["created"], 0);
    }

    #[tokio::test]
    async fn post_library_sync_starts_and_completes() {
        let router = router_with_sync(movie_remote()).await;
        let response = router
            .clone()
            .oneshot(request("POST", "/api/library/sync"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = json_body(response).await;
        assert!(body["status"] == "running" || body["status"] == "succeeded");

        let body = wait_until_not_running(&router).await;
        assert_eq!(body["status"], "succeeded");
        assert_eq!(body["created"], 1);
        assert!(body["started_at"].is_string());
        assert!(body["finished_at"].is_string());
    }

    #[tokio::test]
    async fn post_library_sync_returns_409_when_running() {
        let hold = Arc::new(tokio::sync::Mutex::new(()));
        let _permit = hold.lock().await;
        let mut remote = movie_remote();
        remote.list_hold = Some(hold.clone());
        let router = router_with_sync(remote).await;

        let first = router
            .clone()
            .oneshot(request("POST", "/api/library/sync"))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::ACCEPTED);

        let second = router
            .clone()
            .oneshot(request("POST", "/api/library/sync"))
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::CONFLICT);
        let body = json_body(second).await;
        assert_eq!(body["status"], "running");
    }
}
