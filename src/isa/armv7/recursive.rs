//! Recursive-descent disassembly for Thumb code.
//!
//! Walks control flow from a set of entry points (typically
//! Thumb function symbols) and only treats reached
//! halfwords / words as instructions. Bytes that aren't
//! reached, that don't decode, or that fall in an unaligned
//! tail are classified as data. This is what's required for
//! real shipped binaries, where Thumb `.text` sections
//! routinely contain literal pools (4-byte constants
//! referenced by `ldr Rt, [pc, #imm]`), jump tables, and
//! alignment padding interleaved with code.
//!
//! Compare to [`super::sweep::disassemble_bytes`], which is
//! fail-fast linear and only suitable for clean instruction
//! streams.
//!
//! ## Variable-width handling
//!
//! Thumb instructions are 2 or 4 bytes. The walker advances
//! by the decoded instruction's actual width (`size_bytes`),
//! not a fixed step. The data-classification step also tracks
//! coverage at byte granularity rather than 4-byte words.
//!
//! ## Mode switching
//!
//! Static cross-mode branches (Thumb-to-ARM via `BLX`) are
//! treated as `Call` with their fallthrough back in Thumb;
//! the ARM-mode code at the target needs a separate
//! [`super::arm::recursive::disassemble_recursive`] pass with
//! that address as an entry point. Indirect mode switches
//! (`bx Rm`) lose their target as `IndirectJump`, which is
//! the same behaviour aarch64 has for any indirect branch.
//!
//! ## Algorithm
//!
//! Standard worklist:
//!
//! 1. Push each entry point onto a worklist.
//! 2. Pop an address; walk forward, decoding one Thumb
//!    instruction at a time (2 or 4 bytes).
//! 3. After each instruction, look at its `ControlFlow`:
//!    - `Fall` → continue at `address + size_bytes`.
//!    - `Jump { target }` → enqueue, stop walking here.
//!    - `ConditionalJump { target, fallthrough }` → enqueue
//!      target, continue at fallthrough.
//!    - `Call { target, fallthrough }` → enqueue, continue
//!      (calls return).
//!    - `Return` / `IndirectJump` → stop.
//!    - `IndirectCall { fallthrough }` → continue.
//!    - `Trap` → conservatively continue (most syscalls return).
//! 4. A halfword that has already been classified is skipped.

use crate::isa::armv7::operand::DecodeError;
use crate::isa::armv7::read_instruction;
use crate::isa::armv7::sweep::ThumbDecodedInstruction;
use crate::isa::armv7::table::ThumbWidth;
use crate::isa::armv7::table_generated::{match_generated, ThumbMnemonicGenerated};
use crate::isa::armv7::neon_table_generated::match_thumb as match_neon_thumb;
use crate::isa::armv7::format_decode::decode_operands_from_format;
use crate::isa::armv7::operand::DecodedOperand;
use crate::mc::{ControlFlow, InstructionInfo};
use std::collections::BTreeMap;

/// Result of recursive-descent Thumb disassembly.
#[derive(Debug, Clone, Default)]
pub struct ThumbDisassembly {
    pub instructions: Vec<ThumbDecodedInstruction>,
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
    /// The bytes at this address didn't decode as a Thumb
    /// instruction.
    DecodeError,
    /// No path from any entry point reached this address.
    Unreachable,
    /// Trailing bytes that don't fit a 2-byte halfword
    /// boundary (rare on Thumb — the assembler aligns).
    Padding,
}

impl ThumbDisassembly {
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
    Instruction(&'a ThumbDecodedInstruction),
    Data(&'a DataRange),
}

/// Disassemble `bytes` (interpreted as a contiguous Thumb
/// instruction stream beginning at `base_address`), starting
/// from each address in `entry_points`. Entry-point addresses
/// can carry the Thumb low-bit set (the symbol-table
/// convention); we mask it off before walking.
pub fn disassemble_recursive(
    base_address: u64,
    bytes: &[u8],
    entry_points: &[u64],
) -> ThumbDisassembly {
    // Address must be halfword-aligned and within the buffer.
    let aligned_len = bytes.len() & !1;
    let aligned_end = base_address + aligned_len as u64;

    let in_range = |address: u64| {
        address >= base_address
            && address < aligned_end
            && (address - base_address) % 2 == 0
    };

    // status maps the *start* address of every classified
    // instruction (or decode-error halfword) to its status.
    // For a 32-bit Thumb-2 instruction occupying bytes
    // [a, a+4) the entry lives at `a` only; the second
    // halfword at `a+2` is implicit in `Status::Decoded`'s
    // `size_bytes`.
    let mut status: BTreeMap<u64, Status> = BTreeMap::new();
    let mut worklist: Vec<u64> = entry_points
        .iter()
        .copied()
        // Strip the Thumb-flag low bit (ELF convention:
        // function symbols have st_value | 1 to mark Thumb
        // mode).
        .map(|a| a & !1u64)
        .filter(|&address| in_range(address))
        .collect();

    while let Some(mut address) = worklist.pop() {
        loop {
            if !in_range(address) || status.contains_key(&address) {
                break;
            }

            let offset = (address - base_address) as usize;
            match decode_one(bytes, offset, address) {
                Ok(decoded) => {
                    let cf = decoded.control_flow();
                    let size = decoded.size_bytes();
                    status.insert(address, Status::Decoded(decoded));

                    let next = match cf {
                        ControlFlow::Fall => Some(address + size),
                        ControlFlow::Jump { target } => {
                            // Thumb branch targets always have
                            // bit 0 = 0 in the encoded address
                            // (decoder strips it). No mask
                            // needed.
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
                            // Static call targets within
                            // range are enqueued. Cross-mode
                            // BLX would land on an ARM-mode
                            // address (bit 0 = 0, bit 1 may
                            // be set as the half-word offset);
                            // we detect that via in_range on
                            // the masked address but the
                            // caller is responsible for
                            // routing the ARM target into the
                            // ARM recursive disassembler.
                            if in_range(target) {
                                worklist.push(target);
                            }
                            Some(fallthrough)
                        }
                        ControlFlow::Return | ControlFlow::IndirectJump => None,
                        ControlFlow::IndirectCall { fallthrough } => Some(fallthrough),
                        ControlFlow::Trap => Some(address + size),
                    };

                    match next {
                        Some(a) => address = a,
                        None => break,
                    }
                }
                Err(_) => {
                    status.insert(address, Status::DecodeError);
                    break;
                }
            }
        }
    }

    build_disassembly(base_address, bytes, aligned_len, status)
}

/// Decode one Thumb instruction at `offset` within `bytes`,
/// returning a populated `ThumbDecodedInstruction`. Mirrors
/// the per-instruction logic of [`super::sweep::disassemble_bytes`]
/// but avoids the linear-sweep loop so the recursive walker
/// can decide where to go next based on `ControlFlow`.
fn decode_one(
    bytes: &[u8],
    offset: usize,
    address: u64,
) -> Result<ThumbDecodedInstruction, DecodeError> {
    let (word, width_bytes) = read_instruction(bytes, offset)?;
    let width = if width_bytes == 4 {
        ThumbWidth::Word
    } else {
        ThumbWidth::Halfword
    };
    let primary = match_generated(word, width);
    let (mnemonic, operands, row, neon_row) = match primary {
        Some(row) => {
            let (mut operands, _unhandled) =
                decode_operands_from_format(row.format, word, address, width_bytes);
            let unmasked_word_bits = match width {
                ThumbWidth::Halfword => word & 0xFFFF & !(row.mask & 0xFFFF),
                ThumbWidth::Word => word & !row.mask,
            };
            let tail_mask = match width {
                ThumbWidth::Halfword => 0xFFFFu32 & !(row.mask & 0xFFFF),
                ThumbWidth::Word => !row.mask,
            };
            if tail_mask != 0 {
                operands.push(DecodedOperand::OpaqueBits {
                    bits: unmasked_word_bits,
                    mask: tail_mask,
                });
            }
            (row.mnemonic, operands, Some(row), None)
        }
        None if width_bytes == 4 => {
            // NEON / coprocessor.
            let (neon_row, _norm) = match_neon_thumb(word).ok_or(
                DecodeError::NoMatchingOpcode { word, width_bytes },
            )?;
            let operands = vec![DecodedOperand::OpaqueBits {
                bits: word,
                mask: 0xFFFF_FFFF,
            }];
            (ThumbMnemonicGenerated::Nop, operands, None, Some(neon_row))
        }
        None => {
            return Err(DecodeError::NoMatchingOpcode { word, width_bytes });
        }
    };
    Ok(ThumbDecodedInstruction {
        address,
        word,
        width,
        mnemonic,
        operands,
        row,
        neon_row,
    })
}

enum Status {
    Decoded(ThumbDecodedInstruction),
    DecodeError,
}

fn build_disassembly(
    base_address: u64,
    bytes: &[u8],
    aligned_len: usize,
    status: BTreeMap<u64, Status>,
) -> ThumbDisassembly {
    let mut instructions = Vec::new();
    let mut data_ranges: Vec<DataRange> = Vec::new();

    let mut data_start: Option<u64> = None;
    let mut data_reason: Option<DataReason> = None;

    let mut offset = 0usize;
    // Stride is one halfword. When we land on a decoded
    // 32-bit instruction we skip its second halfword.
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
                let size = instruction.size_bytes() as usize;
                instructions.push(instruction.clone());
                offset += size;
                continue;
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
        offset += 2;
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

    ThumbDisassembly {
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
    fn linear_thumb_function_disassembles_completely() {
        // Reuse the 24-instruction fixture.
        let bytes: &[u8] = &[
            0xf0, 0xb5, 0x04, 0x46, 0x0d, 0x46, 0x04, 0xeb, 0x05, 0x00, 0xa4, 0xf1,
            0x0a, 0x01, 0x88, 0x42, 0x01, 0xd0, 0xff, 0xf7, 0xfe, 0xff, 0x22, 0x68,
            0x2a, 0x60, 0x63, 0x78, 0x6b, 0x70, 0x63, 0x88, 0x6b, 0x80, 0x04, 0xea,
            0x05, 0x00, 0x44, 0xea, 0x05, 0x01, 0x84, 0xea, 0x05, 0x02, 0x4f, 0xea,
            0x84, 0x03, 0x4f, 0xea, 0x14, 0x13, 0x4f, 0xea, 0x24, 0x23, 0x64, 0xfa,
            0x05, 0xf3, 0xc0, 0x46, 0xf0, 0xbd, 0x70, 0x47,
        ];
        let dis = disassemble_recursive(0x1000, bytes, &[0x1000]);
        // The function ends at `pop {…, pc}` (Return) which
        // stops the walker; the trailing `bx lr` after it is
        // technically unreachable but most compilers don't
        // emit instructions there. Either way: at least
        // the 23 instructions before `pop {pc}` should be
        // decoded.
        assert!(
            dis.instructions.len() >= 22,
            "expected ≥22 instructions, got {}",
            dis.instructions.len()
        );
    }

    #[test]
    fn entry_points_with_thumb_low_bit_set_are_normalised() {
        // The same fixture, but the entry point has the
        // Thumb low bit set (ELF convention).
        let bytes: &[u8] = &[
            0xf0, 0xb5, 0x70, 0x47,
        ];
        let dis = disassemble_recursive(0x1000, bytes, &[0x1001]);
        assert!(!dis.instructions.is_empty());
        assert_eq!(dis.instructions[0].address, 0x1000);
    }

    #[test]
    fn unreached_bytes_become_data() {
        // Two halfwords: bx lr (reachable as entry) and a
        // padding halfword (unreached).
        let bytes: &[u8] = &[
            0x70, 0x47, // bx lr at 0x1000
            0x00, 0x00, // unreached halfword at 0x1002
        ];
        let dis = disassemble_recursive(0x1000, bytes, &[0x1000]);
        assert_eq!(dis.instructions.len(), 1);
        assert_eq!(dis.data_ranges.len(), 1);
        assert_eq!(dis.data_ranges[0].address, 0x1002);
        assert_eq!(dis.data_ranges[0].reason, DataReason::Unreachable);
    }

    #[test]
    fn pc_relative_literal_pool_stays_data() {
        // ldr r0, [pc, #0]  → 0x4800 (PC-rel, points to
        //                              the next-aligned word)
        // bx lr             → 0x4770
        // <2 byte align>    → 0x0000
        // <4-byte literal>  → 0xdeadbeef
        let bytes: &[u8] = &[
            0x00, 0x48, // ldr r0, [pc, #0]
            0x70, 0x47, // bx lr
            0xef, 0xbe, 0xad, 0xde, // literal
        ];
        let dis = disassemble_recursive(0x1000, bytes, &[0x1000]);
        assert_eq!(dis.instructions.len(), 2);
        // The 4-byte literal at 0x1004 should land in
        // data_ranges (Unreachable).
        let literal = dis
            .data_ranges
            .iter()
            .find(|r| r.address == 0x1004)
            .expect("literal range");
        assert_eq!(literal.bytes.len(), 4);
        assert_eq!(literal.reason, DataReason::Unreachable);
    }
}
