# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust workspace with two crates: `app/` for the runtime application and `migration/` for SeaORM migrations. The main code lives under `app/src/` and is split by layer: `domain/` for pure rules, `application/` for use cases, `infrastructure/` for external adapters, and `interface/` for CLI, Telegram, and HTTP entrypoints. Configuration samples live in `config/`, helper scripts in `tools/`, and longer design notes in `docs/`.

### Module Dependency Constraints

- `domain` 不能依赖 `application`、`infrastructure`、`interface`
- `application` 可以依赖 `domain`，不能依赖 `interface`
- `infrastructure` 可以依赖 `domain` 和 `application`，用于实现端口和外部适配
- `interface` 可以依赖 `application`，必要时通过 `application` 暴露的端口使用 `infrastructure` 组装出的服务
- `infrastructure/client` 只承载底层 API 调用，不依赖其他模块

## Build, Test, and Development Commands
Use the `Makefile` for common tasks:

- `make build` builds the default workspace target.
- `make build-release` builds optimized binaries.
- `make test` runs all Rust tests with `cargo test`.
- `make fmt` formats the workspace with `cargo fmt --all`.
- `make lint` enforces formatting and runs `cargo clippy -- -D warnings`.

Run the app locally with `cargo run -- server --data-dir ./data`.

## Coding Style & Naming Conventions
Follow `.editorconfig`: use spaces, LF line endings, and a final newline; Rust files use 4-space indentation. Keep modules focused and aligned with the existing layer boundaries rather than adding cross-layer helpers. Use `snake_case` for files, modules, and functions, `PascalCase` for types, and verb-led names for services such as `SyncStrmService`. Format with `cargo fmt` and treat Clippy warnings as errors.

## Testing Guidelines
Tests are primarily Rust unit/integration-style module tests placed close to the code, for example `app/src/application/import/metadata/tests.rs` and `app/src/application/import/group/tests.rs`. Prefer small, targeted tests for domain parsing, import grouping, and transfer path rules. Name tests by behavior, such as `parses_tv_episode_title`. Run all tests with `make test` before opening a PR.

## Commit & Pull Request Guidelines
Recent history follows Conventional Commit-style prefixes like `feat:`, `refactor:`, and `support:`. Keep subjects imperative and specific, for example `feat: add quark share importer`. Pull requests should include a short summary, linked issue or context, test evidence (`make test`, `make lint`), and sample bot/API behavior when user-visible flows change.

## Security & Configuration Tips
Do not commit real credentials. Keep runtime config under `<data-dir>/config/config.yaml`, and treat `data/` as local state for SQLite, cache, and logs. When changing import or redirect behavior, verify both Telegram-triggered flows and local HTTP playback paths.
