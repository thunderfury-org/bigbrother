# Emby Proxy STRM 302 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an optional Emby-only reverse proxy on a separate port that rewrites `PlaybackInfo` for `bigbrother` pan123 STRM files and returns `302 Found` from Emby video stream routes.

**Architecture:** Keep the existing `/d/{path}?file_id=...` media server unchanged. Add a new `emby_proxy` config section, pure application helpers for STRM URL parsing and Emby JSON rewriting, and a root-path Axum proxy router that forwards ordinary Emby requests to the configured upstream. Runtime starts the Emby proxy as an optional third task when enabled.

**Tech Stack:** Rust 2024, Tokio, Axum 0.8, Reqwest 0.12, Serde JSON, Wiremock-style HTTP test servers, existing `ResolveDownloadUrlService`.

---

## File Structure

- Create `app/src/application/emby_proxy.rs`
  - Pure helper functions and data structs for identifying `bigbrother` STRM URLs, preserving Emby token query parameters, rewriting `PlaybackInfo`, and matching Emby media source IDs.
  - This file has no HTTP server dependency and owns most unit tests.
- Modify `app/src/application/mod.rs`
  - Export the new `emby_proxy` application module.
- Modify `app/src/config.rs`
  - Add `EmbyProxyConfig`, parse `emby_proxy`, and expose safe getters.
- Modify `config/config.yaml`
  - Document the new disabled-by-default `emby_proxy` section.
- Create `app/src/interface/http/emby_proxy.rs`
  - Axum router and Reqwest-backed reverse proxy.
  - Handles `PlaybackInfo` response rewriting and video stream interception.
- Modify `app/src/interface/http/mod.rs`
  - Export `emby_proxy`.
- Modify `app/src/bootstrap/app.rs`
  - Carry optional Emby proxy runtime settings in `RuntimeBootstrapInputs`.
- Modify `app/src/bootstrap/mod.rs`
  - Build the optional proxy router and run it alongside the existing media server and Telegram bot.
- Keep `app/src/interface/http/media.rs` behavior unchanged
  - Reuse `map_app_error_to_response` by making it visible to sibling modules.

## Task 1: Add Pure Emby STRM Helpers

**Files:**
- Create: `app/src/application/emby_proxy.rs`
- Modify: `app/src/application/mod.rs`

- [ ] **Step 1: Export the future module**

Edit `app/src/application/mod.rs` and add:

```rust
pub mod emby_proxy;
```

- [ ] **Step 2: Write failing STRM parser and token tests**

Create `app/src/application/emby_proxy.rs` with these tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn matcher() -> BigbrotherStrmMatcher {
        BigbrotherStrmMatcher::new("http://bb.example:3100", "/d")
    }

    #[test]
    fn parses_absolute_bigbrother_strm_url() {
        let parsed = matcher()
            .parse("http://bb.example:3100/d/movies/Inception.mkv?file_id=42")
            .unwrap();

        assert_eq!(parsed.file_id, 42);
    }

    #[test]
    fn parses_proxy_local_bigbrother_strm_path() {
        let parsed = matcher()
            .parse("/d/shows/Show.S01E01.mkv?file_id=99")
            .unwrap();

        assert_eq!(parsed.file_id, 99);
    }

    #[test]
    fn rejects_non_bigbrother_url() {
        assert!(matcher().parse("https://example.com/d/movie.mkv?file_id=42").is_none());
    }

    #[test]
    fn rejects_invalid_file_id() {
        assert!(matcher().parse("/d/movie.mkv?file_id=abc").is_none());
    }

    #[test]
    fn preserves_emby_token_query_case_insensitively() {
        assert_eq!(
            emby_token_query("/Videos/1/stream?DeviceId=x&api_KEY=abc"),
            Some("api_KEY=abc".to_string())
        );
        assert_eq!(
            emby_token_query("/Videos/1/stream?X-Emby-Token=def"),
            Some("X-Emby-Token=def".to_string())
        );
        assert_eq!(emby_token_query("/Videos/1/stream?DeviceId=x"), None);
    }

    #[test]
    fn media_source_ids_match_with_optional_prefix() {
        assert!(media_source_ids_match("mediasource_42", "42"));
        assert!(media_source_ids_match("42", "mediasource_42"));
        assert!(!media_source_ids_match("42", "43"));
    }
}
```

- [ ] **Step 3: Run the focused test and verify RED**

Run:

```bash
rtk cargo test -p bigbrother application::emby_proxy
```

Expected: compilation fails because `BigbrotherStrmMatcher`, `emby_token_query`, and `media_source_ids_match` are not defined yet.

- [ ] **Step 4: Implement minimal parser and token helpers**

Replace the top of `app/src/application/emby_proxy.rs` with:

```rust
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBigbrotherStrm {
    pub file_id: i64,
}

#[derive(Debug, Clone)]
pub struct BigbrotherStrmMatcher {
    advertise_base_url: String,
    strm_path_prefix: String,
}

impl BigbrotherStrmMatcher {
    pub fn new(advertise_base_url: impl Into<String>, strm_path_prefix: impl Into<String>) -> Self {
        Self {
            advertise_base_url: advertise_base_url.into().trim_end_matches('/').to_owned(),
            strm_path_prefix: normalize_prefix(strm_path_prefix.into().as_str()),
        }
    }

    pub fn parse(&self, raw: &str) -> Option<ParsedBigbrotherStrm> {
        let url = parse_url_like(raw, &self.advertise_base_url)?;
        if !url.path().starts_with(self.strm_path_prefix.as_str()) {
            return None;
        }

        if raw.starts_with("http://") || raw.starts_with("https://") {
            let base = Url::parse(self.advertise_base_url.as_str()).ok()?;
            if url.scheme() != base.scheme()
                || url.host_str() != base.host_str()
                || url.port_or_known_default() != base.port_or_known_default()
            {
                return None;
            }
        }

        let file_id = url
            .query_pairs()
            .find_map(|(key, value)| (key == "file_id").then_some(value))
            .and_then(|value| value.parse::<i64>().ok())?;

        Some(ParsedBigbrotherStrm { file_id })
    }
}

pub fn emby_token_query(raw_url: &str) -> Option<String> {
    let url = parse_url_like(raw_url, "http://localhost").ok()?;
    url.query_pairs().find_map(|(key, value)| {
        let lower = key.to_ascii_lowercase();
        (lower == "api_key" || lower == "x-emby-token").then(|| format!("{key}={value}"))
    })
}

pub fn media_source_ids_match(left: &str, right: &str) -> bool {
    strip_media_source_prefix(left) == strip_media_source_prefix(right)
}

fn strip_media_source_prefix(value: &str) -> &str {
    value.strip_prefix("mediasource_").unwrap_or(value)
}

fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim();
    let prefixed = if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    prefixed.trim_end_matches('/').to_owned()
}

fn parse_url_like(raw: &str, base_url: &str) -> Result<Url, url::ParseError> {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        Url::parse(raw)
    } else {
        Url::parse(base_url)?.join(raw)
    }
}
```

Keep the test module below this code.

- [ ] **Step 5: Run the focused test and verify GREEN**

Run:

```bash
rtk cargo test -p bigbrother application::emby_proxy
```

Expected: all helper tests pass.

- [ ] **Step 6: Commit Task 1**

Run:

```bash
rtk git add app/src/application/mod.rs app/src/application/emby_proxy.rs
rtk git commit -m "add emby strm proxy helpers"
```

## Task 2: Add PlaybackInfo Rewriting Helpers

**Files:**
- Modify: `app/src/application/emby_proxy.rs`

- [ ] **Step 1: Write failing rewrite tests**

Append these tests to the existing `#[cfg(test)] mod tests` in `app/src/application/emby_proxy.rs`:

```rust
#[test]
fn rewrites_bigbrother_strm_playback_info() {
    let matcher = matcher();
    let mut body = serde_json::json!({
        "MediaSources": [{
            "Id": "mediasource_42",
            "ItemId": "7",
            "Path": "http://bb.example:3100/d/movies/Inception.mkv?file_id=42",
            "DirectStreamUrl": "/Videos/7/stream?MediaSourceId=mediasource_42&api_key=token",
            "SupportsDirectPlay": false,
            "SupportsDirectStream": false,
            "SupportsTranscoding": true,
            "TranscodingUrl": "/Videos/7/master.m3u8",
            "TranscodingContainer": "ts",
            "TranscodingSubProtocol": "hls"
        }]
    });

    let changed = rewrite_playback_info(&mut body, "7", &matcher);

    assert!(changed);
    let source = &body["MediaSources"][0];
    assert_eq!(source["SupportsDirectPlay"], true);
    assert_eq!(source["SupportsDirectStream"], true);
    assert_eq!(source["SupportsTranscoding"], false);
    assert!(source.get("TranscodingUrl").is_none());
    assert_eq!(
        source["DirectStreamUrl"],
        "/Videos/7/stream?MediaSourceId=mediasource_42&Static=true&api_key=token"
    );
}

#[test]
fn leaves_non_bigbrother_playback_info_unchanged() {
    let matcher = matcher();
    let mut body = serde_json::json!({
        "MediaSources": [{
            "Id": "1",
            "ItemId": "7",
            "Path": "https://other.example/movie.mkv",
            "SupportsDirectPlay": false
        }]
    });
    let original = body.clone();

    let changed = rewrite_playback_info(&mut body, "7", &matcher);

    assert!(!changed);
    assert_eq!(body, original);
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
rtk cargo test -p bigbrother application::emby_proxy::tests::rewrites_bigbrother_strm_playback_info
```

Expected: compilation fails because `rewrite_playback_info` is not defined.

- [ ] **Step 3: Implement minimal JSON rewrite**

Add this code above the test module in `app/src/application/emby_proxy.rs`:

```rust
use serde_json::Value;

pub fn rewrite_playback_info(
    body: &mut Value,
    item_id: &str,
    matcher: &BigbrotherStrmMatcher,
) -> bool {
    let Some(media_sources) = body.get_mut("MediaSources").and_then(Value::as_array_mut) else {
        return false;
    };

    let mut changed = false;
    for source in media_sources {
        if !media_source_contains_bigbrother_strm(source, matcher) {
            continue;
        }

        let media_source_id = source
            .get("Id")
            .and_then(Value::as_str)
            .unwrap_or(item_id)
            .to_owned();
        let token = source
            .get("DirectStreamUrl")
            .and_then(Value::as_str)
            .and_then(emby_token_query);
        let mut direct_stream_url =
            format!("/Videos/{item_id}/stream?MediaSourceId={media_source_id}&Static=true");
        if let Some(token) = token {
            direct_stream_url.push('&');
            direct_stream_url.push_str(token.as_str());
        }

        source["SupportsDirectPlay"] = Value::Bool(true);
        source["SupportsDirectStream"] = Value::Bool(true);
        source["SupportsTranscoding"] = Value::Bool(false);
        source["DirectStreamUrl"] = Value::String(direct_stream_url);

        if let Some(object) = source.as_object_mut() {
            for key in [
                "TranscodingUrl",
                "TranscodingContainer",
                "TranscodingSubProtocol",
                "TrancodeLiveStartIndex",
                "TranscodeReasons",
            ] {
                object.remove(key);
            }
        }

        changed = true;
    }

    changed
}

pub fn media_source_contains_bigbrother_strm(
    source: &Value,
    matcher: &BigbrotherStrmMatcher,
) -> bool {
    ["Path", "DirectStreamUrl", "DirectPlayUrl"]
        .iter()
        .filter_map(|key| source.get(*key).and_then(Value::as_str))
        .any(|value| matcher.parse(value).is_some())
}
```

- [ ] **Step 4: Run the focused tests and verify GREEN**

Run:

```bash
rtk cargo test -p bigbrother application::emby_proxy
```

Expected: all Emby proxy helper tests pass.

- [ ] **Step 5: Commit Task 2**

Run:

```bash
rtk git add app/src/application/emby_proxy.rs
rtk git commit -m "rewrite emby playback info for strm"
```

## Task 3: Add Emby Proxy Configuration

**Files:**
- Modify: `app/src/config.rs`
- Modify: `config/config.yaml`

- [ ] **Step 1: Write failing config tests**

Append these tests to the existing `#[cfg(test)]` module in `app/src/config.rs`; if no test module exists, add one at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_dir() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("bigbrother-config-{suffix}"))
    }

    #[test]
    fn emby_proxy_defaults_to_disabled() {
        let data_dir = unique_temp_dir();
        fs::create_dir_all(data_dir.join("config")).unwrap();
        fs::write(data_dir.join("config/config.yaml"), "").unwrap();

        let config = Manager::try_from(data_dir.to_str().unwrap()).unwrap();

        assert!(!config.get_emby_proxy_config().is_enabled());
        assert_eq!(config.get_emby_proxy_config().get_addr(), "0.0.0.0:8097");

        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn emby_proxy_parses_enabled_config() {
        let data_dir = unique_temp_dir();
        fs::create_dir_all(data_dir.join("config")).unwrap();
        fs::write(
            data_dir.join("config/config.yaml"),
            r#"
emby_proxy:
  enable: true
  host: 127.0.0.1
  port: 18097
  upstream_base_url: http://emby.example:8096/
  api_key: secret
"#,
        )
        .unwrap();

        let config = Manager::try_from(data_dir.to_str().unwrap()).unwrap();
        let emby_proxy = config.get_emby_proxy_config();

        assert!(emby_proxy.is_enabled());
        assert_eq!(emby_proxy.get_addr(), "127.0.0.1:18097");
        assert_eq!(
            emby_proxy.get_upstream_base_url().unwrap(),
            "http://emby.example:8096"
        );
        assert_eq!(emby_proxy.get_api_key(), Some("secret"));

        fs::remove_dir_all(data_dir).unwrap();
    }
}
```

- [ ] **Step 2: Run the focused config test and verify RED**

Run:

```bash
rtk cargo test -p bigbrother config::tests::emby_proxy_defaults_to_disabled
```

Expected: compilation fails because `get_emby_proxy_config` and `EmbyProxyConfig` do not exist.

- [ ] **Step 3: Implement config structs and getters**

Edit `app/src/config.rs`:

Add a field to `AppConfig`:

```rust
pub emby_proxy: EmbyProxyConfig,
```

Add this struct after `MediaServerConfig`:

```rust
#[derive(Debug, Default, Deserialize)]
pub struct EmbyProxyConfig {
    pub enable: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub upstream_base_url: Option<String>,
    pub api_key: Option<String>,
}
```

Add this getter to `impl Manager`:

```rust
pub fn get_emby_proxy_config(&self) -> &EmbyProxyConfig {
    &self.app_config.emby_proxy
}
```

Add this `impl` near `impl MediaServerConfig`:

```rust
impl EmbyProxyConfig {
    pub fn is_enabled(&self) -> bool {
        self.enable
    }

    fn get_host(&self) -> &str {
        self.host.as_deref().unwrap_or("0.0.0.0")
    }

    fn get_port(&self) -> u16 {
        self.port.unwrap_or(8097)
    }

    pub fn get_addr(&self) -> String {
        format!("{}:{}", self.get_host(), self.get_port())
    }

    pub fn get_upstream_base_url(&self) -> Option<String> {
        self.upstream_base_url
            .as_ref()
            .map(|url| url.trim_end_matches('/').to_owned())
            .filter(|url| !url.is_empty())
    }

    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|value| !value.is_empty())
    }
}
```

- [ ] **Step 4: Update config template**

Append this to `config/config.yaml`:

```yaml

emby_proxy:
  enable: false
  host: 0.0.0.0
  port: 8097
  upstream_base_url: http://127.0.0.1:8096
  api_key: ""
```

- [ ] **Step 5: Run config tests and verify GREEN**

Run:

```bash
rtk cargo test -p bigbrother config::tests::emby_proxy
```

Expected: both Emby proxy config tests pass.

- [ ] **Step 6: Commit Task 3**

Run:

```bash
rtk git add app/src/config.rs config/config.yaml
rtk git commit -m "add emby proxy config"
```

## Task 4: Build the Emby Proxy Router Skeleton

**Files:**
- Create: `app/src/interface/http/emby_proxy.rs`
- Modify: `app/src/interface/http/mod.rs`

- [ ] **Step 1: Export the new HTTP module**

Edit `app/src/interface/http/mod.rs` and add:

```rust
pub(crate) mod emby_proxy;
```

- [ ] **Step 2: Write failing transparent proxy test**

Create `app/src/interface/http/emby_proxy.rs` with this test module first:

```rust
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
}
```

Also add these test fakes inside the same test module:

```rust
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

fn fake_resolver() -> crate::application::resolve_download_url::ResolveDownloadUrlService<FakeCache, FakeSource> {
    crate::application::resolve_download_url::ResolveDownloadUrlService::new(FakeCache, FakeSource)
}
```

- [ ] **Step 3: Run the focused router test and verify RED**

Run:

```bash
rtk cargo test -p bigbrother interface::http::emby_proxy::tests::transparent_proxy_forwards_root_paths
```

Expected: compilation fails because `EmbyProxyContext` and `new_router` are not defined.

- [ ] **Step 4: Implement minimal transparent proxy**

Add this implementation above the tests in `app/src/interface/http/emby_proxy.rs`:

```rust
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
        emby_proxy::BigbrotherStrmMatcher,
        resolve_download_url::ResolveDownloadUrlService,
    },
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub(crate) struct EmbyProxyContext<C, S> {
    upstream_base_url: Url,
    api_key: Option<String>,
    client: reqwest::Client,
    matcher: BigbrotherStrmMatcher,
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
        let upstream_base_url = Url::parse(upstream_base_url.trim_end_matches('/'))
            .map_err(|err| AppError::InvalidParameter(format!("invalid emby upstream url: {err}")))?;

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

    let upstream_response = request.body(body).send().await.map_err(|err| {
        AppError::Dependency(format!("failed to proxy request to emby: {err}"))
    })?;
    response_from_reqwest(upstream_response).await
}

async fn response_from_reqwest(upstream_response: reqwest::Response) -> AppResult<Response> {
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let body = upstream_response.bytes().await.map_err(|err| {
        AppError::Dependency(format!("failed to read emby response body: {err}"))
    })?;

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
```

- [ ] **Step 5: Run router test and verify GREEN**

Run:

```bash
rtk cargo test -p bigbrother interface::http::emby_proxy::tests::transparent_proxy_forwards_root_paths
```

Expected: transparent proxy test passes.

- [ ] **Step 6: Commit Task 4**

Run:

```bash
rtk git add app/src/interface/http/mod.rs app/src/interface/http/emby_proxy.rs
rtk git commit -m "add emby proxy router"
```

## Task 5: Rewrite PlaybackInfo in the Proxy

**Files:**
- Modify: `app/src/interface/http/emby_proxy.rs`

- [ ] **Step 1: Write failing proxy rewrite test**

Append this test to `app/src/interface/http/emby_proxy.rs`:

```rust
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

    server.abort();
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
rtk cargo test -p bigbrother interface::http::emby_proxy::tests::playback_info_response_is_rewritten_for_bigbrother_strm
```

Expected: test fails because the proxy returns upstream `PlaybackInfo` unchanged.

- [ ] **Step 3: Add route classification and response modification**

In `proxy_request`, before the fallback `forward` call, add:

```rust
if is_playback_info_route(&method, path) {
    return proxy_playback_info(ctx, method, headers, uri, body, path).await;
}
```

Add these helpers below `proxy_request`:

```rust
fn is_playback_info_route(method: &Method, path: &str) -> bool {
    (*method == Method::GET || *method == Method::POST)
        && path
            .strip_prefix("Items/")
            .and_then(|rest| rest.strip_suffix("/PlaybackInfo"))
            .is_some()
}

fn playback_item_id(path: &str) -> Option<&str> {
    path.strip_prefix("Items/")
        .and_then(|rest| rest.strip_suffix("/PlaybackInfo"))
}

async fn proxy_playback_info<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
    path: &str,
) -> Response {
    match forward_raw(ctx, method, headers, uri, body, path).await {
        Ok((status, headers, bytes)) => {
            if !status.is_success() {
                return build_response(status, headers, bytes);
            }

            let Some(item_id) = playback_item_id(path) else {
                return build_response(status, headers, bytes);
            };

            let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return build_response(status, headers, bytes);
            };

            if crate::application::emby_proxy::rewrite_playback_info(
                &mut json,
                item_id,
                &ctx.matcher,
            ) {
                match serde_json::to_vec(&json) {
                    Ok(body) => {
                        let mut response_headers = headers;
                        response_headers.remove("content-length");
                        response_headers.insert(
                            axum::http::header::CONTENT_TYPE,
                            axum::http::HeaderValue::from_static("application/json"),
                        );
                        build_response(status, response_headers, Bytes::from(body))
                    }
                    Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
                }
            } else {
                build_response(status, headers, bytes)
            }
        }
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}
```

Refactor `forward` into raw and response builders:

```rust
async fn forward_raw<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
    path: &str,
) -> AppResult<(StatusCode, HeaderMap, Bytes)> {
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

    let upstream_response = request.body(body).send().await.map_err(|err| {
        AppError::Dependency(format!("failed to proxy request to emby: {err}"))
    })?;
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let bytes = upstream_response.bytes().await.map_err(|err| {
        AppError::Dependency(format!("failed to read emby response body: {err}"))
    })?;

    Ok((status, headers, bytes))
}

async fn forward<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
    path: &str,
) -> AppResult<Response> {
    let (status, headers, body) = forward_raw(ctx, method, headers, uri, body, path).await?;
    Ok(build_response(status, headers, body))
}

fn build_response(status: StatusCode, headers: HeaderMap, body: Bytes) -> Response {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if name.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(Body::from(body))
        .unwrap_or_else(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())
}
```

Remove the old `response_from_reqwest` helper if it is no longer used.

- [ ] **Step 4: Run focused proxy tests and verify GREEN**

Run:

```bash
rtk cargo test -p bigbrother interface::http::emby_proxy::tests::playback_info_response_is_rewritten_for_bigbrother_strm
rtk cargo test -p bigbrother interface::http::emby_proxy::tests::transparent_proxy_forwards_root_paths
```

Expected: both tests pass.

- [ ] **Step 5: Commit Task 5**

Run:

```bash
rtk git add app/src/interface/http/emby_proxy.rs
rtk git commit -m "rewrite playback info in emby proxy"
```

## Task 6: Intercept Emby Video Stream Routes

**Files:**
- Modify: `app/src/application/emby_proxy.rs`
- Modify: `app/src/interface/http/media.rs`
- Modify: `app/src/interface/http/emby_proxy.rs`

- [ ] **Step 1: Write failing media source extraction helper tests**

Append to `app/src/application/emby_proxy.rs` tests:

```rust
#[test]
fn extracts_file_id_from_matching_media_source() {
    let matcher = matcher();
    let item = serde_json::json!({
        "Items": [{
            "MediaSources": [{
                "Id": "mediasource_42",
                "Path": "http://bb.example:3100/d/movie.mkv?file_id=42"
            }]
        }]
    });

    assert_eq!(file_id_for_media_source(&item, "42", &matcher), Some(42));
}

#[test]
fn ignores_non_matching_media_source() {
    let matcher = matcher();
    let item = serde_json::json!({
        "Items": [{
            "MediaSources": [{
                "Id": "43",
                "Path": "http://bb.example:3100/d/movie.mkv?file_id=42"
            }]
        }]
    });

    assert_eq!(file_id_for_media_source(&item, "42", &matcher), None);
}
```

- [ ] **Step 2: Run helper test and verify RED**

Run:

```bash
rtk cargo test -p bigbrother application::emby_proxy::tests::extracts_file_id_from_matching_media_source
```

Expected: compilation fails because `file_id_for_media_source` is not defined.

- [ ] **Step 3: Implement media source file_id extraction**

Add this to `app/src/application/emby_proxy.rs`:

```rust
pub fn file_id_for_media_source(
    item_response: &Value,
    requested_media_source_id: &str,
    matcher: &BigbrotherStrmMatcher,
) -> Option<i64> {
    item_response
        .get("Items")
        .and_then(Value::as_array)?
        .first()?
        .get("MediaSources")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|source| {
            let id = source.get("Id").and_then(Value::as_str)?;
            if !media_source_ids_match(id, requested_media_source_id) {
                return None;
            }

            ["Path", "DirectStreamUrl", "DirectPlayUrl"]
                .iter()
                .filter_map(|key| source.get(*key).and_then(Value::as_str))
                .find_map(|value| matcher.parse(value).map(|parsed| parsed.file_id))
        })
}
```

- [ ] **Step 4: Expose existing HTTP error mapper**

In `app/src/interface/http/media.rs`, change:

```rust
fn map_app_error_to_response(error: AppError) -> Response {
```

to:

```rust
pub(crate) fn map_app_error_to_response(error: AppError) -> Response {
```

- [ ] **Step 5: Write failing video intercept integration test**

Append to `app/src/interface/http/emby_proxy.rs` tests:

```rust
#[tokio::test]
async fn video_stream_redirects_for_bigbrother_strm() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/Items"))
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

    let ctx = EmbyProxyContext::new(
        upstream.uri(),
        Some("server-api-key".to_string()),
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

    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!(
            "http://{addr}/Videos/7/stream?MediaSourceId=mediasource_42"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "https://download.example/video.mkv"
    );

    server.abort();
}
```

- [ ] **Step 6: Run video intercept test and verify RED**

Run:

```bash
rtk cargo test -p bigbrother interface::http::emby_proxy::tests::video_stream_redirects_for_bigbrother_strm
```

Expected: test fails because video stream routes are still proxied upstream.

- [ ] **Step 7: Implement video route interception**

In `proxy_request`, before playback info handling, add:

```rust
if is_video_stream_route(&method, path) {
    return proxy_video_stream(ctx, method, headers, uri, body, path).await;
}
```

Add these helpers:

```rust
fn is_video_stream_route(method: &Method, path: &str) -> bool {
    *method == Method::GET
        && path
            .strip_prefix("Videos/")
            .and_then(|rest| {
                let mut parts = rest.split('/');
                let _item_id = parts.next()?;
                let action = parts.next()?;
                Some(action == "stream" || action == "original")
            })
            .unwrap_or(false)
}

fn query_param(uri: &Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|pair| {
        let (left, right) = pair.split_once('=')?;
        left.eq_ignore_ascii_case(key).then(|| right.to_string())
    })
}

async fn proxy_video_stream<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
    path: &str,
) -> Response
where
    C: crate::application::ports::DownloadUrlCache,
    S: crate::application::ports::DownloadUrlSource,
{
    let Some(media_source_id) = query_param(&uri, "MediaSourceId") else {
        return proxy_request_fallback(ctx, method, headers, uri, body, path).await;
    };

    match fetch_item_media_sources(ctx, media_source_id.as_str()).await {
        Ok(item_json) => {
            if let Some(file_id) = crate::application::emby_proxy::file_id_for_media_source(
                &item_json,
                media_source_id.as_str(),
                &ctx.matcher,
            ) {
                return match ctx.resolver.resolve(file_id).await {
                    Ok(crate::application::resolve_download_url::ResolveDownloadUrlResult::Redirect(url)) => {
                        (
                            StatusCode::FOUND,
                            [(axum::http::header::LOCATION, url)],
                        )
                            .into_response()
                    }
                    Ok(crate::application::resolve_download_url::ResolveDownloadUrlResult::Unauthorized) => {
                        (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
                    }
                    Ok(crate::application::resolve_download_url::ResolveDownloadUrlResult::NotFound) => {
                        (StatusCode::NOT_FOUND, "File not found").into_response()
                    }
                    Err(err) => crate::interface::http::media::map_app_error_to_response(err),
                };
            }
            proxy_request_fallback(ctx, method, headers, uri, body, path).await
        }
        Err(_) => proxy_request_fallback(ctx, method, headers, uri, body, path).await,
    }
}

async fn proxy_request_fallback<C, S>(
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

async fn fetch_item_media_sources<C, S>(
    ctx: &EmbyProxyContext<C, S>,
    media_source_id: &str,
) -> AppResult<serde_json::Value> {
    let item_id = media_source_id.strip_prefix("mediasource_").unwrap_or(media_source_id);
    let mut url = ctx.upstream_base_url.clone();
    url.set_path("/Items");
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("Ids", item_id);
        query.append_pair("Limit", "1");
        query.append_pair("Fields", "Path,MediaSources");
        if let Some(api_key) = ctx.api_key.as_deref() {
            query.append_pair("api_key", api_key);
        }
    }

    ctx.client
        .get(url)
        .send()
        .await
        .map_err(|err| AppError::Dependency(format!("failed to query emby item: {err}")))?
        .json::<serde_json::Value>()
        .await
        .map_err(|err| AppError::Dependency(format!("failed to parse emby item response: {err}")))
}
```

Then simplify the existing fallback body in `proxy_request` to call `proxy_request_fallback`.

- [ ] **Step 8: Run video and helper tests and verify GREEN**

Run:

```bash
rtk cargo test -p bigbrother application::emby_proxy
rtk cargo test -p bigbrother interface::http::emby_proxy
```

Expected: all Emby proxy helper and HTTP proxy tests pass.

- [ ] **Step 9: Commit Task 6**

Run:

```bash
rtk git add app/src/application/emby_proxy.rs app/src/interface/http/media.rs app/src/interface/http/emby_proxy.rs
rtk git commit -m "redirect emby strm streams"
```

## Task 7: Wire the Optional Proxy Into Runtime

**Files:**
- Modify: `app/src/bootstrap/app.rs`
- Modify: `app/src/bootstrap/mod.rs`

- [ ] **Step 1: Add runtime config fields**

In `app/src/bootstrap/app.rs`, add this struct near `RuntimeBootstrapInputs`:

```rust
#[derive(Clone)]
pub struct EmbyProxyRuntimeConfig {
    pub addr: String,
    pub upstream_base_url: String,
    pub api_key: Option<String>,
    pub advertise_base_url: String,
    pub strm_path_prefix: String,
}
```

Add this field to `RuntimeBootstrapInputs`:

```rust
pub emby_proxy_config: Option<EmbyProxyRuntimeConfig>,
```

In `AppContext::new`, construct it before `Ok(AppContext { ... })`:

```rust
let emby_proxy_config = if config.get_emby_proxy_config().is_enabled() {
    let upstream_base_url = config
        .get_emby_proxy_config()
        .get_upstream_base_url()
        .ok_or_else(|| AppError::InvalidParameter(
            "emby_proxy.upstream_base_url is required when emby_proxy.enable is true".to_string(),
        ))?;
    Some(EmbyProxyRuntimeConfig {
        addr: config.get_emby_proxy_config().get_addr(),
        upstream_base_url,
        api_key: config.get_emby_proxy_config().get_api_key().map(str::to_owned),
        advertise_base_url: config.get_media_server_config().get_advertise_base_url(),
        strm_path_prefix: config.get_media_server_config().get_strm_path_prefix().to_string(),
    })
} else {
    None
};
```

Then include:

```rust
emby_proxy_config,
```

inside `RuntimeBootstrapInputs`.

- [ ] **Step 2: Update server runtime fields**

In `app/src/bootstrap/mod.rs`, add to `ServerRuntime`:

```rust
pub emby_proxy_addr: Option<String>,
pub emby_proxy_server: Option<axum::Router>,
```

In `AppRuntime::from_app`, before constructing `ServerRuntime`, create:

```rust
let emby_proxy_server = inputs.emby_proxy_config.as_ref().map(|config| {
    http::emby_proxy::new_router(http::emby_proxy::EmbyProxyContext::new(
        config.upstream_base_url.clone(),
        config.api_key.clone(),
        config.advertise_base_url.clone(),
        config.strm_path_prefix.clone(),
        MediaDownloadUrlService::new(
            StringCacheStore::new(inputs.cache.clone()),
            Pan123LibraryRemote::new(inputs.clients.pan123.clone()),
        ),
    ).expect("validated emby proxy config"))
});
let emby_proxy_addr = inputs
    .emby_proxy_config
    .as_ref()
    .map(|config| config.addr.clone());
```

Then set these fields in `ServerRuntime`:

```rust
emby_proxy_addr,
emby_proxy_server,
```

- [ ] **Step 3: Run optional Emby proxy task**

Replace `ServerRuntime::run` with:

```rust
async fn run(self) -> AppResult<()> {
    let mut tasks = tokio::task::JoinSet::new();

    tasks.spawn(http::run(self.media_server_addr, self.media_server));
    tasks.spawn(telegram::run(self.bot, self.bot_runtime));

    if let (Some(addr), Some(router)) = (self.emby_proxy_addr, self.emby_proxy_server) {
        tasks.spawn(http::run(addr, router));
    }

    match tasks.join_next().await {
        Some(Ok(result)) => {
            tasks.abort_all();
            result
        }
        Some(Err(err)) => {
            tasks.abort_all();
            Err(AppError::Runtime(format!("server task failed: {err}")))
        }
        None => Ok(()),
    }
}
```

- [ ] **Step 4: Run compile check and fix type errors only**

Run:

```bash
rtk cargo test -p bigbrother --no-run
```

Expected: compilation succeeds. If type errors arise from generic bounds in `new_router`, add the exact `Clone + Send + Sync + 'static` bounds required by the compiler without changing behavior.

- [ ] **Step 5: Commit Task 7**

Run:

```bash
rtk git add app/src/bootstrap/app.rs app/src/bootstrap/mod.rs
rtk git commit -m "wire emby proxy runtime"
```

## Task 8: Full Verification and Cleanup

**Files:**
- Modify only files touched earlier if verification exposes a defect.

- [ ] **Step 1: Run formatting**

Run:

```bash
rtk make fmt
```

Expected: command succeeds.

- [ ] **Step 2: Run full test suite**

Run:

```bash
rtk make test
```

Expected: all tests pass.

- [ ] **Step 3: Run lint**

Run:

```bash
rtk make lint
```

Expected: `cargo fmt --check` and `clippy -D warnings` pass.

- [ ] **Step 4: Inspect final diff**

Run:

```bash
rtk git status --short
rtk git diff --stat HEAD
```

Expected: only intended Emby proxy implementation files are modified.

- [ ] **Step 5: Commit verification fixes if any**

If Step 1-4 required code changes, commit them:

```bash
rtk git add app/src config docs
rtk git commit -m "stabilize emby proxy"
```

If no changes remain, do not create an empty commit.
