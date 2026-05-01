//! Decoder tests: matching opcode rows and asserting decoded mnemonics and
//! operands match `otool` output.

use super::common::*;
use crate::isa::aarch64;

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
