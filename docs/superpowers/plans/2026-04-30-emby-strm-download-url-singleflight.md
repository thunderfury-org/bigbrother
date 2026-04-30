# Emby STRM Download URL Singleflight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Coalesce concurrent first-playback download URL resolutions for the same `file_id` so Emby STRM playback only triggers one pan123 resolve on a cold cache.

**Architecture:** Keep the existing `/d/{path}?file_id=...` route and `ResolveDownloadUrlService` API. Add an in-memory in-flight coordinator inside `ResolveDownloadUrlService` keyed by `file_id`; cache hits return immediately, cache misses either own the upstream resolve or wait for the existing one. Successful resolves still write the existing SQLite-backed cache with the current fixed 30 minute TTL.

**Tech Stack:** Rust, Tokio, Axum, SeaORM-backed cache, existing application port traits, standard `Arc`/`Mutex`/`Notify` synchronization.

---

## File Structure

- Modify `app/src/application/resolve_download_url.rs`
  - Owns the singleflight coordinator and all new unit tests.
  - Keeps public `ResolveDownloadUrlService::new(cache, source)` unchanged.
  - Adds private helper types for in-flight wait state.
- Modify `app/src/error.rs`
  - Adds `Clone` to `AppError` so a resolved owner result can be shared with
    concurrent waiters without changing error semantics.
- No changes to `app/src/interface/http/media.rs`
  - HTTP route behavior and status mapping stay unchanged.
- No changes to config files
  - The download URL TTL remains the existing fixed 30 minutes.
- No database migration
  - Existing `cache` table continues storing resolved URLs.

## Implementation Tasks

### Task 1: Add Failing Concurrency Tests

**Files:**
- Modify: `app/src/application/resolve_download_url.rs`

- [ ] **Step 1: Update test imports**

In the `#[cfg(test)] mod tests` block in `app/src/application/resolve_download_url.rs`, replace the current imports:

```rust
use std::sync::{Arc, Mutex};
```

with:

```rust
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
```

This gives the tests per-key cache storage, call counters, and async timing.

- [ ] **Step 2: Replace `FakeCache` with keyed storage**

Replace the current `FakeCache` definition and impl:

```rust
#[derive(Clone, Default)]
struct FakeCache {
    stored: Arc<Mutex<Option<String>>>,
}

impl DownloadUrlCache for FakeCache {
    async fn get_download_url(&self, _key: &str) -> AppResult<Option<String>> {
        Ok(self.stored.lock().unwrap().clone())
    }

    async fn set_download_url(&self, _key: &str, url: &str, _ttl: Duration) -> AppResult<()> {
        *self.stored.lock().unwrap() = Some(url.to_owned());
        Ok(())
    }
}
```

with:

```rust
#[derive(Clone, Default)]
struct FakeCache {
    stored: Arc<Mutex<HashMap<String, String>>>,
}

impl FakeCache {
    fn with_download_url(file_id: i64, url: &str) -> Self {
        let cache = Self::default();
        cache
            .stored
            .lock()
            .unwrap()
            .insert(format!("pan123:download_url:{file_id}"), url.to_owned());
        cache
    }
}

impl DownloadUrlCache for FakeCache {
    async fn get_download_url(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.stored.lock().unwrap().get(key).cloned())
    }

    async fn set_download_url(&self, key: &str, url: &str, _ttl: Duration) -> AppResult<()> {
        self.stored
            .lock()
            .unwrap()
            .insert(key.to_owned(), url.to_owned());
        Ok(())
    }
}
```

- [ ] **Step 3: Replace `FakeSource` with keyed results and call counts**

Replace the current `FakeSource` definition and impl:

```rust
#[derive(Clone)]
struct FakeSource {
    result: Arc<Mutex<Result<String, &'static str>>>,
}

impl DownloadUrlSource for FakeSource {
    async fn get_download_url(&self, _file_id: i64) -> DownloadUrlResult<String> {
        match &*self.result.lock().unwrap() {
            Ok(url) => Ok(url.clone()),
            Err(kind) if *kind == "not_found" => {
                Err(DownloadUrlError::NotFound("missing".to_string()))
            }
            Err(kind) if *kind == "unauthorized" => Err(DownloadUrlError::Unauthorized),
            Err(kind) => Err(DownloadUrlError::Error((*kind).to_string())),
        }
    }
}
```

with:

```rust
#[derive(Clone)]
struct FakeSource {
    results: Arc<Mutex<HashMap<i64, Result<String, &'static str>>>>,
    calls: Arc<AtomicUsize>,
    delay: Duration,
}

impl FakeSource {
    fn new(default_result: Result<String, &'static str>) -> Self {
        Self {
            results: Arc::new(Mutex::new(HashMap::from([(1, default_result)]))),
            calls: Arc::new(AtomicUsize::new(0)),
            delay: Duration::ZERO,
        }
    }

    fn with_results(results: impl IntoIterator<Item = (i64, Result<String, &'static str>)>) -> Self {
        Self {
            results: Arc::new(Mutex::new(results.into_iter().collect())),
            calls: Arc::new(AtomicUsize::new(0)),
            delay: Duration::ZERO,
        }
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl DownloadUrlSource for FakeSource {
    async fn get_download_url(&self, file_id: i64) -> DownloadUrlResult<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }

        let result = self
            .results
            .lock()
            .unwrap()
            .get(&file_id)
            .cloned()
            .unwrap_or_else(|| Err("not_found"));

        match result {
            Ok(url) => Ok(url),
            Err("not_found") => Err(DownloadUrlError::NotFound("missing".to_string())),
            Err("unauthorized") => Err(DownloadUrlError::Unauthorized),
            Err(kind) => Err(DownloadUrlError::Error(kind.to_string())),
        }
    }
}
```

- [ ] **Step 4: Update existing tests to use the new fakes**

Change `resolve_returns_cached_url` to:

```rust
#[tokio::test]
async fn resolve_returns_cached_url() {
    let cache = FakeCache::with_download_url(1, "https://cached");
    let source = FakeSource::new(Ok("https://remote".to_string()));
    let service = ResolveDownloadUrlService::new(cache, source.clone());

    let result = service.resolve(1).await.unwrap();
    assert!(matches!(
        result,
        ResolveDownloadUrlResult::Redirect(url) if url == "https://cached"
    ));
    assert_eq!(source.calls(), 0);
}
```

Change `resolve_maps_not_found` to:

```rust
#[tokio::test]
async fn resolve_maps_not_found() {
    let service = ResolveDownloadUrlService::new(
        FakeCache::default(),
        FakeSource::new(Err("not_found")),
    );

    let result = service.resolve(1).await.unwrap();
    assert!(matches!(result, ResolveDownloadUrlResult::NotFound));
}
```

Change `resolve_maps_source_error_to_dependency_error` to:

```rust
#[tokio::test]
async fn resolve_maps_source_error_to_dependency_error() {
    let service = ResolveDownloadUrlService::new(
        FakeCache::default(),
        FakeSource::new(Err("boom")),
    );

    let error = service.resolve(1).await.unwrap_err();
    assert!(matches!(error, AppError::Dependency(_)));
}
```

- [ ] **Step 5: Add same-file concurrent coalescing test**

Append this test in the same test module:

```rust
#[tokio::test]
async fn resolve_coalesces_concurrent_cache_misses_for_same_file_id() {
    let source =
        FakeSource::new(Ok("https://remote".to_string())).with_delay(Duration::from_millis(50));
    let service = Arc::new(ResolveDownloadUrlService::new(
        FakeCache::default(),
        source.clone(),
    ));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let service = service.clone();
        handles.push(tokio::spawn(async move { service.resolve(1).await }));
    }

    let mut redirects = Vec::new();
    for handle in handles {
        match handle.await.unwrap().unwrap() {
            ResolveDownloadUrlResult::Redirect(url) => redirects.push(url),
            other => panic!("expected redirect, got {other:?}"),
        }
    }

    assert_eq!(redirects, vec!["https://remote".to_string(); 8]);
    assert_eq!(source.calls(), 1);
}
```

- [ ] **Step 6: Add different-file non-coalescing test**

Append this test:

```rust
#[tokio::test]
async fn resolve_does_not_coalesce_different_file_ids() {
    let source = FakeSource::with_results([
        (1, Ok("https://remote/1".to_string())),
        (2, Ok("https://remote/2".to_string())),
    ])
    .with_delay(Duration::from_millis(20));
    let service = Arc::new(ResolveDownloadUrlService::new(
        FakeCache::default(),
        source.clone(),
    ));

    let first = {
        let service = service.clone();
        tokio::spawn(async move { service.resolve(1).await })
    };
    let second = {
        let service = service.clone();
        tokio::spawn(async move { service.resolve(2).await })
    };

    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();

    assert!(matches!(
        first,
        ResolveDownloadUrlResult::Redirect(url) if url == "https://remote/1"
    ));
    assert!(matches!(
        second,
        ResolveDownloadUrlResult::Redirect(url) if url == "https://remote/2"
    ));
    assert_eq!(source.calls(), 2);
}
```

- [ ] **Step 7: Add failure cleanup and retry test**

Append this test:

```rust
#[tokio::test]
async fn resolve_cleans_inflight_after_failure_and_allows_retry() {
    let source = FakeSource::new(Err("boom")).with_delay(Duration::from_millis(50));
    let service = Arc::new(ResolveDownloadUrlService::new(
        FakeCache::default(),
        source.clone(),
    ));

    let first = {
        let service = service.clone();
        tokio::spawn(async move { service.resolve(1).await })
    };
    let second = {
        let service = service.clone();
        tokio::spawn(async move { service.resolve(1).await })
    };

    assert!(matches!(
        first.await.unwrap().unwrap_err(),
        AppError::Dependency(_)
    ));
    assert!(matches!(
        second.await.unwrap().unwrap_err(),
        AppError::Dependency(_)
    ));
    assert_eq!(source.calls(), 1);

    let retry = service.resolve(1).await.unwrap_err();
    assert!(matches!(retry, AppError::Dependency(_)));
    assert_eq!(source.calls(), 2);
}
```

- [ ] **Step 8: Run focused tests and verify expected failure**

Run:

```bash
cargo test -p bigbrother application::resolve_download_url::tests -- --nocapture
```

Expected: the new same-file concurrent test fails because `source.calls()` is greater than `1`. Existing tests should compile after the fake updates.

- [ ] **Step 9: Commit failing tests**

Run:

```bash
git add app/src/application/resolve_download_url.rs
git commit -m "test download url singleflight behavior"
```

Expected: commit succeeds with only test changes.

### Task 2: Implement Singleflight Resolution

**Files:**
- Modify: `app/src/error.rs`
- Modify: `app/src/application/resolve_download_url.rs`

- [ ] **Step 1: Make `AppError` cloneable**

In `app/src/error.rs`, change:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
```

to:

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum AppError {
```

All variants contain owned `String` values, so cloning preserves the existing
error kind and message for waiters.

- [ ] **Step 2: Add synchronization imports**

At the top of `app/src/application/resolve_download_url.rs`, replace:

```rust
use std::time::Duration;
```

with:

```rust
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use tokio::sync::{Mutex, Notify};
```

Keep `use tracing::error;` for now; it will be expanded in a later step.

- [ ] **Step 3: Make resolve results cloneable**

Change:

```rust
#[derive(Debug)]
pub enum ResolveDownloadUrlResult {
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveDownloadUrlResult {
```

This allows the owner result to be shared with waiters.

- [ ] **Step 4: Add private in-flight state types**

Add these types below `ResolveDownloadUrlResult`:

```rust
type SharedResolveResult = AppResult<ResolveDownloadUrlResult>;

#[derive(Debug)]
struct InflightResolve {
    result: Mutex<Option<SharedResolveResult>>,
    notify: Notify,
}

impl InflightResolve {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            notify: Notify::new(),
        }
    }

    async fn wait(&self) -> SharedResolveResult {
        loop {
            let notified = self.notify.notified();
            if let Some(result) = self.result.lock().await.clone() {
                return result;
            }
            notified.await;
        }
    }

    async fn complete(&self, result: SharedResolveResult) {
        *self.result.lock().await = Some(result);
        self.notify.notify_waiters();
    }
}
```

- [ ] **Step 5: Add in-flight map to the service**

Change the service struct:

```rust
#[derive(Clone)]
pub struct ResolveDownloadUrlService<C, S> {
    cache: C,
    source: S,
}
```

to:

```rust
#[derive(Clone)]
pub struct ResolveDownloadUrlService<C, S> {
    cache: C,
    source: S,
    inflight: Arc<Mutex<HashMap<i64, Arc<InflightResolve>>>>,
}
```

Change `new`:

```rust
pub fn new(cache: C, source: S) -> Self {
    Self { cache, source }
}
```

to:

```rust
pub fn new(cache: C, source: S) -> Self {
    Self {
        cache,
        source,
        inflight: Arc::new(Mutex::new(HashMap::new())),
    }
}
```

- [ ] **Step 6: Split source resolution into a helper**

Inside the `impl<C, S> ResolveDownloadUrlService<C, S> where ...` block, add this private helper below `resolve` temporarily or before replacing `resolve`:

```rust
async fn resolve_uncached(&self, file_id: i64, cache_key: &str) -> SharedResolveResult {
    match self.source.get_download_url(file_id).await {
        Ok(url) => {
            if url.is_empty() {
                return Err(AppError::RuleRejected(
                    "download url source returned empty url".to_owned(),
                ));
            }

            if let Err(err) = self
                .cache
                .set_download_url(cache_key, &url, Duration::from_mins(30))
                .await
            {
                error!("Failed to cache download url for file {file_id}, {err}");
            }

            Ok(ResolveDownloadUrlResult::Redirect(url))
        }
        Err(DownloadUrlError::Unauthorized) => Ok(ResolveDownloadUrlResult::Unauthorized),
        Err(DownloadUrlError::NotFound(_)) => Ok(ResolveDownloadUrlResult::NotFound),
        Err(err) => Err(AppError::Dependency(format!(
            "failed to get download url: {err}"
        ))),
    }
}
```

- [ ] **Step 7: Replace `resolve` with cache-first singleflight logic**

Replace the body of `pub async fn resolve(&self, file_id: i64) -> AppResult<ResolveDownloadUrlResult>` with:

```rust
let cache_key = format!("pan123:download_url:{file_id}");
if let Some(cached_url) = self.cache.get_download_url(&cache_key).await? {
    return Ok(ResolveDownloadUrlResult::Redirect(cached_url));
}

let (entry, owner) = {
    let mut inflight = self.inflight.lock().await;
    if let Some(entry) = inflight.get(&file_id) {
        (entry.clone(), false)
    } else {
        let entry = Arc::new(InflightResolve::new());
        inflight.insert(file_id, entry.clone());
        (entry, true)
    }
};

if !owner {
    return entry.wait().await;
}

let result = self.resolve_uncached(file_id, &cache_key).await;
entry.complete(result.clone()).await;
self.inflight.lock().await.remove(&file_id);
result
```

This preserves cache-first behavior and coalesces only the cold miss path.

- [ ] **Step 8: Run focused tests**

Run:

```bash
cargo test -p bigbrother application::resolve_download_url::tests -- --nocapture
```

Expected: all `resolve_download_url` tests pass.

- [ ] **Step 9: Commit implementation**

Run:

```bash
git add app/src/error.rs app/src/application/resolve_download_url.rs
git commit -m "coalesce download url resolves"
```

Expected: commit succeeds with implementation changes.

### Task 3: Add Safe Observability and Full Verification

**Files:**
- Modify: `app/src/application/resolve_download_url.rs`

- [ ] **Step 1: Expand tracing imports**

At the top of `app/src/application/resolve_download_url.rs`, change:

```rust
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
```

to:

```rust
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
```

Change:

```rust
use tracing::error;
```

to:

```rust
use tracing::{debug, error, warn};
```

- [ ] **Step 2: Add cache miss and coalescing logs**

In `resolve`, after the cache check misses and before taking the in-flight lock, add:

```rust
debug!("download url cache miss for file {file_id}");
```

Inside the in-flight lock branch for an existing entry, before returning `(entry.clone(), false)`, add:

```rust
debug!("joining in-flight download url resolve for file {file_id}");
```

The relevant block should become:

```rust
let (entry, owner) = {
    let mut inflight = self.inflight.lock().await;
    if let Some(entry) = inflight.get(&file_id) {
        debug!("joining in-flight download url resolve for file {file_id}");
        (entry.clone(), false)
    } else {
        let entry = Arc::new(InflightResolve::new());
        inflight.insert(file_id, entry.clone());
        (entry, true)
    }
};
```

- [ ] **Step 3: Add owner resolve duration logs**

In `resolve`, immediately before calling `resolve_uncached`, add:

```rust
let started_at = Instant::now();
```

Then after:

```rust
let result = self.resolve_uncached(file_id, &cache_key).await;
```

add:

```rust
match &result {
    Ok(ResolveDownloadUrlResult::Redirect(_)) => {
        debug!(
            "resolved download url for file {file_id} in {:?}",
            started_at.elapsed()
        );
    }
    Ok(ResolveDownloadUrlResult::Unauthorized) => {
        warn!(
            "download url resolve unauthorized for file {file_id} after {:?}",
            started_at.elapsed()
        );
    }
    Ok(ResolveDownloadUrlResult::NotFound) => {
        warn!(
            "download url resolve not found for file {file_id} after {:?}",
            started_at.elapsed()
        );
    }
    Err(err) => {
        warn!(
            "download url resolve failed for file {file_id} after {:?}: {err}",
            started_at.elapsed()
        );
    }
}
```

Do not log the redirect URL.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test -p bigbrother application::resolve_download_url::tests -- --nocapture
```

Expected: all focused tests pass.

- [ ] **Step 5: Run full project verification**

Run:

```bash
make fmt
make lint
make test
```

Expected:

- `make fmt` exits successfully.
- `make lint` exits successfully with no clippy warnings.
- `make test` exits successfully.

- [ ] **Step 6: Commit observability and verification-ready code**

Run:

```bash
git add app/src/application/resolve_download_url.rs
git commit -m "log download url resolve singleflight"
```

Expected: commit succeeds.

## Self-Review

- Spec coverage:
  - Existing route, STRM format, 302 behavior, and fixed TTL are preserved by not touching HTTP/config/STRM generation files.
  - Same-file concurrent cache misses are covered by Task 1 tests and Task 2 implementation.
  - Different-file requests are covered by Task 1.
  - Failure cleanup and retry are covered by Task 1.
  - Logs without signed URL leakage are covered by Task 3.
- Completeness scan:
  - No task leaves work unspecified.
  - Each code-changing step includes concrete code.
- Type consistency:
  - `ResolveDownloadUrlService::new(cache, source)` remains unchanged for bootstrap and tests.
  - The in-flight map uses `file_id: i64`, matching the existing cache key and source API.
  - Tests use the existing `DownloadUrlCache` and `DownloadUrlSource` traits.
