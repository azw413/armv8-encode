//! ARM-mode linear-sweep disassembly.
//!
//! Fixed 4-byte instructions, little-endian. Mirror of the
//! Thumb sweep but without the width-discrimination step.

use super::format_decode::decode_operands_from_format;
use super::table_generated::{
    match_generated, ArmMnemonicGenerated, ArmOpcodeGenerated,
};
use crate::isa::armv7::neon_table_generated::{match_arm as match_neon_arm, NeonOpcodeGenerated};
use crate::isa::armv7::operand::DecodedOperand;

/// A single ARM-mode instruction after sweep.
///
/// NEON / VFP / coprocessor encodings match against a
/// separate table; for those rows `row` is `None`, `mnemonic`
/// is set to a placeholder value (`ArmMnemonicGenerated::Nop`),
/// and `neon_row` is populated.
#[derive(Debug, Clone)]
pub struct ArmDecodedInstruction {
    pub address: u64,
    pub word: u32,
    pub mnemonic: ArmMnemonicGenerated,
    pub operands: Vec<DecodedOperand>,
    pub row: Option<&'static ArmOpcodeGenerated>,
    pub neon_row: Option<&'static NeonOpcodeGenerated>,
}

impl ArmDecodedInstruction {
    pub fn size_bytes(&self) -> u64 {
        4
    }

    /// Lower-case textual mnemonic from whichever table
    /// matched.
    pub fn mnemonic_name(&self) -> &'static str {
        if let Some(neon) = self.neon_row {
            neon.mnemonic.as_str()
        } else {
            self.mnemonic.as_str()
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ArmDisassembleError {
    UnalignedLength { length: usize },
    NoMatch { address: u64, word: u32 },
}

impl std::fmt::Display for ArmDisassembleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnalignedLength { length } => {
                write!(f, "ARM sweep: input length {length} is not a multiple of 4")
            }
            Self::NoMatch { address, word } => write!(
                f,
                "ARM sweep: no opcode at 0x{address:x} (word 0x{word:08x})"
            ),
        }
    }
}

impl std::error::Error for ArmDisassembleError {}

const INSTRUCTION_BYTES: usize = 4;

/// Decode every ARM instruction in `bytes`, treating it as a
/// contiguous instruction stream beginning at `base_address`.
pub fn disassemble_bytes(
    base_address: u64,
    bytes: &[u8],
) -> Result<Vec<ArmDecodedInstruction>, ArmDisassembleError> {
    if bytes.len() % INSTRUCTION_BYTES != 0 {
        return Err(ArmDisassembleError::UnalignedLength {
            length: bytes.len(),
        });
    }
    let mut out = Vec::with_capacity(bytes.len() / INSTRUCTION_BYTES);
    for (idx, chunk) in bytes.chunks_exact(INSTRUCTION_BYTES).enumerate() {
        let address = base_address.wrapping_add((idx * INSTRUCTION_BYTES) as u64);
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        // Try NEON / coprocessor first (binutils dispatch
        // order — these tables are denser for instructions
        // that overlap with main rows).
        if let Some(neon_row) = match_neon_arm(word) {
            out.push(ArmDecodedInstruction {
                address,
                word,
                mnemonic: ArmMnemonicGenerated::Nop,
                operands: Vec::new(),
                row: None,
                neon_row: Some(neon_row),
            });
            continue;
        }
        let row = match_generated(word).ok_or(ArmDisassembleError::NoMatch { address, word })?;
        let (operands, _unhandled) = decode_operands_from_format(row.format, word, address);
        out.push(ArmDecodedInstruction {
            address,
            word,
            mnemonic: row.mnemonic,
            operands,
            row: Some(row),
            neon_row: None,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disassembles_arm_plt_stub() {
        // Real PLT stub bytes from libtool-checker.so (offset 0xf84):
        //   e52de004    push {lr}            (str lr, [sp, #-4]!)
        //   e59fe004    ldr lr, [pc, #4]
        //   e08fe00e    add lr, pc, lr
        //   e5bef008    ldr pc, [lr, #8]!
        let bytes: &[u8] = &[
            0x04, 0xe0, 0x2d, 0xe5,
            0x04, 0xe0, 0x9f, 0xe5,
            0x0e, 0xe0, 0x8f, 0xe0,
            0x08, 0xf0, 0xbe, 0xe5,
        ];
        let insns = disassemble_bytes(0xf84, bytes).expect("sweep");
        assert_eq!(insns.len(), 4);
        // First insn: binutils provides a dedicated `push`
        // alias for the str-with-pre-decrement-writeback
        // encoding; the row's format starts with "push%c".
        let row0 = insns[0].row.expect("row 0 should match primary table");
        assert!(
            row0.format.starts_with("push"),
            "row 0 format: {}",
            row0.format,
        );
    }

    #[test]
    fn rejects_unaligned() {
        let bytes = [0x00u8; 7];
        assert!(matches!(
            disassemble_bytes(0, &bytes),
            Err(ArmDisassembleError::UnalignedLength { .. })
        ));
    }
}
