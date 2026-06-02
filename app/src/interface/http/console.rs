use std::sync::Arc;

use axum::{
    Router,
    extract::{OriginalUri, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};

use crate::{
    application::{
        file_index::FileIndexService,
        file_index_import::{FileIndexImportService, ImportFileResult},
        ports::{
            FileIndexRepository, FileLocationRecord, FileSearchRecord, ImportRecordFilter,
            ImportRecordPage, ImportRecordPaging, ImportRecordRepository, ImportRecordView,
        },
        recorded_import::RecordedImportService,
    },
    domain::import_record::{ImportSourceKind, ImportStatus, RecordSummary, SummaryItem},
    error::AppError,
    infrastructure::{
        repo::{
            file_index::SeaOrmFileIndexRepository, import_record::SeaOrmImportRecordRepository,
        },
        services::{IdentifyService, ImportService},
    },
    interface::http::console_assets::{AssetFile, resolve_asset},
};

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

#[derive(Embed)]
#[folder = "../web/dist"]
struct Assets;

fn embedded_lookup(path: &str) -> Option<AssetFile> {
    let file = Assets::get(path)?;
    Some(AssetFile {
        bytes: file.data,
        mime: file.metadata.mimetype().to_owned().into(),
    })
}

#[derive(Clone)]
pub(crate) struct ConsoleContext {
    repo: Arc<SeaOrmImportRecordRepository>,
    file_index_service: Arc<FileIndexService<SeaOrmFileIndexRepository>>,
    import_service: Option<Arc<ImportService>>,
    identify_service: Option<Arc<IdentifyService>>,
    recorded_import: Option<Arc<RecordedImportService<SeaOrmImportRecordRepository>>>,
}

impl ConsoleContext {
    pub(crate) fn new(
        repo: SeaOrmImportRecordRepository,
        file_index_service: FileIndexService<SeaOrmFileIndexRepository>,
        import_service: ImportService,
        identify_service: IdentifyService,
    ) -> Self {
        let repo = Arc::new(repo);
        let recorded_import = Arc::new(RecordedImportService::new(repo.as_ref().clone()));
        Self {
            repo,
            file_index_service: Arc::new(file_index_service),
            import_service: Some(Arc::new(import_service)),
            identify_service: Some(Arc::new(identify_service)),
            recorded_import: Some(recorded_import),
        }
    }

    #[cfg(test)]
    fn new_without_import(
        repo: SeaOrmImportRecordRepository,
        file_index_service: FileIndexService<SeaOrmFileIndexRepository>,
    ) -> Self {
        Self {
            repo: Arc::new(repo),
            file_index_service: Arc::new(file_index_service),
            import_service: None,
            identify_service: None,
            recorded_import: None,
        }
    }
}

pub(crate) fn new_router(ctx: ConsoleContext) -> Router {
    Router::new()
        .route("/api/imports", get(list_imports))
        .route("/api/imports/{id}", get(get_import))
        .route("/api/files", get(search_files))
        .route("/api/files/import", post(import_files))
        .fallback(get(static_handler))
        .with_state(ctx)
}

async fn static_handler(OriginalUri(uri): OriginalUri) -> Response {
    resolve_asset(uri.path(), embedded_lookup)
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    status: Option<String>,
    source_kind: Option<String>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    cursor: Option<i64>,
    limit: Option<u64>,
}

async fn list_imports(
    State(ctx): State<ConsoleContext>,
    Query(query): Query<ListQuery>,
) -> Response {
    list_with_repo(ctx.repo.as_ref(), query).await
}

async fn list_with_repo<R: ImportRecordRepository>(repo: &R, query: ListQuery) -> Response {
    let status = match query.status.as_deref() {
        Some(raw) => match ImportStatus::from_str(raw) {
            Some(value) => Some(value),
            None => return bad_request(format!("invalid status: {raw}")),
        },
        None => None,
    };
    let source_kind = query.source_kind.as_deref().map(ImportSourceKind::from_str);
    let filter = ImportRecordFilter {
        status,
        source_kind,
        since: query.since,
        until: query.until,
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let paging = ImportRecordPaging {
        cursor: query.cursor,
        limit,
    };

    match repo.list(&filter, paging).await {
        Ok(page) => json_response(StatusCode::OK, &list_to_json(page)),
        Err(err) => app_error_to_response(err),
    }
}

#[derive(Debug, Default, Deserialize)]
struct SearchQuery {
    q: Option<String>,
    limit: Option<u64>,
}

async fn search_files(
    State(ctx): State<ConsoleContext>,
    Query(query): Query<SearchQuery>,
) -> Response {
    search_files_with_service(ctx.file_index_service.as_ref(), query).await
}

async fn search_files_with_service<R: FileIndexRepository>(
    service: &FileIndexService<R>,
    query: SearchQuery,
) -> Response {
    let keyword = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    match service.search_files(&keyword, limit).await {
        Ok(records) => json_response(StatusCode::OK, &file_search_to_json(records)),
        Err(err) => app_error_to_response(err),
    }
}

#[derive(Debug, Serialize)]
struct FileSearchPageJson {
    items: Vec<FileSearchItemJson>,
}

#[derive(Debug, Serialize)]
struct FileSearchItemJson {
    id: i64,
    size: u64,
    hash_type: String,
    hash_value: String,
    locations: Vec<FileLocationJson>,
}

#[derive(Debug, Serialize)]
struct FileLocationJson {
    file_name: String,
    file_path: String,
    descriptions: Vec<String>,
}

fn file_search_to_json(records: Vec<FileSearchRecord>) -> FileSearchPageJson {
    FileSearchPageJson {
        items: records
            .into_iter()
            .map(|record| FileSearchItemJson {
                id: record.id,
                size: record.size,
                hash_type: record.hash_type,
                hash_value: record.hash_value,
                locations: record
                    .locations
                    .into_iter()
                    .map(file_location_to_json)
                    .collect(),
            })
            .collect(),
    }
}

fn file_location_to_json(loc: FileLocationRecord) -> FileLocationJson {
    FileLocationJson {
        file_name: loc.file_name,
        file_path: loc.file_path,
        descriptions: loc.descriptions,
    }
}

async fn get_import(State(ctx): State<ConsoleContext>, Path(id): Path<i64>) -> Response {
    get_with_repo(ctx.repo.as_ref(), id).await
}

async fn get_with_repo<R: ImportRecordRepository>(repo: &R, id: i64) -> Response {
    match repo.get(id).await {
        Ok(Some(view)) => json_response(StatusCode::OK, &detail_to_json(&view)),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(err) => app_error_to_response(err),
    }
}

#[derive(Debug, Serialize)]
struct ListItemJson {
    id: i64,
    source_kind: String,
    source: String,
    status: String,
    title: String,
    year: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    season: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_summary: Option<String>,
    total_size: u64,
    cost_ms: u64,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct DetailJson {
    id: i64,
    source_kind: String,
    source: String,
    status: String,
    summary: Option<RecordSummary>,
    error: Option<ImportRecordErrorJson>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct ImportRecordErrorJson {
    kind: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct ImportRecordPageJson {
    items: Vec<ListItemJson>,
    next_cursor: Option<i64>,
}

fn list_to_json(page: ImportRecordPage) -> ImportRecordPageJson {
    ImportRecordPageJson {
        items: page.items.iter().map(list_item_to_json).collect(),
        next_cursor: page.next_cursor,
    }
}

fn list_item_to_json(view: &ImportRecordView) -> ListItemJson {
    let summary = view
        .summary_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<RecordSummary>(raw).ok());

    let (title, year, season, episode_summary) = match summary
        .as_ref()
        .and_then(|s| s.items.first())
    {
        Some(SummaryItem::Movie { title, year, .. }) => (title.clone(), year.clone(), None, None),
        Some(SummaryItem::Tv {
            name,
            year,
            season,
            episodes,
            missing_episodes,
            ..
        }) => {
            let total = episodes.len() + missing_episodes.len();
            let succeeded = episodes.iter().filter(|e| e.succeeded).count();
            let summary_str = format!("{succeeded}/{total}");
            (name.clone(), year.clone(), Some(*season), Some(summary_str))
        }
        Some(SummaryItem::Skipped { .. }) | None => (String::new(), String::new(), None, None),
    };

    let total_size = summary.as_ref().map_or(0, |s| s.total_size);
    let cost_ms = summary.as_ref().map_or(0, |s| s.total_cost_ms);

    ListItemJson {
        id: view.id,
        source_kind: view.source_kind.as_str().to_owned(),
        source: view.source.clone(),
        status: view.status.as_str().to_owned(),
        title,
        year,
        season,
        episode_summary,
        total_size,
        cost_ms,
        created_at: view.created_at,
        finished_at: view.finished_at,
    }
}

fn detail_to_json(view: &ImportRecordView) -> DetailJson {
    let summary = view
        .summary_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<RecordSummary>(raw).ok());
    let error = view.error_kind.as_ref().map(|kind| ImportRecordErrorJson {
        kind: kind.clone(),
        message: view.error_message.clone().unwrap_or_default(),
    });
    DetailJson {
        id: view.id,
        source_kind: view.source_kind.as_str().to_owned(),
        source: view.source.clone(),
        status: view.status.as_str().to_owned(),
        summary,
        error,
        created_at: view.created_at,
        updated_at: view.updated_at,
        finished_at: view.finished_at,
    }
}

#[derive(Debug, Deserialize)]
struct ImportFilesRequest {
    ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
struct ImportFilesResponse {
    results: Vec<ImportFileResult>,
}

async fn import_files(
    State(ctx): State<ConsoleContext>,
    axum::Json(body): axum::Json<ImportFilesRequest>,
) -> Response {
    let Some(((import_service, identify_service), recorded_import)) = ctx
        .import_service
        .as_ref()
        .zip(ctx.identify_service.as_ref())
        .zip(ctx.recorded_import.as_ref())
    else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "import service not available",
        )
            .into_response();
    };

    if body.ids.is_empty() {
        return bad_request("ids must not be empty".into());
    }

    let service = FileIndexImportService::new(ctx.file_index_service.as_ref().clone());
    let mut identifier = identify_service.as_ref().clone();
    let mut importer = import_service.as_ref().clone();

    let results = match service
        .import_from_fingerprints(&body.ids, &mut identifier, &mut importer, recorded_import)
        .await
    {
        Ok(results) => results,
        Err(err) => return app_error_to_response(err),
    };

    json_response(StatusCode::OK, &ImportFilesResponse { results })
}

fn json_response<T: Serialize>(status: StatusCode, body: &T) -> Response {
    let bytes = match serde_json::to_vec(body) {
        Ok(value) => value,
        Err(err) => {
            return app_error_to_response(AppError::Internal(format!(
                "failed to serialize response: {err}"
            )));
        }
    };
    (status, [(header::CONTENT_TYPE, "application/json")], bytes).into_response()
}

fn bad_request(message: String) -> Response {
    (StatusCode::BAD_REQUEST, message).into_response()
}

fn app_error_to_response(err: AppError) -> Response {
    let status = match &err {
        AppError::InvalidParameter(_) => StatusCode::BAD_REQUEST,
        AppError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, err.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use chrono::TimeZone;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};
    use tower::ServiceExt;

    use crate::{
        application::{
            file_index::FileIndexService,
            ports::{FileIndexRecordInput, ImportRecordCreate, ImportRecordFinalize},
        },
        infrastructure::repo::file_index::SeaOrmFileIndexRepository,
    };

    use super::*;

    async fn fresh_db() -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    async fn fresh_repo() -> SeaOrmImportRecordRepository {
        SeaOrmImportRecordRepository::new(fresh_db().await)
    }

    async fn fresh_file_repo() -> SeaOrmFileIndexRepository {
        SeaOrmFileIndexRepository::new(fresh_db().await)
    }

    async fn seed_record(
        repo: &SeaOrmImportRecordRepository,
        seconds: i64,
        terminal_status: Option<ImportStatus>,
        error: Option<(&str, &str)>,
    ) -> i64 {
        let id = repo
            .create(&ImportRecordCreate {
                source_kind: ImportSourceKind::Pan189,
                source: format!("https://cloud.189.cn/t/{seconds}"),
                created_at: Utc.timestamp_opt(seconds, 0).unwrap(),
            })
            .await
            .unwrap();
        if let Some(status) = terminal_status {
            let summary = RecordSummary {
                items: vec![SummaryItem::Movie {
                    title: "TestMovie".into(),
                    year: "2024".into(),
                    size: 1_000_000_000,
                    cost_ms: 5000,
                    succeeded: true,
                }],
                total_size: 1_000_000_000,
                total_cost_ms: 5000,
                skipped_files: vec![],
            };
            repo.finalize(
                id,
                &ImportRecordFinalize {
                    status,
                    summary_json: serde_json::to_string(&summary).unwrap(),
                    error_kind: error.map(|e| e.0.to_owned()),
                    error_message: error.map(|e| e.1.to_owned()),
                    finished_at: Utc.timestamp_opt(seconds + 100, 0).unwrap(),
                },
            )
            .await
            .unwrap();
        }
        id
    }

    async fn body_string(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        serde_json::from_str(&body_string(response).await).unwrap()
    }

    async fn router_with(repo: SeaOrmImportRecordRepository) -> Router {
        let file_service = FileIndexService::new(fresh_file_repo().await);
        new_router(ConsoleContext::new_without_import(repo, file_service))
    }

    async fn router_with_files(file_repo: SeaOrmFileIndexRepository) -> Router {
        let import_repo = fresh_repo().await;
        let file_service = FileIndexService::new(file_repo);
        new_router(ConsoleContext::new_without_import(
            import_repo,
            file_service,
        ))
    }

    fn file_input(file_name: &str, hash: &str) -> FileIndexRecordInput {
        FileIndexRecordInput {
            size: 1024,
            hash_type: "md5".into(),
            hash_value: hash.into(),
            file_name: file_name.into(),
            file_path: "/Movies".into(),
            description: Some("from share xyz".into()),
        }
    }

    fn request(uri: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn list_returns_records_as_json_newest_first() {
        let repo = fresh_repo().await;
        seed_record(&repo, 1_700_000_000, Some(ImportStatus::Succeeded), None).await;
        seed_record(&repo, 1_700_000_100, Some(ImportStatus::Failed), None).await;
        let router = router_with(repo).await;

        let response = router.oneshot(request("/api/imports")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["status"], "failed");
        assert_eq!(items[1]["status"], "succeeded");
        // list returns flat fields, not nested summary
        assert!(items[0]["title"].is_string());
        assert!(items[0]["cost_ms"].is_number());
        assert!(items[0]["summary"].is_null());
    }

    #[tokio::test]
    async fn list_filters_by_status() {
        let repo = fresh_repo().await;
        seed_record(&repo, 1_700_000_000, Some(ImportStatus::Succeeded), None).await;
        seed_record(&repo, 1_700_000_100, Some(ImportStatus::Failed), None).await;
        let router = router_with(repo).await;

        let response = router
            .oneshot(request("/api/imports?status=failed"))
            .await
            .unwrap();
        let body = json_body(response).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["status"], "failed");
    }

    #[tokio::test]
    async fn list_rejects_unknown_status_with_400() {
        let repo = fresh_repo().await;
        let router = router_with(repo).await;
        let response = router
            .oneshot(request("/api/imports?status=mystery"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_detail_returns_404_when_missing() {
        let repo = fresh_repo().await;
        let router = router_with(repo).await;
        let response = router.oneshot(request("/api/imports/9999")).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_detail_returns_record_with_parsed_summary() {
        let repo = fresh_repo().await;
        let id = seed_record(&repo, 1_700_000_000, Some(ImportStatus::Succeeded), None).await;
        let router = router_with(repo).await;

        let response = router
            .oneshot(request(&format!("/api/imports/{id}")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["id"], id);
        assert_eq!(body["status"], "succeeded");
        assert!(body["summary"].is_object());
    }

    #[tokio::test]
    async fn get_detail_returns_error_block_when_record_had_failure() {
        let repo = fresh_repo().await;
        let id = seed_record(
            &repo,
            1_700_000_000,
            Some(ImportStatus::Failed),
            Some(("network", "upstream timeout")),
        )
        .await;
        let router = router_with(repo).await;

        let response = router
            .oneshot(request(&format!("/api/imports/{id}")))
            .await
            .unwrap();
        let body = json_body(response).await;
        assert_eq!(body["error"]["kind"], "network");
        assert_eq!(body["error"]["message"], "upstream timeout");
    }

    #[tokio::test]
    async fn search_files_returns_matching_records_as_json() {
        let file_repo = fresh_file_repo().await;
        file_repo
            .record_files(&[file_input("movie.mkv", &"a".repeat(32))])
            .await
            .unwrap();
        let router = router_with_files(file_repo).await;

        let response = router.oneshot(request("/api/files?q=movie")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["hash_type"], "md5");
        assert_eq!(items[0]["size"], 1024);
        let locations = items[0]["locations"].as_array().unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0]["file_name"], "movie.mkv");
        assert_eq!(locations[0]["file_path"], "/Movies");
        assert_eq!(locations[0]["descriptions"][0], "from share xyz");
    }

    #[tokio::test]
    async fn search_files_returns_empty_items_when_no_match() {
        let file_repo = fresh_file_repo().await;
        file_repo
            .record_files(&[file_input("movie.mkv", &"a".repeat(32))])
            .await
            .unwrap();
        let router = router_with_files(file_repo).await;

        let response = router
            .oneshot(request("/api/files?q=somethingnotpresent"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn search_files_without_query_returns_empty_items_not_400() {
        let file_repo = fresh_file_repo().await;
        file_repo
            .record_files(&[file_input("movie.mkv", &"a".repeat(32))])
            .await
            .unwrap();
        let router = router_with_files(file_repo).await;

        let response = router.oneshot(request("/api/files")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn search_files_clamps_oversized_limit_to_max() {
        let file_repo = fresh_file_repo().await;
        let inputs: Vec<_> = (0..(MAX_LIMIT + 5))
            .map(|i| FileIndexRecordInput {
                size: 100 + i,
                hash_type: "md5".into(),
                hash_value: format!("{i:032x}"),
                file_name: "movie.mkv".into(),
                file_path: "/Movies".into(),
                description: None,
            })
            .collect();
        file_repo.record_files(&inputs).await.unwrap();
        let router = router_with_files(file_repo).await;

        let response = router
            .oneshot(request("/api/files?q=movie&limit=99999"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), MAX_LIMIT as usize);
    }

    #[tokio::test]
    async fn search_files_respects_limit_param() {
        let file_repo = fresh_file_repo().await;
        let inputs: Vec<_> = (0..3)
            .map(|i| FileIndexRecordInput {
                size: 100 + i,
                hash_type: "md5".into(),
                hash_value: format!("{i:032x}"),
                file_name: format!("movie-{i}.mkv"),
                file_path: "/Movies".into(),
                description: None,
            })
            .collect();
        file_repo.record_files(&inputs).await.unwrap();
        let router = router_with_files(file_repo).await;

        let response = router
            .oneshot(request("/api/files?q=movie&limit=1"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn import_files_returns_503_without_import_service() {
        let router = router_with(fresh_repo().await).await;

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/files/import")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"ids":[1]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn import_files_returns_503_for_empty_ids_without_import_service() {
        let router = router_with(fresh_repo().await).await;

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/files/import")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"ids":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
