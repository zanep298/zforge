# Skill: rust-patterns

## Purpose
Enforce idiomatic Rust in this project: ownership, error handling, type design,
and module organization. Read this before implementing any Rust task.

## Ownership and Borrowing

- Borrow (`&T`) by default — take ownership only when you need to store or consume
- Accept `&str` over `String`, `&[T]` over `Vec<T>` in function parameters
- Never clone to satisfy the borrow checker without understanding why

```rust
// good
fn word_count(text: &str) -> usize { text.split_whitespace().count() }

// bad — takes ownership when a borrow suffices
fn word_count(text: String) -> usize { text.split_whitespace().count() }
```

## Error Handling

- Use `Result<T, E>` and `?` everywhere — no `unwrap()` in production code paths
- Libraries: define typed errors with `thiserror`
- Application code: use `anyhow` for context propagation

```rust
// library error
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("order {0} not found")]
    NotFound(u64),
    #[error("insufficient stock: need {needed}, have {available}")]
    InsufficientStock { needed: u32, available: u32 },
}

// application code
fn load(path: &Path) -> anyhow::Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&raw).context("invalid config format")
}
```

## Type Design

- Newtype pattern for domain IDs — prevent mixing `UserId` and `OrderId`
- Enums over booleans for state — `Status::Active` not `is_active: bool`
- Make illegal states unrepresentable — put constraints in the type, not if-statements

```rust
struct UserId(u64);
struct OrderId(u64);

enum OrderStatus { Pending, Confirmed, Shipped, Cancelled }
```

## Module Organization

- Organize by feature, not by type — `auth/mod.rs` not `models/user.rs`
- Default to private — `pub(crate)` for internal sharing, `pub` only for public API
- Re-export the public API from `lib.rs` or the module root

## Iterators Over Loops

```rust
// good
let totals: Vec<f64> = orders.iter()
    .filter(|o| o.status == OrderStatus::Confirmed)
    .map(|o| o.total)
    .collect();
```

## Do Not Do

- No `unwrap()` or `expect()` in production paths — only in tests and truly unreachable branches
- No cloning to avoid thinking about lifetimes
- No `pub` on everything — keep surfaces small
- No `unsafe` without a `// SAFETY:` comment explaining every invariant
