use std::sync::Arc;

use axum::{
    Router,
    extract::{OriginalUri, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};

use crate::{
    application::{
        delete_media::DeleteMediaService,
        file_index::FileIndexService,
        file_index_import::{FileIndexImportService, ImportFileResult},
        ports::{
            CommunityCatalogHandle, FileLocationRecord, FileSearchRecord, ImportRecordFilter,
            ImportRecordPage, ImportRecordPaging, ImportRecordRepository, ImportRecordView,
            ShareResolverHandle,
        },
        recorded_import::RecordedImportService,
    },
    domain::import_record::{
        EpisodeOutcome, ImportSourceKind, ImportStatus, RecordSummary, SummaryItem,
    },
    error::AppError,
    infrastructure::repo::{
        import_record::SeaOrmImportRecordRepository, subscription::SeaOrmSubscriptionRepository,
    },
    interface::http::console_assets::{AssetFile, resolve_asset},
    interface::import::format_episodes,
    interface::runtime::{IdentifyService, ImportService, SubscriptionService},
};

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

#[derive(Embed)]
#[folder = "web/dist"]
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
    pub(super) repo: Arc<SeaOrmImportRecordRepository>,
    pub(super) file_index_service: Arc<FileIndexService>,
    pub(super) import_service: Option<Arc<ImportService>>,
    pub(super) identify_service: Option<Arc<IdentifyService>>,
    pub(super) recorded_import: Option<Arc<RecordedImportService>>,
    pub(super) subscription_service: Option<Arc<SubscriptionService>>,
    pub(super) subscription_repo: Option<Arc<SeaOrmSubscriptionRepository>>,
    pub(super) delete_media_service: Option<Arc<DeleteMediaService>>,
    pub(super) community_catalog: Option<CommunityCatalogHandle>,
    pub(super) share_resolver: Option<ShareResolverHandle>,
}

impl ConsoleContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        repo: SeaOrmImportRecordRepository,
        file_index_service: FileIndexService,
        import_service: ImportService,
        identify_service: IdentifyService,
        subscription_service: SubscriptionService,
        subscription_repo: SeaOrmSubscriptionRepository,
        delete_media_service: DeleteMediaService,
        community_catalog: CommunityCatalogHandle,
        share_resolver: ShareResolverHandle,
    ) -> Self {
        let repo = Arc::new(repo);
        let recorded_import = Arc::new(RecordedImportService::new(repo.as_ref().clone()));
        Self {
            repo,
            file_index_service: Arc::new(file_index_service),
            import_service: Some(Arc::new(import_service)),
            identify_service: Some(Arc::new(identify_service)),
            recorded_import: Some(recorded_import),
            subscription_service: Some(Arc::new(subscription_service)),
            subscription_repo: Some(Arc::new(subscription_repo)),
            delete_media_service: Some(Arc::new(delete_media_service)),
            community_catalog: Some(community_catalog),
            share_resolver: Some(share_resolver),
        }
    }

    #[cfg(test)]
    pub(super) fn new_without_import(
        repo: SeaOrmImportRecordRepository,
        file_index_service: FileIndexService,
    ) -> Self {
        Self {
            repo: Arc::new(repo),
            file_index_service: Arc::new(file_index_service),
            import_service: None,
            identify_service: None,
            recorded_import: None,
            subscription_service: None,
            subscription_repo: None,
            delete_media_service: None,
            community_catalog: None,
            share_resolver: None,
        }
    }

    #[cfg(test)]
    fn new_with_subscription(
        repo: SeaOrmImportRecordRepository,
        file_index_service: FileIndexService,
        subscription_service: SubscriptionService,
        subscription_repo: SeaOrmSubscriptionRepository,
    ) -> Self {
        Self {
            repo: Arc::new(repo),
            file_index_service: Arc::new(file_index_service),
            import_service: None,
            identify_service: None,
            recorded_import: None,
            subscription_service: Some(Arc::new(subscription_service)),
            subscription_repo: Some(Arc::new(subscription_repo)),
            delete_media_service: None,
            community_catalog: None,
            share_resolver: None,
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_delete_media(
        repo: SeaOrmImportRecordRepository,
        file_index_service: FileIndexService,
        delete_media_service: DeleteMediaService,
    ) -> Self {
        Self {
            repo: Arc::new(repo),
            file_index_service: Arc::new(file_index_service),
            import_service: None,
            identify_service: None,
            recorded_import: None,
            subscription_service: None,
            subscription_repo: None,
            delete_media_service: Some(Arc::new(delete_media_service)),
            community_catalog: None,
            share_resolver: None,
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_catalog(
        repo: SeaOrmImportRecordRepository,
        file_index_service: FileIndexService,
        community_catalog: CommunityCatalogHandle,
    ) -> Self {
        let mut ctx = Self::new_without_import(repo, file_index_service);
        ctx.community_catalog = Some(community_catalog);
        ctx
    }
}

pub(crate) fn new_router(ctx: ConsoleContext) -> Router {
    use super::media_dirs;
    use super::subscription as sub;
    Router::new()
        .route("/api/imports", get(list_imports))
        .route("/api/imports/{id}", get(get_import))
        .route("/api/files", get(search_files))
        .route("/api/files/import", post(import_files))
        .route(
            "/api/community/threads",
            get(super::community::search_threads),
        )
        .route(
            "/api/community/threads/import",
            post(super::community::import_threads),
        )
        .route(
            "/api/subscriptions",
            get(sub::list_subscriptions).post(sub::create_subscription),
        )
        .route("/api/subscriptions/candidates", get(sub::search_candidates))
        .route("/api/subscriptions/{id}", delete(sub::delete_subscription))
        .route(
            "/api/subscriptions/{id}/rescan",
            post(sub::rescan_subscription),
        )
        .route("/api/media-dirs", get(media_dirs::list_media_dirs))
        .route(
            "/api/media-dirs/delete",
            post(media_dirs::delete_media_dirs),
        )
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

async fn search_files_with_service(service: &FileIndexService, query: SearchQuery) -> Response {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ImportRecordErrorJson>,
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

const LIST_ERROR_MESSAGE_MAX_CHARS: usize = 160;

fn parse_summary(view: &ImportRecordView) -> Option<RecordSummary> {
    view.summary_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
}

fn tv_episode_summary(episodes: &[EpisodeOutcome]) -> Option<String> {
    let succeeded: Vec<u32> = episodes
        .iter()
        .filter(|episode| episode.succeeded)
        .map(|episode| episode.episode)
        .collect();
    let failed: Vec<u32> = episodes
        .iter()
        .filter(|episode| !episode.succeeded)
        .map(|episode| episode.episode)
        .collect();
    let succeeded_text = format_episodes(&succeeded);
    let failed_text = format_episodes(&failed);
    match (succeeded_text.is_empty(), failed_text.is_empty()) {
        (true, true) => None,
        (false, true) => Some(succeeded_text),
        (true, false) => Some(format!("{failed_text} 失败")),
        (false, false) => Some(format!("{succeeded_text} / {failed_text} 失败")),
    }
}

fn excerpt_error_message(message: &str) -> String {
    let first_line = message.lines().next().unwrap_or("").trim();
    if first_line.starts_with('<') {
        return String::new();
    }
    if first_line.chars().count() <= LIST_ERROR_MESSAGE_MAX_CHARS {
        first_line.to_owned()
    } else {
        let truncated: String = first_line
            .chars()
            .take(LIST_ERROR_MESSAGE_MAX_CHARS)
            .collect();
        format!("{truncated}...")
    }
}

fn list_error(view: &ImportRecordView) -> Option<ImportRecordErrorJson> {
    Some(ImportRecordErrorJson {
        kind: view.error_kind.clone()?,
        message: excerpt_error_message(view.error_message.as_deref().unwrap_or("")),
    })
}

fn list_item_to_json(view: &ImportRecordView) -> ListItemJson {
    let summary = parse_summary(view);

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
            ..
        }) => (
            name.clone(),
            year.clone(),
            Some(*season),
            tv_episode_summary(episodes),
        ),
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
        error: list_error(view),
    }
}

fn detail_to_json(view: &ImportRecordView) -> DetailJson {
    let summary = parse_summary(view);
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
    let identifier = identify_service.as_ref().clone();
    let importer = import_service.as_ref().clone();

    let results = match service
        .import_from_fingerprints(&body.ids, &identifier, &importer, recorded_import)
        .await
    {
        Ok(results) => results,
        Err(err) => return app_error_to_response(err),
    };

    json_response(StatusCode::OK, &ImportFilesResponse { results })
}

pub(super) fn json_response<T: Serialize>(status: StatusCode, body: &T) -> Response {
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

pub(super) fn app_error_to_response(err: AppError) -> Response {
    let status = match &err {
        AppError::InvalidParameter(_) => StatusCode::BAD_REQUEST,
        AppError::NotFound(_) => StatusCode::NOT_FOUND,
        AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, err.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use crate::migration::{Migrator, MigratorTrait};
    use axum::body::to_bytes;
    use chrono::TimeZone;
    use sea_orm::{ConnectOptions, Database};
    use tower::ServiceExt;

    use crate::{
        application::{
            file_index::FileIndexService,
            ports::{
                FileIndexRecordInput, FileIndexRepository, ImportRecordCreate, ImportRecordFinalize,
            },
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

    async fn seed_finalized(
        repo: &SeaOrmImportRecordRepository,
        seconds: i64,
        status: ImportStatus,
        summary_json: String,
        error: Option<(&str, &str)>,
    ) -> i64 {
        let id = repo
            .create(&ImportRecordCreate {
                source_kind: ImportSourceKind::Pan115,
                source: format!("https://115cdn.com/s/{seconds}"),
                created_at: Utc.timestamp_opt(seconds, 0).unwrap(),
            })
            .await
            .unwrap();
        repo.finalize(
            id,
            &ImportRecordFinalize {
                status,
                summary_json,
                error_kind: error.map(|e| e.0.to_owned()),
                error_message: error.map(|e| e.1.to_owned()),
                finished_at: Utc.timestamp_opt(seconds + 100, 0).unwrap(),
            },
        )
        .await
        .unwrap();
        id
    }

    fn episode(episode: u32, succeeded: bool) -> EpisodeOutcome {
        EpisodeOutcome { episode, succeeded }
    }

    fn tv_summary(
        name: &str,
        episodes: Vec<EpisodeOutcome>,
        missing_episodes: Vec<u32>,
    ) -> RecordSummary {
        RecordSummary {
            items: vec![SummaryItem::Tv {
                name: name.into(),
                year: "2025".into(),
                season: 1,
                episodes,
                missing_episodes,
                max_episode_number: 15,
                number_of_episodes: 20,
                total_size: 0,
                cost_ms: 106,
            }],
            total_size: 0,
            total_cost_ms: 106,
            skipped_files: vec![],
        }
    }

    async fn first_list_item(repo: SeaOrmImportRecordRepository) -> serde_json::Value {
        let router = router_with(repo).await;
        let response = router.oneshot(request("/api/imports")).await.unwrap();
        json_body(response).await["items"][0].clone()
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
    async fn list_tv_episode_summary_uses_this_run_not_missing_episodes() {
        let repo = fresh_repo().await;
        seed_finalized(
            &repo,
            1_700_000_000,
            ImportStatus::Succeeded,
            serde_json::to_string(&tv_summary(
                "入青云",
                vec![episode(15, true)],
                (1..14).collect(),
            ))
            .unwrap(),
            None,
        )
        .await;
        let item = first_list_item(repo).await;
        assert_eq!(item["title"], "入青云");
        assert_eq!(item["season"], 1);
        assert_eq!(item["episode_summary"], "E15");
        assert!(item["error"].is_null());
        let summary = item["episode_summary"].as_str().unwrap();
        assert!(!summary.contains('/'), "{summary}");
        assert!(!summary.contains("1/"), "{summary}");
    }

    #[tokio::test]
    async fn list_empty_tv_stays_succeeded_without_zero_of_n() {
        let repo = fresh_repo().await;
        seed_finalized(
            &repo,
            1_700_000_000,
            ImportStatus::Succeeded,
            serde_json::to_string(&tv_summary("欢迎回我的频道", vec![], (1..20).collect()))
                .unwrap(),
            None,
        )
        .await;
        let item = first_list_item(repo).await;
        assert_eq!(item["status"], "succeeded");
        assert_eq!(item["title"], "欢迎回我的频道");
        assert_eq!(item["season"], 1);
        assert!(item["episode_summary"].is_null());
        assert!(item["error"].is_null());
    }

    #[tokio::test]
    async fn list_failed_row_with_empty_summary_includes_error_excerpt() {
        let repo = fresh_repo().await;
        seed_finalized(
            &repo,
            1_700_000_000,
            ImportStatus::Failed,
            "{}".into(),
            Some((
                "internal",
                "internal error: error, api error, code: 200020, message: OpenAPI only",
            )),
        )
        .await;
        let item = first_list_item(repo).await;
        assert_eq!(item["title"], "");
        assert_eq!(item["error"]["kind"], "internal");
        assert_eq!(
            item["error"]["message"],
            "internal error: error, api error, code: 200020, message: OpenAPI only"
        );
        assert!(item["summary"].is_null());
    }

    #[tokio::test]
    async fn list_failed_tv_episodes_keep_title_and_mark_failures() {
        let repo = fresh_repo().await;
        seed_finalized(
            &repo,
            1_700_000_000,
            ImportStatus::Failed,
            serde_json::to_string(&tv_summary(
                "大唐迷雾",
                (1..=14).map(|n| episode(n, false)).collect(),
                vec![],
            ))
            .unwrap(),
            None,
        )
        .await;
        let item = first_list_item(repo).await;
        assert_eq!(item["title"], "大唐迷雾");
        assert_eq!(item["episode_summary"], "E01-E14 失败");
        assert!(item["error"].is_null());
    }

    #[tokio::test]
    async fn list_error_excerpt_drops_html_and_truncates() {
        let repo = fresh_repo().await;
        let long = "x".repeat(200);
        seed_finalized(
            &repo,
            1_700_000_000,
            ImportStatus::Failed,
            "{}".into(),
            Some(("internal", &format!("{long}\nsecond line"))),
        )
        .await;
        let item = first_list_item(repo).await;
        let message = item["error"]["message"].as_str().unwrap();
        assert_eq!(message.chars().count(), 163);
        assert!(message.ends_with("..."));
        assert!(!message.contains("second line"));

        let repo = fresh_repo().await;
        seed_finalized(
            &repo,
            1_700_000_100,
            ImportStatus::Failed,
            "{}".into(),
            Some(("internal", "<!DOCTYPE html>\n<html>")),
        )
        .await;
        let item = first_list_item(repo).await;
        assert_eq!(item["error"]["kind"], "internal");
        assert_eq!(item["error"]["message"], "");
    }

    #[tokio::test]
    async fn get_detail_does_not_truncate_error_message() {
        let repo = fresh_repo().await;
        let long = format!("{}\nsecond line", "x".repeat(200));
        let id = seed_finalized(
            &repo,
            1_700_000_000,
            ImportStatus::Failed,
            "{}".into(),
            Some(("internal", &long)),
        )
        .await;
        let router = router_with(repo).await;
        let response = router
            .oneshot(request(&format!("/api/imports/{id}")))
            .await
            .unwrap();
        let body = json_body(response).await;
        assert_eq!(body["error"]["message"], long);
    }

    #[test]
    fn tv_episode_summary_formats_mixed_outcomes() {
        assert_eq!(
            super::tv_episode_summary(&[episode(15, true), episode(16, false)]),
            Some("E15 / E16 失败".into())
        );
        assert_eq!(super::tv_episode_summary(&[]), None);
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
    async fn search_files_ranks_filename_phrase_hits_first() {
        let file_repo = fresh_file_repo().await;
        file_repo
            .record_files(&[
                FileIndexRecordInput {
                    size: 100,
                    hash_type: "md5".into(),
                    hash_value: "a".repeat(32),
                    file_name: "unrelated.mkv".into(),
                    file_path: "/Other".into(),
                    description: Some("Love Is Blind".into()),
                },
                FileIndexRecordInput {
                    size: 200,
                    hash_type: "md5".into(),
                    hash_value: "b".repeat(32),
                    file_name: "Love.Is.Blind.S09E11.mkv".into(),
                    file_path: "/Reality".into(),
                    description: Some("from share xyz".into()),
                },
            ])
            .await
            .unwrap();
        let router = router_with_files(file_repo).await;

        let response = router
            .oneshot(request("/api/files?q=Love+Is+Blind"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0]["locations"][0]["file_name"],
            "Love.Is.Blind.S09E11.mkv"
        );
        assert!(items[0].get("score").is_none());
        assert!(items[0].get("rank").is_none());
        assert!(items[0]["id"].is_number());
        assert!(items[0]["size"].is_number());
        assert!(items[0]["hash_type"].is_string());
        assert!(items[0]["hash_value"].is_string());
        assert!(items[0]["locations"].is_array());
    }

    #[tokio::test]
    async fn search_files_recalls_partial_query_tokens() {
        let file_repo = fresh_file_repo().await;
        file_repo
            .record_files(&[
                FileIndexRecordInput {
                    size: 100,
                    hash_type: "md5".into(),
                    hash_value: "a".repeat(32),
                    file_name: "The.Office.mkv".into(),
                    file_path: "/Sitcoms".into(),
                    description: None,
                },
                FileIndexRecordInput {
                    size: 200,
                    hash_type: "md5".into(),
                    hash_value: "b".repeat(32),
                    file_name: "Love.Is.Blind.S09E11.mkv".into(),
                    file_path: "/Reality".into(),
                    description: Some("from share xyz".into()),
                },
            ])
            .await
            .unwrap();
        let router = router_with_files(file_repo).await;

        let response = router
            .oneshot(request("/api/files?q=Love+Blind"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0]["locations"][0]["file_name"],
            "Love.Is.Blind.S09E11.mkv"
        );
        assert!(items[0].get("score").is_none());
        assert!(items[0].get("rank").is_none());
        assert!(items[0]["id"].is_number());
        assert!(items[0]["size"].is_number());
        assert!(items[0]["hash_type"].is_string());
        assert!(items[0]["hash_value"].is_string());
        assert!(items[0]["locations"].is_array());
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

    #[tokio::test]
    async fn subscription_list_returns_503_without_service() {
        let router = router_with(fresh_repo().await).await;

        let response = router.oneshot(request("/api/subscriptions")).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn subscription_create_returns_503_without_service() {
        let router = router_with(fresh_repo().await).await;

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/subscriptions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"tmdb_id":27205,"media_type":"movie","title_en":"Inception"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn subscription_delete_returns_503_without_service() {
        let router = router_with(fresh_repo().await).await;

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/api/subscriptions/1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn subscription_candidates_returns_503_without_service() {
        let router = router_with(fresh_repo().await).await;

        let response = router
            .oneshot(request("/api/subscriptions/candidates?query=test"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // --- Subscription success-path tests ---

    use crate::application::ports::SubscriptionRepository;
    use crate::application::subscription::manage::ManageSubscriptionsService;
    use crate::domain::subscription::SubscriptionMediaType;
    use crate::infrastructure::import::gateway::TmdbMetadataGateway;
    use crate::infrastructure::repo::subscription::SeaOrmSubscriptionRepository;

    async fn subscription_router_with_seeded() -> (Router, SeaOrmSubscriptionRepository) {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db.clone()));
        let sub_repo = SeaOrmSubscriptionRepository::new(db.clone());
        let tmdb =
            TmdbMetadataGateway::new(crate::infrastructure::client::tmdb::Client::new("test"));
        let sub_service = ManageSubscriptionsService::new(sub_repo.clone(), tmdb);
        sub_repo
            .create(&crate::application::ports::SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: Some("盗梦空间".into()),
                title_en: Some("Inception".into()),
                year: Some("2010".into()),
                poster_path: Some("/inception.jpg".into()),
                overview: Some("A thief who steals corporate secrets.".into()),
            })
            .await
            .unwrap();
        let router = new_router(ConsoleContext::new_with_subscription(
            repo,
            file_service,
            sub_service,
            sub_repo.clone(),
        ));
        (router, sub_repo)
    }

    #[tokio::test]
    async fn subscription_list_returns_seeded_item() {
        let (router, _repo) = subscription_router_with_seeded().await;
        let response = router.oneshot(request("/api/subscriptions")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let items = json["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["tmdb_id"].as_u64().unwrap(), 27205);
        assert_eq!(items[0]["media_type"].as_str().unwrap(), "movie");
        assert_eq!(items[0]["title_en"].as_str().unwrap(), "Inception");
        assert_eq!(items[0]["year"].as_str().unwrap(), "2010");
        assert_eq!(items[0]["poster_path"].as_str().unwrap(), "/inception.jpg");
        assert_eq!(
            items[0]["overview"].as_str().unwrap(),
            "A thief who steals corporate secrets."
        );
    }

    #[tokio::test]
    async fn subscription_create_then_list_returns_display_fields() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db.clone()));
        let sub_repo = SeaOrmSubscriptionRepository::new(db.clone());
        let tmdb =
            TmdbMetadataGateway::new(crate::infrastructure::client::tmdb::Client::new("test"));
        let sub_service = ManageSubscriptionsService::new(sub_repo.clone(), tmdb);
        let router = new_router(ConsoleContext::new_with_subscription(
            repo,
            file_service,
            sub_service,
            sub_repo,
        ));
        let create = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/subscriptions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"tmdb_id":1396,"media_type":"tv","title_zh":"绝命毒师","title_en":"Breaking Bad","year":"2008","poster_path":"/breaking-bad.jpg","overview":"A chemistry teacher turned meth maker."}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);

        let response = router.oneshot(request("/api/subscriptions")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let item = &json["items"][0];
        assert_eq!(item["tmdb_id"].as_u64().unwrap(), 1396);
        assert_eq!(item["media_type"].as_str().unwrap(), "tv");
        assert_eq!(item["year"].as_str().unwrap(), "2008");
        assert_eq!(item["poster_path"].as_str().unwrap(), "/breaking-bad.jpg");
        assert_eq!(
            item["overview"].as_str().unwrap(),
            "A chemistry teacher turned meth maker."
        );
    }

    #[tokio::test]
    async fn subscription_create_returns_201_with_valid_input() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db.clone()));
        let sub_repo = SeaOrmSubscriptionRepository::new(db.clone());
        let tmdb =
            TmdbMetadataGateway::new(crate::infrastructure::client::tmdb::Client::new("test"));
        let sub_service = ManageSubscriptionsService::new(sub_repo.clone(), tmdb);
        let router = new_router(ConsoleContext::new_with_subscription(
            repo,
            file_service,
            sub_service,
            sub_repo,
        ));
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/subscriptions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"tmdb_id":27205,"media_type":"movie","title_en":"Inception"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["id"].as_i64().unwrap() > 0);
    }

    #[tokio::test]
    async fn subscription_create_returns_400_with_empty_titles() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db.clone()));
        let sub_repo = SeaOrmSubscriptionRepository::new(db.clone());
        let tmdb =
            TmdbMetadataGateway::new(crate::infrastructure::client::tmdb::Client::new("test"));
        let sub_service = ManageSubscriptionsService::new(sub_repo.clone(), tmdb);
        let router = new_router(ConsoleContext::new_with_subscription(
            repo,
            file_service,
            sub_service,
            sub_repo,
        ));
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/subscriptions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"tmdb_id":1,"media_type":"movie"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn subscription_create_returns_400_with_invalid_media_type() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let file_service = FileIndexService::new(SeaOrmFileIndexRepository::new(db.clone()));
        let sub_repo = SeaOrmSubscriptionRepository::new(db.clone());
        let tmdb =
            TmdbMetadataGateway::new(crate::infrastructure::client::tmdb::Client::new("test"));
        let sub_service = ManageSubscriptionsService::new(sub_repo.clone(), tmdb);
        let router = new_router(ConsoleContext::new_with_subscription(
            repo,
            file_service,
            sub_service,
            sub_repo,
        ));
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/subscriptions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"tmdb_id":1,"media_type":"invalid","title_en":"Test"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn subscription_delete_returns_204() {
        let (router, _repo) = subscription_router_with_seeded().await;
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/api/subscriptions/1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn subscription_create_duplicate_returns_400() {
        let (router, _repo) = subscription_router_with_seeded().await;
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/subscriptions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"tmdb_id":27205,"media_type":"movie","title_en":"Inception"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
