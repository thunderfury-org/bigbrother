# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BigBrother is a Rust-based backend application for managing and organizing TV show files and metadata. It monitors Telegram channels for media shares, fetches metadata from TMDB, and organizes content in cloud storage (123Pan) with local symlinks. The application runs two concurrent services: an Axum HTTP server for media redirects and a Teloxide Telegram bot for user interaction and channel monitoring.

## Common Commands

### Build and Development

```bash
# Build debug version
cargo build

# Build release version (with LTO optimization)
cargo build --release
# Or use Makefile
make build-release

# Run in debug mode
cargo run -- server --data-dir ./data

# Run release binary
./target/release/bigbrother server --data-dir ./data

# Format code
cargo fmt --all
# Or use Makefile
make fmt

# Lint code
cargo clippy -- -D warnings
# Or use Makefile (includes format check)
make lint
```

### Database Management

```bash
# Apply all pending migrations
cd migration && cargo run

# Generate new migration
cd migration && cargo run -- generate MIGRATION_NAME

# Check migration status
cd migration && cargo run -- status

# Rollback last migration
cd migration && cargo run -- down

# Fresh database (drop all tables and reapply)
cd migration && cargo run -- fresh

# Regenerate entity models from database
./tools/generate_entity.sh
```

Note: Database commands require `DATABASE_URL=sqlite:data/db/data.db?mode=rwc` which the tools set automatically.

## Architecture

### Workspace Structure

The project is a Rust workspace with two members:
- **app/**: Main application with web server, bot, and import logic
- **migration/**: SeaORM database migrations

### Key Dependencies

- **Web**: Axum 0.8 with Tower middleware
- **Bot**: Teloxide 0.17.0 for Telegram integration
- **Database**: SeaORM 2.0-rc with SQLite
- **HTTP**: Reqwest 0.12 with retry middleware
- **Logging**: Tracing with daily rolling logs
- **Config**: Serde + YAML

### Application State

The `AppState` struct is shared between the server and bot via `Arc`:
- `db`: SQLite database connection
- `config`: Configuration manager (loaded from `config/config.yaml`)
- `pan123`: 123Pan cloud storage client with OAuth token management
- `pan189`: Tianyi Pan189 cloud storage client
- `tmdb`: The Movie Database API client

### Concurrent Architecture

The application launches two tasks with `tokio::join!`:

1. **HTTP Server** ([server/mod.rs](app/src/server/mod.rs)): Serves media redirect endpoint at `GET /d/{path}?file_id=X` which fetches download URLs from 123Pan
2. **Telegram Bot** ([bot/mod.rs](app/src/bot/mod.rs)): Handles commands, channel posts, and user messages

Both tasks share the same `AppState` and can be terminated via SIGTERM/CTRL-C.

### Module Organization

- **bot/**: Telegram bot with command handlers, message processing, and channel post monitoring
  - **cmd.rs**: Command handlers (/help, /list_keywords, /add_keyword, /delete_keyword)
  - **msg.rs**: Message processor for extracting URLs, downloading files, triggering imports
  - **format.rs**: Telegram message formatting utilities
- **server/**: Axum HTTP server for media file redirects
- **client/**: External service integrations
  - **pan123.rs**: 123Pan API client with token caching and file operations
  - **pan189.rs**: Tianyi Pan189 API client for share file listing
  - **tmdb.rs**: TMDB API client for movie/TV metadata
- **library/**: Core media import engine (20KB transfer.rs)
  - **import.rs**: Main Importer with caching logic
  - **transfer.rs**: Complex file transfer orchestration
  - **group.rs**: Groups media files into movies/TV shows
  - **metadata.rs**: TMDB metadata fetching and caching
  - **share.rs**: Share URL detection (Pan123/Pan189)
  - **json.rs**: JSON and fslink format parsing
  - **library.rs**: Library path operations
- **media/**: Filename parsing to extract metadata (resolution, codec, episode info, etc.)
  - **parser.rs**: Advanced regex-based filename parsing
  - **normalize.rs**: Text normalization for titles
- **entity/**: SeaORM database models
  - **keyword.rs**: Keywords for channel post filtering
- **config.rs**: Configuration structure loaded from YAML
- **state.rs**: AppState initialization with service clients
- **logger.rs**: Daily rolling log files with ISO8601 timestamps

### Import Flow

The import flow is triggered by Telegram channel posts or direct messages:

1. **Detection**: Bot monitors channel posts for keywords (stored in DB)
2. **Extraction**: Extract share URLs, JSON documents, or fslinks from messages
3. **Listing**: Fetch media files from source (123Pan, Pan189, or JSON)
4. **Parsing**: Parse filenames to extract metadata (title, season, episode, resolution)
5. **Grouping**: Group videos with subtitles, organize by movie/TV/season
6. **TMDB**: Fetch metadata from TMDB API (cached in Importer)
7. **Transfer**: Upload to 123Pan library path via fast upload
8. **Local**: Create symlinks in local library path
9. **Summary**: Send formatted summary to Telegram user

### Configuration

Configuration is loaded from `{data_dir}/config/config.yaml` with these sections:
- **pan123**: Cloud storage credentials (client_id, client_secret, file_id)
- **pan189**: Tianyi Pan189 integration
- **tmdb**: API key for metadata
- **telegram**: Bot token and authorized user ID
- **library**: Remote path (in 123Pan) and local path (for symlinks)
- **media_server**: HTTP server host, port, advertise URL, strm path prefix

Directory structure:
```
data_dir/
├── config/config.yaml
├── db/data.db
├── cache/pan123/token.json  # OAuth token with expiration
└── log/bigbrother.YYYY-MM-DD.log  # Daily rolling logs (max 7 files)
```

### Token Management

The pan123 client manages OAuth tokens with expiration:
- Tokens cached at `{cache_dir}/pan123/token.json`
- Read-write lock for concurrent access
- Automatic refresh when expired
- Token includes access_token, refresh_token, and expiration timestamp

### Error Handling

Custom `AppError` enum with three variants:
- `InvalidParameter`: Invalid input
- `NotFound`: Resource not found
- `Internal`: Internal errors with context

Errors are logged with tracing and returned as appropriate HTTP status codes (400, 404, 500).

### Database Schema

Current tables:
- **keyword**: User-configured keywords for channel post monitoring (id, value, create_time)

When adding new tables:
1. Generate migration: `cd migration && cargo run -- generate NAME`
2. Edit migration file in [migration/src/](migration/src/)
3. Apply migration: `cd migration && cargo run`
4. Regenerate entities: `./tools/generate_entity.sh`

### Media Filename Parsing

The [media/parser.rs](app/src/media/parser.rs) module extracts structured metadata from filenames:
- File type (video/subtitle) and extension
- TMDB ID, titles with language codes, year
- Season/episode numbers for TV shows
- Resolution (2160p, 1080p, 720p), frame rate, quality (BluRay, WEB-DL)
- HDR format, video/audio codecs, release group
- Subtitle languages

This metadata is used for grouping files and fetching TMDB information.

### External Service Integration

All service clients use:
- `reqwest` with retry middleware (exponential backoff)
- Async/await with Tokio runtime
- Tracing for request logging
- Custom error types mapped to `AppError`

**123Pan client** features:
- OAuth2 token management with automatic refresh
- File operations: list, search, upload (direct and fast), mkdir, trash
- Share file listing with pagination
- Download URL generation (valid for limited time)

**TMDB client** features:
- Movie/TV search by title and year
- Detailed metadata fetch by ID
- Language-aware queries (Chinese support)
- Adult content enabled by default

### Logging

Tracing configuration:
- Daily rolling log files in `{data_dir}/log/`
- Max 7 files retained (older files deleted)
- INFO level by default
- ISO8601 timestamps with millisecond precision
- Panic hook captures backtraces when available
- All HTTP requests traced with method, path, and duration

## Development Notes

### When Modifying the Bot

- The bot only responds to the authorized user ID from config
- Channel post handling checks keywords from database before processing
- Message processing extracts multiple URL types (share URLs, JSON downloads, fslinks)
- Always send confirmation messages back to user after operations

### When Adding New Clients

- Follow the pattern in [client/](app/src/client/): struct with methods, async functions, error handling
- Use `reqwest_middleware` for automatic retries
- Add to `AppState` in [state.rs](app/src/state.rs) initialization
- Consider token caching if the API uses OAuth

### When Modifying Import Logic

- The [library/transfer.rs](app/src/library/transfer.rs) file (20KB) is the core orchestrator
- Import flow handles duplicates by comparing file sizes
- TMDB metadata is cached in the Importer struct to avoid redundant API calls
- Group files carefully: videos must match with their subtitles based on episode numbers
- Local symlinks point to 123Pan virtual mount paths (ensure paths are accessible)

### When Adding Database Models

1. Create migration in [migration/src/](migration/src/)
2. Apply migration to dev database
3. Run `./tools/generate_entity.sh` to update models
4. Add business logic methods to entity modules in [app/src/entity/](app/src/entity/)
5. Use SeaORM query builder with proper error handling

### Testing Approach

Currently no automated tests. When testing manually:
- Use a test data directory with separate config/database
- Monitor logs in `{data_dir}/log/` for detailed tracing
- Test bot commands via Telegram direct messages
- Test imports with small sample files first
- Verify file structure in both 123Pan and local paths
