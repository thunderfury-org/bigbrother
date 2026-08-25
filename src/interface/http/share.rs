use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    application::share_import::ShareImportService, interface::import::source_for_share_url,
};

use super::console::{ConsoleContext, app_error_to_response, json_response};

#[derive(Debug, Deserialize)]
pub(super) struct ImportShareRequest {
    url: String,
    #[serde(default)]
    description: Option<String>,
}

pub(super) async fn import_share(
    State(ctx): State<ConsoleContext>,
    axum::Json(body): axum::Json<ImportShareRequest>,
) -> Response {
    let url = body.url.trim();
    if url.is_empty() {
        return (StatusCode::BAD_REQUEST, "url must not be empty").into_response();
    }

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

    let service = ShareImportService::new(ctx.file_index_service.as_ref().clone());
    match service
        .import_url(
            source_for_share_url(url),
            body.description,
            share_resolver.as_ref(),
            identify_service.as_ref(),
            import_service.as_ref(),
            recorded_import.as_ref(),
        )
        .await
    {
        Ok(result) => json_response(StatusCode::OK, &result),
        Err(err) => app_error_to_response(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::file_index::FileIndexService,
        infrastructure::repo::{
            file_index::SeaOrmFileIndexRepository, import_record::SeaOrmImportRecordRepository,
        },
        interface::http::console::new_router,
        migration::{Migrator, MigratorTrait},
    };
    use axum::body::to_bytes;
    use sea_orm::{ConnectOptions, Database};
    use tower::ServiceExt;

    async fn fresh_db() -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    async fn router_without_import() -> axum::Router {
        let db = fresh_db().await;
        let repo = SeaOrmImportRecordRepository::new(db.clone());
        let files = FileIndexService::new(SeaOrmFileIndexRepository::new(db));
        new_router(ConsoleContext::new_without_import(repo, files))
    }

    fn post_json(uri: &str, body: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn import_share_returns_400_for_blank_url() {
        let router = router_without_import().await;
        let response = router
            .oneshot(post_json("/api/shares/import", r#"{"url":"  "}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(bytes.as_ref(), b"url must not be empty");
    }

    #[tokio::test]
    async fn import_share_returns_503_without_import_service() {
        let router = router_without_import().await;
        let response = router
            .oneshot(post_json(
                "/api/shares/import",
                r#"{"url":"https://www.123684.com/s/share-key?pwd=pass"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(bytes.as_ref(), b"import service not available");
    }
}
