//! Fingerprint of the code a verification ran against (IMP-002).
//!
//! A run records the fingerprint its tests passed on (`verified.candidate`);
//! the output it seals must carry the same one.

pub mod candidate;
mod linked;

pub use candidate::{fingerprint, Candidate};
