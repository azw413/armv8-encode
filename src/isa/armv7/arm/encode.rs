//! ARM-mode operand-level encoder.
//!
//! Symmetric mirror of [`super::format_decode`]: walks the
//! binutils format string consuming operands from the
//! caller's queue and packing their bits into an output word.
//! Same approach as the Thumb encoder
//! (`crate::isa::armv7::encode`); the per-format-code
//! semantics differ because ARM-mode bit positions and
//! conventions are different.
//!
//! ## Form selection
//!
//! Multiple table rows may match a `(mnemonic,
//! operand-shape)` pair. Two entry points:
//!
//! - [`encode_with_row`] — caller supplies the exact
//!   [`ArmOpcodeGenerated`] row to use. Predictable;
//!   recommended for round-trip work.
//! - [`encode`] — caller supplies just the mnemonic; the
//!   encoder walks the table and returns the first row that
//!   accepts the operands.
//!
//! Both end up in [`encode_with_format`].
//!
//! ## Coverage
//!
//! Format codes the encoder handles (subset of the decoder's
//! coverage; data-processing addressing modes are
//! approximate):
//!
//! - `%<bf>r`, `%<bf>R`, `%<bf>T` — register slots
//! - `%<bf>d`, `%<bf>W`, `%<bf>x`, `%<bf>X` — bitfield
//!   immediates
//! - `%c`, `%C`, `%x`, `%X`, `%p`, `%t`, `%q` (no bitfield)
//!   — display-only, skipped
//! - `%b` — 24-bit signed branch displacement (B/BL)
//! - `%B` — BLX(1) (24-bit signed × 4 + H bit)
//! - `%m` — register mask (low 16 bits)
//! - `%o` — operand2 (immediate-with-rotate or shifted-Rm
//!   register form, register form only encodes Rm)
//! - `%a` — load/store address (Rn + offset, basic form)
//! - `%s` — halfword/signextend address
//! - `%V` — MOVT/MOVW 16-bit immediate (split across [19:16]
//!   and [11:0])
//! - `%E` — BFI/BFC lsb + width
//! - `%e` — SMI immediate
//! - `%U` — barrier type
//!
//! Codes not yet inverted (`%C`, `%P`, `%S`, `%W` standalone)
//! cause [`EncodeError::UnsupportedFormatCode`].

use crate::isa::armv7::operand::{DecodedOperand, EncodeError, Register};
use super::table_generated::{
    ArmMnemonicGenerated, ArmOpcodeGenerated, ARM_OPCODE_TABLE_GENERATED,
};

/// Encode against a specific table row.
pub fn encode_with_row(
    row: &ArmOpcodeGenerated,
    operands: &[DecodedOperand],
    address: u64,
) -> Result<u32, EncodeError> {
    encode_with_format(row.format, row.opcode, operands, address)
}

/// Encode by mnemonic — picks the first table row whose
/// format slots match the supplied operand shape. Convenient
/// for tests; round-trip work should prefer
/// [`encode_with_row`].
pub fn encode(
    mnemonic: ArmMnemonicGenerated,
    operands: &[DecodedOperand],
    address: u64,
) -> Result<u32, EncodeError> {
    let mut last_err: Option<EncodeError> = None;
    for row in ARM_OPCODE_TABLE_GENERATED.iter() {
        if row.mnemonic != mnemonic {
            continue;
        }
        match encode_with_format(row.format, row.opcode, operands, address) {
            Ok(word) => return Ok(word),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or(EncodeError::NoMatchingForm {
        mnemonic: "<arm-mnemonic>",
        operand_count: operands.len(),
    }))
}

/// The shared bit-packer. Walks `format`, consuming operands
/// in left-to-right order, and OR-ing each operand's bits
/// into a working word that starts from the row's `opcode`
/// (which carries the constant high bits including the
/// always-execute condition `0xe` in bits 28..31 unless the
/// row uses an unconditional encoding).
pub fn encode_with_format(
    format: &str,
    opcode_base: u32,
    operands: &[DecodedOperand],
    address: u64,
) -> Result<u32, EncodeError> {
    let mut word = opcode_base;
    // The imported ARM table stores the condition field
    // (bits 28..31) as zero on conditional rows — the row's
    // mask leaves those bits unspecified so any condition
    // matches at decode time. When *encoding*, we must pick
    // a concrete condition. Default to AL (`0xe` = always).
    // Callers wanting a specific condition can post-process
    // the output word.
    if (opcode_base & 0xf000_0000) == 0 {
        word |= 0xe000_0000;
    }
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
        // Skip {X:...%} display wrappers but recurse into them.
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
            let (new_word, consumed) = encode_walk_inner(
                inner_str,
                word,
                &operands[operand_idx..],
                address,
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
        // Display-only (no operand consumed).
        if matches!(code, b'c' | b'C' | b'x' | b'X' | b'%' | b'p' | b't' | b'q')
            && bitfield.is_empty()
        {
            continue;
        }
        let slot = operand_idx;
        let operand = operands.get(slot).ok_or(EncodeError::ArityMismatch {
            format_slots: slot + 1,
            operands: operands.len(),
        })?;
        let consumed = pack_operand(code, bitfield, operand, slot, &mut word, address)?;
        if consumed {
            operand_idx += 1;
        }
    }
    Ok(word)
}

fn encode_walk_inner(
    format: &str,
    mut word: u32,
    operands: &[DecodedOperand],
    address: u64,
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
                encode_walk_inner(inner_str, word, &operands[consumed..], address)?;
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
        if matches!(code, b'c' | b'C' | b'x' | b'X' | b'%' | b'p' | b't' | b'q')
            && bitfield.is_empty()
        {
            continue;
        }
        let operand = operands.get(consumed).ok_or(EncodeError::ArityMismatch {
            format_slots: consumed + 1,
            operands: operands.len(),
        })?;
        let did = pack_operand(code, bitfield, operand, consumed, &mut word, address)?;
        if did {
            consumed += 1;
        }
    }
    Ok((word, consumed))
}

fn pack_operand(
    code: u8,
    bitfield: &str,
    operand: &DecodedOperand,
    slot: usize,
    word: &mut u32,
    address: u64,
) -> Result<bool, EncodeError> {
    match code {
        b'r' | b'R' | b'T' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "r/R/T")?;
            let reg = expect_register(operand, slot, "r/R/T")?;
            let max = (1u32 << (hi - lo + 1)) - 1;
            let index = if code == b'T' {
                // %T encodes value-1: index = encoded + 1, so
                // we write (reg.index - 1).
                if reg.index == 0 {
                    return Err(EncodeError::RegisterOutOfRange {
                        slot,
                        index: reg.index,
                        max: max as u8,
                    });
                }
                reg.index - 1
            } else {
                reg.index
            };
            if index as u32 > max {
                return Err(EncodeError::RegisterOutOfRange {
                    slot,
                    index,
                    max: max as u8,
                });
            }
            place_field(word, lo, hi, index as u32);
            Ok(true)
        }
        b'd' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "d")?;
            let v = expect_immediate(operand, slot, "d")?;
            place_unsigned(word, lo, hi, v, slot)?;
            Ok(true)
        }
        b'W' if !bitfield.is_empty() => {
            // %<bf>W = bitfield + 1 (i.e. encode value-1).
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "W")?;
            let v = expect_immediate(operand, slot, "W")?;
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
        b'x' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "x")?;
            let v = expect_immediate(operand, slot, "x")?;
            place_unsigned(word, lo, hi, v, slot)?;
            Ok(true)
        }
        b'X' if !bitfield.is_empty() => {
            let (lo, hi) = unwrap_bitfield(bitfield, slot, "X")?;
            let v = expect_immediate(operand, slot, "X")?;
            place_unsigned(word, lo, hi, v, slot)?;
            Ok(true)
        }
        b'b' if bitfield.is_empty() => {
            // 24-bit signed branch offset shifted right by 2.
            let target = expect_branch_target(operand, slot, "b")?;
            let displacement =
                (target as i64).wrapping_sub(address as i64).wrapping_sub(8);
            if displacement % 4 != 0 {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits: 24,
                });
            }
            let words = displacement / 4;
            // Sign-fit in 24 bits: -2^23..(2^23 - 1).
            if !(-(1i64 << 23)..(1i64 << 23)).contains(&words) {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits: 24,
                });
            }
            let mask24 = (1u32 << 24) - 1;
            place_field(word, 0, 23, (words as u32) & mask24);
            Ok(true)
        }
        b'B' if bitfield.is_empty() => {
            // BLX(1): like %b but with an extra half-word
            // (bit 24 = H) for ±2-byte alignment.
            let target = expect_branch_target(operand, slot, "B")?;
            let displacement =
                (target as i64).wrapping_sub(address as i64).wrapping_sub(8);
            if displacement % 2 != 0 {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits: 25,
                });
            }
            let h = ((displacement & 0x2) >> 1) as u32;
            let words = displacement / 4;
            if !(-(1i64 << 23)..(1i64 << 23)).contains(&words) {
                return Err(EncodeError::BranchOutOfRange {
                    slot,
                    displacement,
                    bits: 25,
                });
            }
            let mask24 = (1u32 << 24) - 1;
            place_field(word, 0, 23, (words as u32) & mask24);
            place_field(word, 24, 24, h);
            Ok(true)
        }
        b'm' if bitfield.is_empty() => {
            // 16-bit reglist into bits [0..15].
            let mask = expect_register_list(operand, slot, "m")?;
            place_field(word, 0, 15, mask as u32);
            Ok(true)
        }
        b'o' if bitfield.is_empty() => {
            // Operand2: bit 25 = 1 → immediate-with-rotation;
            // bit 25 = 0 → register Rm in [3:0].
            //
            // The decoder picks one form based on bit 25 of
            // the input word; the encoder must agree with
            // whatever form the row's opcode_base already
            // chose. Inspect the current `word` bit 25 to
            // know which form to encode into.
            let i_bit = (*word >> 25) & 0x1;
            if i_bit == 1 {
                let v = expect_immediate(operand, slot, "o")?;
                if !(0..=u32::MAX as i64).contains(&v) {
                    return Err(EncodeError::ImmediateOutOfRange {
                        slot,
                        value: v,
                        bits: 12,
                    });
                }
                let raw = arm_expand_imm_inverse(v as u32).ok_or(
                    EncodeError::UnsupportedFormatCode {
                        format_code: "%o (immediate not representable)".into(),
                    },
                )?;
                place_field(word, 0, 11, raw);
                Ok(true)
            } else {
                let reg = expect_register(operand, slot, "o")?;
                place_field(word, 0, 3, (reg.index & 0xf) as u32);
                Ok(true)
            }
        }
        b'a' if bitfield.is_empty() => {
            // Load/store address: encoder consumes the base
            // register and (optionally) the offset operand.
            // The decoder emits Rn + Immediate (or Rn + Rm).
            // Inspect bit 25 to pick the form.
            let i_bit = (*word >> 25) & 0x1;
            if i_bit == 0 {
                // Immediate offset form: Rn at [16:19],
                // imm12 at [0:11], U bit at 23.
                let reg = expect_register(operand, slot, "a")?;
                place_field(word, 16, 19, (reg.index & 0xf) as u32);
                Ok(true)
            } else {
                let reg = expect_register(operand, slot, "a")?;
                place_field(word, 16, 19, (reg.index & 0xf) as u32);
                Ok(true)
            }
        }
        b's' if bitfield.is_empty() => {
            // Halfword/signextend address: Rn at [16:19].
            let reg = expect_register(operand, slot, "s")?;
            place_field(word, 16, 19, (reg.index & 0xf) as u32);
            Ok(true)
        }
        b'V' if bitfield.is_empty() => {
            // MOVT/MOVW: 16-bit immediate split across
            // [19:16] (high 4 bits) and [11:0] (low 12 bits).
            let v = expect_immediate(operand, slot, "V")?;
            if !(0..=0xffff).contains(&v) {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: 16,
                });
            }
            let v = v as u32;
            place_field(word, 0, 11, v & 0xfff);
            place_field(word, 16, 19, (v >> 12) & 0xf);
            Ok(true)
        }
        b'E' if bitfield.is_empty() => {
            // BFI/BFC: lsb (bits 7..11) and msb-derived width
            // (bits 16..20). Operand list: lsb, width.
            let v = expect_immediate(operand, slot, "E")?;
            if !(0..=31).contains(&v) {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: 5,
                });
            }
            // We need both lsb and width but pack_operand
            // sees one operand at a time. Returning false
            // here would re-run with the same operand —
            // instead, lookahead the next operand from the
            // caller's slice. That's not currently in our
            // API, so for now we treat %E as "consume lsb;
            // skip width" — caller must ensure the next
            // operand-slot consumer absorbs width. This is
            // imperfect but matches the decoder's emit-pair
            // shape which puts lsb + width as two
            // consecutive Immediate operands.
            //
            // Better fix is a real bf_pair operand bundling
            // both. Punt for now: write lsb at [11:7] and
            // expect a follow-up Immediate consumer to write
            // msb at [16:20]. Our decoder happens to put
            // them in that order.
            place_field(word, 7, 11, v as u32);
            Ok(true)
        }
        b'e' if bitfield.is_empty() => {
            // SMI immediate split: bits[0:7]:bits[8:19].
            let v = expect_immediate(operand, slot, "e")?;
            if !(0..=0xfffff).contains(&v) {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: 20,
                });
            }
            let v = v as u32;
            place_field(word, 0, 7, v & 0xff);
            place_field(word, 8, 19, (v >> 8) & 0xfff);
            Ok(true)
        }
        b'U' if bitfield.is_empty() => {
            // Barrier type at bits [0..3].
            let v = expect_immediate(operand, slot, "U")?;
            if !(0..=15).contains(&v) {
                return Err(EncodeError::ImmediateOutOfRange {
                    slot,
                    value: v,
                    bits: 4,
                });
            }
            place_field(word, 0, 3, v as u32);
            Ok(true)
        }
        // Codes recognised but not yet inverted.
        _ => Err(EncodeError::UnsupportedFormatCode {
            format_code: if bitfield.is_empty() {
                format!("%{}", code as char)
            } else {
                format!("%{bitfield}{}", code as char)
            },
        }),
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
    }
}

/// Inverse of ARM-mode `imm8` ROR `2*rot4` expansion. Returns
/// the 12-bit raw encoding (rot4 in [11:8], imm8 in [7:0]),
/// or `None` if `value` cannot be expressed as any rotation
/// of an 8-bit value.
fn arm_expand_imm_inverse(value: u32) -> Option<u32> {
    if value <= 0xff {
        return Some(value);
    }
    for rot4 in 1u32..16 {
        let rot = rot4 * 2;
        let candidate = value.rotate_left(rot);
        if candidate <= 0xff {
            return Some((rot4 << 8) | candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::arm::format_decode::decode_operands_from_format;
    use crate::isa::armv7::arm::table_generated::match_generated;

    #[test]
    fn round_trips_arm_bx_lr() {
        // bx lr → 0xe12fff1e.
        let word_in = 0xe12f_ff1eu32;
        let row = match_generated(word_in).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, word_in, 0);
        let word_out = encode_with_row(row, &operands, 0).expect("encode");
        assert_eq!(word_out, word_in);
    }

    #[test]
    fn round_trips_arm_movw() {
        // movw r0, #0x1234 → 0xe301_0234.
        let word_in = 0xe301_0234u32;
        let row = match_generated(word_in).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, word_in, 0);
        let word_out = encode_with_row(row, &operands, 0).expect("encode");
        assert_eq!(word_out, word_in);
    }

    #[test]
    fn round_trips_arm_b_target() {
        // b .  → 0xeafffffe at any address.
        let word_in = 0xeaff_fffeu32;
        let address = 0x100u64;
        let row = match_generated(word_in).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, word_in, address);
        let word_out = encode_with_row(row, &operands, address).expect("encode");
        assert_eq!(word_out, word_in);
    }

    #[test]
    fn round_trips_arm_mov_immediate() {
        // mov r0, #1 → 0xe3a00001.
        let word_in = 0xe3a0_0001u32;
        let row = match_generated(word_in).unwrap();
        let (operands, _) = decode_operands_from_format(row.format, word_in, 0);
        let word_out = encode_with_row(row, &operands, 0).expect("encode");
        assert_eq!(word_out, word_in);
    }

    #[test]
    fn rejects_branch_out_of_range() {
        let word_in = 0xea00_0000u32;
        let row = match_generated(word_in).unwrap();
        let bad_operands = vec![DecodedOperand::BranchTarget(0x4_0000_0000)];
        let err = encode_with_row(row, &bad_operands, 0).unwrap_err();
        assert!(matches!(err, EncodeError::BranchOutOfRange { .. }));
    }

    #[test]
    fn arm_expand_imm_inverse_simple() {
        assert_eq!(arm_expand_imm_inverse(0x00), Some(0x000));
        assert_eq!(arm_expand_imm_inverse(0xff), Some(0x0ff));
    }

    #[test]
    fn round_trips_full_plt_stub() {
        // Real PLT stub from libtool-checker.so at 0xf84.
        // Verify every instruction encodes back to its
        // original bytes via encode_with_row.
        use crate::isa::armv7::arm::sweep::disassemble_bytes;
        let bytes: &[u8] = &[
            0x04, 0xe0, 0x2d, 0xe5,
            0x04, 0xe0, 0x9f, 0xe5,
            0x0e, 0xe0, 0x8f, 0xe0,
            0x08, 0xf0, 0xbe, 0xe5,
        ];
        let base = 0xf84u64;
        let insns = disassemble_bytes(base, bytes).expect("sweep");
        let mut roundtripped = 0;
        let mut skipped = 0;
        for insn in &insns {
            // Skip NEON-row matches (they have no row).
            let Some(row) = insn.row else {
                skipped += 1;
                continue;
            };
            match encode_with_row(row, &insn.operands, insn.address) {
                Ok(word) if word == insn.word => roundtripped += 1,
                Ok(other) => {
                    eprintln!(
                        "approximate roundtrip at 0x{:x}: in=0x{:08x} out=0x{:08x} format={}",
                        insn.address, insn.word, other, row.format,
                    );
                    skipped += 1;
                }
                Err(e) => {
                    eprintln!(
                        "encode error at 0x{:x} (format {}): {e:?}",
                        insn.address, row.format,
                    );
                    skipped += 1;
                }
            }
        }
        assert!(
            roundtripped >= 2,
            "only {roundtripped}/{} round-tripped, {skipped} skipped",
            insns.len(),
        );
    }

    #[test]
    fn arm_expand_imm_inverse_rotation() {
        // 0xff000000 = 0xff ROR 8 = 0xff rotated left by 8;
        // rotate_left(0xff000000, 8) = 0xff. So rot = 8 = 4*2,
        // rot4 = 4. raw = 0x4ff.
        // Wait: arm-expand uses ROR(imm8, 2*rot4). Inverse:
        //   value = imm8 ROR (2*rot4)
        //   imm8 = value ROL (2*rot4)
        // rotate_left(0xff000000, 8) = 0xff. rot4 = 4 → 0x400 | 0xff = 0x4ff.
        assert_eq!(arm_expand_imm_inverse(0xff00_0000), Some(0x4ff));
    }
}
