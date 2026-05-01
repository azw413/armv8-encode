//! Recursive-descent disassembly.
//!
//! Walks control flow from a set of entry points (typically function
//! symbols) and only treats reached words as instructions. Bytes that
//! aren't reached, that don't decode, or that fall in an unaligned tail
//! are classified as data. This is what you want for real shipped
//! binaries, where `.text` routinely contains literal pools, jump
//! tables, and alignment padding interleaved with code.
//!
//! Compare to [`super::sweep::disassemble_bytes`], which is fail-fast
//! linear and only suitable for clean instruction streams.
//!
//! ## Algorithm
//!
//! Standard worklist:
//!
//! 1. Push each entry point onto a worklist.
//! 2. Pop an address; walk forward, decoding one 4-byte word at a time.
//! 3. After each instruction, look at its `ControlFlow`:
//!    - `Fall` → continue at `address + 4`.
//!    - `Jump { target }` → enqueue `target`, stop walking here.
//!    - `ConditionalJump { target, fallthrough }` → enqueue `target`,
//!      continue at `fallthrough`.
//!    - `Call { target, fallthrough }` → enqueue `target`, continue at
//!      `fallthrough` (calls return).
//!    - `Return` / `IndirectJump` → stop.
//!    - `IndirectCall { fallthrough }` → continue at `fallthrough`.
//!    - `Trap` → conservatively continue (most syscalls return).
//! 4. A word that has already been classified is skipped.
//!
//! ## Limitations
//!
//! - Indirect jumps lose their targets. Jump-table / vtable analysis
//!   would extend this.
//! - Conservative call / trap handling may decode bytes that, in
//!   practice, aren't reachable (e.g. a `bl __stack_chk_fail` at the end
//!   of a function whose fallthrough is never executed). The cost is one
//!   misclassification per such site, never code lost.

use crate::isa::aarch64::{decode_instruction, DecodedInstruction};
use crate::mc::{ControlFlow, InstructionInfo};
use std::collections::BTreeMap;

const INSTRUCTION_BYTES: usize = 4;

/// Result of recursive-descent disassembly.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Disassembly {
    /// Decoded instructions in address order.
    pub instructions: Vec<DecodedInstruction>,
    /// Byte ranges classified as data, in address order. Adjacent ranges
    /// with the same `DataReason` are merged.
    pub data_ranges: Vec<DataRange>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DataRange {
    /// First byte's address.
    pub address: u64,
    pub bytes: Vec<u8>,
    pub reason: DataReason,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum DataReason {
    /// The 4-byte word at this address didn't decode as an instruction.
    DecodeError,
    /// No path from any entry point reached this address.
    Unreachable,
    /// Trailing bytes that don't fit a 4-byte instruction.
    Padding,
}

impl Disassembly {
    /// Iterate over instructions and data ranges in a single
    /// address-ordered timeline. Useful for printing.
    pub fn timeline(&self) -> impl Iterator<Item = TimelineEntry<'_>> {
        let mut entries: Vec<(u64, TimelineEntry<'_>)> = Vec::with_capacity(
            self.instructions.len() + self.data_ranges.len(),
        );
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
    Instruction(&'a DecodedInstruction),
    Data(&'a DataRange),
}

/// Disassemble `bytes` (interpreted as a contiguous stream beginning at
/// `base_address`), starting from each address in `entry_points`. Returns
/// the reached instructions plus a classification of every byte not
/// reached.
pub fn disassemble_recursive(
    base_address: u64,
    bytes: &[u8],
    entry_points: &[u64],
) -> Disassembly {
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

            match decode_instruction(address, word) {
                Ok(instruction) => {
                    let cf = instruction.control_flow();
                    status.insert(address, Status::Decoded(instruction));

                    let next = match cf {
                        ControlFlow::Fall => Some(address + INSTRUCTION_BYTES as u64),
                        ControlFlow::Jump { target } => {
                            if in_range(target) {
                                worklist.push(target);
                            }
                            None
                        }
                        ControlFlow::ConditionalJump {
                            target,
                            fallthrough,
                        } => {
                            if in_range(target) {
                                worklist.push(target);
                            }
                            Some(fallthrough)
                        }
                        ControlFlow::Call {
                            target,
                            fallthrough,
                        } => {
                            if in_range(target) {
                                worklist.push(target);
                            }
                            Some(fallthrough)
                        }
                        ControlFlow::Return | ControlFlow::IndirectJump => None,
                        ControlFlow::IndirectCall { fallthrough } => Some(fallthrough),
                        // Most traps return; cheap to be conservative.
                        ControlFlow::Trap => Some(address + INSTRUCTION_BYTES as u64),
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

fn build_disassembly(
    base_address: u64,
    bytes: &[u8],
    aligned_len: usize,
    status: BTreeMap<u64, Status>,
) -> Disassembly {
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

    Disassembly {
        instructions,
        data_ranges,
    }
}

enum Status {
    Decoded(DecodedInstruction),
    DecodeError,
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
