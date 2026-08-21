# Agent Notes

## Pointers

- **Domain** — before exploring, or when naming or changing import, subscription, file-index, or media-source concepts: `docs/agents/domain.md`
- **Triage** — when classifying an issue or deciding whether to implement: `docs/agents/triage-labels.md`
- **Issues** — when creating, reading, labeling, commenting, or closing GitHub issues: `docs/agents/issue-tracker.md`
- **Release** — when tagging, bumping version, or running `make release` / `make release-tag` / `make changelog`: `docs/agents/release.md`

## Architecture

- `domain`: policy. No deps on `application`, `infrastructure`, or `interface`.
- `application`: use cases. Depends on `domain` only. Ports live in `application/ports`.
- `infrastructure`: adapters. May depend on `domain` and `application`.
- `interface`: CLI, Telegram, HTTP. Composition root. May depend on other modules.
- `error`: lowest. No module deps.

Ports (traits) live in `application/ports`. Adapters live in `infrastructure`. Composition root is `interface`.
`infrastructure/client` holds third-party API and protocol calls only. Keep third-party field names there; translate in adapters. Match adjacent modules instead of adding a shared layer.

The root crate is `bigbrother`. `web/` is the Svelte console; `make build` compiles it.

## Errors

Use `AppError` and `AppResult`. Map new failures into existing variants; changing the enum is explicit. Convert adapter errors at the boundary. Keep transport rendering in `interface`.

## Workflow

Implement only `ready-for-agent` work. Missing outcome, scope, acceptance, or verification is `needs-info`.

If the current branch is `main` or `master`, create a working branch before edits, commits, or PR prep.

Default run: `cargo run -- server --data-dir ./data`.

Before commit or PR: targeted tests first; `make lint` for any code change; `make test` unless the change is a small isolated fix. State skipped checks and why.

Commits use `feat:`, `fix:`, `refactor:`, `support:` with an imperative subject. PRs include a summary, linked issue, and test evidence. User-visible Telegram/HTTP/playback changes need a concrete interface transcript or payload.

Runtime config lives at `<data-dir>/config/config.yaml`. `data/` is local SQLite, cache, and logs. Committed samples use placeholders; do not copy from `data/`. Redact tokens, chat IDs, cookies, and drive credentials.
