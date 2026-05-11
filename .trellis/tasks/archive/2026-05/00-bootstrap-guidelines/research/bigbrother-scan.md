# Research: bigbrother Codebase Scan

- **Query**: Comprehensive codebase convention scan for the bigbrother project
- **Scope**: internal
- **Date**: 2026-05-11

## Findings

### 1. Top-Level Directory Structure

```
bigbrother/
  app/              # Main application crate (default workspace member)
  migration/        # SeaORM migration crate (workspace member)
  config/           # Example config files
  data/             # Runtime data directory
  docs/             # Documentation
  tools/            # Helper scripts (generate_entity.sh, migrate_db.sh)
  .trellis/spec/    # Existing coding convention specs
  Cargo.toml        # Workspace root
  Cargo.lock
  Dockerfile
  Makefile
  .editorconfig
  AGENTS.md / CLAUDE.md
```

**Correction from task description**: This is a **Rust** project, not Java/Kotlin. Uses Cargo workspace with two members: `app` (main) and `migration`.

### 2. Build System and Key Dependencies

**File**: `/Users/wzy/dev/bigbrother/Cargo.toml` (workspace root)
**File**: `/Users/wzy/dev/bigbrother/app/Cargo.toml` (app crate)

| Category | Crate | Version | Notes |
|---|---|---|---|
| HTTP framework | axum | 0.8 | |
| Async runtime | tokio | 1 (full features) | |
| ORM | sea-orm | 2.0.0-rc | sqlx-sqlite backend |
| Migrations | sea-orm-migration | 2.0.0-rc | |
| HTTP client | reqwest | 0.12 | rustls-tls, json, stream |
| Serialization | serde, serde_json, serde_yaml | 1.x | |
| Error handling | thiserror | 2 | |
| CLI | clap | 4.5 | derive feature |
| Logging | tracing + tracing-subscriber | 0.1 / 0.3 | |
| Telegram bot | teloxide | 0.17.0 | |
| Crypto | sha2, rsa, base64, hex | various | |
| Language detection | lingua | 1.7.2 | English, Chinese, Japanese |
| Testing (mocks) | wiremock | 0.6 | dev-dependency |

Workspace resolver: `"3"` (Rust Edition 2024).
Release profile: LTO enabled, codegen-units = 1.

### 3. Main Packages/Modules (app/src/)

The codebase follows a **layered architecture** with four primary layers:

```
app/src/
  main.rs           # Entry point: CLI parse -> server or command
  config.rs         # YAML config deserialization, Manager struct
  error.rs          # AppError enum, AppResult<T> alias
  logger.rs         # tracing init (file + access log)
  bootstrap/        # AppContext, AppRuntime (wiring/DI)
  domain/           # Pure domain models (no IO)
    import/         # import models, policies, path logic
    library/        # sync plan, path mapping
    media/          # parser, normalize
  application/      # Use cases / services (business logic)
    import/         # complex import workflow (largest module)
    manage_keywords.rs
    file_index.rs
    sync_strm.rs
    share_crawler.rs
    resolve_download_url.rs
    delete_media.rs
    ports.rs        # Port traits (repository and service interfaces)
  infrastructure/   # Adapters / external implementations
    client/         # HTTP clients (pan115, pan123, pan189, quark, tmdb)
    entity/         # SeaORM entity models + query functions
    repo/           # Repository implementations (SeaORM-backed)
    cache/          # DB-backed cache
    event/          # Event store (SeaORM-backed)
    event_bus/      # Pub/sub event bus
    fs/             # Filesystem operations
    import/         # Gateway implementations for import
    services.rs     # Type aliases wiring concrete implementations
    telegram/       # Telegram sender
  interface/        # Inbound adapters
    cli/            # CLI commands (clap)
    http/           # Axum routers (media redirect, emby proxy)
    telegram/       # Telegram bot handler
    import.rs       # CLI import orchestration
  util/             # signal handling, time guard
```

### 4. Architecture Patterns

#### Dependency Injection via Generics (Ports & Adapters)

The project uses **trait-based DI** without any DI framework. Pattern:

- **Ports** defined as async traits in `application/ports.rs` (e.g., `KeywordRepository`, `FileIndexRepository`, `DownloadUrlSource`, `FileStore`, `LibraryRemote`)
- **Services** are generic over the port trait: `ManageKeywordsService<R> where R: KeywordRepository`
- **Concrete implementations** in `infrastructure/` (e.g., `SeaOrmKeywordRepository`, `Pan123LibraryRemote`)
- **Type aliases** in `infrastructure/services.rs` bind generics to concrete types at the wire-up point

Example from `infrastructure/services.rs`:
```rust
pub type KeywordService = ManageKeywordsService<SeaOrmKeywordRepository>;
pub type FileIndexRuntimeService = FileIndexService<SeaOrmFileIndexRepository>;
```

**Wire-up** happens in `bootstrap/mod.rs` (`AppRuntime::from_app`), which manually constructs all service instances from a `RuntimeBootstrapInputs` struct containing `db`, `bot`, `cache`, `event_bus`, `clients`.

#### Error Handling

**File**: `/Users/wzy/dev/bigbrother/app/src/error.rs`

- Single `AppError` enum with 6 variants: `InvalidParameter`, `NotFound`, `Dependency`, `RuleRejected`, `Runtime`, `Internal`
- `AppResult<T>` = `Result<T, AppError>`
- `AppErrorKind` for pattern matching without extracting message
- `From` impls for `io::Error`, `serde_json::Error`, `RequestError`, `sea_orm::DbErr`
- HTTP mapping in `interface/http/media.rs`: `map_app_error_to_response` maps kinds to status codes (400, 404, 502, 422, 500)
- Domain-specific sub-errors like `DownloadUrlError` exist but convert to `AppError` at service boundaries

#### Logging

**File**: `/Users/wzy/dev/bigbrother/app/src/logger.rs`

- Uses `tracing` ecosystem
- Two log files: `bigbrother*.log` (app) and `access.http*.log` (HTTP access)
- Daily rotation, max 3 files retained
- Console mode (`init_console`) for CLI commands
- Custom panic hook logs panics via `tracing::error!` with backtrace
- Access log filtering by module target: `bigbrother::interface::http::log`

### 5. HTTP Patterns (interface/http/)

**File**: `/Users/wzy/dev/bigbrother/app/src/interface/http/media.rs`

- Axum Router with `State` extraction
- Handler functions are `async fn` returning `Response`
- Error responses built via `(StatusCode, message).into_response()`
- `map_app_error_to_response` provides centralized `AppError -> HTTP response` mapping
- Log layer + TraceIdLayer applied to router
- Server run in `interface/http/mod.rs` with graceful shutdown

### 6. Database Layer

**ORM**: SeaORM 2.0.0-rc with SQLite (`sqlx-sqlite`)
**Database**: SQLite (file-based, path from config)

#### Entity Models

**Dir**: `app/src/infrastructure/entity/model/` (auto-generated by `sea-orm-codegen`)

Tables:
- `keyword` -- simple key-value store
- `event` -- event store for pub/sub
- `cache` -- key-value cache with TTL
- `file_index` -- file hash records (md5/sha1 + size)
- `file_location` -- file path/name per index
- `file_description` -- description text with content hash
- `file_location_description` -- many-to-many join

#### Repository Pattern

**Dir**: `app/src/infrastructure/repo/`

Repositories are thin wrappers implementing port traits:
- `SeaOrmKeywordRepository` implements `KeywordRepository`
- `SeaOrmFileIndexRepository` implements `FileIndexRepository`

Actual SeaORM queries live in `infrastructure/entity/*.rs` functions (e.g., `entity::file_index::record_files`, `entity::keyword::list_all_keywords`).

#### Migrations

**Dir**: `/Users/wzy/dev/bigbrother/migration/`

Uses `sea-orm-migration` with `MigratorTrait`. Four migrations:
- `m20251210` -- keyword table
- `m20251219` -- event table
- `m20260130` -- cache table
- `m20260506` -- file_index, file_location, file_description, file_location_description

Naming convention: `m{YYYYMMDD}_{HHMMSS}_{description}.rs`

### 7. CLI Patterns

**File**: `/Users/wzy/dev/bigbrother/app/src/interface/cli/handler.rs`

- CLI handler manually wires dependencies for one-shot commands
- `connect_db` helper connects SQLite + runs migrations
- Commands output to stdout via `println!`
- User-facing messages in Chinese (e.g., "未找到匹配文件")

### 8. Code Style Conventions

#### From .editorconfig
**File**: `/Users/wzy/dev/bigbrother/.editorconfig`

- Default: 2-space indent, UTF-8, LF line endings
- **Rust files**: 4-space indent
- Makefile: 4-width tab indent

#### Naming Conventions

- **Modules**: snake_case (`manage_keywords`, `file_index`, `import_ports`)
- **Structs**: PascalCase (`ManageKeywordsService`, `SeaOrmFileIndexRepository`)
- **Trait methods**: snake_case, async (`list_all_keywords`, `add_keyword`)
- **Error variants**: PascalCase (`InvalidParameter`, `RuleRejected`)
- **Type aliases**: PascalCase, descriptive (`AppResult<T>`, `KeywordService`)
- **File names**: snake_case matching module name
- **Constants**: SCREAMING_SNAKE_CASE (`NO_NEW_MEDIA_MESSAGE`)

#### Package Organization

- Clean 4-layer separation: `domain/`, `application/`, `infrastructure/`, `interface/`
- Domain layer has no IO dependencies (pure types + logic)
- Application layer depends only on domain + port traits
- Infrastructure implements ports using external libs
- Interface handles inbound communication

### 9. Test Patterns

Tests are **inline** (`#[cfg(test)] mod tests` blocks), not in separate files. Dedicated test files exist only for complex import workflows:
- `app/src/application/import/group/tests.rs`
- `app/src/application/import/import_tests.rs`
- `app/src/application/import/metadata/tests.rs`
- `app/src/application/import/tmdb_info/tests.rs`
- `app/src/application/import/transfer_support/tests.rs`

**Key patterns**:

1. **Fake/Mock repositories**: Tests create lightweight fakes implementing port traits:
   ```rust
   #[derive(Clone, Default)]
   struct FakeKeywordRepo {
       keywords: Arc<Mutex<Vec<KeywordRecord>>>,
   }
   impl KeywordRepository for FakeKeywordRepo { ... }
   ```

2. **In-memory SQLite for integration tests**: Repositories use `sqlite::memory:` + run migrations:
   ```rust
   let mut options = ConnectOptions::new("sqlite::memory:");
   options.sqlx_logging(false);
   let db = Database::connect(options).await.unwrap();
   Migrator::up(&db, None).await.unwrap();
   ```

3. **wiremock** for HTTP client testing (dev-dependency)

4. **Async tests** use `#[tokio::test]`

5. **Temp directory helpers** in config tests (manual `TempConfigDir` with `Drop` cleanup)

6. **Test naming**: descriptive snake_case (`add_trims_keyword`, `rejects_duplicate_subscription_for_same_event`)

### 10. Anti-Patterns Avoided

- No `.unwrap()` in production code (only in tests)
- No `panic!` in business logic (panics are caught by custom hook)
- No direct `println!` in library/application code (only in CLI handler)
- No `Box<dyn Error>` -- uses typed `AppError` enum consistently
- No god objects -- services are small and single-responsibility
- No hard-coded secrets -- all from YAML config
- No synchronous blocking in async code (uses `tokio` throughout)
- Separates domain models from SeaORM models (entity layer is isolated)
- Event bus decouples notification from import logic

### 11. External Interfaces

The project integrates with:
- **Pan 115** (115网盘) -- cloud storage
- **Pan 123** (123云盘) -- cloud storage
- **Pan 189** (天翼云盘) -- cloud storage
- **Quark** (夸克网盘) -- cloud storage
- **TMDB** (The Movie Database) -- media metadata
- **Telegram Bot** -- notifications + commands
- **Emby** -- media server (optional proxy)

### 12. Existing Spec Files

Located at `/Users/wzy/dev/bigbrother/.trellis/spec/`:

- `bigbrother/backend/` -- index, directory-structure, database-guidelines, error-handling, logging-guidelines, quality-guidelines
- `migration/backend/` -- same structure for migration crate
- `guides/` -- code-reuse-thinking-guide, cross-layer-thinking-guide

## Caveats / Not Found

- No `rustfmt.toml` or `clippy.toml` found (relies on defaults + .editorconfig)
- No CI config found in the scanned scope (`.github/` exists but not explored)
- The project uses Chinese for user-facing messages throughout
- SeaORM entity files appear auto-generated (marked `@generated by sea-orm-codegen`)
- The `tools/` directory has shell scripts (`generate_entity.sh`, `migrate_db.sh`) not examined in detail
