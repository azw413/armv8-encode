//! Regression tests for the bitfield-family alias decoder
//! (UBFM / SBFM / BFM and their aliases). Covers three previously broken
//! cases — sbfx width, insert-alias lsb wraparound, and sbfx/sbfiz alias
//! disambiguation — plus the documented exact-match precedence cases.

use crate::isa::aarch64::{decode_instruction, DecodedOperand};

fn imm(op: &DecodedOperand) -> i64 {
    match op {
        DecodedOperand::Immediate(value) => *value,
        other => panic!("expected immediate, got {other:?}"),
    }
}

#[test]
fn sbfx_reports_width_not_imms_plus_one() {
    // sbfx x0, x1, #8, #4 → SBFM immr=8, imms=11.
    let word = 0x9340_0000u32 | (8 << 16) | (11 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode sbfx");

    assert_eq!(decoded.format_mnemonic(), "sbfx");
    assert_eq!(imm(&decoded.operands[2]), 8, "lsb");
    assert_eq!(imm(&decoded.operands[3]), 4, "width");
}

#[test]
fn ubfx_reports_width_correctly() {
    // ubfx w0, w1, #4, #8 → UBFM immr=4, imms=11.
    let word = 0x5300_0000u32 | (4 << 16) | (11 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode ubfx");

    assert_eq!(decoded.format_mnemonic(), "ubfx");
    assert_eq!(imm(&decoded.operands[2]), 4, "lsb");
    assert_eq!(imm(&decoded.operands[3]), 8, "width");
}

#[test]
fn bfi_unwraps_immr_to_lsb() {
    // bfi x0, x1, #8, #4 → BFM immr = (64-8) mod 64 = 56, imms = 4-1 = 3.
    let word = 0xB340_0000u32 | (56 << 16) | (3 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode bfi");

    assert_eq!(decoded.format_mnemonic(), "bfi");
    assert_eq!(imm(&decoded.operands[2]), 8, "lsb");
    assert_eq!(imm(&decoded.operands[3]), 4, "width");
}

#[test]
fn ubfiz_unwraps_immr_to_lsb() {
    // ubfiz w0, w1, #4, #8 → UBFM 32-bit immr = (32-4) mod 32 = 28, imms = 7.
    let word = 0x5300_0000u32 | (28 << 16) | (7 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode ubfiz");

    assert_eq!(decoded.format_mnemonic(), "ubfiz");
    assert_eq!(imm(&decoded.operands[2]), 4, "lsb");
    assert_eq!(imm(&decoded.operands[3]), 8, "width");
}

#[test]
fn sbfiz_is_distinguished_from_sbfx() {
    // sbfiz x0, x1, #16, #8 → SBFM immr = (64-16) mod 64 = 48, imms = 7.
    // imms (7) < immr (48), so this must select sbfiz, not sbfx.
    let word = 0x9340_0000u32 | (48 << 16) | (7 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode sbfiz");

    assert_eq!(decoded.format_mnemonic(), "sbfiz");
    assert_eq!(imm(&decoded.operands[2]), 16, "lsb");
    assert_eq!(imm(&decoded.operands[3]), 8, "width");
}

#[test]
fn bfxil_lsb_reported_as_immr() {
    // bfxil x0, x1, #8, #4 → BFM immr=8, imms=8+4-1=11.
    // imms (11) >= immr (8), so this is bfxil.
    let word = 0xB340_0000u32 | (8 << 16) | (11 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode bfxil");

    assert_eq!(decoded.format_mnemonic(), "bfxil");
    assert_eq!(imm(&decoded.operands[2]), 8, "lsb");
    assert_eq!(imm(&decoded.operands[3]), 4, "width");
}

// --- Documented "non-bugs": exact-match aliases win over generic siblings. ---

#[test]
fn bfm_lsb_zero_surfaces_as_bfxil() {
    // bfi x0, x1, #0, #4 has immr=0, imms=3 — imms >= immr, so the spec
    // picks bfxil. Both are semantically equivalent at lsb=0.
    let word = 0xB340_0000u32 | (0 << 16) | (3 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode bfxil");

    assert_eq!(decoded.format_mnemonic(), "bfxil");
}

#[test]
fn sbfm_lsb_zero_width_eight_surfaces_as_sxtb() {
    // sbfiz x0, x1, #0, #8 encodes to SBFM immr=0, imms=7 — which is exactly
    // the sxtb pattern. The exact-match alias should win.
    let word = 0x9340_0000u32 | (0 << 16) | (7 << 10) | (1 << 5);
    let decoded = decode_instruction(0, word).expect("decode sxtb");

    assert_eq!(decoded.format_mnemonic(), "sxtb");
}
