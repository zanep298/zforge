# Skill: rust-testing

## Purpose
Enforce TDD-first Rust testing in this project. Read this before writing any test.
Test command: `{{test_command}}`

## Structure

- Unit tests: `#[cfg(test)] mod tests { ... }` inside the same file as the code
- Integration tests: `tests/` directory at crate root — each file is a separate binary
- Shared test utilities: `tests/common/mod.rs`

```
src/
  orders/
    mod.rs          ← #[cfg(test)] mod tests { ... } here
tests/
  order_flow.rs     ← integration test
  common/
    mod.rs          ← shared fixtures
```

## Unit Test Pattern

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_order_when_stock_is_zero() {
        // Arrange
        let repo = InMemoryRepo::with_stock(0);
        let service = OrderService::new(repo);

        // Act
        let result = service.place_order(OrderRequest { qty: 1, ..Default::default() });

        // Assert
        assert!(matches!(result, Err(OrderError::InsufficientStock { .. })));
    }
}
```

## Naming

Use descriptive names that explain the scenario, not what code is called:

```rust
// good
fn rejects_expired_token()
fn returns_empty_when_no_matches()
fn applies_discount_for_premium_users()

// bad
fn test_token()
fn test1()
fn test_order_service()
```

## Test-First Rule

1. Write the failing test — run `{{test_command}}` and confirm it fails
2. Write the minimal production code to make it pass
3. Run `{{test_command}}` again — confirm it passes
4. Refactor if needed — confirm tests still pass

Never write production code before the failing test exists.

## Temporary Files and Directories

Use `tempfile` crate for tests that touch the filesystem:

```rust
use tempfile::TempDir;

#[test]
fn reads_config_from_disk() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("config.toml"), "[db]\nurl = \"sqlite::memory:\"").unwrap();
    let cfg = Config::load(tmp.path()).unwrap();
    assert_eq!(cfg.db.url, "sqlite::memory:");
}
```

## Do Not Do

- No `#[ignore]` without a documented reason in a comment above the test
- No test that only passes because it catches the wrong error type — assert the specific variant
- No deleting tests that are hard to pass — fix the implementation instead
- No production code written before the failing test is confirmed
