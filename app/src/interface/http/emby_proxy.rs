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
    if is_playback_info_route(&method, uri.path()) {
        return proxy_playback_info(ctx, method, headers, uri, body).await;
    }

    proxy_request_fallback(ctx, method, headers, uri, body).await
}

async fn proxy_request_fallback<C, S>(
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

fn is_playback_info_route(method: &Method, path: &str) -> bool {
    (*method == Method::GET || *method == Method::POST) && playback_item_id(path).is_some()
}

fn playback_item_id(path: &str) -> Option<&str> {
    path.strip_prefix("/Items/")
        .and_then(|rest| rest.strip_suffix("/PlaybackInfo"))
}

async fn proxy_playback_info<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    match forward_raw(ctx, method, headers, uri.clone(), body).await {
        Ok((status, response_headers, response_body)) => {
            if !status.is_success() {
                return build_response(status, response_headers, response_body);
            }

            let Some(item_id) = playback_item_id(uri.path()) else {
                return build_response(status, response_headers, response_body);
            };

            let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&response_body) else {
                return build_response(status, response_headers, response_body);
            };

            if !crate::application::emby_proxy::rewrite_playback_info(
                &mut json,
                item_id,
                &ctx.matcher,
            ) {
                return build_response(status, response_headers, response_body);
            }

            match serde_json::to_vec(&json) {
                Ok(body) => {
                    let mut headers = response_headers;
                    headers.remove(axum::http::header::CONTENT_LENGTH);
                    headers.insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("application/json"),
                    );
                    build_response(status, headers, Bytes::from(body))
                }
                Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
            }
        }
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
    let upstream_response = send_upstream_request(ctx, method, headers, uri, body).await?;
    response_from_reqwest(upstream_response).await
}

async fn forward_raw<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> AppResult<(StatusCode, HeaderMap, Bytes)> {
    let upstream_response = send_upstream_request(ctx, method, headers, uri, body).await?;
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let body = upstream_response
        .bytes()
        .await
        .map_err(|err| AppError::Dependency(format!("failed to read emby response body: {err}")))?;

    Ok((status, headers, body))
}

async fn send_upstream_request<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> AppResult<reqwest::Response> {
    let upstream = build_upstream_url(&ctx.upstream_base_url, &uri)?;

    let mut request = ctx.client.request(method, upstream);
    let connection_headers = connection_header_tokens(&headers);
    for (name, value) in headers.iter() {
        if !should_forward_request_header(name, connection_headers.as_slice()) {
            continue;
        }
        request = request.header(name, value);
    }

    request
        .body(body)
        .send()
        .await
        .map_err(|err| AppError::Dependency(format!("failed to proxy request to emby: {err}")))
}

async fn response_from_reqwest(upstream_response: reqwest::Response) -> AppResult<Response> {
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let connection_headers = connection_header_tokens(&headers);
    let body = Body::from_stream(upstream_response.bytes_stream());

    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if !should_forward_response_header(name, connection_headers.as_slice()) {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(body)
        .map_err(|err| AppError::Internal(format!("failed to build proxy response: {err}")))
}

fn build_response(status: StatusCode, headers: HeaderMap, body: Bytes) -> Response {
    let connection_headers = connection_header_tokens(&headers);

    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if !should_forward_response_header(name, connection_headers.as_slice()) {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(Body::from(body))
        .unwrap_or_else(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())
}

fn build_upstream_url(upstream_base_url: &Url, uri: &Uri) -> AppResult<Url> {
    let mut raw = upstream_base_url.origin().ascii_serialization();
    raw.push_str(uri.path());
    if let Some(query) = uri.query() {
        raw.push('?');
        raw.push_str(query);
    }

    Url::parse(raw.as_str()).map_err(|err| {
        AppError::InvalidParameter(format!("invalid proxied emby request uri: {err}"))
    })
}

fn should_forward_request_header(name: &HeaderName, connection_headers: &[String]) -> bool {
    !is_hop_by_hop_header(name)
        && !is_connection_header_token(name, connection_headers)
        && !name.as_str().eq_ignore_ascii_case("host")
}

fn should_forward_response_header(name: &HeaderName, connection_headers: &[String]) -> bool {
    !is_hop_by_hop_header(name)
        && !is_connection_header_token(name, connection_headers)
        && !name.as_str().eq_ignore_ascii_case("content-length")
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

fn is_connection_header_token(name: &HeaderName, connection_headers: &[String]) -> bool {
    connection_headers
        .iter()
        .any(|header| name.as_str().eq_ignore_ascii_case(header.as_str()))
}

fn connection_header_tokens(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all("connection")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use wiremock::{
        Match, Mock, MockServer, Request, ResponseTemplate,
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
    async fn transparent_proxy_keeps_double_slash_paths_on_configured_upstream() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("//evil.example/System/Info"))
            .respond_with(ResponseTemplate::new(200).set_body_string("still-upstream"))
            .mount(&upstream)
            .await;

        let addr = spawn_proxy(upstream.uri()).await;
        let response = reqwest::get(format!("http://{addr}//evil.example/System/Info"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "still-upstream");
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

    #[tokio::test]
    async fn transparent_proxy_drops_request_headers_named_by_connection() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info"))
            .and(HeaderAbsent("connection"))
            .and(HeaderAbsent("x-internal"))
            .respond_with(ResponseTemplate::new(200).set_body_string("filtered"))
            .mount(&upstream)
            .await;

        let addr = spawn_proxy(upstream.uri()).await;
        let response = reqwest::Client::new()
            .get(format!("http://{addr}/System/Info"))
            .header("connection", "x-internal")
            .header("x-internal", "secret")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "filtered");
    }

    #[tokio::test]
    async fn transparent_proxy_drops_response_headers_named_by_connection() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("connection", "x-internal")
                    .insert_header("x-internal", "secret")
                    .set_body_string("filtered"),
            )
            .mount(&upstream)
            .await;

        let addr = spawn_proxy(upstream.uri()).await;
        let response = reqwest::get(format!("http://{addr}/System/Info"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get("connection").is_none());
        assert!(response.headers().get("x-internal").is_none());
        assert_eq!(response.text().await.unwrap(), "filtered");
    }

    #[tokio::test]
    async fn playback_info_response_is_rewritten_for_bigbrother_strm() {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/Items/7/PlaybackInfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "MediaSources": [{
                    "Id": "mediasource_42",
                    "ItemId": "7",
                    "Path": "http://bb.example:3100/d/movies/Inception.mkv?file_id=42",
                    "DirectStreamUrl": "/Videos/7/stream?MediaSourceId=mediasource_42&X-Emby-Token=token",
                    "SupportsDirectPlay": false,
                    "SupportsTranscoding": true,
                    "TranscodingUrl": "/Videos/7/master.m3u8"
                }]
            })))
            .mount(&upstream)
            .await;

        let addr = spawn_proxy(upstream.uri()).await;
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/Items/7/PlaybackInfo"))
            .send()
            .await
            .unwrap();
        let json: serde_json::Value = response.json().await.unwrap();

        assert_eq!(json["MediaSources"][0]["SupportsDirectPlay"], true);
        assert_eq!(json["MediaSources"][0]["SupportsTranscoding"], false);
        assert_eq!(
            json["MediaSources"][0]["DirectStreamUrl"],
            "/Videos/7/stream?MediaSourceId=mediasource_42&Static=true&X-Emby-Token=token"
        );
        assert!(json["MediaSources"][0].get("TranscodingUrl").is_none());
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

    struct HeaderAbsent(&'static str);

    impl Match for HeaderAbsent {
        fn matches(&self, request: &Request) -> bool {
            !request.headers.contains_key(self.0)
        }
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
