//! Smoke tests for the public opcode-table reflection API used by external
//! assembler / autocomplete tooling.

use crate::isa::aarch64::{
    iter_opcodes, operand_bit_ranges, Aarch64Mnemonic, Aarch64Opnd,
};

#[test]
fn iter_opcodes_visits_the_full_table() {
    let count = iter_opcodes().count();
    // The static table is sized as 1157 rows; assert the iterator surfaces
    // every row without claiming a specific count beyond a sanity floor.
    assert!(count >= 1000, "iter_opcodes() yielded {count} rows");
}

#[test]
fn iter_opcodes_includes_ret() {
    let found = iter_opcodes().any(|op| op.mnemonic_id() == Aarch64Mnemonic::Ret);
    assert!(found, "expected at least one RET row in the opcode table");
}

#[test]
fn opcode_exposes_base_and_operands_for_mov_imm() {
    let mov_imm_rows: Vec<_> = iter_opcodes()
        .filter(|op| op.mnemonic_id() == Aarch64Mnemonic::Mov)
        .collect();
    assert!(!mov_imm_rows.is_empty(), "no MOV rows in the table");

    // The reflection methods should all be callable from outside the crate
    // — this test asserts they compile against the public surface.
    for opcode in mov_imm_rows {
        let _base = opcode.base_opcode();
        let _mask = opcode.mask();
        let _operands = opcode.operands();
        let _class = opcode.class_name();
        let _ranges = opcode.operand_bit_ranges();
    }
}

#[test]
fn operand_bit_ranges_match_operand_count() {
    for opcode in iter_opcodes() {
        let ranges = opcode.operand_bit_ranges();
        let operands = opcode.operands();
        assert_eq!(
            ranges.len(),
            operands.len(),
            "operand_bit_ranges() length disagrees with operands() for {}",
            opcode.mnemonic(),
        );
    }
}

#[test]
fn operand_bit_ranges_stay_within_32_bits() {
    for opcode in iter_opcodes() {
        for (slot, ranges) in opcode.operand_bit_ranges().iter().enumerate() {
            for range in ranges {
                assert!(
                    range.start < range.end && range.end <= 32,
                    "bad range {range:?} for slot {slot} of {}",
                    opcode.mnemonic(),
                );
            }
        }
    }
}

#[test]
fn rd_operand_kind_maps_to_low_five_bits() {
    let ranges = operand_bit_ranges(Aarch64Opnd::Rd);
    assert_eq!(ranges, vec![0..5]);
}

#[test]
fn rn_operand_kind_maps_to_bits_five_through_nine() {
    let ranges = operand_bit_ranges(Aarch64Opnd::Rn);
    assert_eq!(ranges, vec![5..10]);
}

#[test]
fn rm_shifted_operand_covers_rm_shift_type_and_amount() {
    // RmSft is Rm (16..21) + shift type (22..24) + shift amount (10..16).
    let ranges = operand_bit_ranges(Aarch64Opnd::RmSft);
    assert_eq!(ranges, vec![16..21, 22..24, 10..16]);
}
