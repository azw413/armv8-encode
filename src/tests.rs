use crate::isa::aarch64;
use crate::isa::aarch64::EncodeError;
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
fn encode_instruction_is_explicitly_unimplemented() {
    let instruction = aarch64::InstructionTemplate {
        mnemonic: "adc",
        operands: Vec::new(),
    };

    assert_eq!(
        aarch64::encode_instruction(&instruction),
        Err(EncodeError::Unimplemented {
            kind: "instruction"
        })
    );
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
