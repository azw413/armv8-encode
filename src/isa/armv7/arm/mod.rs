//! ARM-mode (32-bit, A32) ISA support.
//!
//! Mirrors the layering of the Thumb side under
//! [`super`]: `table_generated` carries the imported binutils
//! table, `format_decode` parses each format string into
//! `DecodedOperand` values, `sweep` walks fixed 4-byte
//! instructions, `control_flow` classifies branches /
//! returns. Implements [`crate::mc::InstructionInfo`] on the
//! decoded type so the architecture-neutral CFG builder works
//! over ARM-mode instructions without modification.
//!
//! ARM mode and Thumb mode share most format codes but differ
//! in bit positions (ARM is fixed-width with the condition
//! code in bits 28..31) and in some code semantics (ARM `%a`
//! is a load/store address with shift, Thumb's is different).
//! The decoder therefore lives in its own file rather than
//! being parameterised over Thumb's.
//!
//! Mode selection (ARM vs Thumb) is a higher-level concern —
//! callers decide which decoder to invoke based on symbol
//! info / interworking branches. The two stay independent at
//! the per-instruction layer.

pub mod control_flow;
pub mod encode;
pub mod format_decode;
pub mod isa_impl;
pub mod macro_emit;
pub mod recursive;
pub mod sweep;
pub mod table_generated;

pub use isa_impl::{ArmIsa, ArmMacroKind};
pub use table_generated::{
    iter_opcodes, ArmMnemonicGenerated, ArmOpcodeGenerated, ARM_OPCODE_TABLE_GENERATED,
};
