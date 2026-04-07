# BigBrother

BigBrother is a Rust workspace for importing media from cloud shares into a local library and keeping `.strm` files in sync for playback. It combines a Telegram bot, a small redirect server, TMDB metadata lookup, and a SQLite-backed cache/database.

## What it does

- Imports media from supported share links, fslinks, or JSON files sent to the Telegram bot
- Parses and normalizes media names before saving library data
- Syncs a remote Pan123 library into a local `.strm` library tree
- Serves redirect URLs for `.strm` playback through a local HTTP server
- Stores keywords, cache entries, and events in SQLite with SeaORM migrations

## Project layout

- `app/`: main application crate
- `migration/`: SeaORM migrations
- `config/config.yaml`: sample configuration
- `tools/`: helper scripts for entity generation and database migration
- `doc/`: request notes and reference material

## Requirements

- Rust toolchain with Cargo
- A Telegram bot token and your Telegram user ID
- A TMDB API key
- A Pan123 account
- A writable data directory for config, logs, cache, and SQLite data

## Configuration

The application reads configuration from `<data-dir>/config/config.yaml`. If the file does not exist, the app starts with empty defaults, but the bot and sync features will not work until required values are set.

Start from [`config/config.yaml`](config/config.yaml) and place a populated copy at `./data/config/config.yaml` or another `--data-dir` location:

```yaml
media_server:
  host: 0.0.0.0
  port: 3100
  advertise_base_url: http://127.0.0.1:3100
  strm_path_prefix: /d

pan123:
  passport: your-account
  password: your-password

tmdb:
  api_key: your-tmdb-api-key

telegram:
  bot_token: your-bot-token
  user_id: 123456789

library:
  remote_path: /remote/library
  local_path: /local/library
```

Config fields:

- `media_server.host` / `media_server.port`: bind address for the redirect server. Defaults to `0.0.0.0:3100`.
- `media_server.advertise_base_url`: external base URL written into generated `.strm` files. Defaults to the bind address.
- `media_server.strm_path_prefix`: URL prefix for redirect endpoints. Defaults to `/d`.
- `pan123.*`: credentials used for listing files, downloading subtitles, and generating download URLs.
- `tmdb.api_key`: used during media import and metadata enrichment.
- `telegram.*`: bot credentials plus the only user allowed to run commands directly.
- `library.remote_path`: source root on Pan123.
- `library.local_path`: destination root for generated `.strm` and subtitle files.

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

## Telegram bot usage

The bot accepts direct commands from the configured `telegram.user_id` and can also monitor channel posts by matching stored keywords.

Supported commands:

- `/help`
- `/list_keywords`
- `/add_keyword <keyword>`
- `/delete_keyword <keyword>`
- `/sync_strm`

Supported message inputs:

- share URLs recognized by the importer
- fslink lines
- `.json` files up to 10 MB

## STRM redirect flow

`/sync_strm` scans `library.remote_path` on Pan123 and mirrors video entries into `library.local_path` as `.strm` files. Each generated file points to the local redirect server:

```text
http://<advertise-base-url>/<strm-path-prefix>/<remote/path>?file_id=<id>
```

When a player opens that URL, BigBrother resolves the current Pan123 download URL, caches it for 30 minutes, and redirects the client.

## Architecture snapshot

The current runtime is split into a small bootstrap layer plus use-case services:

- `app/src/main.rs` parses CLI input, starts the server command, and owns long-running background tasks.
- `app/src/bootstrap.rs` converts bootstrap-only `AppState` data into an `AppRuntime` made of dedicated bot, server, cache, and event-bus contexts.
- `app/src/application/` contains use-case services such as `SyncStrmService`, `ManageKeywordsService`, `ImportMediaService`, and `ResolveDownloadUrlService`.
- `app/src/bot/` and `app/src/server/` consume focused runtime/context objects instead of reaching into `AppState` directly.
- `doc/architecture-refactor-blueprint.md` records the refactor plan, current status, and remaining cleanup opportunities.

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
- [`tools/migrate_db.sh`](tools/migrate_db.sh): run migration helpers

## License

MIT
