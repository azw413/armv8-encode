use crate::isa::aarch64;
use crate::isa::aarch64::table;
use crate::isa::aarch64::{Aarch64Mnemonic, DecodedOperand, EncodeError, Register, RegisterClass};
use std::collections::BTreeSet;

const BASIC_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/basic.otool.txt");
const INTEGER_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/integer.otool.txt");
const BRANCH_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/branch.otool.txt");
const LOADSTORE_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/loadstore.otool.txt");
const FLOAT_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/float.otool.txt");
const FPIMM_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/fpimm.otool.txt");
const CONVERT_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/convert.otool.txt");
const EXCEPTION_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/exception.otool.txt");
const DATAPROC_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/dataproc.otool.txt");
const EXTEND_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/extend.otool.txt");
const FPPAIR_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/fppair.otool.txt");
const ADRP_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/adrp.otool.txt");
const SYSTEM_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/system.otool.txt");
const SYSREG_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/sysreg.otool.txt");
const SYS_ALIAS_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/sys_alias.otool.txt");
const SYS_GENERIC_OTOOL_FIXTURE: &str =
    include_str!("../tests/fixtures/aarch64/sys_generic.otool.txt");
const PRFM_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/prfm.otool.txt");
const SIMD_SAME_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/simd_same.otool.txt");
const SIMD_SCALAR_OTOOL_FIXTURE: &str =
    include_str!("../tests/fixtures/aarch64/simd_scalar.otool.txt");
const PAIRREG_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/pairreg.otool.txt");
const VECTOR_D1_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/vector_d1.otool.txt");
const SIMD_LIST_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/simd_list.otool.txt");
const SIMD_LDST_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/simd_ldst.otool.txt");
const SHLL_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/shll.otool.txt");
const SIMD_REMAINING_OTOOL_FIXTURE: &str =
    include_str!("../tests/fixtures/aarch64/simd_remaining.otool.txt");
const WHOLE_FUNCTIONS_OTOOL_FIXTURE: &str =
    include_str!("../tests/fixtures/aarch64/whole_functions.otool.txt");
const FORMATTING_OTOOL_FIXTURE: &str =
    include_str!("../tests/fixtures/aarch64/formatting.otool.txt");
const ENCODE_BASIC_OTOOL_FIXTURE: &str =
    include_str!("../tests/fixtures/aarch64/encode_basic.otool.txt");
const DIRECT_TESTED_OPERAND_KINDS: &[&str] = &[
    "AddrAdrp",
    "Barrier",
    "BarrierIsb",
    "BarrierPsb",
    "Cm",
    "Cn",
    "Ed",
    "Em",
    "En",
    "Ft2",
    "Idx",
    "Imm0",
    "ImmVlsl",
    "ImmVlsr",
    "Immr",
    "Imms",
    "Lvn",
    "Lvt",
    "LvtAl",
    "Let",
    "Pairreg",
    "Pstatefield",
    "Prfop",
    "RmExt",
    "RtSys",
    "Sd",
    "Sm",
    "Sn",
    "ShllImm",
    "SimdAddrPost",
    "SimdAddrSimple",
    "SimdFpimm",
    "SimdImm",
    "SimdImmSft",
    "Sysreg",
    "SysregAt",
    "SysregDc",
    "SysregIc",
    "SysregTlbi",
    "Uimm3Op1",
    "Uimm3Op2",
    "Uimm4",
    "Uimm7",
    "Vd",
    "VdD1",
    "Vm",
    "Vn",
    "VnD1",
];

#[derive(Debug, Eq, PartialEq)]
struct OtoolFixtureInsn {
    address: u64,
    word: u32,
    mnemonic: String,
    operands: String,
}

fn parse_otool_fixture(text: &str) -> Vec<OtoolFixtureInsn> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let mut fields = line.split_whitespace();
            let address = u64::from_str_radix(fields.next().expect("address field"), 16)
                .expect("valid hex address");
            let word = u32::from_str_radix(fields.next().expect("word field"), 16)
                .expect("valid hex word");
            let mnemonic = fields.next().expect("mnemonic field").to_string();
            let operands = fields.collect::<Vec<_>>().join(" ");

            Some(OtoolFixtureInsn {
                address,
                word,
                mnemonic,
                operands,
            })
        })
        .collect()
}

#[test]
fn mnemonic_index_returns_ordered_candidate_rows() {
    let candidates = table::opcodes_for_mnemonic(Aarch64Mnemonic::Adc);

    assert!(!candidates.is_empty());
    assert_eq!(candidates[0].mnemonic(), "adc");
    assert_eq!(candidates[0].class_name(), "AddsubCarry");
    assert_eq!(candidates[0].base_opcode(), 0x1a000000);
    assert_eq!(
        candidates[0]
            .operands()
            .into_iter()
            .map(|kind| kind.name())
            .collect::<Vec<_>>(),
        vec!["Rd", "Rn", "Rm"]
    );
}

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
                Aarch64Mnemonic::Other("svc"),
                vec![DecodedOperand::Immediate(0x1234)],
            ),
        ),
        (
            "hvc #0x2345",
            template(
                8,
                Aarch64Mnemonic::Other("hvc"),
                vec![DecodedOperand::Immediate(0x2345)],
            ),
        ),
        (
            "brk #0x4567",
            template(
                0x10,
                Aarch64Mnemonic::Other("brk"),
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

fn template(
    address: u64,
    mnemonic: Aarch64Mnemonic,
    operands: Vec<DecodedOperand>,
) -> aarch64::InstructionTemplate {
    aarch64::InstructionTemplate {
        address,
        mnemonic,
        operands,
    }
}

fn w_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::W,
        index,
    }
}

fn x_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::X,
        index,
    }
}

fn s_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::S,
        index,
    }
}

fn d_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::D,
        index,
    }
}

fn v_reg(index: u8, arrangement: aarch64::VectorArrangement) -> aarch64::VectorRegister {
    aarch64::VectorRegister { index, arrangement }
}

fn v_element(index: u8, element: u8, size: aarch64::VectorElementSize) -> aarch64::VectorElement {
    aarch64::VectorElement {
        index,
        element,
        size,
    }
}

fn v_list(first: u8, count: u8, arrangement: aarch64::VectorArrangement) -> aarch64::VectorList {
    aarch64::VectorList {
        first,
        count,
        arrangement,
        element: None,
    }
}

fn sp_reg() -> Register {
    Register {
        class: RegisterClass::XOrSp,
        index: 31,
    }
}

fn mem_imm(base: Register, offset: i64, mode: aarch64::AddressingMode) -> DecodedOperand {
    DecodedOperand::Memory(aarch64::MemoryOperand {
        base,
        offset: aarch64::MemoryOffset::Immediate(offset),
        mode,
    })
}

fn mem_simple(base: Register) -> DecodedOperand {
    DecodedOperand::Memory(aarch64::MemoryOperand {
        base,
        offset: aarch64::MemoryOffset::None,
        mode: aarch64::AddressingMode::Offset,
    })
}

fn mem_reg(base: Register, register: Register, shift: Option<aarch64::Shift>) -> DecodedOperand {
    DecodedOperand::Memory(aarch64::MemoryOperand {
        base,
        offset: aarch64::MemoryOffset::Register { register, shift },
        mode: aarch64::AddressingMode::Offset,
    })
}

fn ext_reg(
    register: Register,
    extend: aarch64::ExtendKind,
    amount: u8,
) -> aarch64::ExtendedRegister {
    aarch64::ExtendedRegister {
        register,
        extend,
        amount,
    }
}

fn shift_reg(register: Register, kind: aarch64::ShiftKind, amount: u8) -> aarch64::ShiftedRegister {
    aarch64::ShiftedRegister {
        register,
        shift: aarch64::Shift { kind, amount },
    }
}

#[test]
fn table_mnemonics_match_otool_fixture() {
    let fixture = parse_otool_fixture(BASIC_OTOOL_FIXTURE);

    assert!(!fixture.is_empty());

    for expected in fixture {
        let matched = aarch64::match_opcode(expected.word)
            .unwrap_or_else(|| panic!("no opcode match at {:#x}", expected.address));

        assert_eq!(
            matched.mnemonic(),
            expected.mnemonic,
            "mnemonic mismatch at {:#x} for word {:#010x}",
            expected.address,
            expected.word
        );
    }
}

#[test]
fn decoded_basic_operands_match_otool() {
    assert_decoded_fixture_matches_otool(BASIC_OTOOL_FIXTURE, |target| match target {
        0x24 => Some("_callee".to_string()),
        _ => None,
    });
}

#[test]
fn decoded_integer_operands_match_otool() {
    assert_decoded_fixture_matches_otool(INTEGER_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_branch_operands_match_otool() {
    assert_decoded_fixture_matches_otool(BRANCH_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_loadstore_operands_match_otool() {
    assert_decoded_fixture_matches_otool(LOADSTORE_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_float_operands_match_otool() {
    assert_decoded_fixture_matches_otool(FLOAT_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_fpimm_operands_match_otool() {
    assert_decoded_fixture_matches_otool(FPIMM_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_convert_operands_match_otool() {
    assert_decoded_fixture_matches_otool(CONVERT_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_exception_operands_match_otool() {
    assert_decoded_fixture_matches_otool(EXCEPTION_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_dataproc_operands_match_otool() {
    assert_decoded_fixture_matches_otool(DATAPROC_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_extend_operands_match_otool() {
    assert_decoded_fixture_matches_otool(EXTEND_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_fppair_operands_match_otool() {
    assert_decoded_fixture_matches_otool(FPPAIR_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_adrp_operands_match_otool() {
    assert_decoded_fixture_matches_otool(ADRP_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_system_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SYSTEM_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_sysreg_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SYSREG_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_sys_alias_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SYS_ALIAS_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_psb_operand_is_csync() {
    let decoded = aarch64::decode_instruction(0, 0xd503223f).expect("psb should match");

    assert_eq!(decoded.format_mnemonic(), "psb");
    assert_eq!(decoded.format_operands(), "csync");
}

#[test]
fn decoded_sys_generic_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SYS_GENERIC_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_prfm_operands_match_otool() {
    assert_decoded_fixture_matches_otool(PRFM_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_simd_same_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SIMD_SAME_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_simd_scalar_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SIMD_SCALAR_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_pairreg_operands_match_otool() {
    assert_decoded_fixture_matches_otool(PAIRREG_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_vector_d1_operands_match_otool() {
    assert_decoded_fixture_matches_otool(VECTOR_D1_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_simd_list_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SIMD_LIST_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_simd_ldst_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SIMD_LDST_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_shll_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SHLL_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_simd_remaining_operands_match_otool() {
    assert_decoded_fixture_matches_otool(SIMD_REMAINING_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_whole_functions_match_otool() {
    assert_decoded_fixture_matches_otool(WHOLE_FUNCTIONS_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_formatting_cases_match_otool() {
    assert_decoded_fixture_matches_otool(FORMATTING_OTOOL_FIXTURE, |_| None);
}

#[test]
fn decoded_fixture_instructions_roundtrip_through_encoder() {
    for (fixture_name, fixture_text) in all_otool_fixtures() {
        assert_decode_encode_roundtrip_fixture(fixture_name, fixture_text);
    }
}

#[test]
fn addsub_ext_group_is_complete() {
    let summary = aarch64::opcode_class_summary("AddsubExt");

    assert_eq!(summary.row_count, 6);
    assert_eq!(
        summary.mnemonics,
        vec!["add", "adds", "cmn", "cmp", "sub", "subs"]
    );

    assert_fixture_covers_exact_cases(
        EXTEND_OTOOL_FIXTURE,
        &["cmn", "cmp"],
        3,
        &["cmn x8, w9, sxtb #3", "cmp sp, x16, sxtx #4"],
    );
    assert_fixture_covers_exact_cases(
        EXTEND_OTOOL_FIXTURE,
        &["add", "adds", "sub", "subs"],
        4,
        &[
            "add x0, x1, w2, uxtb",
            "add x3, sp, w4, uxth #1",
            "adds x5, x6, w7, uxtw #2",
            "sub x10, x11, x12, uxtx #4",
            "subs x13, x14, w15, sxth #1",
            "sub sp, sp, x17, sxtx",
        ],
    );
}

#[test]
fn dp1src_group_is_complete() {
    let summary = aarch64::opcode_class_summary("Dp1src");

    assert_eq!(summary.row_count, 8);
    assert_eq!(
        summary.mnemonics,
        vec!["cls", "clz", "rbit", "rev", "rev16", "rev32", "rev64"]
    );

    assert_fixture_covers_exact_cases(
        DATAPROC_OTOOL_FIXTURE,
        &["rbit", "rev16", "rev", "rev32", "clz", "cls"],
        2,
        &[
            "rbit w0, w1",
            "rbit x2, x3",
            "rev16 w4, w5",
            "rev16 x6, x7",
            "rev w8, w9",
            "rev x10, x11",
            "rev32 x12, x13",
            "clz w14, w15",
            "clz x16, x17",
            "cls w18, w19",
            "cls x20, x21",
        ],
    );
}

#[test]
fn exception_group_is_complete() {
    let summary = aarch64::opcode_class_summary("Exception");

    assert_eq!(summary.row_count, 8);
    assert_eq!(
        summary.mnemonics,
        vec!["brk", "dcps1", "dcps2", "dcps3", "hlt", "hvc", "smc", "svc"]
    );

    assert_fixture_covers_exact_cases(
        EXCEPTION_OTOOL_FIXTURE,
        &["svc", "hvc", "smc", "brk", "hlt", "dcps1", "dcps2", "dcps3"],
        1,
        &[
            "svc #0",
            "svc #0x1234",
            "hvc #0x2345",
            "smc #0x3456",
            "brk #0x4567",
            "hlt #0x5678",
            "dcps1 #0x1111",
            "dcps2 #0x2222",
            "dcps3 #0x3333",
        ],
    );

    assert_fixture_covers_exact_cases(EXCEPTION_OTOOL_FIXTURE, &["dcps1"], 0, &["dcps1"]);
}

#[test]
fn dp2src_group_is_complete() {
    let summary = aarch64::opcode_class_summary("Dp2src");

    assert_eq!(summary.row_count, 18);
    assert_eq!(
        summary.mnemonics,
        vec![
            "asr", "asrv", "crc32b", "crc32cb", "crc32ch", "crc32cw", "crc32cx", "crc32h",
            "crc32w", "crc32x", "lsl", "lslv", "lsr", "lsrv", "ror", "rorv", "sdiv", "udiv",
        ]
    );

    assert_fixture_covers_exact_cases(
        DATAPROC_OTOOL_FIXTURE,
        &[
            "udiv", "sdiv", "lsl", "lsr", "asr", "ror", "crc32b", "crc32h", "crc32w", "crc32x",
            "crc32cb", "crc32ch", "crc32cw", "crc32cx",
        ],
        3,
        &[
            "udiv w18, w19, w20",
            "udiv x1, x2, x3",
            "sdiv w4, w5, w6",
            "sdiv x21, x22, x23",
            "lsl w24, w25, w26",
            "lsl x27, x28, x29",
            "lsr w30, w0, w1",
            "lsr x0, x1, x2",
            "asr w3, w4, w5",
            "asr x9, x10, x11",
            "ror w12, w13, w14",
            "ror x6, x7, x8",
            "crc32b w0, w1, w2",
            "crc32h w3, w4, w5",
            "crc32w w6, w7, w8",
            "crc32x w9, w10, x11",
            "crc32cb w12, w13, w14",
            "crc32ch w15, w16, w17",
            "crc32cw w18, w19, w20",
            "crc32cx w21, w22, x23",
        ],
    );
}

#[test]
fn dp3src_group_is_complete() {
    let summary = aarch64::opcode_class_summary("Dp3src");

    assert_eq!(summary.row_count, 14);
    assert_eq!(
        summary.mnemonics,
        vec![
            "madd", "mneg", "msub", "mul", "smaddl", "smnegl", "smsubl", "smulh", "smull",
            "umaddl", "umnegl", "umsubl", "umulh", "umull",
        ]
    );

    assert_fixture_covers_exact_cases(
        DATAPROC_OTOOL_FIXTURE,
        &[
            "madd", "msub", "mul", "mneg", "smaddl", "smsubl", "smull", "smnegl", "smulh",
            "umaddl", "umsubl", "umull", "umnegl", "umulh",
        ],
        3,
        &[
            "mul w17, w18, w19",
            "mul x20, x21, x22",
            "mneg w23, w24, w25",
            "mneg x20, x21, x22",
            "smull x0, w1, w2",
            "smnegl x3, w4, w5",
            "smulh x6, x7, x8",
            "umull x17, w18, w19",
            "umnegl x20, w21, w22",
            "umulh x23, x24, x25",
        ],
    );
    assert_fixture_covers_exact_cases(
        DATAPROC_OTOOL_FIXTURE,
        &["madd", "msub", "smaddl", "smsubl", "umaddl", "umsubl"],
        4,
        &[
            "madd w9, w10, w11, w12",
            "madd x0, x1, x2, x3",
            "msub w4, w5, w6, w7",
            "msub x13, x14, x15, x16",
            "smaddl x23, w24, w25, x26",
            "smsubl x27, w28, w29, x30",
            "umaddl x9, w10, w11, x12",
            "umsubl x13, w14, w15, x16",
        ],
    );
}

#[test]
fn branch_imm_group_is_complete() {
    let summary = aarch64::opcode_class_summary("BranchImm");

    assert_eq!(summary.row_count, 2);
    assert_eq!(summary.mnemonics, vec!["b", "bl"]);

    assert_fixture_covers_exact_cases(
        BRANCH_OTOOL_FIXTURE,
        &["b", "bl"],
        1,
        &["b 0x4c", "bl 0x4c"],
    );
}

#[test]
fn compbranch_group_is_complete() {
    let summary = aarch64::opcode_class_summary("Compbranch");

    assert_eq!(summary.row_count, 2);
    assert_eq!(summary.mnemonics, vec!["cbnz", "cbz"]);

    assert_fixture_covers_exact_cases(
        BRANCH_OTOOL_FIXTURE,
        &["cbz", "cbnz"],
        2,
        &[
            "cbz w0, 0x4c",
            "cbz x1, 0x4c",
            "cbnz w2, 0x4c",
            "cbnz x3, 0x4c",
        ],
    );
}

#[test]
fn testbranch_group_is_complete() {
    let summary = aarch64::opcode_class_summary("Testbranch");

    assert_eq!(summary.row_count, 2);
    assert_eq!(summary.mnemonics, vec!["tbnz", "tbz"]);

    assert_fixture_covers_exact_cases(
        BRANCH_OTOOL_FIXTURE,
        &["tbz", "tbnz"],
        3,
        &[
            "tbz w0, #0x3, 0x4c",
            "tbnz w1, #0x4, 0x4c",
            "tbz x2, #0x28, 0x4c",
            "tbnz x3, #0x29, 0x4c",
        ],
    );
}

#[test]
fn branch_reg_group_is_complete() {
    let summary = aarch64::opcode_class_summary("BranchReg");

    assert_eq!(summary.row_count, 5);
    assert_eq!(summary.mnemonics, vec!["blr", "br", "drps", "eret", "ret"]);

    assert_fixture_covers_exact_cases(
        BRANCH_OTOOL_FIXTURE,
        &["br", "blr", "ret"],
        1,
        &["br x2", "blr x3", "ret x4"],
    );
    assert_fixture_covers_exact_cases(
        BRANCH_OTOOL_FIXTURE,
        &["ret", "eret", "drps"],
        0,
        &["ret", "eret", "drps"],
    );
}

fn assert_decoded_fixture_matches_otool<F>(fixture_text: &str, symbol_for_address: F)
where
    F: Fn(u64) -> Option<String> + Copy,
{
    let fixture = parse_otool_fixture(fixture_text);

    assert!(!fixture.is_empty());

    for expected in fixture {
        let decoded = aarch64::decode_instruction_with_symbols(
            expected.address,
            expected.word,
            symbol_for_address,
        )
        .unwrap_or_else(|| panic!("no opcode match at {:#x}", expected.address));

        assert_eq!(
            decoded.format_mnemonic(),
            expected.mnemonic,
            "mnemonic mismatch at {:#x} for word {:#010x}",
            expected.address,
            expected.word
        );
        let operands = decoded.format_operands_with_symbols(symbol_for_address);

        assert_eq!(
            operands, expected.operands,
            "operand mismatch at {:#x} for word {:#010x}",
            expected.address, expected.word
        );
    }
}

fn all_otool_fixtures() -> &'static [(&'static str, &'static str)] {
    &[
        ("basic", BASIC_OTOOL_FIXTURE),
        ("integer", INTEGER_OTOOL_FIXTURE),
        ("branch", BRANCH_OTOOL_FIXTURE),
        ("loadstore", LOADSTORE_OTOOL_FIXTURE),
        ("float", FLOAT_OTOOL_FIXTURE),
        ("fpimm", FPIMM_OTOOL_FIXTURE),
        ("convert", CONVERT_OTOOL_FIXTURE),
        ("exception", EXCEPTION_OTOOL_FIXTURE),
        ("dataproc", DATAPROC_OTOOL_FIXTURE),
        ("extend", EXTEND_OTOOL_FIXTURE),
        ("fppair", FPPAIR_OTOOL_FIXTURE),
        ("adrp", ADRP_OTOOL_FIXTURE),
        ("system", SYSTEM_OTOOL_FIXTURE),
        ("sysreg", SYSREG_OTOOL_FIXTURE),
        ("sys_alias", SYS_ALIAS_OTOOL_FIXTURE),
        ("sys_generic", SYS_GENERIC_OTOOL_FIXTURE),
        ("prfm", PRFM_OTOOL_FIXTURE),
        ("simd_same", SIMD_SAME_OTOOL_FIXTURE),
        ("simd_scalar", SIMD_SCALAR_OTOOL_FIXTURE),
        ("pairreg", PAIRREG_OTOOL_FIXTURE),
        ("vector_d1", VECTOR_D1_OTOOL_FIXTURE),
        ("simd_list", SIMD_LIST_OTOOL_FIXTURE),
        ("simd_ldst", SIMD_LDST_OTOOL_FIXTURE),
        ("shll", SHLL_OTOOL_FIXTURE),
        ("simd_remaining", SIMD_REMAINING_OTOOL_FIXTURE),
        ("whole_functions", WHOLE_FUNCTIONS_OTOOL_FIXTURE),
        ("formatting", FORMATTING_OTOOL_FIXTURE),
        ("encode_basic", ENCODE_BASIC_OTOOL_FIXTURE),
    ]
}

fn assert_decode_encode_roundtrip_fixture(fixture_name: &str, fixture_text: &str) {
    let fixture = parse_otool_fixture(fixture_text);

    assert!(!fixture.is_empty(), "empty fixture: {fixture_name}");

    for expected in fixture {
        let decoded =
            aarch64::decode_instruction(expected.address, expected.word).unwrap_or_else(|| {
                panic!(
                    "no opcode match while roundtripping {fixture_name} at {:#x}",
                    expected.address
                )
            });
        let mnemonic = roundtrip_mnemonic(&decoded);
        let template = aarch64::InstructionTemplate {
            address: decoded.address,
            mnemonic,
            operands: decoded.operands.clone(),
        };
        let encoded = aarch64::encode_instruction(&template).unwrap_or_else(|err| {
            panic!(
                "failed to re-encode {fixture_name} at {:#x} ({} {}; decoded {} {}): {err:?}",
                expected.address,
                expected.mnemonic,
                expected.operands,
                decoded.format_mnemonic(),
                decoded.format_operands()
            )
        });

        let decoded_again =
            aarch64::decode_instruction(expected.address, encoded).expect("roundtrip word decodes");
        assert_eq!(
            decoded_again.format_mnemonic(),
            decoded.format_mnemonic(),
            "roundtrip mnemonic mismatch for {fixture_name} at {:#x} (original word {:#010x}, encoded word {:#010x})",
            expected.address,
            expected.word,
            encoded
        );
        assert_eq!(
            decoded_again.format_operands(),
            decoded.format_operands(),
            "roundtrip operand mismatch for {fixture_name} at {:#x} (original word {:#010x}, encoded word {:#010x})",
            expected.address,
            expected.word,
            encoded
        );
    }
}

fn roundtrip_mnemonic(decoded: &aarch64::DecodedInstruction) -> Aarch64Mnemonic {
    let formatted = decoded.format_mnemonic();
    if formatted.starts_with("b.") {
        return Aarch64Mnemonic::parse(decoded.mnemonic);
    }

    match formatted.split_once('.').map(|(mnemonic, _)| mnemonic) {
        Some("dup") => Aarch64Mnemonic::Other("dup"),
        Some("fmov") => Aarch64Mnemonic::Other("fmov"),
        Some("ld1") => Aarch64Mnemonic::Other("ld1"),
        Some("ld2") => Aarch64Mnemonic::Other("ld2"),
        Some("ld3") => Aarch64Mnemonic::Other("ld3"),
        Some("ld4") => Aarch64Mnemonic::Other("ld4"),
        Some("mov") => Aarch64Mnemonic::Mov,
        Some("movi") => Aarch64Mnemonic::Other("movi"),
        Some("mvni") => Aarch64Mnemonic::Other("mvni"),
        Some("shll") => Aarch64Mnemonic::Other("shll"),
        Some("st1") => Aarch64Mnemonic::Other("st1"),
        Some("st2") => Aarch64Mnemonic::Other("st2"),
        Some("st3") => Aarch64Mnemonic::Other("st3"),
        Some("st4") => Aarch64Mnemonic::Other("st4"),
        Some(_) => Aarch64Mnemonic::parse(decoded.mnemonic),
        None => Aarch64Mnemonic::parse(decoded.mnemonic),
    }
}

fn assert_fixture_covers_exact_cases(
    fixture_text: &str,
    mnemonics: &[&str],
    operand_count: usize,
    expected_cases: &[&str],
) {
    let actual = parse_otool_fixture(fixture_text)
        .into_iter()
        .filter(|instruction| mnemonics.contains(&instruction.mnemonic.as_str()))
        .filter(|instruction| count_operands(&instruction.operands) == operand_count)
        .map(|instruction| format_instruction_case(&instruction))
        .collect::<BTreeSet<_>>();
    let expected = expected_cases
        .iter()
        .map(|case| case.to_string())
        .collect::<BTreeSet<_>>();

    assert_eq!(actual, expected);
}

fn assert_encoded_instruction_matches_fixture(
    fixture: &[OtoolFixtureInsn],
    expected_case: &str,
    instruction: &aarch64::InstructionTemplate,
) {
    let expected = fixture
        .iter()
        .find(|entry| format_instruction_case(entry) == expected_case)
        .unwrap_or_else(|| panic!("missing fixture case: {expected_case}"));
    let encoded = aarch64::encode_instruction(instruction)
        .unwrap_or_else(|err| panic!("failed to encode {expected_case}: {err:?}"));

    assert_eq!(
        encoded, expected.word,
        "encoded word mismatch for {expected_case}"
    );

    let decoded =
        aarch64::decode_instruction(expected.address, encoded).expect("encoded word decodes");
    assert_eq!(decoded.format_mnemonic(), expected.mnemonic);
    assert_eq!(decoded.format_operands(), expected.operands);
}

fn format_instruction_case(instruction: &OtoolFixtureInsn) -> String {
    if instruction.operands.is_empty() {
        instruction.mnemonic.clone()
    } else {
        format!("{} {}", instruction.mnemonic, instruction.operands)
    }
}

fn count_operands(operands: &str) -> usize {
    if operands.is_empty() {
        0
    } else {
        operands.split(", ").count()
    }
}

#[test]
fn operand_kind_coverage_snapshot() {
    let fixture = parse_otool_fixture(BASIC_OTOOL_FIXTURE)
        .into_iter()
        .chain(parse_otool_fixture(INTEGER_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(BRANCH_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(LOADSTORE_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(FLOAT_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(FPIMM_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(CONVERT_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(EXCEPTION_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(DATAPROC_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(EXTEND_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(FPPAIR_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(ADRP_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SYSTEM_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SYSREG_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SYS_ALIAS_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SYS_GENERIC_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(PRFM_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SIMD_SAME_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SIMD_SCALAR_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(PAIRREG_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(VECTOR_D1_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SIMD_LIST_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SIMD_LDST_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SHLL_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(SIMD_REMAINING_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(WHOLE_FUNCTIONS_OTOOL_FIXTURE))
        .chain(parse_otool_fixture(FORMATTING_OTOOL_FIXTURE))
        .collect::<Vec<_>>();
    let fixture_words = fixture
        .iter()
        .map(|instruction| instruction.word)
        .chain([0xd503223f])
        .collect::<Vec<_>>();
    let coverage = aarch64::operand_kind_coverage(&fixture_words, DIRECT_TESTED_OPERAND_KINDS);

    assert!(coverage.table_operand_kinds.contains(&"Aimm"));
    assert!(coverage.table_operand_kinds.contains(&"ImmMov"));
    assert!(coverage.implemented.contains(&"Aimm"));
    assert!(coverage.implemented.contains(&"ImmMov"));
    assert_eq!(
        coverage.direct_tested_but_unimplemented,
        Vec::<&'static str>::new()
    );
    assert!(coverage.fixture_covered.contains(&"Aimm"));
    assert!(coverage.fixture_covered.contains(&"ImmMov"));
    assert!(coverage.fixture_covered.contains(&"AddrPcrel26"));
    assert!(coverage.fixture_covered.contains(&"AddrAdrp"));
    assert!(coverage.fixture_covered.contains(&"AddrPcrel14"));
    assert!(coverage.fixture_covered.contains(&"Cond"));
    assert!(coverage.fixture_covered.contains(&"Cond1"));
    assert!(coverage.fixture_covered.contains(&"BitNum"));
    assert!(coverage.fixture_covered.contains(&"AddrUimm12"));
    assert!(coverage.fixture_covered.contains(&"AddrSimm9"));
    assert!(coverage.fixture_covered.contains(&"AddrRegoff"));
    assert!(coverage.fixture_covered.contains(&"AddrSimple"));
    assert!(coverage.fixture_covered.contains(&"Rs"));
    assert!(coverage.fixture_covered.contains(&"Fd"));
    assert!(coverage.fixture_covered.contains(&"Fn"));
    assert!(coverage.fixture_covered.contains(&"Fm"));
    assert!(coverage.fixture_covered.contains(&"Fa"));
    assert!(coverage.fixture_covered.contains(&"Ft"));
    assert!(coverage.fixture_covered.contains(&"Ft2"));
    assert!(coverage.fixture_covered.contains(&"Fpimm0"));
    assert!(coverage.fixture_covered.contains(&"Fpimm"));
    assert!(coverage.fixture_covered.contains(&"Fbits"));
    assert!(coverage.fixture_covered.contains(&"Exc"));
    assert!(coverage.fixture_covered.contains(&"Ra"));
    assert!(coverage.fixture_covered.contains(&"RmExt"));
    assert!(coverage.fixture_covered.contains(&"Barrier"));
    assert!(coverage.fixture_covered.contains(&"BarrierIsb"));
    assert!(coverage.fixture_covered.contains(&"BarrierPsb"));
    assert!(coverage.fixture_covered.contains(&"Pstatefield"));
    assert!(coverage.fixture_covered.contains(&"Sysreg"));
    assert!(coverage.fixture_covered.contains(&"SysregAt"));
    assert!(coverage.fixture_covered.contains(&"SysregDc"));
    assert!(coverage.fixture_covered.contains(&"SysregIc"));
    assert!(coverage.fixture_covered.contains(&"SysregTlbi"));
    assert!(coverage.fixture_covered.contains(&"RtSys"));
    assert!(coverage.fixture_covered.contains(&"Uimm3Op1"));
    assert!(coverage.fixture_covered.contains(&"Uimm3Op2"));
    assert!(coverage.fixture_covered.contains(&"Cn"));
    assert!(coverage.fixture_covered.contains(&"Cm"));
    assert!(coverage.fixture_covered.contains(&"Prfop"));
    assert!(coverage.fixture_covered.contains(&"Uimm4"));
    assert!(coverage.fixture_covered.contains(&"Uimm7"));
    assert!(coverage.fixture_covered.contains(&"Vd"));
    assert!(coverage.fixture_covered.contains(&"Vn"));
    assert!(coverage.fixture_covered.contains(&"Vm"));
    assert!(coverage.fixture_covered.contains(&"Sd"));
    assert!(coverage.fixture_covered.contains(&"Sn"));
    assert!(coverage.fixture_covered.contains(&"Sm"));
    assert!(coverage.fixture_covered.contains(&"Imm0"));
    assert!(coverage.fixture_covered.contains(&"VdD1"));
    assert!(coverage.fixture_covered.contains(&"VnD1"));
    assert!(coverage.fixture_covered.contains(&"Lvn"));
    assert!(coverage.fixture_covered.contains(&"Lvt"));
    assert!(coverage.fixture_covered.contains(&"LvtAl"));
    assert!(coverage.fixture_covered.contains(&"SimdAddrSimple"));
    assert!(coverage.fixture_covered.contains(&"SimdAddrPost"));
    assert!(coverage.fixture_covered.contains(&"ShllImm"));
    assert!(coverage.fixture_covered.contains(&"Pairreg"));
    assert!(coverage.fixture_covered.contains(&"SimdImmSft"));
    assert!(coverage.fixture_covered.contains(&"SimdImm"));
    assert!(coverage.fixture_covered.contains(&"SimdFpimm"));
    assert!(coverage.fixture_covered.contains(&"Idx"));
    assert!(coverage.fixture_covered.contains(&"Ed"));
    assert!(coverage.fixture_covered.contains(&"En"));
    assert!(coverage.fixture_covered.contains(&"Em"));
    assert!(coverage.fixture_covered.contains(&"ImmVlsl"));
    assert!(coverage.fixture_covered.contains(&"ImmVlsr"));
    assert!(coverage.fixture_covered.contains(&"Let"));
    assert!(coverage.fixture_covered.contains(&"Imms"));
    assert_eq!(
        coverage.direct_tested,
        vec![
            "AddrAdrp",
            "Barrier",
            "BarrierIsb",
            "BarrierPsb",
            "Cm",
            "Cn",
            "Ed",
            "Em",
            "En",
            "Ft2",
            "Idx",
            "Imm0",
            "ImmVlsl",
            "ImmVlsr",
            "Immr",
            "Imms",
            "Let",
            "Lvn",
            "Lvt",
            "LvtAl",
            "Pairreg",
            "Prfop",
            "Pstatefield",
            "RmExt",
            "RtSys",
            "Sd",
            "ShllImm",
            "SimdAddrPost",
            "SimdAddrSimple",
            "SimdFpimm",
            "SimdImm",
            "SimdImmSft",
            "Sm",
            "Sn",
            "Sysreg",
            "SysregAt",
            "SysregDc",
            "SysregIc",
            "SysregTlbi",
            "Uimm3Op1",
            "Uimm3Op2",
            "Uimm4",
            "Uimm7",
            "Vd",
            "VdD1",
            "Vm",
            "Vn",
            "VnD1"
        ]
    );
    assert!(
        coverage.implemented_but_uncovered.is_empty(),
        "implemented operand kinds should have fixture coverage: {:?}",
        coverage.implemented_but_uncovered
    );
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"RmExt"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Ft2"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"AddrAdrp"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Barrier"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"BarrierIsb"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"BarrierPsb"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Pstatefield"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Sysreg"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SysregAt"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SysregDc"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SysregIc"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SysregTlbi"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"RtSys"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Uimm3Op1"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Uimm3Op2"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Cn"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Cm"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Prfop"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Vd"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Vn"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Vm"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"VdD1"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"VnD1"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Sd"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Sn"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Sm"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Imm0"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Lvn"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Lvt"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"LvtAl"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SimdAddrSimple"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SimdAddrPost"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"ShllImm"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Pairreg"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Uimm4"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"Uimm7"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Ed"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"En"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Em"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Idx"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"ImmVlsl"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"ImmVlsr"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Immr"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SimdImm"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SimdImmSft"));
    assert!(!coverage
        .implemented_but_not_direct_tested
        .contains(&"SimdFpimm"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Let"));
    assert!(!coverage.implemented_but_not_direct_tested.contains(&"Imms"));
    assert_eq!(
        coverage.implemented_but_not_direct_tested.len(),
        coverage.implemented.len() - coverage.direct_tested.len(),
        "all direct operand-specific tests should be removed from the not-direct-tested list"
    );
    assert_eq!(coverage.unimplemented, Vec::<&'static str>::new());
    assert_eq!(coverage.table_operand_kinds.len(), 87);
    assert_eq!(coverage.implemented.len(), 87);
    assert_eq!(coverage.unimplemented.len(), 0);
}
