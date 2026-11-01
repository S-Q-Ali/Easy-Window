//! Read-only system telemetry.
//!
//! The monitor never changes system state. Destructive actions live in
//! `crate::actions` and must re-verify process identity before acting.

#[cfg(windows)]
pub mod pdh;

#[cfg(windows)]
pub mod process;

#[cfg(windows)]
pub use process::{ProcessSampler, ProcessSnapshot, SignatureStatus};
