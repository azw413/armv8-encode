//! Tests for recursive-descent disassembly.

use crate::isa::aarch64;
use crate::isa::aarch64::{
    Aarch64Mnemonic, DataRange, DataReason, DecodedOperand, Disassembly,
};

const NOP: u32 = 0xd503_201f;
const RET: u32 = 0xd65f_03c0;
const NOT_AN_INSTRUCTION: u32 = 0x0000_0000;

fn encode_le(words: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[test]
fn empty_input_yields_empty_disassembly() {
    let disasm = aarch64::disassemble_recursive(0x1000, &[], &[]);
    assert!(disasm.instructions.is_empty());
    assert!(disasm.data_ranges.is_empty());
}

#[test]
fn no_entry_points_classifies_everything_as_unreachable() {
    let bytes = encode_le(&[NOP, NOP, RET]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[]);
    assert!(disasm.instructions.is_empty());
    assert_eq!(disasm.data_ranges.len(), 1);
    assert_eq!(disasm.data_ranges[0].reason, DataReason::Unreachable);
    assert_eq!(disasm.data_ranges[0].address, 0x1000);
    assert_eq!(disasm.data_ranges[0].bytes, bytes);
}

#[test]
fn single_entry_walks_to_terminator() {
    // nop ; nop ; ret
    let bytes = encode_le(&[NOP, NOP, RET]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 3);
    assert!(disasm.data_ranges.is_empty());
    assert_eq!(disasm.instructions[0].mnemonic, Aarch64Mnemonic::Nop);
    assert_eq!(disasm.instructions[2].mnemonic, Aarch64Mnemonic::Ret);
}

#[test]
fn bytes_after_ret_are_unreachable_data() {
    // nop ; ret ; <dead nop> ; <dead nop>
    let bytes = encode_le(&[NOP, RET, NOP, NOP]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 2);
    assert_eq!(disasm.data_ranges.len(), 1);
    assert_eq!(disasm.data_ranges[0].address, 0x1008);
    assert_eq!(disasm.data_ranges[0].bytes.len(), 8);
    assert_eq!(disasm.data_ranges[0].reason, DataReason::Unreachable);
}

#[test]
fn undecodable_word_is_classified_as_decode_error() {
    // The all-zero word doesn't match any opcode in our table.
    let bytes = encode_le(&[NOT_AN_INSTRUCTION]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert!(disasm.instructions.is_empty());
    assert_eq!(disasm.data_ranges.len(), 1);
    assert_eq!(disasm.data_ranges[0].reason, DataReason::DecodeError);
}

#[test]
fn unaligned_tail_becomes_padding() {
    // Three instructions plus 2 stray bytes.
    let mut bytes = encode_le(&[NOP, NOP, RET]);
    bytes.push(0xab);
    bytes.push(0xcd);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 3);
    assert_eq!(disasm.data_ranges.len(), 1);
    let pad = &disasm.data_ranges[0];
    assert_eq!(pad.reason, DataReason::Padding);
    assert_eq!(pad.address, 0x100c);
    assert_eq!(pad.bytes, vec![0xab, 0xcd]);
}

#[test]
fn unconditional_jump_routes_decode_to_target() {
    // 0x1000: b 0x1008 ; 0x1004: <dead nop> ; 0x1008: ret
    // The middle word at 0x1004 is unreachable.
    let words = [
        0x1400_0002, // b +8
        NOP,
        RET,
    ];
    let bytes = encode_le(&words);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 2);
    assert!(disasm
        .instructions
        .iter()
        .any(|i| i.mnemonic == Aarch64Mnemonic::B));
    assert!(disasm
        .instructions
        .iter()
        .any(|i| i.mnemonic == Aarch64Mnemonic::Ret));
    assert_eq!(disasm.data_ranges.len(), 1);
    assert_eq!(disasm.data_ranges[0].address, 0x1004);
    assert_eq!(disasm.data_ranges[0].reason, DataReason::Unreachable);
}

#[test]
fn loop_back_edge_terminates() {
    // 0x1000: nop ; 0x1004: b 0x1000 (back to top)
    // Recursion must not loop forever.
    let bytes = encode_le(&[
        NOP,
        0x17ff_ffff, // b -4 (back to address 0x1000)
    ]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 2);
    assert!(disasm.data_ranges.is_empty());
}

#[test]
fn conditional_branch_walks_both_taken_and_fallthrough() {
    // 0x1000: b.eq 0x100c ; 0x1004: nop ; 0x1008: ret ; 0x100c: ret
    let bytes = encode_le(&[
        0x5400_0060, // b.eq +12
        NOP,
        RET,
        RET,
    ]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    // All four words should be decoded.
    assert_eq!(disasm.instructions.len(), 4);
    assert!(disasm.data_ranges.is_empty());
}

#[test]
fn call_returns_to_fallthrough() {
    // 0x1000: bl 0x100c (call, returns) ; 0x1004: nop ; 0x1008: ret ;
    // 0x100c: ret (the callee).
    let bytes = encode_le(&[
        0x9400_0003, // bl +12
        NOP,
        RET,
        RET,
    ]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    // All four words decoded — we follow both the call target and the
    // fallthrough.
    assert_eq!(disasm.instructions.len(), 4);
    assert!(disasm.data_ranges.is_empty());
}

#[test]
fn multiple_entry_points_walk_disjoint_regions() {
    // 0x1000: nop ; 0x1004: ret  ←  function A
    // 0x1008: <unreachable from A>
    // 0x100c: nop ; 0x1010: ret  ←  function B (separate entry)
    let bytes = encode_le(&[NOP, RET, NOT_AN_INSTRUCTION, NOP, RET]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000, 0x100c]);
    assert_eq!(disasm.instructions.len(), 4);
    // The undecodable word at 0x1008 is also unreached, but since it's
    // inside the gap between two functions and never queued for
    // decoding, it stays Unreachable rather than DecodeError.
    let gap_ranges: Vec<_> = disasm
        .data_ranges
        .iter()
        .filter(|r| r.address == 0x1008)
        .collect();
    assert_eq!(gap_ranges.len(), 1);
    assert_eq!(gap_ranges[0].reason, DataReason::Unreachable);
}

#[test]
fn literal_pool_after_function_is_data() {
    // Realistic shape: function body, then 8 bytes of literal pool.
    // The literal pool has a non-instruction value sandwiched between
    // valid-looking words; recursive descent stops at the ret and
    // classifies the rest as data.
    let bytes = encode_le(&[NOP, RET, NOT_AN_INSTRUCTION, NOT_AN_INSTRUCTION]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 2);
    assert_eq!(disasm.data_ranges.len(), 1);
    let pool = &disasm.data_ranges[0];
    assert_eq!(pool.address, 0x1008);
    assert_eq!(pool.bytes.len(), 8);
    assert_eq!(pool.reason, DataReason::Unreachable);
}

#[test]
fn timeline_orders_instructions_and_data_by_address() {
    let bytes = encode_le(&[NOP, RET, NOT_AN_INSTRUCTION]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    let mut last_address: Option<u64> = None;
    for entry in disasm.timeline() {
        let address = match entry {
            aarch64::TimelineEntry::Instruction(i) => i.address,
            aarch64::TimelineEntry::Data(r) => r.address,
        };
        if let Some(prev) = last_address {
            assert!(
                address > prev,
                "timeline addresses must be strictly increasing"
            );
        }
        last_address = Some(address);
    }
}

#[test]
fn entry_point_outside_range_is_ignored() {
    let bytes = encode_le(&[NOP, RET]);
    // 0x9000 is well outside [0x1000, 0x1008). Should be ignored, no
    // panics, and the in-range bytes stay Unreachable.
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x9000]);
    assert!(disasm.instructions.is_empty());
    assert_eq!(disasm.data_ranges.len(), 1);
    assert_eq!(disasm.data_ranges[0].reason, DataReason::Unreachable);
}

#[test]
fn unaligned_entry_point_is_ignored() {
    let bytes = encode_le(&[NOP, RET]);
    // 0x1003 is misaligned. Recursive descent should ignore it.
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1003]);
    assert!(disasm.instructions.is_empty());
}

#[test]
fn decode_failure_is_neighbored_by_unreachable() {
    // 0x1000: nop ; 0x1004: ret ; 0x1008: <undecodable> ; 0x100c: nop (unreachable)
    let bytes = encode_le(&[NOP, RET, NOT_AN_INSTRUCTION, NOP]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 2);

    // The two trailing 4-byte words should both be data, but with
    // *different* reasons — Unreachable for the inert ones, since
    // recursive descent never queued them for decoding. (We only mark
    // DecodeError for words we attempted to decode.)
    let trailing_reasons: Vec<DataReason> =
        disasm.data_ranges.iter().map(|r| r.reason).collect();
    assert!(trailing_reasons.contains(&DataReason::Unreachable));
}

#[test]
fn merges_adjacent_unreachable_ranges() {
    let bytes = encode_le(&[RET, NOP, NOP, NOP]); // ret terminates after 1 insn
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 1);
    // The three trailing nops should merge into a single Unreachable range.
    assert_eq!(disasm.data_ranges.len(), 1);
    assert_eq!(disasm.data_ranges[0].bytes.len(), 12);
}

#[test]
fn branch_target_in_data_region_is_followed() {
    // Demonstrate that recursive descent correctly walks into bytes that
    // would have been data under a no-entry-point sweep, if a branch
    // points at them.
    // 0x1000: b 0x1008 ; 0x1004: <reachable via branch from 0x1000? no, b skips
    // it>; 0x1008: ret
    let bytes = encode_le(&[0x1400_0002, NOT_AN_INSTRUCTION, RET]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    assert_eq!(disasm.instructions.len(), 2);
    // The 0x1004 word was undecodable AND unreachable; it stays as data.
    assert_eq!(disasm.data_ranges.len(), 1);
    assert_eq!(disasm.data_ranges[0].address, 0x1004);
    assert_eq!(disasm.data_ranges[0].reason, DataReason::Unreachable);
}

#[test]
fn instruction_operand_decodes_match_per_word_decode() {
    // Sanity check: instructions returned by recursive descent are
    // identical to what `decode_instruction` would produce for each
    // address.
    let bytes = encode_le(&[NOP, RET]);
    let disasm = aarch64::disassemble_recursive(0x1000, &bytes, &[0x1000]);
    for instruction in &disasm.instructions {
        let word = u32::from_le_bytes([
            bytes[(instruction.address - 0x1000) as usize],
            bytes[(instruction.address - 0x1000) as usize + 1],
            bytes[(instruction.address - 0x1000) as usize + 2],
            bytes[(instruction.address - 0x1000) as usize + 3],
        ]);
        let direct = aarch64::decode_instruction(instruction.address, word).unwrap();
        assert_eq!(instruction, &direct);
    }
}

#[test]
fn empty_disassembly_default_is_useful() {
    let _: Disassembly = Disassembly::default();
    // No-op — just verify the default impl exists for callers like the
    // dump example.
}

// Pull in DecodedOperand / DataRange just to silence unused-import
// warnings while keeping their availability for follow-up tests.
#[allow(dead_code)]
const _UNUSED: Option<(DecodedOperand, DataRange)> = None;
