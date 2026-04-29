use crate::isa::aarch64;
use crate::isa::aarch64::Instruction;

const BASIC_OTOOL_FIXTURE: &str = include_str!("../tests/fixtures/aarch64/basic.otool.txt");

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
    let fixture = parse_otool_fixture(BASIC_OTOOL_FIXTURE);

    assert!(!fixture.is_empty());

    for expected in fixture {
        let decoded =
            aarch64::disassemble_at_with_symbols(expected.address, expected.word, |target| {
                match target {
                    0x24 => Some("_callee".to_string()),
                    _ => None,
                }
            })
            .unwrap_or_else(|| panic!("no opcode match at {:#x}", expected.address));

        assert_eq!(
            decoded.mnemonic, expected.mnemonic,
            "mnemonic mismatch at {:#x} for word {:#010x}",
            expected.address, expected.word
        );
        let operands = decoded.format_operands_with_symbols(|target| match target {
            0x24 => Some("_callee".to_string()),
            _ => None,
        });

        assert_eq!(
            operands, expected.operands,
            "operand mismatch at {:#x} for word {:#010x}",
            expected.address, expected.word
        );
    }
}

#[test]
fn unsupported_operands_are_visible_placeholders() {
    let decoded = aarch64::disassemble_at(0, 0x52800401).expect("mov immediate should match");

    assert_eq!(decoded.mnemonic, "mov");
    assert!(
        decoded.format_operands().contains("<unimplemented:ImmMov>"),
        "unsupported operand should be visible in formatted output"
    );
}

#[test]
fn operand_kind_coverage_snapshot() {
    let fixture = parse_otool_fixture(BASIC_OTOOL_FIXTURE);
    let fixture_words = fixture
        .iter()
        .map(|instruction| instruction.word)
        .collect::<Vec<_>>();
    let coverage = aarch64::operand_kind_coverage(&fixture_words);

    assert!(coverage.table_operand_kinds.contains(&"Aimm"));
    assert!(coverage.table_operand_kinds.contains(&"ImmMov"));
    assert!(coverage.implemented.contains(&"Aimm"));
    assert!(coverage.fixture_covered.contains(&"Aimm"));
    assert!(coverage.fixture_covered.contains(&"AddrPcrel26"));
    assert!(coverage.unimplemented.contains(&"ImmMov"));
    assert!(
        coverage.implemented_but_uncovered.is_empty(),
        "implemented operand kinds should have fixture coverage: {:?}",
        coverage.implemented_but_uncovered
    );
}
