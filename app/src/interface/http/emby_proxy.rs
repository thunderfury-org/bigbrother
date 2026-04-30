use std::sync::Arc;

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
};
use reqwest::Url;

use crate::{
    application::{
        emby_proxy::BigbrotherStrmMatcher, resolve_download_url::ResolveDownloadUrlService,
    },
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub(crate) struct EmbyProxyContext<C, S> {
    upstream_base_url: Url,
    #[allow(dead_code)]
    api_key: Option<String>,
    client: reqwest::Client,
    #[allow(dead_code)]
    matcher: BigbrotherStrmMatcher,
    #[allow(dead_code)]
    resolver: Arc<ResolveDownloadUrlService<C, S>>,
}

impl<C, S> EmbyProxyContext<C, S> {
    pub(crate) fn new(
        upstream_base_url: String,
        api_key: Option<String>,
        advertise_base_url: String,
        strm_path_prefix: String,
        resolver: ResolveDownloadUrlService<C, S>,
    ) -> AppResult<Self> {
        let upstream_base_url =
            Url::parse(upstream_base_url.trim_end_matches('/')).map_err(|err| {
                AppError::InvalidParameter(format!("invalid emby upstream url: {err}"))
            })?;

        Ok(Self {
            upstream_base_url,
            api_key,
            client: reqwest::Client::new(),
            matcher: BigbrotherStrmMatcher::new(advertise_base_url, strm_path_prefix),
            resolver: Arc::new(resolver),
        })
    }
}

pub(crate) fn new_router<C, S>(ctx: EmbyProxyContext<C, S>) -> Router
where
    C: crate::application::ports::DownloadUrlCache + Clone + Send + Sync + 'static,
    S: crate::application::ports::DownloadUrlSource + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/{*path}", any(proxy_handler::<C, S>))
        .route("/", any(proxy_root_handler::<C, S>))
        .with_state(ctx)
}

async fn proxy_root_handler<C, S>(
    State(ctx): State<EmbyProxyContext<C, S>>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response
where
    C: crate::application::ports::DownloadUrlCache + Clone + Send + Sync + 'static,
    S: crate::application::ports::DownloadUrlSource + Clone + Send + Sync + 'static,
{
    proxy_request(&ctx, method, headers, uri, body, "").await
}

async fn proxy_handler<C, S>(
    State(ctx): State<EmbyProxyContext<C, S>>,
    Path(path): Path<String>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response
where
    C: crate::application::ports::DownloadUrlCache + Clone + Send + Sync + 'static,
    S: crate::application::ports::DownloadUrlSource + Clone + Send + Sync + 'static,
{
    proxy_request(&ctx, method, headers, uri, body, path.as_str()).await
}

async fn proxy_request<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
    path: &str,
) -> Response {
    match forward(ctx, method, headers, uri, body, path).await {
        Ok(response) => response,
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}

async fn forward<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
    path: &str,
) -> AppResult<Response> {
    let mut upstream = ctx.upstream_base_url.clone();
    upstream.set_path(format!("/{path}").as_str());
    upstream.set_query(uri.query());

    let mut request = ctx.client.request(method, upstream);
    for (name, value) in headers.iter() {
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        request = request.header(name, value);
    }

    let upstream_response =
        request.body(body).send().await.map_err(|err| {
            AppError::Dependency(format!("failed to proxy request to emby: {err}"))
        })?;
    response_from_reqwest(upstream_response).await
}

async fn response_from_reqwest(upstream_response: reqwest::Response) -> AppResult<Response> {
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let body = upstream_response
        .bytes()
        .await
        .map_err(|err| AppError::Dependency(format!("failed to read emby response body: {err}")))?;

    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if name.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(Body::from(body))
        .map_err(|err| AppError::Internal(format!("failed to build proxy response: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[tokio::test]
    async fn transparent_proxy_forwards_root_paths() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info"))
            .respond_with(ResponseTemplate::new(200).set_body_string("emby-ok"))
            .mount(&upstream)
            .await;

        let ctx = EmbyProxyContext::new(
            upstream.uri(),
            None,
            "http://bb.example:3100".to_string(),
            "/d".to_string(),
            fake_resolver(),
        )
        .unwrap();
        let app = new_router(ctx);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let response = reqwest::get(format!("http://{addr}/System/Info"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "emby-ok");

        server.abort();
    }

    use crate::{
        application::ports::{DownloadUrlCache, DownloadUrlResult, DownloadUrlSource},
        error::AppResult,
    };
    use std::time::Duration;

    #[derive(Clone)]
    struct FakeCache;

    impl DownloadUrlCache for FakeCache {
        async fn get_download_url(&self, _key: &str) -> AppResult<Option<String>> {
            Ok(None)
        }

        async fn set_download_url(&self, _key: &str, _url: &str, _ttl: Duration) -> AppResult<()> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeSource;

    impl DownloadUrlSource for FakeSource {
        async fn get_download_url(&self, _file_id: i64) -> DownloadUrlResult<String> {
            Ok("https://download.example/video.mkv".to_string())
        }
    }

    fn fake_resolver()
    -> crate::application::resolve_download_url::ResolveDownloadUrlService<FakeCache, FakeSource>
    {
        crate::application::resolve_download_url::ResolveDownloadUrlService::new(
            FakeCache, FakeSource,
        )
    }
}
