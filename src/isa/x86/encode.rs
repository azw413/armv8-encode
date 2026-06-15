//! x86 / x86_64 encoding, backed by `iced-x86`.
//!
//! Two entry points:
//!
//! - [`encode_instruction`] re-encodes a single `iced_x86::Instruction`
//!   at a fixed address via `iced_x86::Encoder`. RIP-relative memory
//!   operands re-resolve to the same absolute target automatically, so
//!   moving an instruction keeps its data references valid.
//! - [`assemble`] lays out and encodes a *sequence* of instructions via
//!   `iced_x86::BlockEncoder`, which is the natural fit for x86's
//!   variable-length branches: it auto-selects the rel8 / rel32 form of
//!   each branch, fixes up intra-block branch displacements as the
//!   layout shifts, and rewrites RIP-relative references. This is how
//!   the rewrite path emits an edited x86 section without the
//!   per-instruction widening dance the fixed-width ISAs use.
//!
//! Branch retargeting is expressed the iced way: set an instruction's
//! near-branch target (`Instruction::set_near_branch64`) to the absolute
//! address of another instruction in the block (or an extern), and
//! `BlockEncoder` resolves it.

use crate::isa::x86::sweep::Bitness;
use iced_x86::{BlockEncoder, BlockEncoderOptions, Encoder, Instruction, InstructionBlock};

/// Error from the x86 encode path.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EncodeError {
    /// `iced-x86` rejected the instruction or block. Carries iced's
    /// message (e.g. branch out of range, invalid operand combo).
    Iced(String),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::Iced(msg) => write!(f, "x86 encode failed: {msg}"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// Encode a single instruction so that it executes correctly when placed
/// at `address`.
pub fn encode_instruction(
    instr: &Instruction,
    address: u64,
    bitness: Bitness,
) -> Result<Vec<u8>, EncodeError> {
    let mut encoder = Encoder::new(bitness.as_u32());
    encoder
        .encode(instr, address)
        .map_err(|e| EncodeError::Iced(e.to_string()))?;
    Ok(encoder.take_buffer())
}

/// Output of [`assemble`].
#[derive(Debug, Clone)]
pub struct Assembled {
    /// Base address the block was encoded at.
    pub base_address: u64,
    /// Encoded bytes of the whole block.
    pub bytes: Vec<u8>,
    /// Final offset (relative to `base_address`) of each input
    /// instruction after layout. Branch widening can change these, so
    /// callers that need to know where an instruction landed read it
    /// here. `u32::MAX` marks an instruction iced dropped/rewrote.
    pub instruction_offsets: Vec<u32>,
}

/// Lay out and encode `instructions` as a contiguous block starting at
/// `base_address`, via `BlockEncoder`.
///
/// Intra-block branches (whose `near_branch64` target equals another
/// instruction's IP in the block) are fixed up and resized as layout
/// shifts; branches/calls to addresses outside the block (e.g. PLT
/// stubs) keep their absolute target and are encoded as the reaching
/// form. Set each instruction's `ip()` to its intended source address
/// before calling so iced can match intra-block targets.
pub fn assemble(
    instructions: &[Instruction],
    base_address: u64,
    bitness: Bitness,
) -> Result<Assembled, EncodeError> {
    let block = InstructionBlock::new(instructions, base_address);
    let result = BlockEncoder::encode(
        bitness.as_u32(),
        block,
        BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
    )
    .map_err(|e| EncodeError::Iced(e.to_string()))?;
    Ok(Assembled {
        base_address: result.rip,
        bytes: result.code_buffer,
        instruction_offsets: result.new_instruction_offsets,
    })
}
