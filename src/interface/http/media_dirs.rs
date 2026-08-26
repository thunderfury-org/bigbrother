use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::application::delete_media::{MediaDeleteCandidate, MediaDirDeleteItem, MediaDirEntry};

use super::console::{ConsoleContext, app_error_to_response, json_response};

#[derive(Debug, Default, Deserialize)]
pub(super) struct ListMediaDirsQuery {
    q: Option<String>,
    parent_id: Option<i64>,
}

#[derive(Debug, Serialize)]
struct MediaDirsResponse {
    items: Vec<MediaDirItemJson>,
}

#[derive(Debug, Serialize)]
struct MediaDirItemJson {
    dir_id: i64,
    display_name: String,
    deletable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    relative_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DeleteMediaDirsRequest {
    items: Vec<DeleteMediaDirItemRequest>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DeleteMediaDirItemRequest {
    dir_id: i64,
    relative_path: String,
}

pub(super) async fn list_media_dirs(
    State(ctx): State<ConsoleContext>,
    Query(query): Query<ListMediaDirsQuery>,
) -> Response {
    let Some(svc) = ctx.delete_media_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "delete media service not available",
        )
            .into_response();
    };

    let keyword = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let result = if let Some(keyword) = keyword {
        svc.search_candidates(keyword)
            .await
            .map(|candidates| candidates.into_iter().map(search_item).collect())
    } else {
        svc.list_children(query.parent_id)
            .await
            .map(|entries| entries.into_iter().map(browse_item).collect())
    };

    match result {
        Ok(items) => json_response(StatusCode::OK, &MediaDirsResponse { items }),
        Err(err) => app_error_to_response(err),
    }
}

pub(super) async fn delete_media_dirs(
    State(ctx): State<ConsoleContext>,
    axum::Json(body): axum::Json<DeleteMediaDirsRequest>,
) -> Response {
    let Some(svc) = ctx.delete_media_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "delete media service not available",
        )
            .into_response();
    };

    let items = body
        .items
        .into_iter()
        .map(|item| MediaDirDeleteItem {
            dir_id: item.dir_id,
            relative_path: item.relative_path,
        })
        .collect::<Vec<_>>();

    match svc.delete_dirs(&items).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => app_error_to_response(err),
    }
}

fn browse_item(entry: MediaDirEntry) -> MediaDirItemJson {
    MediaDirItemJson {
        dir_id: entry.dir_id,
        display_name: entry.display_name,
        deletable: entry.deletable,
        relative_path: None,
    }
}

fn search_item(candidate: MediaDeleteCandidate) -> MediaDirItemJson {
    MediaDirItemJson {
        dir_id: candidate.dir_id,
        display_name: candidate.display_name,
        deletable: true,
        relative_path: Some(candidate.relative_path),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::{Router, body::to_bytes, http::StatusCode};
    use tower::ServiceExt;

    use crate::{
        application::{
            delete_media::DeleteMediaService,
            file_index::FileIndexService,
            import_local_store::ImportLocalStore,
            ports::{FileStore, LibraryGateway, LocalEntry, MediaDirectoryRecord},
        },
        domain::{import::LibraryFile, share::FileHash},
        error::AppResult,
        infrastructure::repo::{
            file_index::SeaOrmFileIndexRepository, import_record::SeaOrmImportRecordRepository,
        },
        interface::http::console::ConsoleContext,
        migration::{Migrator, MigratorTrait},
    };

    use super::*;

    #[derive(Clone, Default)]
    struct FakeLibraryGateway {
        records: Arc<Vec<MediaDirectoryRecord>>,
        trashed: Arc<Mutex<Vec<Vec<i64>>>>,
        root_path: Option<String>,
        root_id: Option<i64>,
        children: Arc<HashMap<i64, Vec<LibraryFile>>>,
    }

    #[async_trait::async_trait]
    impl LibraryGateway for FakeLibraryGateway {
        async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
            Ok(self.children.get(&dir_id).cloned().unwrap_or_default())
        }

        async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
            Ok(self
                .root_path
                .as_deref()
                .filter(|root| *root == path)
                .and(self.root_id))
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

        async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()> {
            self.trashed.lock().unwrap().push(file_ids.to_vec());
            Ok(())
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
            unimplemented!()
        }

        async fn search_media_dirs(&self, _keyword: &str) -> AppResult<Vec<MediaDirectoryRecord>> {
            Ok(self.records.as_ref().clone())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingFileStore {
        removed_dirs: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl FileStore for RecordingFileStore {
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
        async fn remove_dir_all_if_exists(&self, path: &str) -> AppResult<()> {
            self.removed_dirs.lock().unwrap().push(path.to_owned());
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

    fn dir_file(file_id: i64, file_name: &str) -> LibraryFile {
        LibraryFile {
            file_id,
            file_name: file_name.to_owned(),
            is_dir: true,
            size: 0,
            hash: String::new(),
        }
    }

    fn service_with(library: FakeLibraryGateway) -> DeleteMediaService {
        DeleteMediaService::new(
            library,
            ImportLocalStore::new(
                RecordingFileStore::default(),
                "/remote".into(),
                "/local".into(),
                "http://d".into(),
            ),
            "/remote".into(),
            std::sync::Arc::new(crate::application::ports::NoopLibraryUpdateNotifier),
        )
    }

    async fn router_with_service(service: DeleteMediaService) -> Router {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        crate::interface::http::console::new_router(ConsoleContext::new_with_delete_media(
            repo,
            file_service,
            service,
        ))
    }

    fn browse_library() -> FakeLibraryGateway {
        let mut children = HashMap::new();
        children.insert(1, vec![dir_file(2, "电影"), dir_file(3, "电视剧")]);
        children.insert(2, vec![dir_file(21, "Inception (2010) {tmdb-27205}")]);
        FakeLibraryGateway {
            root_path: Some("/remote".into()),
            root_id: Some(1),
            children: Arc::new(children),
            records: Arc::new(vec![MediaDirectoryRecord {
                dir_id: 21,
                display_name: "Inception (2010) {tmdb-27205}".into(),
                remote_path: "/remote/电影/Inception (2010) {tmdb-27205}".into(),
            }]),
            ..FakeLibraryGateway::default()
        }
    }

    fn request(uri: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn list_without_service_returns_503() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        let router = crate::interface::http::console::new_router(
            ConsoleContext::new_without_import(repo, file_service),
        );
        let response = router.oneshot(request("/api/media-dirs")).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn delete_without_service_returns_503() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        let router = crate::interface::http::console::new_router(
            ConsoleContext::new_without_import(repo, file_service),
        );
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/media-dirs/delete")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"items":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn list_root_returns_children() {
        let router = router_with_service(service_with(browse_library())).await;
        let response = router.oneshot(request("/api/media-dirs")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = json_body(response).await;
        assert_eq!(json["items"][0]["display_name"], "电影");
        assert_eq!(json["items"][0]["deletable"], false);
        assert!(json["items"][0].get("relative_path").is_none());
        assert_eq!(json["items"][1]["display_name"], "电视剧");
    }

    #[tokio::test]
    async fn list_parent_id_returns_child_dir() {
        let router = router_with_service(service_with(browse_library())).await;
        let response = router
            .oneshot(request("/api/media-dirs?parent_id=2"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = json_body(response).await;
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["items"][0]["dir_id"], 21);
        assert_eq!(json["items"][0]["deletable"], true);
    }

    #[tokio::test]
    async fn search_query_ignores_parent_id() {
        let router = router_with_service(service_with(browse_library())).await;
        let response = router
            .oneshot(request("/api/media-dirs?q=Inception&parent_id=999"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = json_body(response).await;
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            json["items"][0]["relative_path"],
            "电影/Inception (2010) {tmdb-27205}"
        );
        assert_eq!(json["items"][0]["deletable"], true);
    }

    #[tokio::test]
    async fn delete_returns_204() {
        let library = browse_library();
        let router = router_with_service(service_with(library.clone())).await;
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/media-dirs/delete")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"items":[{"dir_id":21,"relative_path":"电影/Inception (2010) {tmdb-27205}"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(library.trashed.lock().unwrap().as_slice(), &[vec![21]]);
    }

    #[tokio::test]
    async fn delete_rejects_non_media_path() {
        let router = router_with_service(service_with(browse_library())).await;
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/media-dirs/delete")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"items":[{"dir_id":2,"relative_path":"电影"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
