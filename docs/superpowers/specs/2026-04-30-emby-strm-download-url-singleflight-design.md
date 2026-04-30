# Emby STRM Download URL Singleflight Design

## Context

`bigbrother` already writes `.strm` files whose content points to the local media
server:

```text
http://<advertise-base-url>/d<remote-file-path>?file_id=<pan123-file-id>
```

The media server resolves that `file_id` through `ResolveDownloadUrlService`,
caches the resulting pan123 download URL for 30 minutes, and responds with a 302
redirect. This is compatible with Emby STRM playback and keeps media traffic out
of `bigbrother`.

The remaining first-playback issue is duplicated on-demand resolution. When
Emby or its ffmpeg process probes the same STRM URL several times before the
download URL cache is populated, each request can miss the cache and call pan123
independently. That amplifies latency and upstream pressure during the first
play attempt.

## Goals

- Keep the existing STRM format and `/d/{path}?file_id=...` route.
- Keep the existing 302 redirect behavior.
- Keep the current fixed 30 minute download URL cache TTL.
- Coalesce concurrent cache misses for the same `file_id` so pan123 is called
  once and all waiters share the result.
- Add focused logs for cache misses, coalesced requests, upstream resolve
  duration, and failure class without logging the resolved download URL.
- Preserve existing HTTP error mapping.

## Non-Goals

- No proactive download URL preheating. The pan123 URL is time-limited, so
  generating it before a real playback request may waste validity.
- No new config in this change.
- No HEAD, GET, or Range compatibility changes.
- No MediaWarp-style Emby reverse proxy, PlaybackInfo rewriting, or
  `/Videos/{id}/stream` interception.
- No schema migration or new cache table.

## Proposed Approach

Add a singleflight-style in-flight coordinator inside, or directly alongside,
`ResolveDownloadUrlService`.

The service remains the only application-level entry point:

```rust
resolve(file_id: i64) -> AppResult<ResolveDownloadUrlResult>
```

On every call, the service first reads the existing `DownloadUrlCache`. If the
cache has a value, it immediately returns `Redirect(cached_url)`.

If the cache misses, the service checks an in-memory map keyed by `file_id`.
When another request is already resolving that file, the current request waits
for the existing result. When no request is resolving it, the current request
becomes the owner, registers the in-flight work, calls the `DownloadUrlSource`,
stores successful URLs in the existing cache with the current 30 minute TTL, and
notifies all waiters.

The in-flight entry is removed after either success or failure. A failed owner
does not poison later requests; the next request can attempt resolution again.

## Data Flow

1. Emby requests `/d/{path}?file_id=123`.
2. HTTP extracts and validates `file_id`.
3. `ResolveDownloadUrlService` checks `pan123:download_url:123` in the existing
   SQLite-backed cache.
4. Cache hit returns `Redirect(cached_url)`.
5. Cache miss checks the in-flight map for `123`.
6. Existing in-flight work means the request waits and reuses that result.
7. Missing in-flight work means the request calls pan123 once.
8. Success stores the URL in cache and returns `Redirect(url)` to the owner and
   waiters.
9. Failure wakes all waiters with the same mapped result or error.

## Error Handling

Existing behavior is preserved:

- `DownloadUrlError::Unauthorized` maps to `Unauthorized`, then HTTP 401.
- `DownloadUrlError::NotFound` maps to `NotFound`, then HTTP 404.
- Other `DownloadUrlError` values map to `AppError::Dependency`, then HTTP 502.
- An empty URL remains `AppError::RuleRejected`, then HTTP 422.
- Cache write failures are logged but do not prevent a successful redirect.

The in-flight entry must be cleaned up on every exit path, including upstream
errors and cache write failures.

## Observability

Add logs that help diagnose first-playback latency without leaking signed URLs:

- cache miss for `file_id`
- request joined an existing in-flight resolve for `file_id`
- owner resolve duration and success or failure class
- cache write failure for `file_id` remains logged as today

Do not log the resolved pan123 download URL.

## Tests

Add focused tests around `ResolveDownloadUrlService`:

- Returns cached URL without calling source.
- Multiple concurrent resolves for the same uncached `file_id` call the fake
  source exactly once and all return the same redirect.
- Concurrent resolves for different `file_id` values are not incorrectly
  coalesced.
- Source failure is shared with waiters and does not leave a stuck in-flight
  entry; a later retry can call the source again.
- Existing not found and dependency error mappings remain covered.

Existing HTTP route tests should continue to pass without route changes.
