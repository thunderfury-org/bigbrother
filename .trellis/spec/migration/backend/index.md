# Backend Development Guidelines (migration)

> SeaORM migration crate — database schema management.

---

## Overview

The `migration` crate manages database schema changes using `sea-orm-migration`. It is a standalone workspace member that runs migrations independently of the main `app` crate.

---

## Guidelines Index

| Guide | Description |
|-------|-------------|
| [Directory Structure](./directory-structure.md) | File layout and module registration |
| [Database Guidelines](./database-guidelines.md) | Migration writing conventions |
| [Error Handling](./error-handling.md) | Error propagation in migrations |
| [Quality Guidelines](./quality-guidelines.md) | Standards for migration code |
| [Logging Guidelines](./logging-guidelines.md) | Logging in migration context |

---

## Key Conventions (Quick Reference)

- File naming: `m{YYYYMMDD}_{HHMMSS}_{description}.rs`
- Each migration: one table or one logical change
- Register in `lib.rs` `Migrator::migrations()` vec
- Always provide both `up` and `down` (even if `down` is best-effort)
- Run via `tools/migrate_db.sh` or app startup
