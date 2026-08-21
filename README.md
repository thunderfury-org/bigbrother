# BigBrother

BigBrother is a Rust app for importing media from cloud shares into a local library and keeping `.strm` files in sync for playback. It combines a Telegram bot, an optional web console, a redirect server, TMDB metadata lookup, and a SQLite-backed cache/database.

## What it does

- Imports media from pan123, pan189, and pan115 share links, fslinks, or JSON files sent to the Telegram bot
- Gates channel imports through stored subscriptions
- Parses and normalizes media names before saving library data
- Serves an optional web console for import history, file index, subscriptions, and media directories
- Syncs a remote Pan123 library into a local `.strm` library tree
- Serves redirect URLs for `.strm` playback, with an optional Emby proxy
- Stores subscriptions, cache entries, and events in SQLite with SeaORM migrations

## Requirements

- Rust toolchain with Cargo
- A Telegram bot token and your Telegram user ID
- A TMDB API key
- Credentials for the cloud drives you use (pan123, pan189, and/or pan115)
- A writable data directory for config, logs, cache, and SQLite data

## Configuration

The application reads configuration from `<data-dir>/config/config.yaml`. If the file does not exist, the app starts with empty defaults, but bot, import, and sync features will not work until required values are set.

Copy [`config/config.yaml`](config/config.yaml) to `./data/config/config.yaml` or another `--data-dir` location and fill in the sections you need: `pan123`, `pan115`, `pan189`, `tmdb`, `telegram`, `library`, `media_server`, `console`, `emby_proxy`, and `openai`.

## Running

Start the app with the bundled server command:

```bash
cargo run -- server --data-dir ./data
```

On startup, BigBrother:

- creates the SQLite database under `<data-dir>/db/data.db`
- applies pending migrations automatically
- starts the HTTP redirect server
- starts the Telegram bot
- starts the event bus and cache cleanup task
- starts the web console when `console.enable` is true (default bind `0.0.0.0:3200`)
- starts the Emby proxy when `emby_proxy.enable` is true

## Telegram bot usage

The bot accepts direct commands from the configured `telegram.user_id` and can also monitor channel posts against stored subscriptions.

Supported commands:

- `/help`
- `/sync_strm`
- `/delete_media`

Supported message inputs:

- pan123, pan189, and pan115 share URLs
- fslink lines
- `.json` files up to 10 MB

## STRM redirect flow

`/sync_strm` scans `library.remote_path` on Pan123 and mirrors video entries into `library.local_path` as `.strm` files. Each generated file points to the local redirect server:

```text
http://<advertise-base-url>/<strm-path-prefix>/<remote/path>?file_id=<id>
```

When a player opens that URL, BigBrother resolves the current Pan123 download URL, caches it for 30 minutes, and redirects the client.

## Development

Common commands:

```bash
make build
make build-release
make test
make fmt
make lint
```

Helper scripts:

- [`tools/generate_entity.sh`](tools/generate_entity.sh): regenerate SeaORM entities

## Releasing

See [`docs/agents/release.md`](docs/agents/release.md).

## License

MIT
