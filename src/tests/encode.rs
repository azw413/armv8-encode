//! Encoder tests: building `InstructionTemplate`s and asserting the encoded
//! word matches a fixture entry.

use super::common::*;
use crate::isa::aarch64;
use crate::isa::aarch64::{Aarch64Mnemonic, DecodedOperand, EncodeError};

#[test]
fn encode_instruction_reports_unknown_mnemonic() {
    let instruction = template(0, Aarch64Mnemonic::Other("not-an-opcode"), Vec::new());

    assert_eq!(
        aarch64::encode_instruction(&instruction),
        Err(EncodeError::UnknownMnemonic {
            mnemonic: "not-an-opcode"
        })
    );
}

#[test]
fn encode_instruction_reports_no_matching_operand_count() {
    let instruction = template(0, Aarch64Mnemonic::Adc, Vec::new());

    assert_eq!(
        aarch64::encode_instruction(&instruction),
        Err(EncodeError::NoMatchingForm { mnemonic: "adc" })
    );
}

#[test]
fn encode_instruction_can_emit_operandless_table_rows() {
    let instruction = template(0, Aarch64Mnemonic::Nop, Vec::new());

    assert_eq!(aarch64::encode_instruction(&instruction), Ok(0xd503201f));
}

#[test]
fn encode_instruction_can_emit_register_operands() {
    let instruction = template(
        0,
        Aarch64Mnemonic::Adc,
        vec![
            DecodedOperand::Register(w_reg(0)),
            DecodedOperand::Register(w_reg(1)),
            DecodedOperand::Register(w_reg(2)),
        ],
    );

    assert_eq!(aarch64::encode_instruction(&instruction), Ok(0x1a020020));
}

#[test]
fn encoded_register_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(ENCODE_BASIC_OTOOL_FIXTURE);
    let cases = [
        (
            "adc w0, w1, w2",
            template(
                0,
                Aarch64Mnemonic::Adc,
                vec![
                    DecodedOperand::Register(w_reg(0)),
                    DecodedOperand::Register(w_reg(1)),
                    DecodedOperand::Register(w_reg(2)),
                ],
            ),
        ),
        (
            "adc x2, x2, x3",
            template(
                4,
                Aarch64Mnemonic::Adc,
                vec![
                    DecodedOperand::Register(x_reg(2)),
                    DecodedOperand::Register(x_reg(2)),
                    DecodedOperand::Register(x_reg(3)),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_ra_operand_matches_otool_fixture() {
    let fixture = parse_otool_fixture(DATAPROC_OTOOL_FIXTURE);
    let cases = [
        (
            "madd w9, w10, w11, w12",
            template(
                0x7c,
                Aarch64Mnemonic::Madd,
                vec![
                    DecodedOperand::Register(w_reg(9)),
                    DecodedOperand::Register(w_reg(10)),
                    DecodedOperand::Register(w_reg(11)),
                    DecodedOperand::Register(w_reg(12)),
                ],
            ),
        ),
        (
            "madd x0, x1, x2, x3",
            template(
                0x80,
                Aarch64Mnemonic::Madd,
                vec![
                    DecodedOperand::Register(x_reg(0)),
                    DecodedOperand::Register(x_reg(1)),
                    DecodedOperand::Register(x_reg(2)),
                    DecodedOperand::Register(x_reg(3)),
                ],
            ),
        ),
        (
            "msub w4, w5, w6, w7",
            template(
                0x84,
                Aarch64Mnemonic::Msub,
                vec![
                    DecodedOperand::Register(w_reg(4)),
                    DecodedOperand::Register(w_reg(5)),
                    DecodedOperand::Register(w_reg(6)),
                    DecodedOperand::Register(w_reg(7)),
                ],
            ),
        ),
        (
            "msub x13, x14, x15, x16",
            template(
                0x88,
                Aarch64Mnemonic::Msub,
                vec![
                    DecodedOperand::Register(x_reg(13)),
                    DecodedOperand::Register(x_reg(14)),
                    DecodedOperand::Register(x_reg(15)),
                    DecodedOperand::Register(x_reg(16)),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_rt_and_pcrel19_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);
    let cases = [
        (
            "b.eq 0x4c",
            template(
                0x8,
                Aarch64Mnemonic::Beq,
                vec![DecodedOperand::BranchTarget(0x4c)],
            ),
        ),
        (
            "b.ne 0x4c",
            template(
                0xc,
                Aarch64Mnemonic::Bne,
                vec![DecodedOperand::BranchTarget(0x4c)],
            ),
        ),
        (
            "cbz w0, 0x4c",
            template(
                0x10,
                Aarch64Mnemonic::Cbz,
                vec![
                    DecodedOperand::Register(w_reg(0)),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
        (
            "cbz x1, 0x4c",
            template(
                0x14,
                Aarch64Mnemonic::Cbz,
                vec![
                    DecodedOperand::Register(x_reg(1)),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
        (
            "cbnz w2, 0x4c",
            template(
                0x18,
                Aarch64Mnemonic::Cbnz,
                vec![
                    DecodedOperand::Register(w_reg(2)),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
        (
            "cbnz x3, 0x4c",
            template(
                0x1c,
                Aarch64Mnemonic::Cbnz,
                vec![
                    DecodedOperand::Register(x_reg(3)),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_pcrel26_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);
    let cases = [
        (
            "b 0x4c",
            template(
                0,
                Aarch64Mnemonic::B,
                vec![DecodedOperand::BranchTarget(0x4c)],
            ),
        ),
        (
            "bl 0x4c",
            template(
                4,
                Aarch64Mnemonic::Bl,
                vec![DecodedOperand::BranchTarget(0x4c)],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_bitnum_and_pcrel14_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);
    let cases = [
        (
            "tbz w0, #0x3, 0x4c",
            template(
                0x20,
                Aarch64Mnemonic::Tbz,
                vec![
                    DecodedOperand::Register(w_reg(0)),
                    DecodedOperand::Immediate(3),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
        (
            "tbnz w1, #0x4, 0x4c",
            template(
                0x24,
                Aarch64Mnemonic::Tbnz,
                vec![
                    DecodedOperand::Register(w_reg(1)),
                    DecodedOperand::Immediate(4),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
        (
            "tbz x2, #0x28, 0x4c",
            template(
                0x28,
                Aarch64Mnemonic::Tbz,
                vec![
                    DecodedOperand::Register(x_reg(2)),
                    DecodedOperand::Immediate(0x28),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
        (
            "tbnz x3, #0x29, 0x4c",
            template(
                0x2c,
                Aarch64Mnemonic::Tbnz,
                vec![
                    DecodedOperand::Register(x_reg(3)),
                    DecodedOperand::Immediate(0x29),
                    DecodedOperand::BranchTarget(0x4c),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_rt2_and_simm7_pair_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(BASIC_OTOOL_FIXTURE);
    let cases = [
        (
            "stp x29, x30, [sp, #-0x10]!",
            template(
                0,
                Aarch64Mnemonic::Stp,
                vec![
                    DecodedOperand::Register(x_reg(29)),
                    DecodedOperand::Register(x_reg(30)),
                    mem_imm(sp_reg(), -0x10, aarch64::AddressingMode::PreIndex),
                ],
            ),
        ),
        (
            "ldp x29, x30, [sp], #0x10",
            template(
                0x1c,
                Aarch64Mnemonic::Ldp,
                vec![
                    DecodedOperand::Register(x_reg(29)),
                    DecodedOperand::Register(x_reg(30)),
                    mem_imm(sp_reg(), 0x10, aarch64::AddressingMode::PostIndex),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_rs_and_simple_address_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(LOADSTORE_OTOOL_FIXTURE);
    let cases = [
        (
            "ldxr x20, [x21]",
            template(
                0x28,
                Aarch64Mnemonic::Ldxr,
                vec![DecodedOperand::Register(x_reg(20)), mem_simple(x_reg(21))],
            ),
        ),
        (
            "stxr w22, x23, [x24]",
            template(
                0x2c,
                Aarch64Mnemonic::Stxr,
                vec![
                    DecodedOperand::Register(w_reg(22)),
                    DecodedOperand::Register(x_reg(23)),
                    mem_simple(x_reg(24)),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_uimm12_address_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(LOADSTORE_OTOOL_FIXTURE);
    let cases = [
        (
            "str x0, [x1, #0x10]",
            template(
                0,
                Aarch64Mnemonic::Str,
                vec![
                    DecodedOperand::Register(x_reg(0)),
                    mem_imm(x_reg(1), 0x10, aarch64::AddressingMode::Offset),
                ],
            ),
        ),
        (
            "ldr x2, [x3, #0x18]",
            template(
                4,
                Aarch64Mnemonic::Ldr,
                vec![
                    DecodedOperand::Register(x_reg(2)),
                    mem_imm(x_reg(3), 0x18, aarch64::AddressingMode::Offset),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_simm9_address_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(LOADSTORE_OTOOL_FIXTURE);
    let cases = [
        (
            "stur x4, [x5, #-0x8]",
            template(
                0x8,
                Aarch64Mnemonic::Stur,
                vec![
                    DecodedOperand::Register(x_reg(4)),
                    mem_imm(x_reg(5), -0x8, aarch64::AddressingMode::Offset),
                ],
            ),
        ),
        (
            "ldur x6, [x7, #-0x10]",
            template(
                0xc,
                Aarch64Mnemonic::Ldur,
                vec![
                    DecodedOperand::Register(x_reg(6)),
                    mem_imm(x_reg(7), -0x10, aarch64::AddressingMode::Offset),
                ],
            ),
        ),
        (
            "str x8, [x9], #0x8",
            template(
                0x10,
                Aarch64Mnemonic::Str,
                vec![
                    DecodedOperand::Register(x_reg(8)),
                    mem_imm(x_reg(9), 0x8, aarch64::AddressingMode::PostIndex),
                ],
            ),
        ),
        (
            "ldr x10, [x11, #0x8]!",
            template(
                0x14,
                Aarch64Mnemonic::Ldr,
                vec![
                    DecodedOperand::Register(x_reg(10)),
                    mem_imm(x_reg(11), 0x8, aarch64::AddressingMode::PreIndex),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_register_offset_address_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(LOADSTORE_OTOOL_FIXTURE);
    let cases = [
        (
            "ldr x12, [x13, x14]",
            template(
                0x18,
                Aarch64Mnemonic::Ldr,
                vec![
                    DecodedOperand::Register(x_reg(12)),
                    mem_reg(x_reg(13), x_reg(14), None),
                ],
            ),
        ),
        (
            "ldr x15, [x16, x17, lsl #3]",
            template(
                0x1c,
                Aarch64Mnemonic::Ldr,
                vec![
                    DecodedOperand::Register(x_reg(15)),
                    mem_reg(
                        x_reg(16),
                        x_reg(17),
                        Some(aarch64::Shift {
                            kind: aarch64::ShiftKind::Lsl,
                            amount: 3,
                        }),
                    ),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_sp_and_extended_register_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(EXTEND_OTOOL_FIXTURE);
    let cases = [
        (
            "add x0, x1, w2, uxtb",
            template(
                0,
                Aarch64Mnemonic::Add,
                vec![
                    DecodedOperand::Register(x_reg(0)),
                    DecodedOperand::Register(x_reg(1)),
                    DecodedOperand::ExtendedRegister(ext_reg(
                        w_reg(2),
                        aarch64::ExtendKind::Uxtb,
                        0,
                    )),
                ],
            ),
        ),
        (
            "add x3, sp, w4, uxth #1",
            template(
                4,
                Aarch64Mnemonic::Add,
                vec![
                    DecodedOperand::Register(x_reg(3)),
                    DecodedOperand::Register(sp_reg()),
                    DecodedOperand::ExtendedRegister(ext_reg(
                        w_reg(4),
                        aarch64::ExtendKind::Uxth,
                        1,
                    )),
                ],
            ),
        ),
        (
            "adds x5, x6, w7, uxtw #2",
            template(
                8,
                Aarch64Mnemonic::Adds,
                vec![
                    DecodedOperand::Register(x_reg(5)),
                    DecodedOperand::Register(x_reg(6)),
                    DecodedOperand::ExtendedRegister(ext_reg(
                        w_reg(7),
                        aarch64::ExtendKind::Uxtw,
                        2,
                    )),
                ],
            ),
        ),
        (
            "sub x10, x11, x12, uxtx #4",
            template(
                0x10,
                Aarch64Mnemonic::Sub,
                vec![
                    DecodedOperand::Register(x_reg(10)),
                    DecodedOperand::Register(x_reg(11)),
                    DecodedOperand::ExtendedRegister(ext_reg(
                        x_reg(12),
                        aarch64::ExtendKind::Uxtx,
                        4,
                    )),
                ],
            ),
        ),
        (
            "subs x13, x14, w15, sxth #1",
            template(
                0x14,
                Aarch64Mnemonic::Subs,
                vec![
                    DecodedOperand::Register(x_reg(13)),
                    DecodedOperand::Register(x_reg(14)),
                    DecodedOperand::ExtendedRegister(ext_reg(
                        w_reg(15),
                        aarch64::ExtendKind::Sxth,
                        1,
                    )),
                ],
            ),
        ),
        (
            "sub sp, sp, x17, sxtx",
            template(
                0x1c,
                Aarch64Mnemonic::Sub,
                vec![
                    DecodedOperand::Register(sp_reg()),
                    DecodedOperand::Register(sp_reg()),
                    DecodedOperand::ExtendedRegister(ext_reg(
                        x_reg(17),
                        aarch64::ExtendKind::Sxtx,
                        0,
                    )),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_shifted_register_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(INTEGER_OTOOL_FIXTURE);
    let instruction = template(
        0,
        Aarch64Mnemonic::Add,
        vec![
            DecodedOperand::Register(x_reg(4)),
            DecodedOperand::Register(x_reg(5)),
            DecodedOperand::ShiftedRegister(shift_reg(x_reg(6), aarch64::ShiftKind::Lsl, 3)),
        ],
    );

    assert_encoded_instruction_matches_fixture(&fixture, "add x4, x5, x6, lsl #3", &instruction);
}

#[test]
fn encoded_arithmetic_immediate_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(BASIC_OTOOL_FIXTURE);
    let cases = [
        (
            "add x0, x0, #0x1",
            template(
                0x8,
                Aarch64Mnemonic::Add,
                vec![
                    DecodedOperand::Register(x_reg(0)),
                    DecodedOperand::Register(x_reg(0)),
                    DecodedOperand::Immediate(1),
                ],
            ),
        ),
        (
            "sub x1, x1, #0x2",
            template(
                0xc,
                Aarch64Mnemonic::Sub,
                vec![
                    DecodedOperand::Register(x_reg(1)),
                    DecodedOperand::Register(x_reg(1)),
                    DecodedOperand::Immediate(2),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_move_wide_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(INTEGER_OTOOL_FIXTURE);
    let cases = [
        (
            "mov x9, #0x1234",
            template(
                0xc,
                Aarch64Mnemonic::Mov,
                vec![
                    DecodedOperand::Register(x_reg(9)),
                    DecodedOperand::Immediate(0x1234),
                ],
            ),
        ),
        (
            "movk x9, #0xabcd, lsl #16",
            template(
                0x10,
                Aarch64Mnemonic::Movk,
                vec![
                    DecodedOperand::Register(x_reg(9)),
                    DecodedOperand::ShiftedImmediate(aarch64::ShiftedImmediate {
                        value: 0xabcd,
                        shift: 16,
                    }),
                ],
            ),
        ),
        (
            "mov x10, #-0x56",
            template(
                0x14,
                Aarch64Mnemonic::Mov,
                vec![
                    DecodedOperand::Register(x_reg(10)),
                    DecodedOperand::Immediate(-0x56),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_bitfield_immediate_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(INTEGER_OTOOL_FIXTURE);
    let cases = [
        (
            "ubfx x11, x12, #8, #16",
            template(
                0x18,
                Aarch64Mnemonic::Ubfx,
                vec![
                    DecodedOperand::Register(x_reg(11)),
                    DecodedOperand::Register(x_reg(12)),
                    DecodedOperand::Immediate(8),
                    DecodedOperand::Immediate(16),
                ],
            ),
        ),
        (
            "bfxil x13, x14, #4, #12",
            template(
                0x1c,
                Aarch64Mnemonic::Bfxil,
                vec![
                    DecodedOperand::Register(x_reg(13)),
                    DecodedOperand::Register(x_reg(14)),
                    DecodedOperand::Immediate(4),
                    DecodedOperand::Immediate(12),
                ],
            ),
        ),
        (
            "lsl x15, x16, #5",
            template(
                0x20,
                Aarch64Mnemonic::Lsl,
                vec![
                    DecodedOperand::Register(x_reg(15)),
                    DecodedOperand::Register(x_reg(16)),
                    DecodedOperand::Immediate(5),
                ],
            ),
        ),
        (
            "lsr x17, x18, #6",
            template(
                0x24,
                Aarch64Mnemonic::Lsr,
                vec![
                    DecodedOperand::Register(x_reg(17)),
                    DecodedOperand::Register(x_reg(18)),
                    DecodedOperand::Immediate(6),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_logical_immediate_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(INTEGER_OTOOL_FIXTURE);
    let cases = [
        (
            "and x7, x7, #0xff",
            template(
                4,
                Aarch64Mnemonic::And,
                vec![
                    DecodedOperand::Register(x_reg(7)),
                    DecodedOperand::Register(x_reg(7)),
                    DecodedOperand::Immediate(0xff),
                ],
            ),
        ),
        (
            "eor x8, x8, #0xff00",
            template(
                8,
                Aarch64Mnemonic::Eor,
                vec![
                    DecodedOperand::Register(x_reg(8)),
                    DecodedOperand::Register(x_reg(8)),
                    DecodedOperand::Immediate(0xff00),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_pcrel21_operand_matches_otool_fixture() {
    let fixture = parse_otool_fixture(INTEGER_OTOOL_FIXTURE);
    let instruction = template(
        0x28,
        Aarch64Mnemonic::Adr,
        vec![
            DecodedOperand::Register(x_reg(19)),
            DecodedOperand::Immediate(4),
        ],
    );

    assert_encoded_instruction_matches_fixture(&fixture, "adr x19, #4", &instruction);
}

#[test]
fn encoded_adrp_operand_matches_otool_fixture() {
    let fixture = parse_otool_fixture(ADRP_OTOOL_FIXTURE);
    let cases = [
        (
            "adrp x0, 0 ; 0x0",
            template(
                0,
                Aarch64Mnemonic::Adrp,
                vec![
                    DecodedOperand::Register(x_reg(0)),
                    DecodedOperand::PageTarget(0),
                ],
            ),
        ),
        (
            "adrp x1, 1 ; 0x1000",
            template(
                4,
                Aarch64Mnemonic::Adrp,
                vec![
                    DecodedOperand::Register(x_reg(1)),
                    DecodedOperand::PageTarget(0x1000),
                ],
            ),
        ),
        (
            "adrp x2, -1 ; 0xfffffffffffff000",
            template(
                8,
                Aarch64Mnemonic::Adrp,
                vec![
                    DecodedOperand::Register(x_reg(2)),
                    DecodedOperand::PageTarget(0xffff_ffff_ffff_f000),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_condition_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);
    let cases = [
        (
            "csel x5, x6, x7, eq",
            template(
                0x3c,
                Aarch64Mnemonic::Csel,
                vec![
                    DecodedOperand::Register(x_reg(5)),
                    DecodedOperand::Register(x_reg(6)),
                    DecodedOperand::Register(x_reg(7)),
                    DecodedOperand::Condition("eq"),
                ],
            ),
        ),
        (
            "cinc x8, x9, ne",
            template(
                0x40,
                Aarch64Mnemonic::Cinc,
                vec![
                    DecodedOperand::Register(x_reg(8)),
                    DecodedOperand::Register(x_reg(9)),
                    DecodedOperand::Condition("ne"),
                ],
            ),
        ),
        (
            "ccmp x10, x11, #0x0, lt",
            template(
                0x44,
                Aarch64Mnemonic::Ccmp,
                vec![
                    DecodedOperand::Register(x_reg(10)),
                    DecodedOperand::Register(x_reg(11)),
                    DecodedOperand::Immediate(0),
                    DecodedOperand::Condition("lt"),
                ],
            ),
        ),
        (
            "ccmp x10, #0x7, #0x0, lt",
            template(
                0x48,
                Aarch64Mnemonic::Ccmp,
                vec![
                    DecodedOperand::Register(x_reg(10)),
                    DecodedOperand::Immediate(7),
                    DecodedOperand::Immediate(0),
                    DecodedOperand::Condition("lt"),
                ],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_fp_register_and_immediate_operands_match_otool_fixture() {
    let float_fixture = parse_otool_fixture(FLOAT_OTOOL_FIXTURE);
    let fpimm_fixture = parse_otool_fixture(FPIMM_OTOOL_FIXTURE);
    let cases = [
        (
            &float_fixture,
            "fadd s0, s1, s2",
            template(
                0,
                Aarch64Mnemonic::Other("fadd"),
                vec![
                    DecodedOperand::Register(s_reg(0)),
                    DecodedOperand::Register(s_reg(1)),
                    DecodedOperand::Register(s_reg(2)),
                ],
            ),
        ),
        (
            &float_fixture,
            "fsub d3, d4, d5",
            template(
                4,
                Aarch64Mnemonic::Other("fsub"),
                vec![
                    DecodedOperand::Register(d_reg(3)),
                    DecodedOperand::Register(d_reg(4)),
                    DecodedOperand::Register(d_reg(5)),
                ],
            ),
        ),
        (
            &float_fixture,
            "fmadd s12, s13, s14, s15",
            template(
                0x10,
                Aarch64Mnemonic::Other("fmadd"),
                vec![
                    DecodedOperand::Register(s_reg(12)),
                    DecodedOperand::Register(s_reg(13)),
                    DecodedOperand::Register(s_reg(14)),
                    DecodedOperand::Register(s_reg(15)),
                ],
            ),
        ),
        (
            &float_fixture,
            "fcmp d2, #0.0",
            template(
                0x1c,
                Aarch64Mnemonic::Other("fcmp"),
                vec![
                    DecodedOperand::Register(d_reg(2)),
                    DecodedOperand::FloatImmediate("0.0".to_string()),
                ],
            ),
        ),
        (
            &float_fixture,
            "fmov s23, #1.00000000",
            template(
                0x24,
                Aarch64Mnemonic::Other("fmov"),
                vec![
                    DecodedOperand::Register(s_reg(23)),
                    DecodedOperand::FloatImmediate("1.00000000".to_string()),
                ],
            ),
        ),
        (
            &fpimm_fixture,
            "fmov d0, #-1.93750000",
            template(
                0xdc,
                Aarch64Mnemonic::Other("fmov"),
                vec![
                    DecodedOperand::Register(d_reg(0)),
                    DecodedOperand::FloatImmediate("-1.93750000".to_string()),
                ],
            ),
        ),
    ];

    for (fixture, expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_exception_immediate_operands_match_otool_fixture() {
    let fixture = parse_otool_fixture(EXCEPTION_OTOOL_FIXTURE);
    let cases = [
        (
            "svc #0x1234",
            template(
                4,
                Aarch64Mnemonic::Svc,
                vec![DecodedOperand::Immediate(0x1234)],
            ),
        ),
        (
            "hvc #0x2345",
            template(
                8,
                Aarch64Mnemonic::Hvc,
                vec![DecodedOperand::Immediate(0x2345)],
            ),
        ),
        (
            "brk #0x4567",
            template(
                0x10,
                Aarch64Mnemonic::Brk,
                vec![DecodedOperand::Immediate(0x4567)],
            ),
        ),
    ];

    for (expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(&fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_system_operands_match_otool_fixture() {
    let system_fixture = parse_otool_fixture(SYSTEM_OTOOL_FIXTURE);
    let sysreg_fixture = parse_otool_fixture(SYSREG_OTOOL_FIXTURE);
    let generic_fixture = parse_otool_fixture(SYS_GENERIC_OTOOL_FIXTURE);
    let prfm_fixture = parse_otool_fixture(PRFM_OTOOL_FIXTURE);
    let cases = [
        (
            &system_fixture,
            "dsb sy",
            template(
                0xc,
                Aarch64Mnemonic::Other("dsb"),
                vec![DecodedOperand::System("sy".to_string())],
            ),
        ),
        (
            &system_fixture,
            "isb #3",
            template(
                0x18,
                Aarch64Mnemonic::Other("isb"),
                vec![DecodedOperand::System("#3".to_string())],
            ),
        ),
        (
            &system_fixture,
            "msr DAIFClr, #0x4",
            template(
                0x24,
                Aarch64Mnemonic::Other("msr"),
                vec![
                    DecodedOperand::System("DAIFClr".to_string()),
                    DecodedOperand::Immediate(4),
                ],
            ),
        ),
        (
            &sysreg_fixture,
            "mrs x2, TPIDR_EL0",
            template(
                8,
                Aarch64Mnemonic::Other("mrs"),
                vec![
                    DecodedOperand::Register(x_reg(2)),
                    DecodedOperand::System("TPIDR_EL0".to_string()),
                ],
            ),
        ),
        (
            &sysreg_fixture,
            "msr NZCV, x1",
            template(
                4,
                Aarch64Mnemonic::Other("msr"),
                vec![
                    DecodedOperand::System("NZCV".to_string()),
                    DecodedOperand::Register(x_reg(1)),
                ],
            ),
        ),
        (
            &generic_fixture,
            "sys #0x1, c2, c3, #0x4, x5",
            template(
                0,
                Aarch64Mnemonic::Other("sys"),
                vec![
                    DecodedOperand::Immediate(1),
                    DecodedOperand::System("c2".to_string()),
                    DecodedOperand::System("c3".to_string()),
                    DecodedOperand::Immediate(4),
                    DecodedOperand::Register(x_reg(5)),
                ],
            ),
        ),
        (
            &generic_fixture,
            "sysl x6, #0x1, c2, c3, #0x4",
            template(
                8,
                Aarch64Mnemonic::Other("sysl"),
                vec![
                    DecodedOperand::Register(x_reg(6)),
                    DecodedOperand::Immediate(1),
                    DecodedOperand::System("c2".to_string()),
                    DecodedOperand::System("c3".to_string()),
                    DecodedOperand::Immediate(4),
                ],
            ),
        ),
        (
            &prfm_fixture,
            "prfm pldl2strm, [x1, #0x10]",
            template(
                4,
                Aarch64Mnemonic::Other("prfm"),
                vec![
                    DecodedOperand::System("pldl2strm".to_string()),
                    mem_imm(x_reg(1), 0x10, aarch64::AddressingMode::Offset),
                ],
            ),
        ),
    ];

    for (fixture, expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(fixture, expected_case, &instruction);
    }
}

#[test]
fn encoded_simd_operands_match_otool_fixture() {
    let simd_same = parse_otool_fixture(SIMD_SAME_OTOOL_FIXTURE);
    let simd_scalar = parse_otool_fixture(SIMD_SCALAR_OTOOL_FIXTURE);
    let simd_list = parse_otool_fixture(SIMD_LIST_OTOOL_FIXTURE);
    let simd_ldst = parse_otool_fixture(SIMD_LDST_OTOOL_FIXTURE);
    let vector_d1 = parse_otool_fixture(VECTOR_D1_OTOOL_FIXTURE);
    let cases = [
        (
            &simd_same,
            "add.16b v3, v4, v5",
            template(
                4,
                Aarch64Mnemonic::Add,
                vec![
                    DecodedOperand::VectorRegister(v_reg(3, aarch64::VectorArrangement::B16)),
                    DecodedOperand::VectorRegister(v_reg(4, aarch64::VectorArrangement::B16)),
                    DecodedOperand::VectorRegister(v_reg(5, aarch64::VectorArrangement::B16)),
                ],
            ),
        ),
        (
            &simd_same,
            "add.4s v15, v16, v17",
            template(
                0x14,
                Aarch64Mnemonic::Add,
                vec![
                    DecodedOperand::VectorRegister(v_reg(15, aarch64::VectorArrangement::S4)),
                    DecodedOperand::VectorRegister(v_reg(16, aarch64::VectorArrangement::S4)),
                    DecodedOperand::VectorRegister(v_reg(17, aarch64::VectorArrangement::S4)),
                ],
            ),
        ),
        (
            &simd_scalar,
            "add d0, d1, d2",
            template(
                0,
                Aarch64Mnemonic::Add,
                vec![
                    DecodedOperand::Register(d_reg(0)),
                    DecodedOperand::Register(d_reg(1)),
                    DecodedOperand::Register(d_reg(2)),
                ],
            ),
        ),
        (
            &simd_list,
            "tbl.16b v3, { v4, v5 }, v6",
            template(
                4,
                Aarch64Mnemonic::Other("tbl"),
                vec![
                    DecodedOperand::VectorRegister(v_reg(3, aarch64::VectorArrangement::B16)),
                    DecodedOperand::VectorList(v_list(4, 2, aarch64::VectorArrangement::B16)),
                    DecodedOperand::VectorRegister(v_reg(6, aarch64::VectorArrangement::B16)),
                ],
            ),
        ),
        (
            &simd_ldst,
            "ld1.16b { v0 }, [x1]",
            template(
                0,
                Aarch64Mnemonic::Other("ld1"),
                vec![
                    DecodedOperand::VectorList(v_list(0, 1, aarch64::VectorArrangement::B16)),
                    mem_simple(x_reg(1)),
                ],
            ),
        ),
        (
            &simd_ldst,
            "st1.16b { v2 }, [x3], #16",
            template(
                4,
                Aarch64Mnemonic::Other("st1"),
                vec![
                    DecodedOperand::VectorList(v_list(2, 1, aarch64::VectorArrangement::B16)),
                    mem_imm(x_reg(3), 16, aarch64::AddressingMode::PostIndex),
                ],
            ),
        ),
        (
            &vector_d1,
            "fmov.d v0[1], x1",
            template(
                0,
                Aarch64Mnemonic::Other("fmov"),
                vec![
                    DecodedOperand::VectorElement(v_element(0, 1, aarch64::VectorElementSize::D)),
                    DecodedOperand::Register(x_reg(1)),
                ],
            ),
        ),
    ];

    for (fixture, expected_case, instruction) in cases {
        assert_encoded_instruction_matches_fixture(fixture, expected_case, &instruction);
    }
}

#[test]
fn encode_instruction_normalizes_conditional_branch_mnemonics() {
    let instruction = template(
        0,
        Aarch64Mnemonic::parse("b.ne"),
        vec![DecodedOperand::BranchTarget(0x1000)],
    );

    assert_eq!(aarch64::encode_instruction(&instruction), Ok(0x54008001));
}

// ---- mov with register 31 — wzr/wsp disambiguation ---------------------
//
// Two distinct AArch64 instructions share the visual form `mov reg, REG31`:
//
//   * `mov Wd, wzr` — the `orr Wd, wzr, Wm` alias (LogShift form,
//     base 0x2a0003e0). `Rn=31` here means the zero register.
//   * `mov Wd, wsp` — the `add Wd, Wsp, #0` alias (AddsubImm form,
//     base 0x11000000). `Rn=31` here means the stack pointer.
//
// They differ only in the source register's class (W/X vs WOrSp/XOrSp).
// The encoder must respect that distinction; otherwise `mov w8, wzr` —
// which appears in nearly every clang-emitted prologue as
// `0x2a1f03e8` — round-trips into `mov w8, wsp` (`0x110003e8`), which
// reads SP into a GPR and corrupts execution. Caught by the ELF
// runtime harness; covered here so it doesn't regress.

#[test]
fn encode_mov_w_from_wzr_uses_orr_alias() {
    use crate::isa::aarch64::{Register, RegisterClass};
    let wzr = Register {
        class: RegisterClass::W,
        index: 31,
    };
    let instruction = template(
        0,
        Aarch64Mnemonic::Mov,
        vec![
            DecodedOperand::Register(w_reg(8)),
            DecodedOperand::Register(wzr),
        ],
    );
    assert_eq!(aarch64::encode_instruction(&instruction), Ok(0x2a1f03e8));
}

#[test]
fn encode_mov_w_from_wsp_uses_add_alias() {
    use crate::isa::aarch64::{Register, RegisterClass};
    let wsp = Register {
        class: RegisterClass::WOrSp,
        index: 31,
    };
    let wd_sp = Register {
        class: RegisterClass::WOrSp,
        index: 8,
    };
    let instruction = template(
        0,
        Aarch64Mnemonic::Mov,
        vec![DecodedOperand::Register(wd_sp), DecodedOperand::Register(wsp)],
    );
    assert_eq!(aarch64::encode_instruction(&instruction), Ok(0x110003e8));
}

#[test]
fn encode_mov_x_from_xzr_uses_orr_alias() {
    use crate::isa::aarch64::{Register, RegisterClass};
    let xzr = Register {
        class: RegisterClass::X,
        index: 31,
    };
    let instruction = template(
        0,
        Aarch64Mnemonic::Mov,
        vec![
            DecodedOperand::Register(x_reg(8)),
            DecodedOperand::Register(xzr),
        ],
    );
    assert_eq!(aarch64::encode_instruction(&instruction), Ok(0xaa1f03e8));
}

#[test]
fn encode_mov_x_from_sp_uses_add_alias() {
    use crate::isa::aarch64::{Register, RegisterClass};
    let sp = Register {
        class: RegisterClass::XOrSp,
        index: 31,
    };
    let xd_sp = Register {
        class: RegisterClass::XOrSp,
        index: 8,
    };
    let instruction = template(
        0,
        Aarch64Mnemonic::Mov,
        vec![DecodedOperand::Register(xd_sp), DecodedOperand::Register(sp)],
    );
    assert_eq!(aarch64::encode_instruction(&instruction), Ok(0x910003e8));
}

#[test]
fn decode_then_encode_roundtrips_mov_w_from_wzr() {
    // End-to-end: decoder produces what the encoder consumes. Catches the
    // case where decoder labels the source as `W{index:31}` (correct,
    // wzr) but the encoder picks the AddsubImm form because its `RnSp`
    // codec accepts W-class.
    let word = 0x2a1f03e8;
    let decoded = aarch64::decode_instruction(0, word).expect("decode mov w8,wzr");
    let template = aarch64::InstructionTemplate {
        address: 0,
        mnemonic: decoded.mnemonic,
        operands: decoded.operands,
    };
    assert_eq!(aarch64::encode_instruction(&template), Ok(word));
}

#[test]
fn decode_then_encode_roundtrips_mov_x_from_xzr() {
    let word = 0xaa1f03e8;
    let decoded = aarch64::decode_instruction(0, word).expect("decode mov x8,xzr");
    let template = aarch64::InstructionTemplate {
        address: 0,
        mnemonic: decoded.mnemonic,
        operands: decoded.operands,
    };
    assert_eq!(aarch64::encode_instruction(&template), Ok(word));
}
