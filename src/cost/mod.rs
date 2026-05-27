//! Per-spawn cost telemetry.
//!
//! Records one line per agent invocation to
//! `<project>/.zforge/cost-log.jsonl`. Format: append-only JSON Lines so
//! `tail -f` / `jq` / `grep` work out of the box.
//!
//! Token counts use REPORTED values when the agent emits a parseable usage
//! report:
//!
//! - claude `--output-format json` / `stream-json`: `usage` block with
//!   input / output / cache_read / cache_creation token counts.
//! - codex non-interactive: `tokens used N` line (total only — recorded
//!   as `reported_total_tokens` and surfaced in report total-token columns,
//!   not used to split input/output cost).
//!
//! When no report is parseable we fall back to a byte-length estimate:
//! 4 bytes ≈ 1 token. Byte-based (not char-based) so non-ASCII prompts
//! (Vietnamese / CJK) don't underreport. Good enough for budget tracking;
//! reconcile against vendor bills for exact figures.
//!
//! Price table is baked into `prices.rs`. Override coming in a follow-up
//! PR via `~/.zforge/prices.yaml`.

pub mod log;
pub mod prices;
pub mod report;
pub mod schema;
pub mod usage;

pub use schema::CostEntry;
