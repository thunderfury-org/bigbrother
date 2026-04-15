# Repository Guidelines

## Project Structure & Module Organization
`bigbrother` is a Rust workspace with two crates: `app/` for the runtime and `migration/` for SeaORM migrations. Inside `app/src/`, code follows a layered layout: `domain/` for pure rules, `application/` for use cases, `infrastructure/` for adapters, `interface/` for CLI/HTTP/Telegram entrypoints, and `bootstrap/` for wiring. Use `config/config.yaml` as the config template, `data/` for local runtime state, `doc/` for architecture notes, and `tools/` for database/entity helper scripts.

## Build, Test, and Development Commands
Run repository commands through `rtk` per local agent instructions.

- `rtk make build`: build the default `app` crate.
- `rtk make build-release`: produce an optimized release build.
- `rtk make test`: run the full workspace test suite.
- `rtk make fmt`: format all Rust code.
- `rtk make lint`: enforce `rustfmt` and `clippy -D warnings`.
- `rtk cargo run -- server --data-dir ./data`: start the HTTP server, Telegram bot, migrations, and background tasks locally.

## Coding Style & Naming Conventions
Follow `.editorconfig`: UTF-8, LF endings, trim trailing whitespace, 4-space indentation in Rust, tabs in `Makefile`. Keep modules focused and aligned with the existing architecture boundaries. Prefer `snake_case` for files, modules, and functions; `PascalCase` for types and traits; and descriptive service names such as `SyncStrmService`. Run `rtk make fmt` before submitting changes.

## Testing Guidelines
Tests are standard Rust unit/integration-style module tests, usually colocated with the implementation or in sibling `tests.rs` files. Async cases use `#[tokio::test]`; HTTP/client boundaries often use `wiremock`. Name tests by behavior, for example `sync_strm_skips_non_video_entries`. At minimum, run `rtk make test` and `rtk make lint` before opening a PR.

## Commit & Pull Request Guidelines
Recent history favors short, imperative commit subjects such as `add client coverage tests` and `stabilize import test contract`. Keep commits focused and written in the imperative mood. PRs should include a concise summary, linked issue if applicable, test evidence, and config or behavior notes when touching bot, HTTP, or migration flows. If user-visible Telegram or HTTP behavior changes, include example commands, payloads, or screenshots.

## Security & Configuration Tips
Do not commit real credentials. Populate `<data-dir>/config/config.yaml` from `config/config.yaml`, and keep secrets such as Telegram tokens, TMDB API keys, and Pan123 credentials in local-only files or secret stores.
