//! Tests for the linear-sweep disassembler.

use super::common::*;
use crate::isa::aarch64;
use crate::isa::aarch64::{DisassembleError, Aarch64Mnemonic};
use crate::mc::{ControlFlow, InstructionInfo};

/// Encode each fixture word as little-endian bytes and concatenate.
fn fixture_bytes(fixture_text: &str) -> (u64, Vec<u8>) {
    let entries = parse_otool_fixture(fixture_text);
    assert!(!entries.is_empty());
    let base = entries[0].address;
    let mut bytes = Vec::with_capacity(entries.len() * 4);
    for entry in &entries {
        bytes.extend_from_slice(&entry.word.to_le_bytes());
    }
    (base, bytes)
}

#[test]
fn empty_input_yields_empty_disassembly() {
    let result = aarch64::disassemble_bytes(0x1000, &[]).expect("empty input is valid");
    assert!(result.is_empty());
}

#[test]
fn single_nop_decodes() {
    let bytes = 0xd503201fu32.to_le_bytes();
    let result = aarch64::disassemble_bytes(0, &bytes).expect("nop decodes");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].mnemonic, Aarch64Mnemonic::Nop);
    assert_eq!(result[0].address, 0);
}

#[test]
fn unaligned_length_is_rejected() {
    let bytes = [0u8; 5];
    let err = aarch64::disassemble_bytes(0, &bytes).expect_err("unaligned must error");
    assert_eq!(err, DisassembleError::UnalignedLength { length: 5 });
}

#[test]
fn unalignment_by_one_byte_is_rejected() {
    // Three full instructions plus a stray byte.
    let mut bytes = vec![0u8; 13];
    bytes[..4].copy_from_slice(&0xd503201fu32.to_le_bytes());
    bytes[4..8].copy_from_slice(&0xd503201fu32.to_le_bytes());
    bytes[8..12].copy_from_slice(&0xd503201fu32.to_le_bytes());
    let err = aarch64::disassemble_bytes(0, &bytes).expect_err("unaligned must error");
    assert_eq!(err, DisassembleError::UnalignedLength { length: 13 });
}

#[test]
fn decode_error_includes_address_and_word() {
    // 0x00000000 is `udf #0`, which the current opcode table doesn't match.
    // Sandwich it between two valid instructions to confirm the address
    // reported is the address of the bad word, not 0.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0xd503201fu32.to_le_bytes()); // nop @ 0x1000
    bytes.extend_from_slice(&0x00000000u32.to_le_bytes()); // bad @ 0x1004
    bytes.extend_from_slice(&0xd503201fu32.to_le_bytes()); // nop @ 0x1008

    let err = aarch64::disassemble_bytes(0x1000, &bytes).expect_err("bad word must error");
    match err {
        DisassembleError::DecodeFailed { address, word, .. } => {
            assert_eq!(address, 0x1004);
            assert_eq!(word, 0);
        }
        other => panic!("expected DecodeFailed, got {other:?}"),
    }
}

#[test]
fn sweep_matches_per_instruction_decode_for_branch_fixture() {
    let (base, bytes) = fixture_bytes(BRANCH_OTOOL_FIXTURE);
    let swept = aarch64::disassemble_bytes(base, &bytes).expect("branch fixture decodes");
    let entries = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);

    assert_eq!(swept.len(), entries.len());
    for (decoded, expected) in swept.iter().zip(entries.iter()) {
        assert_eq!(decoded.address, expected.address);
        assert_eq!(decoded.word, expected.word);
        // And the per-instruction API agrees with the sweep.
        let single = aarch64::decode_instruction(expected.address, expected.word)
            .expect("per-instruction decode");
        assert_eq!(decoded, &single);
    }
}

#[test]
fn sweep_addresses_are_contiguous_from_base() {
    let base = 0xdead_0000;
    let mut bytes = Vec::new();
    for _ in 0..8 {
        bytes.extend_from_slice(&0xd503201fu32.to_le_bytes());
    }
    let swept = aarch64::disassemble_bytes(base, &bytes).expect("nop sled decodes");
    for (i, instruction) in swept.iter().enumerate() {
        assert_eq!(instruction.address, base + (i as u64) * 4);
    }
}

#[test]
fn sweep_feeds_classifier_naturally() {
    // The whole point of the sweep: feed it into the classifier and get a
    // control-flow stream. This is what basic-block discovery will consume.
    let (base, bytes) = fixture_bytes(BRANCH_OTOOL_FIXTURE);
    let swept = aarch64::disassemble_bytes(base, &bytes).expect("branch fixture decodes");

    // The branch fixture has a single `b 0x4c` at the start.
    let first = &swept[0];
    assert_eq!(
        first.control_flow(),
        ControlFlow::Jump { target: 0x4c }
    );
}
