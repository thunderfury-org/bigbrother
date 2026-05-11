# Directory Structure

> How backend code is organized in this project.

---

## Overview

The `app` crate follows a **4-layer (hexagonal) architecture** with clear dependency boundaries. Each layer has a single responsibility and depends only inward.

---

## Directory Layout

```
app/src/
  main.rs                 # Entry point: CLI parse → server or command
  config.rs               # YAML config deserialization (Manager struct)
  error.rs                # AppError enum + AppResult<T> alias
  logger.rs               # tracing init (file + access log + panic hook)
  bootstrap/
    mod.rs                # AppContext, AppRuntime — manual DI wire-up
  domain/                 # Pure models, NO IO dependencies
    import/               # Import models, policies, path logic
    library/              # Sync plan, path mapping
    media/                # Parser, normalize, language detection
  application/            # Use cases / business logic services
    ports.rs              # Port traits (Repository, Service interfaces)
    import/               # Complex import workflow (largest module)
    manage_keywords.rs
    file_index.rs
    sync_strm.rs
    share_crawler.rs
    resolve_download_url.rs
    delete_media.rs
  infrastructure/         # Adapters — external lib implementations
    client/               # HTTP clients (pan115, pan123, pan189, quark, tmdb)
    entity/               # SeaORM models + query functions (auto-generated)
    repo/                 # Repository impls (thin wrappers over entity)
    cache/                # DB-backed cache
    event/                # Event store (SeaORM-backed)
    event_bus/            # In-process pub/sub
    fs/                   # Filesystem operations
    import/               # Gateway implementations for import
    services.rs           # Type aliases binding generics → concrete types
    telegram/             # Telegram sender
  interface/              # Inbound adapters
    cli/                  # CLI commands (clap derive)
    http/                 # Axum routers (media redirect, emby proxy)
    telegram/             # Telegram bot handler
    import.rs             # CLI import orchestration
  util/                   # signal handling, time guard
```

---

## Layer Dependency Rules

```
interface → application → domain
    ↓           ↓
infrastructure ─┘
```

- **domain/**: Zero IO. Pure types + logic. No imports from other layers.
- **application/**: Depends on domain + port traits only. Never imports infrastructure.
- **infrastructure/**: Implements port traits using SeaORM, reqwest, etc.
- **interface/**: Wires infrastructure types to application services via type aliases.

---

## Naming Conventions

- **Modules/dirs**: snake_case (`manage_keywords`, `file_index`)
- **Files**: snake_case matching module name (`manage_keywords.rs`)
- **Structs/Traits**: PascalCase (`ManageKeywordsService`, `KeywordRepository`)
- **Type aliases**: PascalCase, descriptive (`AppResult<T>`, `KeywordService`)
- **Constants**: SCREAMING_SNAKE_CASE (`NO_NEW_MEDIA_MESSAGE`)

---

## Key Files as Examples

- Layer wire-up: `app/src/infrastructure/services.rs`
- DI bootstrap: `app/src/bootstrap/mod.rs`
- Port definitions: `app/src/application/ports.rs`
- Clean service: `app/src/application/manage_keywords.rs`
