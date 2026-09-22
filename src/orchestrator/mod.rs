//! Agent orchestration: spawn → decide → fallback → retry.
//!
//! `run_phase` is the entry point CLI commands route through when a task has
//! an `assigned_agent`. Pre-PR2 tasks (no agent in `.state.yaml`) skip the
//! orchestrator entirely and stay on the legacy `Engine::dispatch()` path.

pub mod agent_args;
pub mod fallback;
pub mod headless_args;
pub mod history;
pub mod model_args;
pub mod run;
pub mod spawn;
pub(crate) mod trace_record;
pub mod verifier_loop;

pub use run::{run_phase, run_phase_with_lock};
pub use verifier_loop::{iterate, iterate_from, LoopOutcome, Start};
