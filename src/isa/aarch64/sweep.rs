//! Linear-sweep disassembly.
//!
//! Given a base address and a contiguous instruction-stream byte slice,
//! decode every 4-byte word in order. This is the simplest possible input
//! shape for the analysis layers above and the API that basic-block
//! discovery / CFG construction sit on.
//!
//! The sweep is fail-fast: the first word that doesn't decode aborts the
//! whole disassembly with the offending address. A best-effort variant that
//! tolerates non-instruction bytes (literal pools, jump tables, alignment
//! padding) belongs in a future recursive-descent pass that has section and
//! relocation context.

use crate::isa::aarch64::{
    decode_instruction, Aarch64Mnemonic, DecodeError, DecodedInstruction, Word,
};

const INSTRUCTION_BYTES: usize = 4;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DisassembleError {
    /// `bytes.len()` was not a multiple of 4. AArch64 instructions are
    /// always exactly four bytes; an unaligned length is almost certainly
    /// a caller bug (wrong section, truncated read).
    UnalignedLength { length: usize },

    /// A specific instruction word failed to decode. Carries enough context
    /// for the caller to point at the bad word in its source.
    DecodeFailed {
        address: u64,
        word: Word,
        source: DecodeError,
    },
}

/// Decode every instruction in `bytes`, treated as a contiguous AArch64
/// instruction stream beginning at `base_address`.
pub fn disassemble_bytes(
    base_address: u64,
    bytes: &[u8],
) -> Result<Vec<DecodedInstruction>, DisassembleError> {
    if bytes.len() % INSTRUCTION_BYTES != 0 {
        return Err(DisassembleError::UnalignedLength {
            length: bytes.len(),
        });
    }

    let count = bytes.len() / INSTRUCTION_BYTES;
    let mut out = Vec::with_capacity(count);

    for (index, chunk) in bytes.chunks_exact(INSTRUCTION_BYTES).enumerate() {
        let address = base_address.wrapping_add((index as u64) * INSTRUCTION_BYTES as u64);
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        match decode_instruction(address, word) {
            Ok(instruction) => out.push(instruction),
            Err(source) => {
                return Err(DisassembleError::DecodeFailed {
                    address,
                    word,
                    source,
                });
            }
        }
    }

    Ok(out)
}

/// Best-effort linear sweep: like [`disassemble_bytes`], but instead of aborting
/// on the first word that doesn't decode, it substitutes a placeholder `NOP`
/// (preserving the original `word`) and records the failing address, then
/// continues. Returns the decoded stream plus the sorted list of addresses whose
/// words could not be decoded.
///
/// This lets a caller lift an entire section that contains a few instructions
/// outside the decoder's coverage (e.g. newer SIMD / atomic encodings) without
/// failing the whole job. The placeholder is a real `NOP` so CFG construction and
/// downstream analysis stay well-formed — but it does NOT round-trip the original
/// bytes, so the caller MUST avoid emitting any function that overlaps a returned
/// `(address, word)` (leave it native / verbatim). The original `word` is
/// returned (and retained on the placeholder) so callers can diagnose what was
/// skipped.
pub fn disassemble_bytes_tolerant(
    base_address: u64,
    bytes: &[u8],
) -> Result<(Vec<DecodedInstruction>, Vec<(u64, Word)>), DisassembleError> {
    if bytes.len() % INSTRUCTION_BYTES != 0 {
        return Err(DisassembleError::UnalignedLength {
            length: bytes.len(),
        });
    }

    let count = bytes.len() / INSTRUCTION_BYTES;
    let mut out = Vec::with_capacity(count);
    let mut undecodable = Vec::new();

    for (index, chunk) in bytes.chunks_exact(INSTRUCTION_BYTES).enumerate() {
        let address = base_address.wrapping_add((index as u64) * INSTRUCTION_BYTES as u64);
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        match decode_instruction(address, word) {
            Ok(instruction) => out.push(instruction),
            Err(_) => {
                undecodable.push((address, word));
                // Placeholder NOP retaining the original word. Never emitted —
                // the caller excludes any function overlapping `undecodable`.
                out.push(DecodedInstruction {
                    address,
                    word,
                    mnemonic: Aarch64Mnemonic::Nop,
                    operands: Vec::new(),
                });
            }
        }
    }

    Ok((out, undecodable))
}
