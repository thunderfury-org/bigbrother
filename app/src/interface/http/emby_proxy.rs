// This module is wired into runtime in Task 7, when the optional Emby proxy is
// exposed from configuration. Keep the Task 4 skeleton lint-clean until then.
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, HeaderName, Method, StatusCode, Uri},
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
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .build()
            .map_err(|err| {
                AppError::Internal(format!("failed to build emby proxy client: {err}"))
            })?;

        Ok(Self {
            upstream_base_url,
            api_key,
            client,
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
    proxy_request(&ctx, method, headers, uri, body).await
}

async fn proxy_handler<C, S>(
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
    proxy_request(&ctx, method, headers, uri, body).await
}

async fn proxy_request<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    match forward(ctx, method, headers, uri, body).await {
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
) -> AppResult<Response> {
    let mut upstream = ctx.upstream_base_url.clone();
    let path_and_query = uri
        .path_and_query()
        .map_or(uri.path(), |value| value.as_str());
    upstream = upstream.join(path_and_query).map_err(|err| {
        AppError::InvalidParameter(format!("invalid proxied emby request uri: {err}"))
    })?;

    let mut request = ctx.client.request(method, upstream);
    for (name, value) in headers.iter() {
        if !should_forward_request_header(name) {
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
    let body = Body::from_stream(upstream_response.bytes_stream());

    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if !should_forward_response_header(name) {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(body)
        .map_err(|err| AppError::Internal(format!("failed to build proxy response: {err}")))
}

fn should_forward_request_header(name: &HeaderName) -> bool {
    !is_hop_by_hop_header(name) && !name.as_str().eq_ignore_ascii_case("host")
}

fn should_forward_response_header(name: &HeaderName) -> bool {
    !is_hop_by_hop_header(name) && !name.as_str().eq_ignore_ascii_case("content-length")
}

fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string, header, method, path, query_param},
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

    #[tokio::test]
    async fn transparent_proxy_preserves_encoded_path_segments() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Videos/a%2Fb/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_string("encoded-ok"))
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

        let response = reqwest::get(format!("http://{addr}/Videos/a%2Fb/stream"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "encoded-ok");

        server.abort();
    }

    #[tokio::test]
    async fn transparent_proxy_forwards_request_and_response_parts() {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/Items/42"))
            .and(query_param("api_key", "secret"))
            .and(query_param("format", "json"))
            .and(header("x-emby-client", "bigbrother-test"))
            .and(body_string(r#"{"name":"movie"}"#))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("x-emby-result", "created")
                    .set_body_string("created-ok"),
            )
            .mount(&upstream)
            .await;

        let addr = spawn_proxy(upstream.uri()).await;
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/Items/42?api_key=secret&format=json"))
            .header("host", "bb.example")
            .header("x-emby-client", "bigbrother-test")
            .body(r#"{"name":"movie"}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.headers().get("x-emby-result").unwrap(), "created");
        assert_eq!(response.text().await.unwrap(), "created-ok");
    }

    #[tokio::test]
    async fn transparent_proxy_maps_upstream_connection_errors_to_bad_gateway() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let addr = spawn_proxy(upstream).await;
        let response = reqwest::get(format!("http://{addr}/System/Info"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn transparent_proxy_preserves_upstream_redirects() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/redirect-me"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "/target")
                    .set_body_string("redirecting"),
            )
            .mount(&upstream)
            .await;

        let addr = spawn_proxy(upstream.uri()).await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let response = client
            .get(format!("http://{addr}/redirect-me"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(response.headers().get("location").unwrap(), "/target");
        assert_eq!(response.text().await.unwrap(), "redirecting");
    }

    async fn spawn_proxy(upstream_uri: String) -> std::net::SocketAddr {
        let ctx = EmbyProxyContext::new(
            upstream_uri,
            None,
            "http://bb.example:3100".to_string(),
            "/d".to_string(),
            fake_resolver(),
        )
        .unwrap();
        let app = new_router(ctx);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
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
