# Logging Guidelines (migration)

> Logging in the migration context.

---

## Logging

The migration crate does **not** use `tracing`. SeaORM's internal logging may output migration progress via its own mechanisms.

For manual logging during development, `println!` is acceptable in the `main.rs` entry point.

---

## Rules

- No structured logging setup in this crate — it's intentionally minimal
- If you need verbose migration output, enable SeaORM's sqlx logging via `ConnectOptions`
- Do not add `tracing` dependency to the migration crate
