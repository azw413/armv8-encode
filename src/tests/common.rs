//! Shared fixtures, builders, and assertions for the AArch64 unit tests.

use crate::isa::aarch64;
use crate::isa::aarch64::{Aarch64Mnemonic, DecodedOperand, Register, RegisterClass};
use std::collections::BTreeSet;

pub(super) const BASIC_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/basic.otool.txt");
pub(super) const INTEGER_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/integer.otool.txt");
pub(super) const BRANCH_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/branch.otool.txt");
pub(super) const LOADSTORE_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/loadstore.otool.txt");
pub(super) const FLOAT_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/float.otool.txt");
pub(super) const FPIMM_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/fpimm.otool.txt");
pub(super) const CONVERT_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/convert.otool.txt");
pub(super) const EXCEPTION_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/exception.otool.txt");
pub(super) const DATAPROC_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/dataproc.otool.txt");
pub(super) const EXTEND_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/extend.otool.txt");
pub(super) const FPPAIR_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/fppair.otool.txt");
pub(super) const ADRP_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/adrp.otool.txt");
pub(super) const SYSTEM_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/system.otool.txt");
pub(super) const SYSREG_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/sysreg.otool.txt");
pub(super) const SYS_ALIAS_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/sys_alias.otool.txt");
pub(super) const SYS_GENERIC_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/sys_generic.otool.txt");
pub(super) const PRFM_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/prfm.otool.txt");
pub(super) const SIMD_SAME_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/simd_same.otool.txt");
pub(super) const SIMD_SCALAR_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/simd_scalar.otool.txt");
pub(super) const PAIRREG_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/pairreg.otool.txt");
pub(super) const VECTOR_D1_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/vector_d1.otool.txt");
pub(super) const SIMD_LIST_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/simd_list.otool.txt");
pub(super) const SIMD_LDST_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/simd_ldst.otool.txt");
pub(super) const SHLL_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/shll.otool.txt");
pub(super) const SIMD_REMAINING_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/simd_remaining.otool.txt");
pub(super) const WHOLE_FUNCTIONS_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/whole_functions.otool.txt");
pub(super) const FORMATTING_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/formatting.otool.txt");
pub(super) const ENCODE_BASIC_OTOOL_FIXTURE: &str =
    include_str!("../../tests/fixtures/aarch64/encode_basic.otool.txt");

pub(super) const DIRECT_TESTED_OPERAND_KINDS: &[&str] = &[
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
pub(super) struct OtoolFixtureInsn {
    pub address: u64,
    pub word: u32,
    pub mnemonic: String,
    pub operands: String,
}

pub(super) fn parse_otool_fixture(text: &str) -> Vec<OtoolFixtureInsn> {
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

pub(super) fn template(
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

pub(super) fn w_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::W,
        index,
    }
}

pub(super) fn x_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::X,
        index,
    }
}

pub(super) fn s_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::S,
        index,
    }
}

pub(super) fn d_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::D,
        index,
    }
}

pub(super) fn v_reg(index: u8, arrangement: aarch64::VectorArrangement) -> aarch64::VectorRegister {
    aarch64::VectorRegister { index, arrangement }
}

pub(super) fn v_element(
    index: u8,
    element: u8,
    size: aarch64::VectorElementSize,
) -> aarch64::VectorElement {
    aarch64::VectorElement {
        index,
        element,
        size,
    }
}

pub(super) fn v_list(
    first: u8,
    count: u8,
    arrangement: aarch64::VectorArrangement,
) -> aarch64::VectorList {
    aarch64::VectorList {
        first,
        count,
        arrangement,
        element: None,
    }
}

pub(super) fn sp_reg() -> Register {
    Register {
        class: RegisterClass::XOrSp,
        index: 31,
    }
}

pub(super) fn mem_imm(
    base: Register,
    offset: i64,
    mode: aarch64::AddressingMode,
) -> DecodedOperand {
    DecodedOperand::Memory(aarch64::MemoryOperand {
        base,
        offset: aarch64::MemoryOffset::Immediate(offset),
        mode,
    })
}

pub(super) fn mem_simple(base: Register) -> DecodedOperand {
    DecodedOperand::Memory(aarch64::MemoryOperand {
        base,
        offset: aarch64::MemoryOffset::None,
        mode: aarch64::AddressingMode::Offset,
    })
}

pub(super) fn mem_reg(
    base: Register,
    register: Register,
    shift: Option<aarch64::Shift>,
) -> DecodedOperand {
    DecodedOperand::Memory(aarch64::MemoryOperand {
        base,
        offset: aarch64::MemoryOffset::Register { register, shift },
        mode: aarch64::AddressingMode::Offset,
    })
}

pub(super) fn ext_reg(
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

pub(super) fn shift_reg(
    register: Register,
    kind: aarch64::ShiftKind,
    amount: u8,
) -> aarch64::ShiftedRegister {
    aarch64::ShiftedRegister {
        register,
        shift: aarch64::Shift { kind, amount },
    }
}

pub(super) fn assert_decoded_fixture_matches_otool<F>(fixture_text: &str, symbol_for_address: F)
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
        .unwrap_or_else(|err| panic!("decode failed at {:#x}: {err:?}", expected.address));

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

pub(super) fn all_otool_fixtures() -> &'static [(&'static str, &'static str)] {
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

pub(super) fn assert_decode_encode_roundtrip_fixture(fixture_name: &str, fixture_text: &str) {
    let fixture = parse_otool_fixture(fixture_text);

    assert!(!fixture.is_empty(), "empty fixture: {fixture_name}");

    for expected in fixture {
        let decoded = aarch64::decode_instruction(expected.address, expected.word)
            .unwrap_or_else(|err| {
                panic!(
                    "decode failed while roundtripping {fixture_name} at {:#x}: {err:?}",
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
    let _ = decoded.format_mnemonic();
    decoded.mnemonic
}

pub(super) fn assert_fixture_covers_exact_cases(
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

pub(super) fn assert_encoded_instruction_matches_fixture(
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
