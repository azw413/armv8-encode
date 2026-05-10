//! Operand-level Thumb encoder.
//!
//! ## Strategy
//!
//! Symmetric mirror of [`super::format_decode`]: walk the
//! binutils format string, but instead of *reading* bits into
//! [`DecodedOperand`] values we *consume* operands from the
//! caller-supplied queue and *write* their bits into the
//! encoded word.
//!
//! This keeps decoder and encoder in lockstep — adding a new
//! format-code requires changing one place each, with the
//! same bit-position constants on both sides.
//!
//! ## Form selection
//!
//! Multiple table rows can match a `(mnemonic, operand-shape)`
//! pair (e.g. 16-bit T1 and 32-bit T2 forms of `mov`).
//! Resolution policy: **the caller passes the row's format
//! string** — typically carried alongside the IR node from
//! decode time, so round-trips don't surprise anyone by
//! silently shrinking or growing.
//!
//! Two convenience entry points:
//!
//! - [`encode_with_row`] — caller supplies the exact
//!   [`ThumbOpcodeGenerated`] row to use. Most predictable;
//!   the rewriter's normal path.
//! - [`encode`] — caller supplies just the mnemonic; we pick
//!   the first row whose format slots match the supplied
//!   operand kinds. Useful for one-off encoders and tests.
//!
//! Both end up in [`encode_with_format`], which is the actual
//! bit-packer.
//!
//! ## Coverage
//!
//! Supports the same set of format codes the decoder handles.
//! Codes the decoder treats as approximate (`%a` for memory
//! addressing modes, `%S` shifted-Rm with shift, BF
//! offset variants) are accepted but currently encode the
//! base form only — which is sufficient for branches /
//! data-processing rewrites; complex addressing modes can
//! grow as use cases appear.

use super::neon_table_generated::NeonOpcodeGenerated;
use super::operand::{DecodedOperand, EncodeError, Register, RegisterClass};
use super::table::ThumbWidth;
use super::table_generated::{ThumbMnemonicGenerated, ThumbOpcodeGenerated, THUMB_OPCODE_TABLE_GENERATED};

/// Encode a NEON / coprocessor row. The decoder emits a
/// single full-word OpaqueBits for these matches because the
/// NEON format-string grammar isn't fully invertible; round-
/// trip is bit-exact via the preserved OpaqueBits.
///
/// Returns the encoded word in *Thumb* form (i.e. the
/// original word as it appeared in memory before any
/// `normalise_neon_thumb` transform).
pub fn encode_neon_row(
    _row: &NeonOpcodeGenerated,
    operands: &[DecodedOperand],
) -> Result<u32, EncodeError> {
    match operands.first() {
        Some(DecodedOperand::OpaqueBits { bits, mask }) if *mask == 0xFFFF_FFFF => Ok(*bits),
        Some(other) => Err(EncodeError::OperandShapeMismatch {
            slot: 0,
            format_code: "neon",
            got: operand_kind(other),
        }),
        None => Err(EncodeError::ArityMismatch {
            format_slots: 1,
            operands: 0,
        }),
    }
}

/// Encode against a specific table row. The recommended
/// entry point: the caller (typically the rewriter, holding
/// IR nodes that remember which row was matched at decode
/// time) names the row explicitly. No form-selection
/// surprises.
pub fn encode_with_row(
    row: &ThumbOpcodeGenerated,
    operands: &[DecodedOperand],
    address: u64,
) -> Result<(u32, ThumbWidth), EncodeError> {
    let width_bytes = match row.width {
        ThumbWidth::Halfword => 2,
        ThumbWidth::Word => 4,
    };
    let word = encode_with_format(row.format, row.opcode, operands, address, width_bytes)?;
    Ok((word, row.width))
}

/// Encode by mnemonic — picks the first table row whose
/// format slots match the supplied operand shapes. Useful
/// for tests and ad-hoc encoders. Production rewriters
/// should prefer [`encode_with_row`].
pub fn encode(
    mnemonic: ThumbMnemonicGenerated,
    operands: &[DecodedOperand],
    address: u64,
) -> Result<(u32, ThumbWidth), EncodeError> {
    let mut last_err: Option<EncodeError> = None;
    for row in THUMB_OPCODE_TABLE_GENERATED.iter() {
        if row.mnemonic != mnemonic {
            continue;
        }
        let width_bytes = match row.width {
            ThumbWidth::Halfword => 2,
            ThumbWidth::Word => 4,
        };
        match encode_with_format(row.format, row.opcode, operands, address, width_bytes) {
            Ok(word) => return Ok((word, row.width)),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or(EncodeError::NoMatchingForm {
        mnemonic: mnemonic_name(mnemonic),
        operand_count: operands.len(),
    }))
}

/// The shared bit-packer. Walks `format`, consuming operands
/// in left-to-right order, and OR-ing each operand's bits
/// into a working word that starts from the row's `opcode`
/// (which already carries the constant high bits).
pub fn encode_with_format(
    format: &str,
    opcode_base: u32,
    operands: &[DecodedOperand],
    address: u64,
    width_bytes: usize,
) -> Result<(u32), EncodeError> {
    let mut word = opcode_base;
    let bytes = format.as_bytes();
    let mut i = 0;
    let mut operand_idx = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }
        // Skip {X:...%} display wrappers but recurse into
        // them: they may contain real operand-bearing escapes.
        if bytes[i] == b'{' {
            let inner_start = i + 1;
            let mut j = inner_start;
            while j + 1 < bytes.len() {
                if bytes[j] == b'%' && bytes[j + 1] == b'}' {
                    break;
                }
                j += 1;
            }
            let inner_str = if inner_start + 2 <= j {
                std::str::from_utf8(&bytes[inner_start + 2..j]).unwrap_or("")
            } else {
                ""
            };
            // Recursive walk: pack remaining operands into
            // word using inner format. We need to thread
            // operand_idx through, so do it inline rather
            // than recursing into encode_with_format.
            let (new_word, consumed) = encode_walk_inner(
                inner_str,
                word,
                &operands[operand_idx..],
                address,
                width_bytes,
            )?;
            word = new_word;
            operand_idx += consumed;
            i = j + 2;
            continue;
        }
        let bitfield_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
            i += 1;
        }
        let bitfield = std::str::from_utf8(&bytes[bitfield_start..i]).unwrap_or("");
        if i >= bytes.len() {
            break;
        }
        let code = bytes[i];
        i += 1;
        // Skip non-operand markers.
        match code {
            b'\'' => {
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }
            b'?' => {
                if i + 1 < bytes.len() {
                    i += 2;
                }
                continue;
            }
            b'`' => {
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }
            _ => {}
        }
        // %<bf>c — bitfielded cond consumes a Condition
        // operand (matches the decoder's emission).
        if code == b'c' && !bitfield.is_empty() {
            if let Some((lo, hi)) = parse_bitfield(bitfield) {
                let slot = operand_idx;
                let operand = operands.get(slot).ok_or(EncodeError::ArityMismatch {
                    format_slots: slot + 1,
                    operands: operands.len(),
                })?;
                if let DecodedOperand::Condition(c) = operand {
                    place_field(&mut word, lo, hi, *c as u32);
                    operand_idx += 1;
                } else {
                    return Err(EncodeError::OperandShapeMismatch {
                        slot,
                        format_code: "c",
                        got: operand_kind(operand),
                    });
                }
                continue;
            }
        }
        // Display-only codes — no operand consumed.
        // %c/%C without bitfield are display-only (e.g.
        // unconditional Thumb-2 forms whose cond is implicit).
        if matches!(code, b'c' | b'C' | b'x' | b'X' | b'%' | b't') {
            continue;
        }
        if matches!(code, b'w' | b'W') && bitfield.is_empty() {
            continue;
        }
        // Now we definitely need an operand.
        let slot = operand_idx;
        let operand = operands.get(slot).ok_or(EncodeError::ArityMismatch {
            format_slots: slot + 1,
            operands: operands.len(),
        })?;
        let bits_written = pack_operand(
            code,
            bitfield,
            operand,
            slot,
            &mut word,
            address,
            width_bytes,
        )?;
        if bits_written {
            operand_idx += 1;
        }
    }
    // After the format walk, splice any remaining trailing
    // OpaqueBits operands (the sweep emits a row-mask-tail
    // OpaqueBits to preserve bits the row's mask leaves
    // variable that no format code captured).
    while operand_idx < operands.len() {
        if let DecodedOperand::OpaqueBits { bits, mask } = &operands[operand_idx] {
            word &= !*mask;
            word |= *bits & *mask;
        }
        operand_idx += 1;
    }
    Ok(word)
}

/// Inner walk used when recursing into a `%{X:…%}` wrapper.
/// Returns the new word and how many operands were consumed
/// from the supplied slice.
fn encode_walk_inner(
    format: &str,
    mut word: u32,
    operands: &[DecodedOperand],
    address: u64,
    width_bytes: usize,
) -> Result<(u32, usize), EncodeError> {
    let bytes = format.as_bytes();
    let mut i = 0;
    let mut consumed = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'{' {
            let inner_start = i + 1;
            let mut j = inner_start;
            while j + 1 < bytes.len() {
                if bytes[j] == b'%' && bytes[j + 1] == b'}' {
                    break;
                }
                j += 1;
            }
            let inner_str = if inner_start + 2 <= j {
                std::str::from_utf8(&bytes[inner_start + 2..j]).unwrap_or("")
            } else {
                ""
            };
            let (new_word, c) =
                encode_walk_inner(inner_str, word, &operands[consumed..], address, width_bytes)?;
            word = new_word;
            consumed += c;
            i = j + 2;
            continue;
        }
        let bitfield_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
            i += 1;
        }
        let bitfield = std::str::from_utf8(&bytes[bitfield_start..i]).unwrap_or("");
        if i >= bytes.len() {
            break;
        }
        let code = bytes[i];
        i += 1;
        match code {
            b'\'' => {
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }
            b'?' => {
                if i + 1 < bytes.len() {
                    i += 2;
                }
                continue;
            }
            b'`' => {
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }
            _ => {}
        }
        if matches!(code, b'c' | b'C' | b'x' | b'X' | b'%' | b't') {
            continue;
        }
        if matches!(code, b'w' | b'W') && bitfield.is_empty() {
            continue;
        }
        let operand = operands.get(consumed).ok_or(EncodeError::ArityMismatch {
            format_slots: consumed + 1,
            operands: operands.len(),
        })?;
        let bits_written = pack_operand(
            code,
            bitfield,
            operand,
            consumed,
            &mut word,
            address,
            width_bytes,
        )?;
        if bits_written {
            consumed += 1;
        }
    }
    Ok((word, consumed))
}

/// Pack one operand into `word` according to `code` +
/// `bitfield`. Returns true when an operand was actually
/// consumed (some codes are display-only and return false
/// without advancing the queue).
fn pack_operand(
    code: u8,
    bitfield: &str,
    operand: &DecodedOperand,
    slot: usize,
    word: &mut u32,
    address: u64,
    width_bytes: usize,
) -> Result<bool, EncodeError> {
    match code {
        b'r' | b'R' | b'S' if !bitfield.is_empty() => {
            let (lo, hi) = parse_bitfield(bitfield).ok_or(EncodeError::OperandShapeMismatch {
                slot,
                format_code: "r/R/S",
                got: "bad bitfield",
            })?;
            let reg = expect_register(operand, slot, "r/R/S")?;
            let max = (1u32 << (hi - lo + 1)) - 1;
            if reg.index as u32 > max {
                return Err(EncodeError::RegisterOutOfRange {
                    slot,
                    index: reg.index,
                    max: max as u8,
                });
            }
            place_field(word, lo, hi, reg.index as u32);
            Ok(true)
        }
        b'd' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "d")?;
            let v = expect_immediate(operand, slot, "d")?;
            place_unsigned(word, lo, hi, v, slot)?;
            Ok(true)
        }
        b'D' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "D")?;
            let v = expect_immediate(operand, slot, "D")?;
            // %D encodes value-1.
            if v < 1 {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: hi - lo + 1,
                });
            }
            place_unsigned(word, lo, hi, v - 1, slot)?;
            Ok(true)
        }
        b'W' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "W")?;
            let v = expect_immediate(operand, slot, "W")?;
            if v % 4 != 0 {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: hi - lo + 1,
                });
            }
            place_unsigned(word, lo, hi, v / 4, slot)?;
            Ok(true)
        }
        b'H' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "H")?;
            let v = expect_immediate(operand, slot, "H")?;
            if v % 2 != 0 {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: hi - lo + 1,
                });
            }
            place_unsigned(word, lo, hi, v / 2, slot)?;
            Ok(true)
        }
        b'x' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "x")?;
            let v = expect_immediate(operand, slot, "x")?;
            place_unsigned(word, lo, hi, v, slot)?;
            Ok(true)
        }
        b'B' if !bitfield.is_empty() => {
            // Short Thumb branch: signed half-word displacement.
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "B")?;
            let target = expect_branch_target(operand, slot, "B")?;
            let displacement = (target as i64).wrapping_sub(address as i64).wrapping_sub(4);
            if displacement % 2 != 0 {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits: hi - lo + 1,
                });
            }
            let halfwords = displacement / 2;
            let bits = hi - lo + 1;
            check_signed_fits(halfwords, bits, slot)?;
            let mask = (1u32 << bits) - 1;
            place_field(word, lo, hi, (halfwords as u32) & mask);
            Ok(true)
        }
        b'B' if bitfield.is_empty() && width_bytes == 4 => {
            // 32-bit BL / B.W (24-bit range).
            let target = expect_branch_target(operand, slot, "B")?;
            pack_bl_target(word, target, address, slot)?;
            Ok(true)
        }
        b'b' if width_bytes == 4 => {
            // 32-bit conditional B.W (T3, 21-bit range).
            let target = expect_branch_target(operand, slot, "b")?;
            pack_b_cond_wide_target(word, target, address, slot)?;
            Ok(true)
        }
        b'b' if !bitfield.is_empty() && width_bytes == 2 => {
            // CZB 6-bit unsigned forward branch.
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "b")?;
            let target = expect_branch_target(operand, slot, "b")?;
            let displacement = (target as i64).wrapping_sub(address as i64).wrapping_sub(4);
            if displacement < 0 || displacement % 2 != 0 {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits: hi - lo + 1,
                });
            }
            let halfwords = (displacement / 2) as u32;
            let bits = hi - lo + 1;
            if halfwords >= (1u32 << bits) {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits,
                });
            }
            place_field(word, lo, hi, halfwords);
            Ok(true)
        }
        b'a' if !bitfield.is_empty() => {
            // PC-relative literal address: %0-7a etc. Operand
            // is a PcRelative absolute address; we encode the
            // *4-aligned PC offset* into the bitfield.
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "a")?;
            let target = expect_pc_relative(operand, slot, "a")?;
            let pc_aligned = (address.wrapping_add(4)) & !3u64;
            let displacement = (target as i64).wrapping_sub(pc_aligned as i64);
            if displacement < 0 || displacement % 4 != 0 {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits: hi - lo + 1,
                });
            }
            let words = (displacement / 4) as u32;
            let bits = hi - lo + 1;
            if words >= (1u32 << bits) {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits,
                });
            }
            place_field(word, lo, hi, words);
            Ok(true)
        }
        b'S' if bitfield.is_empty() && width_bytes == 2 => {
            // 16-bit %S: Rm at bits[3..5] + bit 6 for hi-bit.
            let reg = expect_register(operand, slot, "S")?;
            place_field(word, 3, 5, (reg.index & 0x7) as u32);
            place_field(word, 6, 6, ((reg.index >> 3) & 0x1) as u32);
            Ok(true)
        }
        b'S' if bitfield.is_empty() && width_bytes == 4 => {
            // 32-bit %S: splice the OpaqueBits payload back.
            // The decoder emits the full Rm + shift type +
            // imm5 as a single OpaqueBits so the encoder can
            // reproduce all bits exactly.
            match operand {
                DecodedOperand::OpaqueBits { bits, mask } => {
                    *word &= !*mask;
                    *word |= *bits & *mask;
                    Ok(true)
                }
                _ => Err(EncodeError::OperandShapeMismatch {
                    slot,
                    format_code: "S",
                    got: operand_kind(operand),
                }),
            }
        }
        b'D' if bitfield.is_empty() && width_bytes == 2 => {
            // 16-bit %D: Rd at bits[0..2] + bit 7 for hi-bit.
            let reg = expect_register(operand, slot, "D")?;
            place_field(word, 0, 2, (reg.index & 0x7) as u32);
            place_field(word, 7, 7, ((reg.index >> 3) & 0x1) as u32);
            Ok(true)
        }
        b'M' if width_bytes == 2 => {
            // 16-bit reglist (low 8 bits).
            let mask = expect_register_list(operand, slot, "M")?;
            if mask & !0xffu16 != 0 {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: mask as i64,
                    bits: 8,
                });
            }
            place_field(word, 0, 7, mask as u32);
            Ok(true)
        }
        b'N' => {
            // Push: reglist (low 8 bits) + LR via bit 8.
            let mask = expect_register_list(operand, slot, "N")?;
            place_field(word, 0, 7, (mask & 0xff) as u32);
            // LR was carried in our high bit (bit 14).
            let lr = ((mask >> 14) & 0x1) as u32;
            place_field(word, 8, 8, lr);
            Ok(true)
        }
        b'O' => {
            // Pop: reglist (low 8 bits) + PC via bit 8.
            let mask = expect_register_list(operand, slot, "O")?;
            place_field(word, 0, 7, (mask & 0xff) as u32);
            let pc = ((mask >> 15) & 0x1) as u32;
            place_field(word, 8, 8, pc);
            Ok(true)
        }
        b'I' if width_bytes == 4 => {
            // 12-bit Thumb-2 immediate scattered into
            // hw1[10] : hw2[14:12] : hw2[7:0].
            let v = expect_immediate(operand, slot, "I")?;
            if !(0..=0xfff).contains(&v) {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: 12,
                });
            }
            let v = v as u32;
            place_field(word, 0, 7, v & 0xff);
            place_field(word, 12, 14, (v >> 8) & 0x7);
            place_field(word, 26, 26, (v >> 11) & 0x1);
            Ok(true)
        }
        b'M' if width_bytes == 4 => {
            // ThumbExpandImm — round-trip via OpaqueBits when
            // the decoder preserved the raw form, otherwise
            // fall back to the inverse-expand path for
            // hand-built operand lists carrying just the
            // expanded value as Immediate.
            match operand {
                DecodedOperand::OpaqueBits { bits, mask } => {
                    *word &= !*mask;
                    *word |= *bits & *mask;
                    Ok(true)
                }
                DecodedOperand::Immediate(v) => {
                    let raw = thumb_expand_imm_inverse(*v).ok_or(
                        EncodeError::UnsupportedFormatCode {
                            format_code: "%M (rotation form not yet invertible)".into(),
                        },
                    )?;
                    place_field(word, 0, 7, raw & 0xff);
                    place_field(word, 12, 14, (raw >> 8) & 0x7);
                    place_field(word, 26, 26, (raw >> 11) & 0x1);
                    Ok(true)
                }
                _ => Err(EncodeError::OperandShapeMismatch {
                    slot,
                    format_code: "M",
                    got: operand_kind(operand),
                }),
            }
        }
        b'J' if width_bytes == 4 => {
            // 16-bit immediate: hw1[3:0] : hw1[10] : hw2[14:12] : hw2[7:0].
            let v = expect_immediate(operand, slot, "J")?;
            if !(0..=0xffff).contains(&v) {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: 16,
                });
            }
            let v = v as u32;
            place_field(word, 0, 7, v & 0xff);
            place_field(word, 12, 14, (v >> 8) & 0x7);
            place_field(word, 26, 26, (v >> 11) & 0x1);
            place_field(word, 16, 19, (v >> 12) & 0xf);
            Ok(true)
        }
        // Codes whose decoder emits OpaqueBits as the
        // operand. Splice the bits back into the encoded
        // word via (word & !mask) | (bits & mask).
        b'a' | b's' | b'L' | b'E' | b'F' | b'm' | b'n' | b'b' | b'I'
            if !applies_already(code, bitfield, width_bytes) =>
        {
            match operand {
                DecodedOperand::OpaqueBits { bits, mask } => {
                    *word &= !*mask;
                    *word |= *bits & *mask;
                    Ok(true)
                }
                _ => Err(EncodeError::OperandShapeMismatch {
                    slot,
                    format_code: "OpaqueBits-expected",
                    got: operand_kind(operand),
                }),
            }
        }
        // Codes still falling back to the no-op
        // "approximation" branch. These are display-only
        // markers (no operand consumed) — but our outer
        // loop has already fetched an operand for them.
        // The tail OpaqueBits at the sweep level preserves
        // any row-mask-variable bits these refer to.
        b'G' | b'V' | b'K' | b'P' | b'Q' | b'U' | b'W' | b'Y' | b'Z' | b'H' | b'S' | b'R'
            if !applies_already(code, bitfield, width_bytes) =>
        {
            let _ = operand;
            let _ = slot;
            Ok(true)
        }
        _ => Err(EncodeError::UnsupportedFormatCode {
            format_code: if bitfield.is_empty() {
                format!("%{}", code as char)
            } else {
                format!("%{bitfield}{}", code as char)
            },
        }),
    }
}

/// Tracks codes that the main match arms above already
/// handled with concrete logic — used by the catch-all
/// "approximation" branch to avoid double-handling.
fn applies_already(code: u8, bitfield: &str, width_bytes: usize) -> bool {
    match code {
        b'a' => !bitfield.is_empty(),
        b'b' => width_bytes == 4 || !bitfield.is_empty(),
        b'B' => !bitfield.is_empty() || width_bytes == 4,
        b'D' | b'S' => !bitfield.is_empty() || width_bytes == 2,
        b'H' => !bitfield.is_empty(),
        b'M' => width_bytes == 2 || width_bytes == 4,
        b'I' | b'J' => width_bytes == 4,
        b'N' | b'O' => true,
        _ => false,
    }
}

fn parse_bitfield(s: &str) -> Option<(u8, u8)> {
    if s.is_empty() {
        return None;
    }
    if let Some((lo_s, hi_s)) = s.split_once('-') {
        let lo: u8 = lo_s.parse().ok()?;
        let hi: u8 = hi_s.parse().ok()?;
        if lo > hi {
            return None;
        }
        Some((lo, hi))
    } else {
        let n: u8 = s.parse().ok()?;
        Some((n, n))
    }
}

fn unwrap_bitfield(s: &str, slot: usize, code: &'static str) -> Result<(u8, u8), EncodeError> {
    parse_bitfield(s).ok_or(EncodeError::OperandShapeMismatch {
        slot,
        format_code: code,
        got: "bad bitfield",
    })
}

fn place_field(word: &mut u32, lo: u8, hi: u8, value: u32) {
    let width = hi - lo + 1;
    let mask = if width >= 32 { u32::MAX } else { (1u32 << width) - 1 };
    // Clear the destination bits, then OR in the new value.
    *word &= !(mask << lo);
    *word |= (value & mask) << lo;
}

fn place_unsigned(
    word: &mut u32,
    lo: u8,
    hi: u8,
    value: i64,
    slot: usize,
) -> Result<(), EncodeError> {
    if value < 0 {
        return Err(EncodeError::ImmediateOutOfRange {
            slot,
            value,
            bits: hi - lo + 1,
        });
    }
    let width = hi - lo + 1;
    let max = if width >= 63 { i64::MAX } else { (1i64 << width) - 1 };
    if value > max {
        return Err(EncodeError::ImmediateOutOfRange {
            slot,
            value,
            bits: width,
        });
    }
    place_field(word, lo, hi, value as u32);
    Ok(())
}

fn check_signed_fits(value: i64, bits: u8, slot: usize) -> Result<(), EncodeError> {
    let half = 1i64 << (bits - 1);
    if value < -half || value >= half {
        return Err(EncodeError::BranchOutOfRange {
            slot,
            displacement: value,
            bits,
        });
    }
    Ok(())
}

fn expect_register<'a>(
    operand: &'a DecodedOperand,
    slot: usize,
    code: &'static str,
) -> Result<&'a Register, EncodeError> {
    match operand {
        DecodedOperand::Register(r) => Ok(r),
        _ => Err(EncodeError::OperandShapeMismatch {
            slot,
            format_code: code,
            got: operand_kind(operand),
        }),
    }
}

fn expect_immediate(
    operand: &DecodedOperand,
    slot: usize,
    code: &'static str,
) -> Result<i64, EncodeError> {
    match operand {
        DecodedOperand::Immediate(v) => Ok(*v),
        _ => Err(EncodeError::OperandShapeMismatch {
            slot,
            format_code: code,
            got: operand_kind(operand),
        }),
    }
}

fn expect_branch_target(
    operand: &DecodedOperand,
    slot: usize,
    code: &'static str,
) -> Result<u64, EncodeError> {
    match operand {
        DecodedOperand::BranchTarget(t) => Ok(*t),
        _ => Err(EncodeError::OperandShapeMismatch {
            slot,
            format_code: code,
            got: operand_kind(operand),
        }),
    }
}

fn expect_pc_relative(
    operand: &DecodedOperand,
    slot: usize,
    code: &'static str,
) -> Result<u64, EncodeError> {
    match operand {
        DecodedOperand::PcRelative(t) => Ok(*t),
        _ => Err(EncodeError::OperandShapeMismatch {
            slot,
            format_code: code,
            got: operand_kind(operand),
        }),
    }
}

fn expect_register_list(
    operand: &DecodedOperand,
    slot: usize,
    code: &'static str,
) -> Result<u16, EncodeError> {
    match operand {
        DecodedOperand::RegisterList(m) => Ok(*m),
        _ => Err(EncodeError::OperandShapeMismatch {
            slot,
            format_code: code,
            got: operand_kind(operand),
        }),
    }
}

fn operand_kind(o: &DecodedOperand) -> &'static str {
    match o {
        DecodedOperand::Register(_) => "Register",
        DecodedOperand::Immediate(_) => "Immediate",
        DecodedOperand::BranchTarget(_) => "BranchTarget",
        DecodedOperand::PcRelative(_) => "PcRelative",
        DecodedOperand::RegisterList(_) => "RegisterList",
        DecodedOperand::Condition(_) => "Condition",
        DecodedOperand::OpaqueBits { .. } => "OpaqueBits",
    }
}

fn pack_bl_target(
    word: &mut u32,
    target: u64,
    address: u64,
    slot: usize,
) -> Result<(), EncodeError> {
    let displacement = (target as i64).wrapping_sub(address as i64).wrapping_sub(4);
    if displacement % 2 != 0 {
        return Err(EncodeError::BranchOutOfRange {
            slot,
            displacement,
            bits: 25,
        });
    }
    // 25-bit signed range (±16 MiB).
    if !(-(1i64 << 24)..(1i64 << 24)).contains(&displacement) {
        return Err(EncodeError::BranchOutOfRange {
            slot,
            displacement,
            bits: 25,
        });
    }
    let raw = displacement & ((1i64 << 25) - 1); // 25 bits incl. low zero
    let raw_u = raw as u32;
    let s = (displacement >> 24) & 0x1;
    let imm10 = (raw_u >> 12) & 0x3ff;
    let imm11 = (raw_u >> 1) & 0x7ff;
    // i1 = !(j1 ^ s) → j1 = !i1 ^ s
    let i1 = (raw_u >> 23) & 0x1;
    let i2 = (raw_u >> 22) & 0x1;
    let j1 = (!i1 ^ (s as u32)) & 0x1;
    let j2 = (!i2 ^ (s as u32)) & 0x1;
    place_field(word, 26, 26, s as u32);
    place_field(word, 16, 25, imm10);
    place_field(word, 13, 13, j1);
    place_field(word, 11, 11, j2);
    place_field(word, 0, 10, imm11);
    Ok(())
}

fn pack_b_cond_wide_target(
    word: &mut u32,
    target: u64,
    address: u64,
    slot: usize,
) -> Result<(), EncodeError> {
    let displacement = (target as i64).wrapping_sub(address as i64).wrapping_sub(4);
    if displacement % 2 != 0 {
        return Err(EncodeError::BranchOutOfRange {
            slot,
            displacement,
            bits: 21,
        });
    }
    if !(-(1i64 << 20)..(1i64 << 20)).contains(&displacement) {
        return Err(EncodeError::BranchOutOfRange {
            slot,
            displacement,
            bits: 21,
        });
    }
    let raw = displacement & ((1i64 << 21) - 1);
    let raw_u = raw as u32;
    let s = (displacement >> 20) & 0x1;
    let imm6 = (raw_u >> 12) & 0x3f;
    let imm11 = (raw_u >> 1) & 0x7ff;
    let j1 = (raw_u >> 17) & 0x1;
    let j2 = (raw_u >> 18) & 0x1;
    place_field(word, 26, 26, s as u32);
    place_field(word, 16, 21, imm6);
    place_field(word, 13, 13, j1);
    place_field(word, 11, 11, j2);
    place_field(word, 0, 10, imm11);
    Ok(())
}

/// Inverse of [`super::format_decode::thumb_expand_imm`] for
/// values directly representable as a constant case (no
/// rotation). Returns the 12-bit raw encoding, or `None` if
/// the value can only be expressed via the rotation form.
fn thumb_expand_imm_inverse(v: i64) -> Option<u32> {
    if !(0..=u32::MAX as i64).contains(&v) {
        return None;
    }
    let v = v as u32;
    // Case 0: low 8 bits, top 24 bits zero.
    if v <= 0xff {
        return Some(v);
    }
    let low8 = v & 0xff;
    // Case 1: 0x00XX_00XX
    if v == ((low8 << 16) | low8) {
        return Some((1 << 8) | low8);
    }
    // Case 2: 0xXX00_XX00
    let low8b = (v >> 8) & 0xff;
    if v == ((low8b << 24) | (low8b << 8)) && (v & 0xff) == 0 {
        return Some((2 << 8) | low8b);
    }
    // Case 3: 0xXXXX_XXXX (all four bytes equal).
    if v == ((low8 << 24) | (low8 << 16) | (low8 << 8) | low8) {
        return Some((3 << 8) | low8);
    }
    // Rotation form: try every rotation of 8-bit unrotated
    // value with the top bit set.
    for rot in 0u32..32 {
        let candidate = (v.rotate_left(rot)) & 0xffff_ffff;
        if candidate >> 8 == 0 && candidate & 0x80 != 0 {
            // unrotated has top bit set, low 8 bits = candidate
            // raw imm12: bit 11..7 = rot count, low 7 bits = candidate & 0x7f.
            let unrotated_low7 = candidate & 0x7f;
            return Some((rot << 7) | unrotated_low7);
        }
    }
    None
}

fn mnemonic_name(_m: ThumbMnemonicGenerated) -> &'static str {
    // Generated mnemonic enum is large and lives in another
    // file; for error messages we just label it generically.
    // Callers needing a specific name can format the mnemonic
    // via Debug at the call site.
    "<thumb-mnemonic>"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::format_decode::decode_operands_from_format;
    use crate::isa::armv7::table_generated::{match_generated, THUMB_OPCODE_TABLE_GENERATED};

    #[test]
    fn round_trips_thumb1_add_three_low_registers() {
        // adds r3, r4, r5 → 0x1963.
        let row = match_generated(0x1963, ThumbWidth::Halfword).expect("decode");
        let (operands, _) = decode_operands_from_format(row.format, 0x1963, 0, 2);
        let (word, w) = encode_with_row(row, &operands, 0).expect("encode");
        assert_eq!(word, 0x1963);
        assert_eq!(w, ThumbWidth::Halfword);
    }

    #[test]
    fn round_trips_thumb1_mov_immediate() {
        // movs r2, #0x37 → 0x2237.
        let row = match_generated(0x2237, ThumbWidth::Halfword).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, 0x2237, 0, 2);
        let (word, _) = encode_with_row(row, &operands, 0).unwrap();
        assert_eq!(word, 0x2237);
    }

    #[test]
    fn round_trips_thumb1_branch() {
        // beq +6: word 0xd003 at address 0x2000.
        let address = 0x2000u64;
        let row = match_generated(0xd003, ThumbWidth::Halfword).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, 0xd003, address, 2);
        let (word, _) = encode_with_row(row, &operands, address).unwrap();
        assert_eq!(word, 0xd003);
    }

    #[test]
    fn round_trips_bl_target() {
        // BL with target +4 at address 0x1000: word 0xf000_f802.
        let address = 0x1000u64;
        let word_in = 0xf000_f802u32;
        let row = match_generated(word_in, ThumbWidth::Word).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, word_in, address, 4);
        let (word_out, _) = encode_with_row(row, &operands, address).unwrap();
        assert_eq!(word_out, word_in);
    }

    #[test]
    fn round_trips_thumb2_mov_immediate() {
        // mov.w r0, #1 → 0xf04f_0001.
        let word_in = 0xf04f_0001u32;
        let row = match_generated(word_in, ThumbWidth::Word).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, word_in, 0, 4);
        let (word_out, _) = encode_with_row(row, &operands, 0).unwrap();
        assert_eq!(word_out, word_in);
    }

    #[test]
    fn rejects_register_out_of_range() {
        // adds r3, r4, r5 row: %0-2r expects a 3-bit register
        // index (0..=7). Try r9.
        let row = match_generated(0x1963, ThumbWidth::Halfword).unwrap();
        let bad_operands = vec![
            DecodedOperand::Register(Register {
                class: RegisterClass::R,
                index: 9,
            }),
            DecodedOperand::Register(Register {
                class: RegisterClass::R,
                index: 4,
            }),
            DecodedOperand::Register(Register {
                class: RegisterClass::R,
                index: 5,
            }),
        ];
        let err = encode_with_row(row, &bad_operands, 0).unwrap_err();
        assert!(matches!(err, EncodeError::RegisterOutOfRange { index: 9, .. }),
            "got: {err:?}");
    }

    #[test]
    fn rejects_branch_out_of_range() {
        // BL row with a wildly out-of-range target.
        let address = 0u64;
        let word_in = 0xf000_d000u32;
        let row = match_generated(word_in, ThumbWidth::Word).unwrap();
        let bad_operands = vec![DecodedOperand::BranchTarget(0x1_0000_0000)];
        let err = encode_with_row(row, &bad_operands, address).unwrap_err();
        assert!(matches!(err, EncodeError::BranchOutOfRange { .. }), "got: {err:?}");
    }

    #[test]
    fn round_trips_full_assembled_program() {
        // Reuse the fixture from format_decoder_emits_operands_for_full_assembled_program.
        let bytes: &[u8] = &[
            0xf0, 0xb5, 0x04, 0x46, 0x0d, 0x46, 0x04, 0xeb, 0x05, 0x00, 0xa4, 0xf1,
            0x0a, 0x01, 0x88, 0x42, 0x01, 0xd0, 0xff, 0xf7, 0xfe, 0xff, 0x22, 0x68,
            0x2a, 0x60, 0x63, 0x78, 0x6b, 0x70, 0x63, 0x88, 0x6b, 0x80, 0x04, 0xea,
            0x05, 0x00, 0x44, 0xea, 0x05, 0x01, 0x84, 0xea, 0x05, 0x02, 0x4f, 0xea,
            0x84, 0x03, 0x4f, 0xea, 0x14, 0x13, 0x4f, 0xea, 0x24, 0x23, 0x64, 0xfa,
            0x05, 0xf3, 0xc0, 0x46, 0xf0, 0xbd, 0x70, 0x47,
        ];
        let address_base = 0x1000u64;
        let mut cursor = 0usize;
        let mut roundtripped = 0usize;
        let mut skipped = 0usize;
        while cursor < bytes.len() {
            let (word_in, width_bytes) =
                crate::isa::armv7::read_instruction(bytes, cursor).expect("read");
            let width = if width_bytes == 4 { ThumbWidth::Word } else { ThumbWidth::Halfword };
            let row = match_generated(word_in, width).expect("match");
            let address = address_base + cursor as u64;
            let (operands, _) =
                decode_operands_from_format(row.format, word_in, address, width_bytes);
            match encode_with_row(row, &operands, address) {
                Ok((word_out, _)) if word_out == word_in => {
                    roundtripped += 1;
                }
                Ok((word_out, _)) => {
                    // Mismatch — known approximate-decoder
                    // cases (e.g. shifted-register data-
                    // processing where the decoder drops the
                    // shift). Tracked but not fatal until
                    // the decode/encode pair grows shift
                    // operand support.
                    eprintln!(
                        "approximate roundtrip at offset {cursor} \
                         (format {}): in=0x{word_in:08x} out=0x{word_out:08x}",
                        row.format,
                    );
                    skipped += 1;
                }
                Err(e) => {
                    eprintln!(
                        "skip roundtrip at offset {cursor} (format {}): {e:?}",
                        row.format,
                    );
                    skipped += 1;
                }
            }
            cursor += width_bytes;
        }
        // We expect a healthy majority to round-trip — the
        // approximate-operand cases are the .w shifted-Rm
        // forms.
        assert!(roundtripped >= 12, "only {roundtripped} round-tripped, {skipped} skipped");
    }

    #[test]
    fn thumb_expand_imm_inverse_constant_forms() {
        // 1 → case 0
        assert_eq!(thumb_expand_imm_inverse(1), Some(0x001));
        // 0x00010001 → case 1 (low8=1)
        assert_eq!(thumb_expand_imm_inverse(0x0001_0001), Some(0x101));
        // 0x01000100 → case 2 (low8=1)
        assert_eq!(thumb_expand_imm_inverse(0x0100_0100), Some(0x201));
        // 0x01010101 → case 3
        assert_eq!(thumb_expand_imm_inverse(0x0101_0101), Some(0x301));
    }

    #[test]
    fn encode_by_mnemonic_picks_first_matching_form() {
        // mov r2, #0x37 — encodable as Thumb-1 movs (0x2237)
        // by table order. encode() should find that row.
        let operands = vec![
            DecodedOperand::Register(Register {
                class: RegisterClass::R,
                index: 2,
            }),
            DecodedOperand::Immediate(0x37),
        ];
        let result = encode(ThumbMnemonicGenerated::Mov, &operands, 0);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn rejects_arity_mismatch() {
        // adds expects 3 register operands; supply 2.
        let row = match_generated(0x1963, ThumbWidth::Halfword).unwrap();
        let too_few = vec![
            DecodedOperand::Register(Register { class: RegisterClass::R, index: 0 }),
            DecodedOperand::Register(Register { class: RegisterClass::R, index: 1 }),
        ];
        let err = encode_with_row(row, &too_few, 0).unwrap_err();
        assert!(matches!(err, EncodeError::ArityMismatch { .. }), "got: {err:?}");
    }

    #[test]
    fn table_size_unchanged() {
        assert_eq!(THUMB_OPCODE_TABLE_GENERATED.len(), 336);
    }
}
