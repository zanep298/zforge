//! Background job runtime.
//!
//! `ship --async` (and the MCP `code_async` tool) spawn the worker as a
//! detached subprocess of `zforge` itself — `Commands::Worker { job_id }`
//! routes back here. The controller exits immediately with the job ID;
//! callers poll `job status` / `job wait` / `job log` to learn the result.
//!
//! Layout per job:
//! ```text
//! <project>/.zforge/jobs/<JOB-ID>/
//!   job.yaml   # status + metadata, atomically written
//!   log        # combined stdout + stderr stream
//! ```

pub mod schema;
pub mod store;
pub mod lifecycle;
pub mod spawn;
pub mod worker;

pub use schema::{Job, JobStatus, JobKind};
pub use store::{create_job, list_jobs, load_job, save_atomic};
