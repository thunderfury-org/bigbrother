# Repository Guidelines

## Repository Facts

### Workspace Layout

This repository is a Rust workspace with two crates: `app/` for the runtime application and `migration/` for SeaORM migrations. The main code lives under `app/src/` and is split by layer: `domain/` for pure rules, `application/` for use cases, `infrastructure/` for external adapters, and `interface/` for CLI, Telegram, and HTTP entrypoints. Configuration samples live in `config/`, helper scripts in `tools/`, and `docs/` contains mixed project knowledge, including design notes, agent guidance, and request/protocol reference material.

### Agent Skills

#### Issue tracker

Issues and PRDs for this repo live in GitHub Issues. See `docs/agents/issue-tracker.md`.

#### Triage labels

Use the default five-label vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

#### Domain docs

Single-context layout. See `docs/agents/domain.md`.

## Agent Rules

### Requirement Readiness

Only start implementation when the requirement is specific enough that an agent does not need to guess about the intended outcome.

A requirement is `ready-for-agent` only when all of the following are true:

- the problem and desired outcome are stated clearly
- the in-scope work and non-goals are stated clearly enough to avoid accidental scope expansion
- acceptance criteria are defined in behavior terms
- the affected entrypoints, modules, or surfaces are identified when they are known
- dependencies, prerequisites, configuration needs, and rollout constraints are identified when they affect implementation
- there are no unresolved product or architecture decisions that would materially change the implementation approach
- the expected verification method is stated, such as targeted tests, manual flow checks, or specific regressions to avoid

If any of the above is missing and blocks confident implementation, the requirement is not ready and should be treated as `needs-info` rather than `ready-for-agent`.

### Module Dependency Constraints

These are architecture rules that agents must follow. They are enforced primarily by review and module boundaries, not by a dedicated automatic architecture checker.

- `domain` must not depend on `application`, `infrastructure`, or `interface`
- `application` may depend on `domain`, but must not depend on `interface` or `infrastructure`
- `infrastructure` may depend on `domain` and `application`, and is responsible for ports and external adapters
- `interface` may depend on other modules
- `error` is the lowest-level module and must not depend on other modules
- `infrastructure/client` is a stricter sub-area: it should only contain low-level API calls and protocol details, and must not absorb business orchestration, repository composition, cross-layer convenience logic, or dependencies on other modules

When adding new functionality, prefer the organization style of adjacent existing modules rather than inventing new cross-layer helpers or another “shared” layer.

### Error Handling

Use `AppError` and `AppResult` as the unified application error boundary. Do not introduce new top-level error structs or parallel error enums for normal feature work.

- prefer mapping new failure cases into the existing `AppError` variants
- if a lower-level library or adapter has its own local error type, convert it back into `AppError` at the boundary instead of leaking it upward
- keep transport-specific rendering in `interface`, but keep the underlying failure classification in `AppError`
- if an existing `AppError` variant is insufficient, change should be explicit and justified; do not add ad hoc error structures just for one module

## Build, Test, and Development Commands
Use the `Makefile` for common tasks:

- `make build` builds the default workspace target.
- `make build-release` builds optimized binaries.
- `make test` runs all Rust tests with `cargo test`.
- `make fmt` formats the workspace with `cargo fmt --all`.
- `make lint` enforces formatting and runs `cargo clippy -- -D warnings`.

Use `cargo run -- server --data-dir ./data` as the default local startup example.

## Coding Style & Naming Conventions
Follow `.editorconfig`: use spaces, LF line endings, and a final newline; Rust files use 4-space indentation. Keep modules focused and aligned with the existing layer boundaries rather than adding cross-layer helpers. Use `snake_case` for files, modules, and functions, `PascalCase` for types, and verb-led names for services such as `SyncStrmService`. Format with `cargo fmt` and treat Clippy warnings as errors.

## Testing Guidelines
Prefer small, targeted Rust tests placed close to the code they verify. Cover the behavior surface that is actually at risk for the change, including domain parsing, import orchestration, transfer path rules, download URL resolution, proxy/redirect behavior, and Telegram or HTTP entrypoint behavior when applicable. Name tests by behavior, such as `parses_tv_episode_title`.

Verification strategy matters more than full manual startup. Use targeted tests first, then run broader checks such as `make test` or `make lint` when the scope justifies them. Do not treat “the server boots” as sufficient validation for a change.

## Commit & Pull Request Guidelines
Agent-authored commits should use Conventional Commit-style prefixes such as `feat:`, `fix:`, `refactor:`, and `support:`. Keep subjects imperative and specific, for example `feat: add quark share importer`.

Pull requests should include a short summary, linked issue or context, and test evidence such as `make test` or `make lint`. When user-visible flows change, include concrete evidence for the affected interface, such as bot interaction transcripts, sample request/response payloads, or redirect/playback behavior for HTTP and proxy flows.

## Security & Configuration Tips
Do not commit real credentials. Committed config samples must use placeholders only; never copy real values out of local runtime state. Treat tokens, chat IDs, cookies, and third-party drive credentials as secrets that must be redacted in code, config, logs, and PR text.

Keep runtime config under `<data-dir>/config/config.yaml`, and treat `data/` as local state for SQLite, cache, and logs. Do not backfill committed examples from `data/`.

When changing import, redirect, proxy, or delivery behavior, verify the relevant user-facing path, including Telegram-triggered flows and local HTTP playback or redirect paths when applicable.

## Preferred Practices

- If a guideline in this file conflicts with an existing ADR or a clearly established module pattern, surface the conflict explicitly instead of silently picking one.
- Prefer changes that keep policy in `domain`, orchestration in `application`, external integration in `infrastructure`, and transport concerns in `interface`.
