# Backend Development Guidelines (bigbrother)

> Rust/Axum backend — layered architecture with SeaORM + SQLite.

---

## Overview

The `app` crate is a Rust backend service using:
- **Axum** 0.8 (HTTP) + **Tokio** (async runtime)
- **SeaORM** 2.0.0-rc with SQLite
- **tracing** (structured logging)
- **clap** (CLI)
- **teloxide** (Telegram bot)

Architecture: clean 4-layer (domain → application → infrastructure → interface).

---

## Guidelines Index

| Guide | Description |
|-------|-------------|
| [Directory Structure](./directory-structure.md) | Module organization, layer rules, naming |
| [Database Guidelines](./database-guidelines.md) | SeaORM patterns, repo pattern, migrations |
| [Error Handling](./error-handling.md) | AppError enum, conversions, HTTP mapping |
| [Quality Guidelines](./quality-guidelines.md) | Forbidden patterns, testing, DI conventions |
| [Logging Guidelines](./logging-guidelines.md) | tracing setup, log levels, structured fields |

---

## Key Conventions (Quick Reference)

- 4-space indent for Rust files
- snake_case modules/files, PascalCase types
- Trait-based DI (no framework) — services generic over port traits
- Typed `AppError` enum, never `Box<dyn Error>`
- Inline tests with fake repos (`Arc<Mutex<Vec<T>>>`)
- User-facing messages in Chinese
- Domain layer has zero IO dependencies
