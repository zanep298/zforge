//! Move a store or a project from the removed task pipeline to v1.5.
//! Nothing is deleted: what a migration takes away goes to a `v1-archive/`
//! directory, and every file it rewrites is copied there first.

pub mod archive;
pub mod global;
pub mod models_text;
pub mod project;
pub mod yaml_text;
