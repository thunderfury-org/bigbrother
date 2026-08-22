use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::application::{community::CommunityService, ports::CommunityThread};

use super::console::{ConsoleContext, app_error_to_response, json_response};

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

#[derive(Debug, Default, Deserialize)]
pub(super) struct SearchQuery {
    q: Option<String>,
    limit: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CommunityThreadPageJson {
    items: Vec<CommunityThreadJson>,
}

#[derive(Debug, Serialize)]
struct CommunityThreadJson {
    tid: i64,
    title: String,
    tags: Vec<String>,
    author: String,
    posted_at: String,
    comments: u32,
    likes: u32,
    url: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ImportThreadsRequest {
    tids: Vec<i64>,
}

#[derive(Debug, Serialize)]
struct ImportThreadsResponse {
    results: Vec<crate::application::community::CommunityImportResult>,
}

pub(super) async fn search_threads(
    State(ctx): State<ConsoleContext>,
    Query(query): Query<SearchQuery>,
) -> Response {
    let Some(catalog) = ctx.community_catalog.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "community search is not available",
        )
            .into_response();
    };
    let keyword = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let service =
        CommunityService::from_handle(catalog.clone(), ctx.file_index_service.as_ref().clone());
    match service.search_threads(&keyword, limit).await {
        Ok(threads) => json_response(StatusCode::OK, &threads_to_json(threads)),
        Err(err) => app_error_to_response(err),
    }
}

pub(super) async fn import_threads(
    State(ctx): State<ConsoleContext>,
    axum::Json(body): axum::Json<ImportThreadsRequest>,
) -> Response {
    let Some(catalog) = ctx.community_catalog.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "community search is not available",
        )
            .into_response();
    };
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
    let Some(share_resolver) = ctx.share_resolver.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "share resolver not available",
        )
            .into_response();
    };
    if body.tids.is_empty() {
        return (StatusCode::BAD_REQUEST, "tids must not be empty").into_response();
    }

    let service =
        CommunityService::from_handle(catalog.clone(), ctx.file_index_service.as_ref().clone());
    match service
        .import_threads(
            &body.tids,
            share_resolver.as_ref(),
            identify_service.as_ref(),
            import_service.as_ref(),
            recorded_import.as_ref(),
        )
        .await
    {
        Ok(results) => json_response(StatusCode::OK, &ImportThreadsResponse { results }),
        Err(err) => app_error_to_response(err),
    }
}

fn threads_to_json(threads: Vec<CommunityThread>) -> CommunityThreadPageJson {
    CommunityThreadPageJson {
        items: threads
            .into_iter()
            .map(|thread| CommunityThreadJson {
                tid: thread.tid,
                title: thread.title,
                tags: thread.tags,
                author: thread.author,
                posted_at: thread.posted_at,
                comments: thread.comments,
                likes: thread.likes,
                url: thread.url,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            file_index::FileIndexService,
            ports::{CommunityCatalog, CommunityCatalogHandle, CommunityThreadShares},
        },
        error::AppResult,
        infrastructure::repo::{
            file_index::SeaOrmFileIndexRepository, import_record::SeaOrmImportRecordRepository,
        },
        interface::http::console::new_router,
        migration::{Migrator, MigratorTrait},
    };
    use axum::body::to_bytes;
    use sea_orm::{ConnectOptions, Database};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct FakeCatalog {
        threads: Vec<CommunityThread>,
    }

    #[async_trait::async_trait]
    impl CommunityCatalog for FakeCatalog {
        async fn search_threads(
            &self,
            keyword: &str,
            limit: u64,
        ) -> AppResult<Vec<CommunityThread>> {
            let items = self
                .threads
                .iter()
                .filter(|thread| thread.title.contains(keyword))
                .take(limit as usize)
                .cloned()
                .collect();
            Ok(items)
        }

        async fn share_urls_for_thread(&self, _tid: i64) -> AppResult<CommunityThreadShares> {
            unimplemented!()
        }
    }

    async fn fresh_db() -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    fn sample_thread() -> CommunityThread {
        CommunityThread {
            tid: 50570,
            title: "欧美剧 《黑镜 1-7季》中文字幕".into(),
            tags: vec!["1080p".into(), "完结".into()],
            author: "奶糖小兔".into(),
            posted_at: "2026-01-06 10:13".into(),
            comments: 104,
            likes: 1,
            url: "https://pan1.me/?thread-50570.htm".into(),
        }
    }

    #[tokio::test]
    async fn search_threads_returns_matching_json() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let files = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        let catalog: CommunityCatalogHandle = std::sync::Arc::new(FakeCatalog {
            threads: vec![sample_thread()],
        });
        let router = new_router(ConsoleContext::new_with_catalog(repo, files, catalog));

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/community/threads?q=%E9%BB%91%E9%95%9C")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["items"][0]["tid"], 50570);
        assert_eq!(body["items"][0]["title"], "欧美剧 《黑镜 1-7季》中文字幕");
    }

    #[tokio::test]
    async fn search_threads_returns_503_without_catalog() {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let files = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        let router = new_router(ConsoleContext::new_without_import(repo, files));
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/community/threads?q=test")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
