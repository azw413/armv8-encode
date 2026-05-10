//! Instruction set architecture support.
//!
//! Architecture modules own raw instruction encodings, operand schemas, and
//! encode/decode validation. Higher layers should depend on typed ISA results
//! rather than matching raw opcode tables directly.

pub mod aarch64;
pub mod armv7;

mod isa_trait;
pub use isa_trait::{
    FusionRelocationInfo, Isa, IsaEncodeOutput, MacroEmitError, MacroEmittedRelocation, PcRelKind,
};
