//! Recursive-descent disassembly for ARM-mode (A32) code.
//!
//! Same algorithm as the AArch64 / Thumb recursive
//! disassemblers. ARM-mode is fixed-width (4 bytes) so the
//! walker advances by a constant; data ranges are
//! word-granular.
//!
//! Compare to [`super::sweep::disassemble_bytes`], which is
//! fail-fast linear and only suitable for clean instruction
//! streams.

use crate::isa::armv7::arm::format_decode::decode_operands_from_format;
use crate::isa::armv7::arm::sweep::ArmDecodedInstruction;
use crate::isa::armv7::arm::table_generated::{
    match_generated, ArmMnemonicGenerated,
};
use crate::isa::armv7::neon_table_generated::match_arm as match_neon_arm;
use crate::isa::armv7::operand::DecodedOperand;
use crate::mc::{ControlFlow, InstructionInfo};
use std::collections::BTreeMap;

const INSTRUCTION_BYTES: usize = 4;

#[derive(Debug, Clone, Default)]
pub struct ArmDisassembly {
    pub instructions: Vec<ArmDecodedInstruction>,
    pub data_ranges: Vec<DataRange>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DataRange {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub reason: DataReason,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum DataReason {
    DecodeError,
    Unreachable,
    Padding,
}

impl ArmDisassembly {
    pub fn timeline(&self) -> impl Iterator<Item = TimelineEntry<'_>> {
        let mut entries: Vec<(u64, TimelineEntry<'_>)> =
            Vec::with_capacity(self.instructions.len() + self.data_ranges.len());
        for instruction in &self.instructions {
            entries.push((instruction.address, TimelineEntry::Instruction(instruction)));
        }
        for range in &self.data_ranges {
            entries.push((range.address, TimelineEntry::Data(range)));
        }
        entries.sort_by_key(|(address, _)| *address);
        entries.into_iter().map(|(_, entry)| entry)
    }
}

#[derive(Debug)]
pub enum TimelineEntry<'a> {
    Instruction(&'a ArmDecodedInstruction),
    Data(&'a DataRange),
}

pub fn disassemble_recursive(
    base_address: u64,
    bytes: &[u8],
    entry_points: &[u64],
) -> ArmDisassembly {
    let aligned_len = bytes.len() & !(INSTRUCTION_BYTES - 1);
    let aligned_end = base_address + aligned_len as u64;

    let in_range = |address: u64| {
        address >= base_address
            && address < aligned_end
            && (address - base_address) % INSTRUCTION_BYTES as u64 == 0
    };

    let mut status: BTreeMap<u64, Status> = BTreeMap::new();
    let mut worklist: Vec<u64> = entry_points
        .iter()
        .copied()
        // Strip Thumb low-bit marker; ARM-mode entry points
        // shouldn't have it but the symbol table can be
        // sloppy on inter-mode references.
        .map(|a| a & !1u64)
        .filter(|&address| in_range(address))
        .collect();

    while let Some(mut address) = worklist.pop() {
        loop {
            if !in_range(address) || status.contains_key(&address) {
                break;
            }

            let offset = (address - base_address) as usize;
            let word = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);

            match decode_one(word, address) {
                Ok(decoded) => {
                    let cf = decoded.control_flow();
                    status.insert(address, Status::Decoded(decoded));

                    let next = match cf {
                        ControlFlow::Fall => Some(address + INSTRUCTION_BYTES as u64),
                        ControlFlow::Jump { target } => {
                            if in_range(target) {
                                worklist.push(target);
                            }
                            None
                        }
                        ControlFlow::ConditionalJump { target, fallthrough } => {
                            if in_range(target) {
                                worklist.push(target);
                            }
                            Some(fallthrough)
                        }
                        ControlFlow::Call { target, fallthrough } => {
                            if in_range(target) {
                                worklist.push(target);
                            }
                            Some(fallthrough)
                        }
                        ControlFlow::Return | ControlFlow::IndirectJump => None,
                        ControlFlow::IndirectCall { fallthrough } => Some(fallthrough),
                        ControlFlow::Trap => Some(address + INSTRUCTION_BYTES as u64),
                    };

                    match next {
                        Some(a) => address = a,
                        None => break,
                    }
                }
                Err(()) => {
                    status.insert(address, Status::DecodeError);
                    break;
                }
            }
        }
    }

    build_disassembly(base_address, bytes, aligned_len, status)
}

fn decode_one(word: u32, address: u64) -> Result<ArmDecodedInstruction, ()> {
    if let Some(neon_row) = match_neon_arm(word) {
        let operands = vec![DecodedOperand::OpaqueBits {
            bits: word,
            mask: 0xFFFF_FFFF,
        }];
        return Ok(ArmDecodedInstruction {
            address,
            word,
            mnemonic: ArmMnemonicGenerated::Nop,
            operands,
            row: None,
            neon_row: Some(neon_row),
        });
    }
    let row = match_generated(word).ok_or(())?;
    let (mut operands, _unhandled) = decode_operands_from_format(row.format, word, address);
    let tail_mask = !row.mask;
    if tail_mask != 0 {
        operands.push(DecodedOperand::OpaqueBits {
            bits: word & tail_mask,
            mask: tail_mask,
        });
    }
    Ok(ArmDecodedInstruction {
        address,
        word,
        mnemonic: row.mnemonic,
        operands,
        row: Some(row),
        neon_row: None,
    })
}

enum Status {
    Decoded(ArmDecodedInstruction),
    DecodeError,
}

fn build_disassembly(
    base_address: u64,
    bytes: &[u8],
    aligned_len: usize,
    status: BTreeMap<u64, Status>,
) -> ArmDisassembly {
    let mut instructions = Vec::new();
    let mut data_ranges: Vec<DataRange> = Vec::new();

    let mut data_start: Option<u64> = None;
    let mut data_reason: Option<DataReason> = None;

    let mut offset = 0usize;
    while offset < aligned_len {
        let address = base_address + offset as u64;
        match status.get(&address) {
            Some(Status::Decoded(instruction)) => {
                flush_data(
                    bytes,
                    base_address,
                    address,
                    &mut data_start,
                    &mut data_reason,
                    &mut data_ranges,
                );
                instructions.push(instruction.clone());
            }
            Some(Status::DecodeError) => {
                accumulate(
                    bytes,
                    base_address,
                    address,
                    DataReason::DecodeError,
                    &mut data_start,
                    &mut data_reason,
                    &mut data_ranges,
                );
            }
            None => {
                accumulate(
                    bytes,
                    base_address,
                    address,
                    DataReason::Unreachable,
                    &mut data_start,
                    &mut data_reason,
                    &mut data_ranges,
                );
            }
        }
        offset += INSTRUCTION_BYTES;
    }

    flush_data(
        bytes,
        base_address,
        base_address + aligned_len as u64,
        &mut data_start,
        &mut data_reason,
        &mut data_ranges,
    );

    if aligned_len < bytes.len() {
        data_ranges.push(DataRange {
            address: base_address + aligned_len as u64,
            bytes: bytes[aligned_len..].to_vec(),
            reason: DataReason::Padding,
        });
    }

    ArmDisassembly {
        instructions,
        data_ranges,
    }
}

fn accumulate(
    bytes: &[u8],
    base_address: u64,
    address: u64,
    reason: DataReason,
    data_start: &mut Option<u64>,
    data_reason: &mut Option<DataReason>,
    data_ranges: &mut Vec<DataRange>,
) {
    if data_reason.is_some() && *data_reason != Some(reason) {
        flush_data(bytes, base_address, address, data_start, data_reason, data_ranges);
    }
    if data_start.is_none() {
        *data_start = Some(address);
        *data_reason = Some(reason);
    }
}

fn flush_data(
    bytes: &[u8],
    base_address: u64,
    end_address: u64,
    data_start: &mut Option<u64>,
    data_reason: &mut Option<DataReason>,
    data_ranges: &mut Vec<DataRange>,
) {
    if let Some(start) = data_start.take() {
        let start_offset = (start - base_address) as usize;
        let end_offset = (end_address - base_address) as usize;
        data_ranges.push(DataRange {
            address: start,
            bytes: bytes[start_offset..end_offset].to_vec(),
            reason: data_reason.take().expect("reason set when start set"),
        });
    } else {
        *data_reason = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_plt_stub_disassembles_completely() {
        // Real PLT stub from libtool-checker.so at 0xf84.
        let bytes: &[u8] = &[
            0x04, 0xe0, 0x2d, 0xe5,
            0x04, 0xe0, 0x9f, 0xe5,
            0x0e, 0xe0, 0x8f, 0xe0,
            0x08, 0xf0, 0xbe, 0xe5,
        ];
        let dis = disassemble_recursive(0xf84, bytes, &[0xf84]);
        // The walker stops at `ldr pc, [lr, #8]!` (IndirectJump).
        // Three instructions before it should be reached.
        assert!(
            dis.instructions.len() >= 3,
            "expected ≥3 instructions, got {}",
            dis.instructions.len()
        );
    }

    #[test]
    fn unreached_words_become_data() {
        // bx lr at 0x1000, padding word at 0x1004.
        let bytes: &[u8] = &[
            0x1e, 0xff, 0x2f, 0xe1, // bx lr
            0x00, 0x00, 0x00, 0x00, // unreached
        ];
        let dis = disassemble_recursive(0x1000, bytes, &[0x1000]);
        assert_eq!(dis.instructions.len(), 1);
        assert_eq!(dis.data_ranges.len(), 1);
        assert_eq!(dis.data_ranges[0].address, 0x1004);
    }
}
