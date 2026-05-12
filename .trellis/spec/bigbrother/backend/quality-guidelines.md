# Quality Guidelines

> Code standards, testing patterns, and forbidden practices.

---

## Code Style

- **Rust**: 4-space indent (per `.editorconfig`)
- **Line endings**: LF
- **Encoding**: UTF-8
- **No `rustfmt.toml`** — rely on defaults + `.editorconfig`
- **Always run `cargo fmt` before commit** — trellis check 必须包含 fmt

---

## Naming

- Modules/files: snake_case (`manage_keywords.rs`)
- Structs/traits/enums: PascalCase (`ManageKeywordsService`, `AppError`)
- Functions/methods: snake_case (`list_all_keywords`)
- Constants: SCREAMING_SNAKE_CASE (`NO_NEW_MEDIA_MESSAGE`)
- Type aliases: PascalCase, descriptive (`AppResult<T>`)

---

## Architecture Rules

- **domain/** must have zero IO imports — pure types + logic only
- **application/** depends on domain + port traits — never imports infrastructure
- **infrastructure/** implements ports — that's where SeaORM, reqwest, etc. live
- **interface/** is the outermost layer — handles HTTP, CLI, Telegram

---

## Dependency Injection

Services are generic over port traits, bound via type aliases at wire-up:

```rust
// application/manage_keywords.rs
pub struct ManageKeywordsService<R: KeywordRepository> { repo: R }

// infrastructure/services.rs
pub type KeywordService = ManageKeywordsService<SeaOrmKeywordRepository>;
```

Do not add a DI framework. This pattern is intentional and sufficient.

---

## Forbidden Patterns

| Pattern | Why |
|---|---|
| `.unwrap()` in production code | Panics on error — use `?` or `match` instead |
| `panic!` in business logic | Unwinds the runtime — return `AppError` instead |
| `Box<dyn Error>` | Lose type information — use typed `AppError` enum |
| `println!` outside CLI | Breaks structured logging — use `tracing` macros |
| Synchronous blocking in async | Blocks the Tokio runtime — use `tokio::task` or async equivalents |
| Hard-coded secrets/config | Read from YAML config via `config.rs` |
| SeaORM types in domain/application | Leaks infrastructure — convert at repo boundary |
| God objects | Keep services single-responsibility |

---

## Testing

### Patterns

Tests are **inline** (`#[cfg(test)] mod tests`), not separate files. Exception: complex import workflows get dedicated test files under `application/import/*/tests.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct FakeKeywordRepo {
        keywords: Arc<Mutex<Vec<KeywordRecord>>>,
    }

    #[async_trait]
    impl KeywordRepository for FakeKeywordRepo {
        async fn list_all_keywords(&self) -> AppResult<Vec<KeywordRecord>> {
            Ok(self.keywords.lock().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn test_add_keyword() {
        let repo = FakeKeywordRepo::default();
        let svc = ManageKeywordsService::new(repo.clone());
        svc.add_keyword("test").await.unwrap();
        assert_eq!(repo.keywords.lock().unwrap().len(), 1);
    }
}
```

### Test Conventions

- Use `#[tokio::test]` for async tests
- Fake repos: `Arc<Mutex<Vec<T>>>` pattern for in-memory state
- Integration tests: in-memory SQLite (`sqlite::memory:`) + run migrations
- HTTP mocking: `wiremock` crate
- Test naming: descriptive snake_case (`add_trims_keyword`, `rejects_duplicate`)
- `.unwrap()` is fine in tests (not production)

---

## User-Facing Messages

All user-facing strings (Telegram bot replies, CLI output) should be in **Chinese**.
