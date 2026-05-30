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

### Working Branch

If the current branch is `main`, `master`, or another default branch, create the working branch before making code changes, commits, or PR preparation updates.

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

### Thinking Before Coding

Do not assume. Do not hide confusion. Surface tradeoffs.

- State assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them instead of picking silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what is confusing. Ask.

### Simplicity and Precision

Write the minimum code that solves the problem. Nothing speculative.

- Do not add features beyond what was asked.
- Do not create abstractions for single-use code.
- Do not add flexibility or configurability that was not requested.
- Do not add error handling for impossible scenarios.
- If code could be significantly shorter, rewrite it.

When editing existing code, touch only what is necessary. Do not improve adjacent code, comments, or formatting that is unrelated to the task. Match existing style even if you would do it differently. If your changes create unused imports, variables, or functions, remove them; do not remove pre-existing dead code unless asked.

Every changed line should trace directly to the user's request.

### Goal-Driven Execution

Define success criteria before implementing. Loop until verified.

Transform tasks into verifiable goals, for example:

- "Add validation" → write tests for invalid inputs, then make them pass
- "Fix the bug" → write a test that reproduces it, then make it pass
- "Refactor X" → ensure tests pass before and after

For multi-step tasks, state a brief plan with verification steps. Strong success criteria allow independent execution; weak criteria such as "make it work" require constant clarification.

### Module Dependency Constraints

These are architecture rules that agents must follow. They are enforced primarily by review and module boundaries, not by a dedicated automatic architecture checker.

- `domain` must not depend on `application`, `infrastructure`, or `interface`
- `application` may depend on `domain`, but must not depend on `interface` or `infrastructure`
- `infrastructure` may depend on `domain` and `application`, and is responsible for ports and external adapters
- `interface` may depend on other modules
- `error` is the lowest-level module and must not depend on other modules
- `infrastructure/client` is a stricter sub-area: it should only contain low-level API calls and protocol details, and must not absorb business orchestration, repository composition, cross-layer convenience logic, or dependencies on other modules
- Do not rename or reshape `infrastructure/client` fields just to match domain terminology when those fields mirror third-party protocols. Keep the third-party naming at the client boundary and translate it in adapters or mappers.

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

When running package-scoped Cargo commands, use the real crate package name instead of the directory name. In this workspace, the crate under `app/` is packaged as `bigbrother`, so prefer commands like `cargo test -p bigbrother` and `cargo clippy -p bigbrother --all-targets -- -D warnings` rather than `-p app`.

## Coding Style & Naming Conventions
Follow `.editorconfig`: use spaces, LF line endings, and a final newline; Rust files use 4-space indentation. Keep modules focused and aligned with the existing layer boundaries rather than adding cross-layer helpers. Use `snake_case` for files, modules, and functions, `PascalCase` for types, and verb-led names for services such as `SyncStrmService`. Format with `cargo fmt` and treat Clippy warnings as errors.

## Testing Guidelines
Prefer small, targeted Rust tests placed close to the code they verify. Cover the behavior surface that is actually at risk for the change, including domain parsing, import orchestration, transfer path rules, download URL resolution, proxy/redirect behavior, and Telegram or HTTP entrypoint behavior when applicable. Name tests by behavior, such as `parses_tv_episode_title`.

Verification strategy matters more than full manual startup. Use targeted tests first, then run broader checks such as `make test` or `make lint` when the scope justifies them. Do not treat “the server boots” as sufficient validation for a change.

Before creating a commit or pull request:

- Run targeted tests for the changed behavior first.
- If the change includes any code changes, run `make lint`.
- If the change is broader than a small, isolated fix, run `make test` unless the user explicitly agrees to narrower verification.
- If the expected checks are not run, tell the user exactly what was skipped and why before commit or PR creation.

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
