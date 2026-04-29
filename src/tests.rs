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
        &["b 0x34", "bl 0x34"],
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
fn unsupported_operands_are_visible_placeholders() {
    let decoded = aarch64::decode_instruction(0, 0xd53b4200).expect("mrs should match");

    assert_eq!(decoded.mnemonic, "mrs");
    assert!(
        decoded.format_operands().contains("<unimplemented:Sysreg>"),
        "unsupported operand should be visible in formatted output"
    );
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
        .collect::<Vec<_>>();
    let fixture_words = fixture
        .iter()
        .map(|instruction| instruction.word)
        .collect::<Vec<_>>();
    let coverage = aarch64::operand_kind_coverage(&fixture_words);

    assert!(coverage.table_operand_kinds.contains(&"Aimm"));
    assert!(coverage.table_operand_kinds.contains(&"ImmMov"));
    assert!(coverage.implemented.contains(&"Aimm"));
    assert!(coverage.implemented.contains(&"ImmMov"));
    assert!(coverage.fixture_covered.contains(&"Aimm"));
    assert!(coverage.fixture_covered.contains(&"ImmMov"));
    assert!(coverage.fixture_covered.contains(&"AddrPcrel26"));
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
    assert!(coverage.fixture_covered.contains(&"Fpimm0"));
    assert!(coverage.fixture_covered.contains(&"Fpimm"));
    assert!(coverage.fixture_covered.contains(&"Fbits"));
    assert!(coverage.fixture_covered.contains(&"Exc"));
    assert!(coverage.fixture_covered.contains(&"Ra"));
    assert!(coverage.unimplemented.contains(&"Sysreg"));
    assert!(
        coverage.implemented_but_uncovered.is_empty(),
        "implemented operand kinds should have fixture coverage: {:?}",
        coverage.implemented_but_uncovered
    );
}
