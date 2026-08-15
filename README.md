# BigBrother

BigBrother is a Rust app for importing media from cloud shares into a local library and keeping `.strm` files in sync for playback. It combines a Telegram bot, a small redirect server, TMDB metadata lookup, and a SQLite-backed cache/database.

## What it does

- Imports media from supported share links, fslinks, or JSON files sent to the Telegram bot
- Parses and normalizes media names before saving library data
- Syncs a remote Pan123 library into a local `.strm` library tree
- Serves redirect URLs for `.strm` playback through a local HTTP server
- Stores subscriptions, cache entries, and events in SQLite with SeaORM migrations

## Project layout

- `src/domain/`: pure business rules and models
- `src/application/`: use-case services and ports
- `src/infrastructure/`: external adapters for storage, remote APIs, and delivery
- `src/interface/`: Telegram and HTTP entrypoints plus runtime-facing handlers
- `src/migration/`: SeaORM migrations
- `config/config.yaml`: sample configuration
- `tools/`: helper scripts for entity generation
- `docs/`: architecture notes and reference material

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

The bot accepts direct commands from the configured `telegram.user_id` and can also monitor channel posts against stored subscriptions.

Supported commands:

- `/help`
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

The crate is organized as a single hexagonal layout:

- `src/domain/` holds policy such as media parsing, import rules, and library path/sync planning.
- `src/application/` contains use-case services. External ports live in `src/application/ports/`.
- `src/infrastructure/` implements those ports as adapters for storage, remote APIs, and delivery.
- `src/interface/` is the composition root: CLI, Telegram, and HTTP assemble adapters into use-case services.
- `src/main.rs` parses CLI input and delegates to `interface`.

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

One-time setup:

```bash
cargo install git-cliff
```

Release workflow:

```bash
# 1. Ensure on main, up to date
git checkout main && git pull

# 2. Preview the changelog to decide the next semver level
make changelog

# 3. Execute the release (runs tests, lint, bumps version, generates changelog, commits and tags)
make release VERSION=0.2.0

# 4. Review the commit and tag
git log -1
git show v0.2.0

# 5. Push to trigger CI (Docker image build + GitHub Release)
git push --follow-tags
```

On push, CI will:
- Build a multi-arch Docker image and push it to `ghcr.io`
- Create a GitHub Release with the changelog section for the new version

## License

MIT
