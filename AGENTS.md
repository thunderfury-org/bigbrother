# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust workspace. The main application lives in `app/`, and database migrations live in `migration/`. Application code is organized by responsibility under `app/src/`, including `bot/`, `client/`, `library/`, `media/`, `server/`, and `entity/`. Configuration defaults live in `config/config.yaml`; helper scripts are in `tools/`; request notes and other reference material are in `doc/`. Tests are mostly inline unit tests within each Rust module, with fixture data under `app/src/media/testdata/`.

## Build, Test, and Development Commands
Use the `Makefile` for common tasks:

- `make build`: build the default workspace member (`app`).
- `make build-release`: compile the optimized release binary.
- `make test`: run all workspace tests with `cargo test`.
- `make fmt`: format all Rust code with `cargo fmt --all`.
- `make lint`: enforce formatting and fail on Clippy warnings.

For local execution, run `cargo run -- server --data-dir ./data`. For container builds, the repo includes a multi-stage `Dockerfile`.

## Coding Style & Naming Conventions
Rust uses edition 2024. Follow `.editorconfig`: 4-space indentation for `*.rs`, LF line endings, UTF-8, and a final newline. Keep modules and files in `snake_case`; use `CamelCase` for types and `SCREAMING_SNAKE_CASE` for constants. Prefer small, focused modules and derive-based Clap/Serde patterns consistent with the existing codebase. Run `make fmt` and `make lint` before opening a PR.

## Testing Guidelines
Add unit tests next to the code they validate using `#[cfg(test)]`. Use `#[tokio::test]` for async flows and `wiremock` when covering HTTP clients. Keep test names descriptive, such as `parses_episode_filename` or `returns_cached_entry`. When parser behavior depends on fixtures, place sample data in `app/src/media/testdata/`.

## Commit & Pull Request Guidelines
Recent history uses short, imperative commit subjects such as `sync strm` and `fix download url`, often followed by a PR number in parentheses. Keep subjects concise and lowercase when possible; separate unrelated changes into distinct commits. Pull requests should summarize behavior changes, mention config or migration impacts, link the relevant issue, and include logs or screenshots when bot/server output changes user-visible behavior.

## Configuration & Data
Do not commit secrets or local data directories. Keep API keys and service credentials in local config only, and use `--data-dir` to isolate test data from real state.
