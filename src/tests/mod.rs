//! AArch64 unit tests, split by concern: operand-level encoding, decode + format
//! comparisons against `otool`, and table/group coverage assertions.

mod bitfield_aliases;
mod bitfield_corpus;
mod cfg;
mod common;
mod container;
mod control_flow;
mod coverage;
mod data;
mod decode;
mod editor;
mod encode;
mod pcrel_range_audit;
mod pe;
mod public_opcode_api;
mod recursive;
mod rewrite;
mod sweep;
mod x86;
