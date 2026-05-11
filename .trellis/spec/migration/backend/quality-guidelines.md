# Quality Guidelines (migration)

> Code standards for migration files.

---

## Code Style

Same as the main project: 4-space indent, snake_case files, PascalCase types.

---

## Migration Rules

- **Never modify an existing migration** that has been deployed — create a new one instead
- **Keep migrations atomic** — one logical change per migration file
- **Always implement both `up` and `down`** — `down` can be best-effort but must exist
- **Use `DeriveIden` enums** for table/column identifiers — never use raw strings
- **Test by running the full app** — the `app` crate's in-memory SQLite tests exercise all migrations

---

## Naming

- File: `m{YYYYMMDD}_{HHMMSS}_{snake_case_description}.rs`
- Description should be clear: `create_table_cache`, `add_index_file_hash`
- Timestamps should reflect when the migration was written (not when it runs)

---

## Review Checklist

Before merging a new migration:
- [ ] File follows naming convention
- [ ] Registered in `lib.rs` (mod + migrations vec)
- [ ] `up` creates/modifies the intended schema
- [ ] `down` reverses the change
- [ ] No data loss risk (or risk is documented)
- [ ] SQLite-compatible DDL (no unsupported ALTER operations)
