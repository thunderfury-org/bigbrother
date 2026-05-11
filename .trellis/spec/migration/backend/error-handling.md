# Error Handling (migration)

> Error propagation in the migration crate.

---

## Error Type

Migrations use `sea_orm::DbErr` directly — there is no custom error type in this crate. All `MigrationTrait` methods return `Result<(), DbErr>`.

---

## Patterns

- Use `?` to propagate `DbErr` from schema manager calls
- No custom error conversion needed — `DbErr` is the final type
- If a migration fails, the entire migration run aborts (SeaORM behavior)

---

## Rules

- Never `.unwrap()` in migration code — use `?` to propagate errors
- Keep migration logic simple — complex business logic belongs in the `app` crate
- If a migration is destructive (drop table/column), document the risk in a comment
