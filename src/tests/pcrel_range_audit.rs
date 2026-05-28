//! Audit that every Thumb / ARM-mode mnemonic whose format
//! string carries a PC-relative branch operand (`%b` / `%B`)
//! has a [`Isa::pcrel_range_bytes`] entry. Without one, the
//! layout pass can't determine whether a redirected branch
//! fits, and either over-tolerates or panics at emit time.
//!
//! This is the Tier-4 analogue of the bitfield corpus test:
//! reflect over the static opcode tables and assert ISA
//! coverage rather than rely on hand-curated fixtures.

use crate::isa::armv7::arm::iter_opcodes as arm_iter_opcodes;
use crate::isa::armv7::iter_opcodes as thumb_iter_opcodes;
use crate::isa::armv7::arm::ArmIsa;
use crate::isa::armv7::ThumbIsa;
use crate::isa::Isa;

/// Return true if `format` contains a `%b` or `%B` operand
/// code (optionally preceded by a bitfield like `%22-25B`).
fn has_branch_target_code(format: &str) -> bool {
    let bytes = format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        // Skip optional bitfield digits + '-'.
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
            i += 1;
        }
        if i < bytes.len() && (bytes[i] == b'b' || bytes[i] == b'B') {
            return true;
        }
        i += 1;
    }
    false
}

#[test]
fn has_branch_target_code_detects_basic_branch_formats() {
    // Smoke for the helper itself.
    assert!(has_branch_target_code("b%24'l%c\t%b"));
    assert!(has_branch_target_code("b%c.w\t%B%x"));
    assert!(has_branch_target_code("b%c.n\t%0-7B%X"));
    assert!(!has_branch_target_code("mov%c\t%12-15r, %0-3r"));
    assert!(!has_branch_target_code("ret"));
}

#[test]
fn every_thumb_mnemonic_with_branch_operand_has_pcrel_range() {
    use std::collections::BTreeSet;
    let mut missing: BTreeSet<&'static str> = BTreeSet::new();
    for row in thumb_iter_opcodes() {
        if !has_branch_target_code(row.format) {
            continue;
        }
        if <ThumbIsa as Isa>::pcrel_range_bytes(row.mnemonic).is_none() {
            missing.insert(row.mnemonic.as_str());
        }
    }
    assert!(
        missing.is_empty(),
        "Thumb mnemonics with branch-target operand but no pcrel_range_bytes entry: {missing:?}"
    );
}

#[test]
fn every_arm_mnemonic_with_branch_operand_has_pcrel_range() {
    use std::collections::BTreeSet;
    let mut missing: BTreeSet<&'static str> = BTreeSet::new();
    for row in arm_iter_opcodes() {
        if !has_branch_target_code(row.format) {
            continue;
        }
        if <ArmIsa as Isa>::pcrel_range_bytes(row.mnemonic).is_none() {
            missing.insert(row.mnemonic.as_str());
        }
    }
    assert!(
        missing.is_empty(),
        "ARM-mode mnemonics with branch-target operand but no pcrel_range_bytes entry: {missing:?}"
    );
}

#[test]
fn thumb_pcrel_ranges_are_within_thumb2_limits() {
    // Sanity: no Thumb mnemonic claims a range larger than the
    // ±16 MiB max reach of B.W (T4). If a future change adds
    // an entry over that, it's almost certainly wrong.
    const T4_MAX: i64 = 16 * 1024 * 1024;
    for row in thumb_iter_opcodes() {
        if let Some(range) = <ThumbIsa as Isa>::pcrel_range_bytes(row.mnemonic) {
            assert!(
                range <= T4_MAX,
                "Thumb mnemonic {} claims range {range} bytes > {T4_MAX} (T4 B.W max)",
                row.mnemonic.as_str()
            );
            assert!(range > 0, "Thumb mnemonic {} has non-positive range", row.mnemonic.as_str());
        }
    }
}

#[test]
fn thumb_operand_bit_ranges_stay_within_word() {
    for row in thumb_iter_opcodes() {
        for (slot, ranges) in row.operand_bit_ranges().iter().enumerate() {
            for range in ranges {
                assert!(
                    range.start < range.end && range.end <= 32,
                    "Thumb {} slot {slot} has out-of-range bits {range:?} (format: {:?})",
                    row.mnemonic.as_str(),
                    row.format,
                );
            }
        }
    }
}

#[test]
fn arm_operand_bit_ranges_stay_within_word() {
    for row in arm_iter_opcodes() {
        for (slot, ranges) in row.operand_bit_ranges().iter().enumerate() {
            for range in ranges {
                assert!(
                    range.start < range.end && range.end <= 32,
                    "ARM {} slot {slot} has out-of-range bits {range:?} (format: {:?})",
                    row.mnemonic.as_str(),
                    row.format,
                );
            }
        }
    }
}

#[test]
fn arm_pcrel_ranges_are_within_arm_limits() {
    // ARM-mode B reach is ±32 MiB (24-bit signed × 4).
    const ARM_MAX: i64 = 32 * 1024 * 1024;
    for row in arm_iter_opcodes() {
        if let Some(range) = <ArmIsa as Isa>::pcrel_range_bytes(row.mnemonic) {
            assert!(
                range <= ARM_MAX,
                "ARM mnemonic {} claims range {range} bytes > {ARM_MAX} (B max)",
                row.mnemonic.as_str()
            );
            assert!(range > 0, "ARM mnemonic {} has non-positive range", row.mnemonic.as_str());
        }
    }
}
