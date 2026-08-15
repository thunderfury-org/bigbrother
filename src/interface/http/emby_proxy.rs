use std::sync::Arc;

use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, HeaderName, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
};
use futures::future::BoxFuture;
use reqwest::Url;
use tracing::error;

use crate::{
    application::emby_proxy::BigbrotherStrmMatcher,
    error::{AppError, AppResult},
    interface::runtime::MediaDownloadUrlService,
};

pub(crate) trait DownloadUrlResolver: Send + Sync {
    fn resolve_download_url(&self, file_id: i64) -> BoxFuture<'static, AppResult<String>>;
}

impl DownloadUrlResolver for MediaDownloadUrlService {
    fn resolve_download_url(&self, file_id: i64) -> BoxFuture<'static, AppResult<String>> {
        let resolver = self.clone();
        Box::pin(async move { resolver.resolve(file_id).await })
    }
}

#[derive(Clone)]
pub(crate) struct EmbyProxyContext {
    upstream_base_url: Url,
    api_key: Option<String>,
    client: reqwest::Client,
    matcher: BigbrotherStrmMatcher,
    resolver: Arc<dyn DownloadUrlResolver>,
}

impl EmbyProxyContext {
    pub(crate) fn new(
        upstream_base_url: String,
        api_key: Option<String>,
        advertise_base_url: String,
        strm_path_prefix: String,
        resolver: impl DownloadUrlResolver + 'static,
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

pub(crate) fn new_router(ctx: EmbyProxyContext) -> Router {
    Router::new()
        .route("/{*path}", any(proxy_handler))
        .route("/", any(proxy_root_handler))
        .with_state(ctx)
}

async fn proxy_root_handler(
    State(ctx): State<EmbyProxyContext>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    proxy_request_fallback(&ctx, method, headers, uri, body).await
}

async fn proxy_handler(
    State(ctx): State<EmbyProxyContext>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    proxy_request(&ctx, method, headers, uri, body).await
}

async fn proxy_request(
    ctx: &EmbyProxyContext,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if is_video_stream_route(&method, uri.path()) {
        return proxy_video_stream(ctx, method, headers, uri, body).await;
    }

    if is_playback_info_route(&method, uri.path()) {
        return proxy_playback_info(ctx, method, headers, uri, body).await;
    }

    proxy_request_fallback(ctx, method, headers, uri, body).await
}

async fn proxy_request_fallback(
    ctx: &EmbyProxyContext,
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
    path.strip_prefix("/emby/Items/")
        .and_then(|rest| rest.strip_suffix("/PlaybackInfo"))
}

fn is_video_stream_route(method: &Method, path: &str) -> bool {
    *method == Method::GET && video_stream_item_id(path).is_some()
}

fn video_stream_item_id(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/emby/Videos/")?;
    let mut parts = rest.split('/');
    let item_id = parts.next()?;
    matches!(parts.next(), Some("stream" | "original")).then_some(item_id)
}

fn query_param(uri: &Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|pair| {
        let (left, right) = pair.split_once('=')?;
        left.eq_ignore_ascii_case(key).then(|| right.to_string())
    })
}

fn request_emby_auth_query(uri: &Uri, headers: &HeaderMap) -> Option<(String, String)> {
    if let Some(value) = query_param(uri, "api_key") {
        return Some(("api_key".to_string(), value));
    }
    if let Some(value) = query_param(uri, "X-Emby-Token") {
        return Some(("X-Emby-Token".to_string(), value));
    }
    headers
        .get("X-Emby-Token")
        .and_then(|value| value.to_str().ok())
        .map(|value| ("X-Emby-Token".to_string(), value.to_string()))
}

async fn proxy_video_stream(
    ctx: &EmbyProxyContext,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let Some(media_source_id) = query_param(&uri, "MediaSourceId") else {
        return proxy_request_fallback(ctx, method, headers, uri, body).await;
    };
    let Some(item_id) = video_stream_item_id(uri.path()) else {
        return proxy_request_fallback(ctx, method, headers, uri, body).await;
    };
    let request_auth_query = request_emby_auth_query(&uri, &headers);

    match fetch_item_media_sources(
        ctx,
        item_id,
        request_auth_query
            .as_ref()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
    .await
    {
        Ok(item_json) => {
            if let Some(file_id) = crate::application::emby_proxy::file_id_for_media_source(
                &item_json,
                media_source_id.as_str(),
                &ctx.matcher,
            ) {
                return match ctx.resolver.resolve_download_url(file_id).await {
                    Ok(url) => {
                        (StatusCode::FOUND, [(axum::http::header::LOCATION, url)]).into_response()
                    }
                    Err(AppError::Unauthorized(_)) => {
                        (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
                    }
                    Err(AppError::NotFound(_)) => {
                        (StatusCode::NOT_FOUND, "File not found").into_response()
                    }
                    Err(err) => crate::interface::http::media::map_app_error_to_response(err),
                };
            }

            proxy_request_fallback(ctx, method, headers, uri, body).await
        }
        Err(_) => proxy_request_fallback(ctx, method, headers, uri, body).await,
    }
}

async fn fetch_item_media_sources(
    ctx: &EmbyProxyContext,
    item_id: &str,
    request_auth_query: Option<(&str, &str)>,
) -> AppResult<serde_json::Value> {
    let mut url = ctx.upstream_base_url.clone();
    url.set_path("/Items");
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("Ids", item_id);
        query.append_pair("Limit", "1");
        query.append_pair("Fields", "Path,MediaSources");
        if let Some(api_key) = ctx.api_key.as_deref() {
            query.append_pair("api_key", api_key);
        } else if let Some((key, value)) = request_auth_query {
            query.append_pair(key, value);
        }
    }

    ctx.client
        .get(url)
        .send()
        .await
        .map_err(|err| {
            AppError::ExternalService(format!("failed to query emby item: {err}"), false)
        })?
        .json::<serde_json::Value>()
        .await
        .map_err(|err| {
            AppError::ExternalService(format!("failed to parse emby item response: {err}"), false)
        })
}

async fn proxy_playback_info(
    ctx: &EmbyProxyContext,
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
                error!(
                    "serde json failed for playback info for item_id: {}",
                    item_id
                );
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

async fn forward(
    ctx: &EmbyProxyContext,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> AppResult<Response> {
    let upstream_response = send_upstream_request(ctx, method, headers, uri, body).await?;
    response_from_reqwest(upstream_response).await
}

async fn forward_raw(
    ctx: &EmbyProxyContext,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> AppResult<(StatusCode, HeaderMap, Bytes)> {
    let upstream_response = send_upstream_request(ctx, method, headers, uri, body).await?;
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let body = upstream_response.bytes().await.map_err(|err| {
        AppError::ExternalService(format!("failed to read emby response body: {err}"), false)
    })?;

    Ok((status, headers, body))
}

async fn send_upstream_request(
    ctx: &EmbyProxyContext,
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

    request.body(body).send().await.map_err(|err| {
        AppError::ExternalService(format!("failed to proxy request to emby: {err}"), false)
    })
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
            .and(path("/emby/Items/7/PlaybackInfo"))
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
            .post(format!("http://{addr}/emby/Items/7/PlaybackInfo"))
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

    #[tokio::test]
    async fn video_stream_redirects_for_bigbrother_strm() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Items"))
            .and(query_param("Ids", "7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Items": [{
                    "MediaSources": [{
                        "Id": "mediasource_42",
                        "Path": "http://bb.example:3100/d/movie.mkv?file_id=42"
                    }]
                }]
            })))
            .mount(&upstream)
            .await;

        let addr =
            spawn_proxy_with_api_key(upstream.uri(), Some("server-api-key".to_string())).await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let response = client
            .get(format!(
                "http://{addr}/emby/Videos/7/stream?MediaSourceId=mediasource_42"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get("location").unwrap(),
            "https://download.example/video.mkv"
        );
    }

    #[tokio::test]
    async fn video_stream_uses_request_token_for_item_lookup_when_server_key_is_absent() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Items"))
            .and(query_param("Ids", "7"))
            .and(query_param("X-Emby-Token", "user-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Items": [{
                    "MediaSources": [{
                        "Id": "mediasource_42",
                        "Path": "http://bb.example:3100/d/movie.mkv?file_id=42"
                    }]
                }]
            })))
            .mount(&upstream)
            .await;

        let addr = spawn_proxy(upstream.uri()).await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let response = client
            .get(format!(
                "http://{addr}/emby/Videos/7/stream?MediaSourceId=mediasource_42&X-Emby-Token=user-token"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get("location").unwrap(),
            "https://download.example/video.mkv"
        );
    }

    async fn spawn_proxy(upstream_uri: String) -> std::net::SocketAddr {
        spawn_proxy_with_api_key(upstream_uri, None).await
    }

    async fn spawn_proxy_with_api_key(
        upstream_uri: String,
        api_key: Option<String>,
    ) -> std::net::SocketAddr {
        let ctx = EmbyProxyContext::new(
            upstream_uri,
            api_key,
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

    #[derive(Clone)]
    struct FakeResolver;

    impl DownloadUrlResolver for FakeResolver {
        fn resolve_download_url(&self, _file_id: i64) -> BoxFuture<'static, AppResult<String>> {
            Box::pin(async { Ok("https://download.example/video.mkv".to_string()) })
        }
    }

    fn fake_resolver() -> FakeResolver {
        FakeResolver
    }
}
