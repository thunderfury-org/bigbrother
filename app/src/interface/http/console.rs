use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    application::ports::{
        ImportRecordFilter, ImportRecordPage, ImportRecordPaging, ImportRecordRepository,
        ImportRecordView,
    },
    domain::import_record::{ImportSourceKind, ImportStatus, RecordSummary},
    error::AppError,
    infrastructure::repo::import_record::SeaOrmImportRecordRepository,
};

const INDEX_HTML: &str = include_str!("./console_index.html");
const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

#[derive(Clone)]
pub(crate) struct ConsoleContext {
    repo: Arc<SeaOrmImportRecordRepository>,
}

impl ConsoleContext {
    pub(crate) fn new(repo: SeaOrmImportRecordRepository) -> Self {
        Self {
            repo: Arc::new(repo),
        }
    }
}

pub(crate) fn new_router(ctx: ConsoleContext) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/imports", get(index))
        .route("/api/imports", get(list_imports))
        .route("/api/imports/{id}", get(get_import))
        .with_state(ctx)
}

async fn index() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        INDEX_HTML,
    )
        .into_response()
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

async fn get_import(State(ctx): State<ConsoleContext>, Path(id): Path<i64>) -> Response {
    get_with_repo(ctx.repo.as_ref(), id).await
}

async fn get_with_repo<R: ImportRecordRepository>(repo: &R, id: i64) -> Response {
    match repo.get(id).await {
        Ok(Some(view)) => json_response(StatusCode::OK, &record_to_json(&view)),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(err) => app_error_to_response(err),
    }
}

#[derive(Debug, Serialize)]
struct ImportRecordJson {
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
    items: Vec<ImportRecordJson>,
    next_cursor: Option<i64>,
}

fn list_to_json(page: ImportRecordPage) -> ImportRecordPageJson {
    ImportRecordPageJson {
        items: page.items.iter().map(record_to_json).collect(),
        next_cursor: page.next_cursor,
    }
}

fn record_to_json(view: &ImportRecordView) -> ImportRecordJson {
    let summary = view
        .summary_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<RecordSummary>(raw).ok());
    let error = match (view.error_kind.as_ref(), view.error_message.as_ref()) {
        (Some(kind), Some(message)) => Some(ImportRecordErrorJson {
            kind: kind.clone(),
            message: message.clone(),
        }),
        (Some(kind), None) => Some(ImportRecordErrorJson {
            kind: kind.clone(),
            message: String::new(),
        }),
        _ => None,
    };
    ImportRecordJson {
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

    use crate::application::ports::{ImportRecordCreate, ImportRecordFinalize};

    use super::*;

    async fn fresh_repo() -> SeaOrmImportRecordRepository {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        SeaOrmImportRecordRepository::new(db)
    }

    async fn seed_record(
        repo: &SeaOrmImportRecordRepository,
        seconds: i64,
        terminal_status: Option<ImportStatus>,
        error: Option<(&str, &str)>,
    ) -> i64 {
        let id = repo
            .create(&ImportRecordCreate {
                source_kind: ImportSourceKind::Quark,
                source: format!("https://pan.quark.cn/s/{seconds}"),
                created_at: Utc.timestamp_opt(seconds, 0).unwrap(),
            })
            .await
            .unwrap();
        if let Some(status) = terminal_status {
            repo.finalize(
                id,
                &ImportRecordFinalize {
                    status,
                    summary_json: serde_json::to_string(&RecordSummary::default()).unwrap(),
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
        new_router(ConsoleContext::new(repo))
    }

    fn request(uri: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn get_root_serves_embedded_html() {
        let repo = fresh_repo().await;
        let router = router_with(repo).await;

        let response = router.oneshot(request("/")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            "text/html; charset=utf-8"
        );
        let body = body_string(response).await;
        assert!(body.contains("<html"));
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
}
