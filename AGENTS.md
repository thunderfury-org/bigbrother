# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`bigbrother` is a Rust workspace for media library management, integrating with multiple cloud storage services (Pan123, Pan115, Pan189) and TMDB. It imports media via share URLs or JSON/CAS files, generates STRM files for Emby, and runs as a server with a Telegram bot interface.

## Build, Test, and Lint

```bash
make build          # cargo build
make build-release  # cargo build -r
make test           # cargo test
make fmt            # cargo fmt --all
make lint           # cargo fmt --check && cargo clippy -D warnings
```

Single test: `cargo test <test_name>`

## Repository Structure

Two crates:

- `app/` — Main application: runtime, HTTP server, Telegram bot, import pipeline, sync, cleanup, file index.
- `migration/` — SeaORM database migrations (SQLite).

Within `app/src/`, the code is layered:

- `domain/` — Pure business logic (import rules, media grouping, policy, sync plans).
- `application/` — Use cases (import pipeline, STRM sync, delete media, keyword management, file index, emby proxy, download URL resolution). Ports are traits in `application/ports.rs` and `application/import_ports.rs`.
- `infrastructure/` — Adapters: SeaORM entities/repo, API clients (pan123, pan115, pan189, tmdb), HTTP client (reqwest + retry middleware), event bus, cache, filesystem.
- `interface/` — Entry points: Telegram bot (`interface/telegram/`), Axum HTTP server (`interface/http/`), CLI (`interface/cli/`).
- `bootstrap/` — Wiring in `bootstrap/app.rs` (AppContext, Client, RuntimeBootstrapInputs) and `bootstrap/services.rs` (type aliases and builder functions for services).
- `config/` — YAML config loading from `<data-dir>/config/config.yaml`.

Data directory layout at runtime: `<data-dir>/{config,db,log,cache,pan123,pan189,ingest}`.

## Architecture Constraints

This project follows **Hexagonal Architecture (Ports and Adapters)**. Layer boundaries must be respected.

### Dependency Direction

```
domain ← application ← infrastructure
                   ↖ interface ↗
                     bootstrap (composition root)
```

Arrows mean "depends on". The dependency must never reverse:

- **domain/** — Zero imports from other project layers. Only external crates.
- **application/** — Imports from `domain` only. Defines port traits; never imports concrete infrastructure types.
- **infrastructure/** — Imports from `application` only (implements port traits). Never imports from `interface` or `bootstrap`.
- **interface/** — Imports from `application` (via trait bounds) and `infrastructure` for adapter wiring. Never imports from `bootstrap`.
- **bootstrap/** — The composition root. Imports from everything. Only wires dependencies and starts runtimes.

### Layer Responsibilities

| Layer | Owns | Must NOT contain |
|---|---|---|
| `domain/` | Business rules, value objects, domain logic | HTTP calls, DB queries, framework code |
| `application/` | Use cases, port traits (interfaces), service orchestration | Concrete API clients, DB repositories, Telegram API calls |
| `infrastructure/` | API clients, repositories, event bus, cache, filesystem adapters | Business logic, route handlers |
| `interface/` | HTTP routes, Telegram command handlers, CLI parsing, event delivery handlers | Business logic, dependency construction |
| `bootstrap/` | `AppContext` creation, service wiring, runtime assembly and startup | Handler logic, business rules, API calls |

### Key Rules

1. **bootstrap is wiring only.** It must not implement handler functions, business logic, or contain Telegram/HTTP API calls. If a function in bootstrap calls `bot.get_file()`, `bot.download_file()`, or performs keyword matching, it belongs in `interface/` or `application/`.
2. **Port traits live in application/.** All external boundaries (repos, API clients, caches) are defined as traits in `application/ports.rs` or `application/import_ports.rs`. Infrastructure provides implementations.
3. **Services are generic over ports.** Application services accept port traits as generic parameters, never concrete infrastructure types. Concrete types are monomorphized in `bootstrap/services.rs`.
4. **Event handlers live in interface/.** Event bus subscriber handlers (e.g., `on_process_media_sources`, `on_send_telegram_message`) belong in the interface layer, not in bootstrap.

## Key Architecture Notes

### Import Pipeline

`ImportMediaService` (application/import_media.rs) is the main import facade. It delegates to `ImportUseCaseFactory` which creates `ShareImportUseCase` or `JsonImportUseCase`. The pipeline: resolve source → collect files → build media files (metadata lookup) → TMDB match → group by media → transfer to library.

Share providers: pan123, pan115, pan189 (with CAS support). JSON/CAS files use `parse_files_from_json` in `domain/import/source.rs`.

### File Index

Stores every hash-identifiable resource seen from Telegram or CLI import. Four SQLite tables: `file_index` (identity: size/md5/sha1), `file_location` (file name/path with location_hash for dedup), `file_description` (content_hash for dedup), `file_location_description` (link).

Application service: `FileIndexService` handles upsert/search; `FileIndexIngestService` resolves sources and writes through. Telegram publishes `IndexFilesFromSource` events; CLI `import-share-url` indexes synchronously.

### Event Bus

`EventBus` (infrastructure/event_bus/) uses the `event` SQLite table. Workers subscribe to named events and retry on handler errors. The bus is started in `bootstrap/mod.rs` alongside the HTTP server and Telegram bot.

### Telegram Bot

`BotRuntime` holds all services. Messages are filtered by user ID. Commands are defined in `interface/telegram/cmd.rs` using teloxide's `BotCommands` derive. Channel monitoring triggers keyword matching; authorized user messages trigger import.

### Testing

Tests use `wiremock` for HTTP client boundaries and `sea-orm` in-memory SQLite for repository tests. Async tests use `#[tokio::test]`.

## Coding Conventions

- `.editorconfig`: UTF-8, LF, 4-space indentation for Rust, tabs for Makefile.
- `snake_case` for files/modules/functions, `PascalCase` for types/traits.
- Run `make fmt` before committing.
- `clippy -D warnings` is enforced; no warnings allowed.

## Config

Populate `<data-dir>/config/config.yaml` from `config/config.yaml`. Secrets (Telegram tokens, TMDB API keys, Pan123 credentials) must not be committed.
