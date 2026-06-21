//! x86 / x86_64 instruction support, backed by the `iced-x86`
//! decoder/encoder.
//!
//! Unlike the AArch64 and ARMv7 modules — which carry hand-imported
//! opcode tables — the x86 layer delegates the heavy lifting (variable-
//! length decode, operand extraction, re-encoding) to `iced-x86`. This
//! module's job is to wrap iced's types in the crate's neutral
//! abstractions:
//!
//! - [`X86DecodedInstruction`] wraps an `iced_x86::Instruction` plus the
//!   crate-level address, and implements
//!   [`InstructionInfo`](crate::mc::InstructionInfo) so the
//!   architecture-neutral CFG builder works unchanged.
//! - [`sweep::disassemble_bytes`] is the linear-sweep entry point that
//!   mirrors `aarch64::disassemble_bytes` / `armv7::…::disassemble_bytes`.
//!
//! Both 64-bit (x86-64) and 32-bit (i386) code are supported; the decode
//! width is selected by [`Bitness`], derived from the container's
//! [`Architecture`](crate::container::Architecture) via
//! [`bitness_for_architecture`].
//!
//! The encode path and the [`Isa`](crate::isa::Isa) implementation that
//! plugs x86 into the rewrite pipeline arrive in a later phase.

pub mod control_flow;
pub mod encode;
pub mod isa_impl;
pub mod sweep;

pub use encode::{assemble, encode_instruction, Assembled, EncodeError};
pub use isa_impl::X86Isa;
pub use sweep::{
    disassemble_bytes, project_operands, Bitness, DisassembleError, X86DecodedInstruction,
    X86Operand,
};

use crate::container::Architecture;

/// Map a container architecture onto the x86 decode width. Returns
/// `None` for non-x86 architectures.
pub fn bitness_for_architecture(architecture: Architecture) -> Option<Bitness> {
    match architecture {
        Architecture::X86_64 => Some(Bitness::Bits64),
        Architecture::X86 => Some(Bitness::Bits32),
        _ => None,
    }
}
