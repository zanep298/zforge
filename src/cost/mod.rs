//! Per-spawn cost telemetry.
//!
//! Records one line per agent invocation to
//! `<project>/.zforge/cost-log.jsonl`. Format: append-only JSON Lines so
//! `tail -f` / `jq` / `grep` work out of the box.
//!
//! Token counts are **estimates** unless an agent emits a parseable usage
//! line (codex: `tokens used N` total — captured when present). Rough
//! conversion: 4 chars ≈ 1 token. Good enough for budget tracking; do not
//! reconcile against vendor bills line-by-line.
//!
//! Price table is baked into `prices.rs`. Override coming in a follow-up
//! PR via `~/.zforge/prices.yaml`.

pub mod log;
pub mod prices;
pub mod report;
pub mod schema;

pub use schema::CostEntry;
