//! AArch64 unit tests, split by concern: operand-level encoding, decode + format
//! comparisons against `otool`, and table/group coverage assertions.

mod cfg;
mod common;
mod container;
mod control_flow;
mod coverage;
mod data;
mod decode;
mod editor;
mod encode;
mod public_opcode_api;
mod recursive;
mod rewrite;
mod sweep;
