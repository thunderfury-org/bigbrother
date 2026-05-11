# Database Guidelines (migration)

> Migration writing conventions and patterns.

---

## Framework

**sea-orm-migration** 2.0.0-rc with SQLite backend. Uses `MigratorTrait` + `MigrationTrait`.

---

## Writing a Migration

### Step 1: Create the file

`m{YYYYMMDD}_{HHMMSS}_{description}.rs` — e.g., `m20260506_000000_create_table_file_index.rs`

### Step 2: Implement MigrationTrait

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MyTable::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(MyTable::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(MyTable::Name).string().not_null())
                    .col(ColumnDef::new(MyTable::CreatedAt).timestamp().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MyTable::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MyTable {
    Table,
    Id,
    Name,
    CreatedAt,
}
```

### Step 3: Register in lib.rs

Add `mod` declaration and register in `Migrator::migrations()`.

---

## Conventions

- **One table per migration** (or one logical change: add column, add index)
- **Always provide `down`** — even if best-effort, it helps rollback during development
- **Use `if_not_exists`** for `create_table` to be idempotent
- **Derive `DeriveIden`** for table/column enums — keeps identifiers type-safe
- **Timestamps**: use `timestamp` type, include `created_at` on every table
- **IDs**: `integer().not_null().auto_increment().primary_key()` for SQLite

---

## SQLite Limitations

- No `ALTER COLUMN` — use create-temp-table-migrate-drop-rename for column changes
- No `ADD CONSTRAINT` after creation — define all constraints in `create_table`
- Foreign keys enabled via `PRAGMA foreign_keys = ON` (app startup handles this)

---

## Testing Migrations

The `app` crate tests use in-memory SQLite with real migrations:

```rust
let mut options = ConnectOptions::new("sqlite::memory:");
let db = Database::connect(options).await.unwrap();
Migrator::up(&db, None).await.unwrap();
```

No separate migration tests needed — if the app tests pass with migrations, they work.
