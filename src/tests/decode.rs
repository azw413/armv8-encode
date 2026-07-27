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

/// AdvSIMD load/store **multiple structures** (LD2/3/4, ST2/3/4) and the RCpc /
/// RCpc2 acquire/release load/stores (LDAPR, LDAPUR/STLUR). These appear in real
/// `clang -O2` / Rust output (verified against `otool`) and previously failed to
/// decode. The words are taken verbatim from a real arm64 Mach-O `__text`.
#[test]
fn decodes_simd_struct_and_rcpc_loadstores() {
    // (word, expected mnemonic) — otool-confirmed.
    let cases: &[(u32, &str)] = &[
        // AdvSIMD multiple-structure interleaved loads/stores.
        (0x4c408da4, "ld2"),  // LD2 {v4.2d,v5.2d},[x13]
        (0x4c404d80, "ld3"),  // LD3 ...
        (0x4c008dc0, "st2"),  // ST2 ... (store, L=0)
        (0x4c9f8dd2, "st2"),  // ST2 ... post-index
        (0x0cdf8166, "ld2"),  // LD2 Q=0, post-index
        // RCpc: LDAPR (load-acquire), base-register only.
        (0xf8bfc108, "ldapr"),  // LDAPR x8,[x8]
        (0xb8bfc000, "ldapr"),  // LDAPR w0,[x0]
        // RCpc2: unscaled acquire/release load/store.
        (0x19430108, "ldapurb"),
        (0xd9408129, "ldapur"), // 64-bit
        (0x19020113, "stlurb"),
    ];
    for &(word, mnemonic) in cases {
        let decoded = aarch64::decode_instruction(0x1000, word)
            .unwrap_or_else(|e| panic!("{word:#010x} ({mnemonic}) failed to decode: {e:?}"));
        assert_eq!(
            decoded.mnemonic.as_str(),
            mnemonic,
            "wrong mnemonic for {word:#010x}",
        );
    }
}

/// Pointer Authentication (FEAT_PAuth) instructions — the hint-space prologue
/// forms, RET-with-auth, the data-proc sign/auth forms (incl. zero-modifier),
/// and the authenticated branch forms. Words are taken verbatim from real
/// arm64e `__text` (`/bin/ls`, `ssh`) and otool-confirmed. Round-trips through
/// encode (the `braa`/`blraa` modifier lives in bits[4:0] via `RmLow`).
#[test]
fn decodes_and_reencodes_pointer_auth() {
    use crate::isa::aarch64::InstructionTemplate;
    let cases: &[(u32, &str)] = &[
        (0xd503233f, "paciasp"),
        (0xd503237f, "pacibsp"),
        (0xd50323ff, "autibsp"),
        (0xd65f0bff, "retaa"),
        (0xd65f0fff, "retab"),
        (0xdac123f0, "paciza"),       // paciza x16 (zero-modifier)
        (0xdac143e0, "xpaci"),        // xpaci x0
        (0xdac101b0, "pacia"),        // pacia x16,x13 (two-register)
        (0xdac10a30, "pacda"),        // pacda x16,x17
        (0xd63f091f, "blraaz"),       // blraaz x8
        (0xd73f0a11, "blraa"),        // blraa x16,x17 (modifier in bits[4:0])
    ];
    for &(word, mnemonic) in cases {
        let decoded = aarch64::decode_instruction(0x1000, word)
            .unwrap_or_else(|e| panic!("{word:#010x} ({mnemonic}) failed to decode: {e:?}"));
        assert_eq!(decoded.mnemonic.as_str(), mnemonic, "mnemonic for {word:#010x}");
        // Symmetric re-encode reproduces the original word.
        let template = InstructionTemplate {
            address: 0x1000,
            mnemonic: decoded.mnemonic,
            operands: decoded.operands.clone(),
        };
        let reencoded = aarch64::encode_instruction(&template)
            .unwrap_or_else(|e| panic!("{word:#010x} ({mnemonic}) failed to re-encode: {e:?}"));
        assert_eq!(reencoded, word, "re-encode mismatch for {mnemonic} {word:#010x}");
    }
}

/// The structure form (ld1..ld4 / st1..st4) and register-list length are selected
/// by `opcode[15:12]`; check the full map decodes the right list count.
#[test]
fn simd_ldst_list_counts() {
    use crate::isa::aarch64::DecodedOperand;
    // (word, expected mnemonic, expected register-list count)
    let cases: &[(u32, &str, u8)] = &[
        (0x0c407000, "ld1", 1), // opcode 0x7
        (0x0c40a000, "ld1", 2), // opcode 0xa
        (0x0c406000, "ld1", 3), // opcode 0x6
        (0x0c402000, "ld1", 4), // opcode 0x2
        (0x0c408000, "ld2", 2), // opcode 0x8
        (0x0c404000, "ld3", 3), // opcode 0x4
        (0x0c400000, "ld4", 4), // opcode 0x0
    ];
    for &(word, mnemonic, count) in cases {
        let decoded = aarch64::decode_instruction(0x1000, word)
            .unwrap_or_else(|e| panic!("{word:#010x} failed to decode: {e:?}"));
        assert_eq!(decoded.mnemonic.as_str(), mnemonic, "mnemonic for {word:#010x}");
        let Some(DecodedOperand::VectorList(list)) = decoded.operands.first() else {
            panic!("{word:#010x}: first operand isn't a vector list");
        };
        assert_eq!(list.count, count, "list count for {word:#010x}");
    }
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

/// `csinv Xd, Xn, Xm, cond` with `Xm != Xn` must NOT be decoded as the two-source
/// `cinv Xd, Xn, cond` alias — that alias is only legal when `Xm == Xn`. Mis-
/// applying it silently drops the `Xm` operand, so any consumer doing register
/// dataflow (e.g. liveness) never sees `Xm` read. Regression for the gecko
/// `pack_instruction` miscompile: `csinv x12, x12, x13, hs` (bytes `8c218dda`)
/// had `x13` dropped, so a mutation pass clobbered it as "dead". Same class of
/// bug for `csneg`'s `cneg` alias.
#[test]
fn conditional_select_alias_requires_equal_source_registers() {
    // csinv x12, x12, x13, hs — distinct Rm, must stay csinv (keep x13).
    let csinv = aarch64::disassemble_bytes(0x1000, &[0x8c, 0x21, 0x8d, 0xda]).unwrap();
    assert_eq!(
        format!("{} {}", csinv[0].format_mnemonic(), csinv[0].format_operands()),
        "csinv x12, x12, x13, hs"
    );
    // csneg x5, x6, x7, gt — distinct Rm, must stay csneg (keep x7).
    let csneg = aarch64::disassemble_bytes(0x1000, &[0xc5, 0xc4, 0x87, 0xda]).unwrap();
    assert_eq!(
        format!("{} {}", csneg[0].format_mnemonic(), csneg[0].format_operands()),
        "csneg x5, x6, x7, gt"
    );

    // The aliases ARE correct when Rm == Rn: cinv x9, x9, hi / cneg x0, x1, eq.
    let cinv = aarch64::disassemble_bytes(0x1000, &[0x29, 0x91, 0x89, 0xda]).unwrap();
    assert_eq!(cinv[0].format_mnemonic(), "cinv");
    let cneg = aarch64::disassemble_bytes(0x1000, &[0x20, 0x14, 0x81, 0xda]).unwrap();
    assert_eq!(cneg[0].format_mnemonic(), "cneg");
}
