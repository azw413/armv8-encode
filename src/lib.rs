//! Layered machine-code tooling.
//!
//! The crate is intentionally split into three layers:
//! - `isa`: architecture-specific instruction encode/decode.
//! - `mc`: architecture-neutral machine-code representation.
//! - `rewrite`: control-flow and relocation-aware code rewriting.

pub mod container;
pub mod isa;
pub mod mc;
pub mod rewrite;

#[cfg(test)]
mod tests;
