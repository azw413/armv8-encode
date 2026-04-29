use crate::isa::aarch64;
use crate::isa::aarch64::Instruction;

const BASIC_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/basic.otool.txt");
const INTEGER_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/integer.otool.txt");
const BRANCH_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/branch.otool.txt");
const LOADSTORE_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/loadstore.otool.txt");
const FLOAT_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/float.otool.txt");

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
fn round_trips_placeholder_aarch64_instruction() {
    let instruction = Instruction::ADC {
        sf: false,
        rm: 4,
        rn: 2,
        rd: 1,
    };

    let encoded = aarch64::encode_word(&instruction).expect("instruction should encode");
    let decoded = aarch64::decode_word(encoded);

    assert_eq!(decoded, instruction);
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
fn decoded_operands_match_otool_fixture() {
    assert_decoded_fixture_matches_otool(BASIC_OTOOL_FIXTURE, |target| match target {
        0x24 => Some("_callee".to_string()),
        _ => None,
    });
    assert_decoded_fixture_matches_otool(INTEGER_OTOOL_FIXTURE, |_| None);
    assert_decoded_fixture_matches_otool(BRANCH_OTOOL_FIXTURE, |_| None);
    assert_decoded_fixture_matches_otool(LOADSTORE_OTOOL_FIXTURE, |_| None);
    assert_decoded_fixture_matches_otool(FLOAT_OTOOL_FIXTURE, |_| None);
}

fn assert_decoded_fixture_matches_otool<F>(fixture_text: &str, symbol_for_address: F)
where
    F: Fn(u64) -> Option<String> + Copy,
{
    let fixture = parse_otool_fixture(fixture_text);

    assert!(!fixture.is_empty());

    for expected in fixture {
        let decoded = aarch64::disassemble_at_with_symbols(
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

#[test]
fn unsupported_operands_are_visible_placeholders() {
    let decoded = aarch64::disassemble_at(0, 0xd53b4200).expect("mrs should match");

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
    assert!(coverage.unimplemented.contains(&"Sysreg"));
    assert!(
        coverage.implemented_but_uncovered.is_empty(),
        "implemented operand kinds should have fixture coverage: {:?}",
        coverage.implemented_but_uncovered
    );
}
