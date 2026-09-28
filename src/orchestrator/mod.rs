//! Agent orchestration for runs: spawn a headless agent, pick its model and
//! named agent, record what it did, and drive the code → verify loop.

pub mod agent_args;
pub mod headless_args;
pub mod model_args;
pub mod spawn;
pub(crate) mod trace_record;
pub mod verifier_loop;

pub use verifier_loop::{iterate, LoopOutcome};
