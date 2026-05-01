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

use crate::isa::aarch64::{decode_instruction, DecodeError, DecodedInstruction, Word};

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
