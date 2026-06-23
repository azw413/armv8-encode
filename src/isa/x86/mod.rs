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
    disassemble_bytes, flag_bits, project_operands, Bitness, DisassembleError,
    X86DecodedInstruction, X86Operand, X86RegUse, X86RrOp,
};

use crate::container::Architecture;

/// Mnemonic for an unconditional near jump. Callers synthesizing a `jmp <block>`
/// connector — e.g. to make a fall-through edge explicit before reordering basic
/// blocks — pair this with a [`crate::rewrite::ir::RewriteOperand::Branch`] block
/// target; the emit pass re-encodes it to `jmp rel32` (5 bytes) via
/// [`X86Isa::encode`]. Lets a consumer build the op without naming `iced_x86`.
pub fn jmp_mnemonic() -> iced_x86::Mnemonic {
    iced_x86::Mnemonic::Jmp
}

/// Mnemonic for `jne` (jump-if-not-equal / ZF=0). Paired with a
/// [`crate::rewrite::ir::RewriteOperand::Branch`] block target, the emit pass
/// re-encodes it to `jne rel32` (6 bytes) via [`X86Isa::encode`]. Used to build
/// an opaque-predicate branch (never taken after a `cmp r,r`, which forces ZF=1)
/// without naming `iced_x86`.
pub fn jne_mnemonic() -> iced_x86::Mnemonic {
    iced_x86::Mnemonic::Jne
}

/// Map a container architecture onto the x86 decode width. Returns
/// `None` for non-x86 architectures.
pub fn bitness_for_architecture(architecture: Architecture) -> Option<Bitness> {
    match architecture {
        Architecture::X86_64 => Some(Bitness::Bits64),
        Architecture::X86 => Some(Bitness::Bits32),
        _ => None,
    }
}
