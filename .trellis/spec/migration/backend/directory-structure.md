# Directory Structure (migration)

> File layout for the migration crate.

---

## Layout

```
migration/src/
  lib.rs                  # Migrator struct + migration registry
  main.rs                 # CLI entry point for running migrations
  m20251210_105056_create_table_keyword.rs
  m20251219_173900_create_table_event.rs
  m20260130_000000_create_table_cache.rs
  m20260506_000000_create_table_file_index.rs
```

---

## Module Registration

Every new migration file must be:
1. Declared as `mod m{...};` in `lib.rs`
2. Added to the `Migrator::migrations()` vec in chronological order

```rust
// lib.rs
mod m20260506_000000_create_table_file_index;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            // ... existing migrations ...
            Box::new(m20260506_000000_create_table_file_index::Migration),
        ]
    }
}
```

---

## Naming Convention

Format: `m{YYYYMMDD}_{HHMMSS}_{description}.rs`

- Use the date/time of creation (UTC)
- Description in snake_case, describes the change (e.g., `create_table_file_index`, `add_column_email`)
