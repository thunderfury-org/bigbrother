# Emby Proxy STRM 302 Design

## Context

`bigbrother` already imports media into pan123-backed libraries and writes
`.strm` files whose content points to the local media server:

```text
http://<advertise-base-url>/d<remote-file-path>?file_id=<pan123-file-id>
```

The existing `/d/{path}?file_id=...` route resolves the `file_id` through
`ResolveDownloadUrlService`, caches the pan123 download URL, and returns a 302
redirect. That path works for clients that read the STRM content directly.

Emby clients, however, normally connect to Emby and follow Emby's playback
contract. MediaWarp solves this class of problem by acting as an Emby reverse
proxy, modifying `PlaybackInfo`, and intercepting video stream routes. This
design adopts only that core idea for Emby and for `bigbrother`-generated pan123
STRM files.

Reference: <https://github.com/AkimioJR/MediaWarp>

## Goals

- Add an Emby-only reverse proxy service that listens on a separate port.
- Expose the proxy at the root path, so clients can connect to it as their Emby
  server URL.
- Transparently proxy ordinary Emby HTTP requests to the configured upstream
  Emby server.
- Modify `/Items/{item_id}/PlaybackInfo` only when it describes a
  `bigbrother`-generated pan123 STRM.
- Intercept `/Videos/{item_id}/stream` and `/Videos/{item_id}/original` only
  when they point to a `bigbrother`-generated pan123 STRM, then return
  `302 Found` to the resolved pan123 download URL.
- Reuse the existing `ResolveDownloadUrlService`, cache, and error mapping.
- Keep the existing `/d/{path}?file_id=...` route unchanged.

## Non-Goals

- No Jellyfin, FNTV, or generic media-server support.
- No Alist STRM support.
- No Web UI injection, CSS/JS modification, external player buttons, subtitle
  conversion, image caching, or client filtering.
- No proactive pan123 download URL preheating.
- No schema migration.
- No change to the current STRM file format.

## Configuration

Add an `emby_proxy` configuration section:

```yaml
emby_proxy:
  enable: false
  host: 0.0.0.0
  port: 8097
  upstream_base_url: http://127.0.0.1:8096
  api_key: ""
```

`enable` controls whether the extra proxy server is started. The existing media
server keeps serving `/d/...` on its current address. The Emby proxy listens on
its own address and proxies from `/` to the upstream Emby root.

`api_key` is used only for server-side supplemental Emby API calls when the
proxied response does not contain enough STRM path data to decide whether a
media source is a `bigbrother` STRM.

## Architecture

Add a new HTTP module, likely `app/src/interface/http/emby_proxy.rs`.

The app runtime will own two optional HTTP tasks:

- Existing media server: serves `/d/{path}?file_id=...`.
- New Emby proxy server: serves all paths at root and forwards to upstream
  Emby.

The proxy router uses route classification before fallback proxying:

- `GET` or `POST /Items/{item_id}/PlaybackInfo`
  - Forward to upstream Emby.
  - Read the upstream response body.
  - If it is a supported JSON response, modify matching media sources.
  - Return the modified response with updated content headers.
  - If it cannot be parsed or does not match, return the upstream response
    unchanged.
- `GET /Videos/{item_id}/stream` and `GET /Videos/{item_id}/original`
  - Try to identify the requested media source as a `bigbrother` STRM.
  - If identified, resolve the embedded `file_id` and return `302 Found`.
    Resolution errors use the same error response mapping as `/d`.
  - If not identified, transparently proxy the request.
- All other paths
  - Transparently proxy the request to upstream Emby.

## STRM Recognition

A `bigbrother` STRM is recognized by parsing a URL-like string and confirming:

- It points to the configured `advertise_base_url` plus the configured
  `strm_path_prefix`, or otherwise matches the local `/d/...` route shape after
  proxy rewriting.
- It contains a valid integer `file_id` query parameter.

The parser should live outside the HTTP handler, for example in an application
or domain helper, so tests can cover it without running a server.

The first implementation should inspect `PlaybackInfo.MediaSources[*].Path`,
`MediaSources[*].DirectStreamUrl`, and related URL fields already present in the
proxied response. Only if those fields are missing or insufficient should it call
the upstream Emby `Items` API with `Fields=Path,MediaSources`.

## PlaybackInfo Rewriting

For a matching `MediaSource`, set playback metadata so Emby clients choose the
proxy-interceptable direct stream path:

- `SupportsDirectPlay = true`
- `SupportsDirectStream = true` when the field exists or is needed by the JSON
  shape
- `SupportsTranscoding = false`
- Remove or null out transcoding-specific fields such as `TranscodingUrl`,
  `TranscodingContainer`, `TranscodingSubProtocol`, and related live-start
  fields when present
- Set `DirectStreamUrl` to a proxy-local Emby stream path:

```text
/Videos/{item_id}/stream?MediaSourceId={media_source_id}&Static=true&<emby-token-query>
```

The token query should preserve the original Emby authentication key from the
upstream `DirectStreamUrl` when available, supporting both `api_key` and
`X-Emby-Token` spellings case-insensitively.

The proxy should avoid logging the resolved pan123 URL or full signed URLs.

## Video Stream Interception

For `/Videos/{item_id}/stream` and `/Videos/{item_id}/original`, the proxy reads
`MediaSourceId` from the query string. Emby may use plain numeric IDs or
`mediasource_`-prefixed IDs, so matching strips that prefix for comparison.

The proxy then tries to find the requested media source:

1. Prefer data already available from cached or request-local `PlaybackInfo`
   state if the implementation introduces such a cache.
2. Otherwise call upstream Emby `Items` with `Fields=Path,MediaSources`.
3. If the selected source maps to a `bigbrother` STRM, extract `file_id`.
4. Call `ResolveDownloadUrlService::resolve(file_id)`.
5. Return `302 Found` with `Location` on success. The existing `/d` route keeps
   its current redirect status so existing STRM users are not affected.

If the media source cannot be found, is not STRM, or is not a `bigbrother` STRM,
the request is proxied to upstream Emby unchanged.

## Error Handling

- Invalid or missing proxy configuration fails startup with a clear runtime
  error.
- Upstream Emby connection failures return `502 Bad Gateway` from the proxy.
- Non-JSON or unexpected `PlaybackInfo` responses are returned unchanged.
- Non-`bigbrother` STRM, non-STRM, missing `MediaSourceId`, or unparseable
  `file_id` fall back to upstream proxying.
- Once a request is confirmed as a `bigbrother` STRM, download URL resolution
  failures use the existing mapping:
  - Unauthorized -> 401
  - Not found -> 404
  - Dependency error -> 502
  - Rule rejected -> 422
  - Runtime/internal error -> 500
- HEAD requests should be proxied upstream in the first implementation.

## Testing

Add focused tests before implementation:

- Config parsing accepts the new `emby_proxy` section and preserves existing
  defaults.
- Disabled proxy does not require `upstream_base_url`.
- Root proxy forwards ordinary requests to a wiremock upstream.
- `PlaybackInfo` containing a `bigbrother` STRM is rewritten with direct-play
  fields and proxy-local `DirectStreamUrl`.
- `PlaybackInfo` for non-STRM or non-`bigbrother` STRM is returned unchanged.
- Token preservation handles `api_key`, `X-Emby-Token`, and case variants.
- `/Videos/{item_id}/stream` for a matching STRM returns the resolver redirect.
- `/Videos/{item_id}/stream` for missing `MediaSourceId`, unknown source,
  non-STRM, or non-`bigbrother` STRM is proxied upstream.
- Existing `/d/{path}?file_id=...` tests continue to pass.

## Rollout

The first release should default `emby_proxy.enable` to `false`. Users can keep
using current STRM files and `/d` directly. To enable the new behavior, they
point Emby clients at the Emby proxy port while the proxy points to the real
Emby upstream.
