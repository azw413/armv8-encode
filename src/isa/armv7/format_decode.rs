//! Decode operands from a binutils Thumb format string + a
//! word value.
//!
//! ## Strategy
//!
//! The opcode table imported from binutils carries a format
//! string per row, e.g.:
//!
//! ```text
//!   "add%C\t%0-2r, %3-5r, %6-8r"
//!   "ldr%c\t%0-2r, [%3-5r, %{I:#%6-10W%}]"
//!   "bl%c\t%B%x"
//! ```
//!
//! At decode time we walk the format string and emit a
//! [`DecodedOperand`] for each operand-bearing escape. Other
//! escapes (condition-code printing, decorative chars,
//! warnings) are skipped — they're disassembler-display
//! concerns rather than rewriter-relevant operands.
//!
//! ## Coverage
//!
//! Supported format codes (cover the bulk of common
//! instructions the rewriter needs):
//!
//! - `%<n>-<m>r`  — bitfield as ARM register (R class)
//! - `%<n>-<m>R`  — bitfield as ARM register, r15 UNPREDICTABLE
//! - `%<n>-<m>S`  — bitfield as ARM register, r13/r15 UNPREDICTABLE
//! - `%<n>-<m>d`  — bitfield as decimal immediate
//! - `%<n>-<m>D`  — bitfield + 1 as decimal
//! - `%<n>-<m>W`  — bitfield * 4 as decimal
//! - `%<n>-<m>H`  — bitfield * 2 as decimal
//! - `%<n>-<m>x`  — bitfield as hex immediate
//! - `%<n>-<m>B`  — Thumb branch destination (signed displacement)
//! - `%<n>-<m>a`  — bitfield * 4 as PC-relative offset
//! - `%S` (16-bit) — Rm in bits 3..5, optionally extended via bit 6
//! - `%D` (16-bit) — Rd in bits 0..2, optionally extended via bit 7
//! - `%I` (32-bit) — 12-bit immediate from hw1[10],hw2[14:12,7:0]
//! - `%M` (32-bit) — modified 12-bit immediate
//! - `%I` (16-bit) — IT instruction suffix (skipped — not an operand)
//! - `%M` (16-bit) — register mask
//! - `%N` — register mask with LR (push)
//! - `%O` — register mask with PC (pop)
//! - `%B` — unconditional branch offset (32-bit BL/B.W form)
//! - `%b` — short conditional branch offset (32-bit conditional B.W;
//!   16-bit CZB form via no-bitfield branch target)
//! - `%K` (32-bit) — 16-bit immediate hw2[3:0]:hw1[3:0]:hw2[11:4]
//! - `%H` (32-bit) — 16-bit immediate hw2[3:0]:hw1[11:0]
//! - `%V` (32-bit) — hvc 16-bit immediate hw1[3:0]:hw2[11:0]
//! - `%S` (32-bit) — shifted Rm register
//! - `%s` (16-bit) — lsr/asr immediate (0 → 32)
//! - `%s` (32-bit) — SSAT/USAT shift immediate
//! - `%a` (32-bit) — load/store address (base + offset, PC-rel form
//!   also emits PcRelative)
//! - `%L` — ldrd/strd PC-relative comment address
//! - `%E` — bfc/bfi (lsb, width)
//! - `%F` — sbfx/ubfx (lsb, width)
//! - `%m` / `%n` — 32-bit register mask (ldm/stm/clrm)
//! - `%U` — barrier type
//! - `%P` / `%Q` — low-overhead loop end / start offsets
//! - `%G` — branch-future fallback offset
//! - `%W` / `%Y` / `%Z` (no bitfield, 32-bit) — BF/BFL/BFCSEL targets
//!
//! Format codes that correspond to display-only artefacts
//! (`%c`, `%C`, `%x`, `%X`, `%{R:foo%}`, `%{I:#...%}`,
//! `%<n>'c`, `%<n>?ab`) are skipped without emitting an
//! operand.
//!
//! Anything not yet supported emits no operand and is
//! recorded internally as a "not handled" miss. Callers that
//! need exhaustive coverage can introspect this via
//! [`DecodeStats`] (returned alongside the operand list) to
//! know which format codes their target binary needs.

use super::operand::{DecodedOperand, Register, RegisterClass};

/// Decode operands by walking the format string.
///
/// The format string is the verbatim binutils format. The
/// decoder emits one [`DecodedOperand`] per recognised
/// operand-bearing escape; non-operand escapes (condition
/// codes, decorative wrappers) are skipped.
///
/// Returns the operand list plus any unrecognised codes the
/// decoder skipped, to make gap-finding easy.
pub fn decode_operands_from_format(
    format: &str,
    word: u32,
    address: u64,
    width_bytes: usize,
) -> (Vec<DecodedOperand>, Vec<String>) {
    let mut operands = Vec::new();
    let mut unhandled = Vec::new();
    let bytes = format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        // Past the '%'.
        i += 1;
        if i >= bytes.len() {
            break;
        }
        // Skip the {R:...%} and {I:...%} display wrappers.
        // They contain inner format escapes that we DO want
        // to walk into.
        if bytes[i] == b'{' {
            // Skip past '{X:' (e.g. '{R:' or '{I:').
            // After that, recurse over the inner content
            // until we hit '%}'.
            let inner_start = i + 1;
            // Find the literal '%}' close marker.
            let mut j = inner_start;
            while j + 1 < bytes.len() {
                if bytes[j] == b'%' && bytes[j + 1] == b'}' {
                    break;
                }
                j += 1;
            }
            // Decode the inner content recursively. Strip the
            // leading "X:" tag bytes (always 2 bytes).
            let inner_str = if inner_start + 2 <= j {
                std::str::from_utf8(&bytes[inner_start + 2..j]).unwrap_or("")
            } else {
                ""
            };
            let (mut inner_ops, mut inner_unh) =
                decode_operands_from_format(inner_str, word, address, width_bytes);
            operands.append(&mut inner_ops);
            unhandled.append(&mut inner_unh);
            i = j + 2; // skip past '%}'
            continue;
        }
        // Bitfield prefix: optional `<digits>-<digits>` or
        // `<digits>` followed by a single-letter code.
        let bitfield_start = i;
        while i < bytes.len()
            && (bytes[i].is_ascii_digit() || bytes[i] == b'-')
        {
            i += 1;
        }
        let bitfield = std::str::from_utf8(&bytes[bitfield_start..i]).unwrap_or("");
        if i >= bytes.len() {
            break;
        }
        let code = bytes[i];
        i += 1;
        // Skip "<bitnum>'c" (print char if bit is one) and
        // "<bitnum>?ab" (print a if bit is one else b) —
        // these aren't operands.
        if code == b'\'' {
            // The next byte is the char to print
            // conditionally.
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        if code == b'?' {
            // Skip the next two bytes (a / b chars).
            if i + 1 < bytes.len() {
                i += 2;
            }
            continue;
        }
        // ` is the binutils "print iff bitfield all zero"
        // marker; skip the following char.
        if code == b'`' {
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        // Dispatch on code letter. `bitfield` is the
        // optional `n-m` prefix (or empty for codes that
        // don't take one).
        match code {
            // Display-only: condition-code, warnings,
            // formatting. Skip.
            b'c' | b'C' | b'x' | b'X' | b'%' | b't' | b'w' | b'W' if bitfield.is_empty() => {
                // None of these take operand fields when
                // they appear without a bitfield prefix.
                // (`%W` *with* a bitfield is the *4 decimal
                // — handled below.)
            }
            // Bitfield-bearing operand codes.
            b'r' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    operands.push(register_operand(word, lo, hi, RegisterClass::R));
                }
            }
            b'R' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    // R: r15 UNPREDICTABLE (we don't enforce
                    // here — preserve the encoded register).
                    operands.push(register_operand(word, lo, hi, RegisterClass::R));
                }
            }
            b'S' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    // S: r13 + r15 UNPREDICTABLE (preserved).
                    operands.push(register_operand(word, lo, hi, RegisterClass::R));
                }
            }
            b'd' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate(v as i64));
                }
            }
            b'D' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate(v as i64 + 1));
                }
            }
            b'W' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate((v * 4) as i64));
                }
            }
            b'H' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate((v * 2) as i64));
                }
            }
            b'x' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate(v as i64));
                }
            }
            b'a' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi) as u64 * 4;
                    // PC-relative: target = (PC & ~3) + 4 + v.
                    // For Thumb, PC reads as the address of
                    // the current instruction + 4, aligned
                    // down to 4 bytes for `ldr Rt, [pc, #x]`-
                    // style references.
                    let pc = (address.wrapping_add(4)) & !3u64;
                    operands.push(DecodedOperand::PcRelative(pc.wrapping_add(v)));
                }
            }
            b'B' => {
                // Thumb branch destination. With a bitfield:
                // signed displacement extracted from
                // `bitfield`. Without a bitfield (32-bit
                // BL/B.W): handled in the caller-aware way
                // below.
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let raw = extract_field(word, lo, hi) as i64;
                    let bits = (hi - lo + 1) as u32;
                    // Sign-extend.
                    let signed = sign_extend(raw, bits);
                    let offset = signed.wrapping_mul(2);
                    let target = (address as i64).wrapping_add(4).wrapping_add(offset);
                    operands.push(DecodedOperand::BranchTarget(target as u64));
                } else {
                    // 32-bit BL / unconditional B.W (`%B` with
                    // no bitfield).
                    if width_bytes == 4 {
                        let target = decode_bl_b_target(word, address);
                        operands.push(DecodedOperand::BranchTarget(target));
                    } else {
                        unhandled.push("%B (16-bit, no bitfield)".into());
                    }
                }
            }
            b'b' if width_bytes == 4 || !bitfield.is_empty() => {
                // Conditional 32-bit B.W (encoding T3):
                //   target = PC + 4 + sign_extend(
                //     S:J2:J1:imm6:imm11:0, 21)
                // Bits in our 32-bit word layout:
                //   S        = bit 26
                //   J1       = bit 13
                //   J2       = bit 11
                //   imm6     = bits 16..21
                //   imm11    = bits 0..10
                if width_bytes == 4 {
                    let target = decode_b_cond_wide_target(word, address);
                    operands.push(DecodedOperand::BranchTarget(target));
                } else if !bitfield.is_empty() {
                    // 16-bit %b is CZB's 6-bit unsigned branch destination.
                    if let Some((lo, hi)) = parse_bitfield(bitfield) {
                        let raw = extract_field(word, lo, hi) as i64;
                        let target = (address as u64).wrapping_add(4).wrapping_add(raw as u64 * 2);
                        operands.push(DecodedOperand::BranchTarget(target));
                    }
                }
            }
            b'S' if bitfield.is_empty() && width_bytes == 2 => {
                // 16-bit %S: Rm in bits 3..5, extended to
                // full register if bit 6 is set.
                let lo3 = ((word >> 3) & 0x7) as u8;
                let hi_bit = ((word >> 6) & 0x1) as u8;
                let reg = (hi_bit << 3) | lo3;
                operands.push(DecodedOperand::Register(Register {
                    class: RegisterClass::R,
                    index: reg,
                }));
            }
            b'D' if bitfield.is_empty() && width_bytes == 2 => {
                // 16-bit %D: Rd in bits 0..2, extended via
                // bit 7.
                let lo3 = (word & 0x7) as u8;
                let hi_bit = ((word >> 7) & 0x1) as u8;
                let reg = (hi_bit << 3) | lo3;
                operands.push(DecodedOperand::Register(Register {
                    class: RegisterClass::R,
                    index: reg,
                }));
            }
            b'M' if width_bytes == 2 => {
                // 16-bit %M: register mask.
                let mask = (word & 0xff) as u16;
                operands.push(DecodedOperand::RegisterList(mask));
            }
            b'N' => {
                // Push: M-bit + 8-bit reglist. M=1 means LR.
                let reglist = (word & 0xff) as u16;
                let m = ((word >> 8) & 0x1) as u16;
                operands.push(DecodedOperand::RegisterList(reglist | (m << 14)));
            }
            b'O' => {
                // Pop: P-bit + 8-bit reglist. P=1 means PC.
                let reglist = (word & 0xff) as u16;
                let p = ((word >> 8) & 0x1) as u16;
                operands.push(DecodedOperand::RegisterList(reglist | (p << 15)));
            }
            b'I' if width_bytes == 4 => {
                // 12-bit Thumb-2 immediate: hw1[10] : hw2[14:12] : hw2[7:0].
                // In our 32-bit word: hw1 = bits 31..16, hw2 = bits 15..0.
                let i_bit = (word >> 26) & 0x1;
                let imm3 = (word >> 12) & 0x7;
                let imm8 = word & 0xff;
                let imm12 = (i_bit << 11) | (imm3 << 8) | imm8;
                operands.push(DecodedOperand::Immediate(imm12 as i64));
            }
            b'M' if width_bytes == 4 => {
                // Modified 12-bit immediate (ThumbExpandImm).
                // Same bit positions as %I; the value is the
                // expanded constant. Implemented per ARM ARM
                // A6.3.2.
                let i_bit = (word >> 26) & 0x1;
                let imm3 = (word >> 12) & 0x7;
                let imm8 = word & 0xff;
                let raw = (i_bit << 11) | (imm3 << 8) | imm8;
                operands.push(DecodedOperand::Immediate(thumb_expand_imm(raw) as i64));
            }
            b'J' if width_bytes == 4 => {
                // 16-bit immediate: hw1[3:0] : hw1[10] : hw2[14:12] : hw2[7:0].
                let imm4 = (word >> 16) & 0xf;
                let i_bit = (word >> 26) & 0x1;
                let imm3 = (word >> 12) & 0x7;
                let imm8 = word & 0xff;
                let imm16 = (imm4 << 12) | (i_bit << 11) | (imm3 << 8) | imm8;
                operands.push(DecodedOperand::Immediate(imm16 as i64));
            }
            b'K' if width_bytes == 4 => {
                // 16-bit immediate: hw2[3:0] : hw1[3:0] : hw2[11:4].
                let a = word & 0xf;
                let b = (word >> 16) & 0xf;
                let c = (word >> 4) & 0xff;
                let imm16 = (a << 12) | (b << 8) | c;
                operands.push(DecodedOperand::Immediate(imm16 as i64));
            }
            b'H' if width_bytes == 4 => {
                // 16-bit immediate: hw2[3:0] : hw1[11:0].
                let a = word & 0xf;
                let b = (word >> 16) & 0xfff;
                let imm16 = (a << 12) | b;
                operands.push(DecodedOperand::Immediate(imm16 as i64));
            }
            b'V' if width_bytes == 4 => {
                // hvc: 16-bit immediate from hw1[3:0]:hw2[11:0].
                let a = (word >> 16) & 0xf;
                let b = word & 0xfff;
                let imm16 = (a << 12) | b;
                operands.push(DecodedOperand::Immediate(imm16 as i64));
            }
            b'S' if bitfield.is_empty() && width_bytes == 4 => {
                // 32-bit %S: shifted Rm. Rm in hw2[3:0],
                // shift type in hw2[5:4], imm5 in
                // hw2[14:12]:hw2[7:6]. We emit the register;
                // the shift is encoded for display only.
                let rm = (word & 0xf) as u8;
                operands.push(DecodedOperand::Register(Register {
                    class: RegisterClass::R,
                    index: rm,
                }));
            }
            b's' if bitfield.is_empty() && width_bytes == 4 => {
                // SSAT/USAT shift: imm5 from hw2[14:12]:hw2[7:6].
                // No operand emitted when shift is zero (lsl #0).
                let imm5 = (((word >> 12) & 0x7) << 2) | ((word >> 6) & 0x3);
                if imm5 != 0 {
                    operands.push(DecodedOperand::Immediate(imm5 as i64));
                }
            }
            b's' if bitfield.is_empty() && width_bytes == 2 => {
                // 16-bit %s: lsr/asr immediate from bits[10:6],
                // where 0 means 32.
                let raw = (word >> 6) & 0x1f;
                let imm = if raw == 0 { 32 } else { raw };
                operands.push(DecodedOperand::Immediate(imm as i64));
            }
            b'b' if bitfield.is_empty() && width_bytes == 2 => {
                // 16-bit %b: CZB (CBZ/CBNZ) target. Address
                // = pc + 4 + ((bits[7:3] << 1) | (bit[9] << 6)).
                let off = (((word >> 3) & 0x1f) << 1) | (((word >> 9) & 0x1) << 6);
                let target = address.wrapping_add(4).wrapping_add(off as u64);
                operands.push(DecodedOperand::BranchTarget(target));
            }
            b'm' | b'n' if width_bytes == 4 => {
                // 32-bit register mask: low 16 bits of hw2.
                let mask = (word & 0xffff) as u16;
                operands.push(DecodedOperand::RegisterList(mask));
            }
            b'E' if bitfield.is_empty() && width_bytes == 4 => {
                // bfc/bfi: lsb (5-bit) and msb (5-bit). lsb =
                // hw2[14:12]:hw2[7:6]; msb = hw2[4:0]. Emits
                // (lsb, width = msb - lsb + 1).
                let msb = word & 0x1f;
                let lsb = (((word >> 12) & 0x7) << 2) | ((word >> 6) & 0x3);
                operands.push(DecodedOperand::Immediate(lsb as i64));
                let width = msb.saturating_sub(lsb).wrapping_add(1);
                operands.push(DecodedOperand::Immediate(width as i64));
            }
            b'F' if bitfield.is_empty() && width_bytes == 4 => {
                // sbfx/ubfx: lsb (5-bit) and width = (5-bit
                // value) + 1. Same lsb layout as %E.
                let widthm1 = word & 0x1f;
                let lsb = (((word >> 12) & 0x7) << 2) | ((word >> 6) & 0x3);
                operands.push(DecodedOperand::Immediate(lsb as i64));
                operands.push(DecodedOperand::Immediate((widthm1 + 1) as i64));
            }
            b'L' if bitfield.is_empty() && width_bytes == 4 => {
                // ldrd/strd PC-relative comment. Only emits
                // an operand when Rn=PC. offset = imm8*4,
                // sign from U bit (bit 23 of hw1 → bit 23 of
                // hw1's high half = bit 39, but `given` here is
                // 32-bit packed → bit 23 of word).
                let rn = (word >> 16) & 0xf;
                if rn == 0xf {
                    let imm8 = word & 0xff;
                    let u = (word >> 23) & 0x1;
                    let off = (imm8 * 4) as i64;
                    let signed = if u == 1 { off } else { -off };
                    let pc_aligned = address & !3u64;
                    let target = (pc_aligned as i64)
                        .wrapping_add(4)
                        .wrapping_add(signed) as u64;
                    operands.push(DecodedOperand::PcRelative(target));
                }
            }
            b'a' if bitfield.is_empty() && width_bytes == 4 => {
                // 32-bit %a: load/store address [Rn, #±imm].
                // We emit the base register and the signed
                // offset (when present). For PC-relative
                // forms (Rn=15), also emit a PcRelative
                // pointer to the literal.
                let rn = ((word >> 16) & 0xf) as u8;
                let u = (word >> 23) & 0x1;
                let i12 = word & 0xfff;
                let i8 = word & 0xff;
                let op = (word >> 8) & 0xf;
                operands.push(DecodedOperand::Register(Register {
                    class: RegisterClass::R,
                    index: rn,
                }));
                let offset: i64 = if u == 1 {
                    i12 as i64
                } else if rn == 15 {
                    -(i12 as i64)
                } else if op == 0 {
                    // Shifted register form — emit Rm only.
                    let rm = (i8 & 0xf) as u8;
                    operands.push(DecodedOperand::Register(Register {
                        class: RegisterClass::R,
                        index: rm,
                    }));
                    0
                } else {
                    // 8-bit immediate variants (op = E/C/F/D/B/9):
                    // sign determined by op.
                    match op {
                        0xE | 0xF | 0xB => i8 as i64,
                        0xC | 0xD | 0x9 => -(i8 as i64),
                        _ => 0,
                    }
                };
                if offset != 0 || u == 1 {
                    operands.push(DecodedOperand::Immediate(offset));
                }
                if rn == 15 {
                    let pc_aligned = address & !3u64;
                    let target = (pc_aligned as i64)
                        .wrapping_add(4)
                        .wrapping_add(offset) as u64;
                    operands.push(DecodedOperand::PcRelative(target));
                }
            }
            b'U' if bitfield.is_empty() && width_bytes == 4 => {
                // Barrier type: hw2[3:0]. Encoded as a small
                // immediate so consumers can map to barrier
                // names if needed.
                let kind = (word & 0xf) as i64;
                operands.push(DecodedOperand::Immediate(kind));
            }
            b'P' if bitfield.is_empty() && width_bytes == 4 => {
                // Low Overhead Loop end offset: pc + 4 -
                // ((immh << 2) | (imml << 1)). immh =
                // hw2[10:1], imml = hw2[11].
                let immh = (word >> 1) & 0x3ff;
                let imml = (word >> 11) & 0x1;
                let off = ((immh << 2) | (imml << 1)) as i64;
                let target = (address as i64).wrapping_add(4).wrapping_sub(off) as u64;
                operands.push(DecodedOperand::BranchTarget(target));
            }
            b'Q' if bitfield.is_empty() && width_bytes == 4 => {
                // Low Overhead Loop start offset: pc + 4 +
                // ((immh << 2) | (imml << 1)).
                let immh = (word >> 1) & 0x3ff;
                let imml = (word >> 11) & 0x1;
                let off = ((immh << 2) | (imml << 1)) as i64;
                let target = (address as i64).wrapping_add(4).wrapping_add(off) as u64;
                operands.push(DecodedOperand::BranchTarget(target));
            }
            b'G' if bitfield.is_empty() && width_bytes == 4 => {
                // Branch Future fallback offset: bits 23..26
                // shifted left by 1.
                let off = ((word >> 23) & 0xf) << 1;
                operands.push(DecodedOperand::Immediate(off as i64));
            }
            b'W' | b'Y' | b'Z' if bitfield.is_empty() && width_bytes == 4 => {
                // BF/BFL/BFCSEL: signed offset assembled
                // from immA (top bits of hw1), immB
                // (hw2[10:1]), immC (hw2[11]). Width of
                // immA differs per variant: %W uses 5 bits
                // (hw1[20:16]), %Y uses 7 bits (hw1[22:16]),
                // %Z uses 1 bit (hw1[16]).
                let imm_b = (word >> 1) & 0x3ff;
                let imm_c = (word >> 11) & 0x1;
                let (imm_a_bits, sign_bit_pos) = match code {
                    b'W' => (((word >> 16) & 0x1f) << 12, 16usize),
                    b'Y' => (((word >> 16) & 0x7f) << 12, 18usize),
                    b'Z' => (((word >> 16) & 0x1) << 12, 12usize),
                    _ => unreachable!(),
                };
                let raw = imm_a_bits | (imm_b << 2) | (imm_c << 1);
                let offset = if (raw >> sign_bit_pos) & 0x1 == 1 {
                    raw as i64 - (1i64 << (sign_bit_pos + 1))
                } else {
                    raw as i64
                };
                let target = (address as i64).wrapping_add(4).wrapping_add(offset) as u64;
                operands.push(DecodedOperand::BranchTarget(target));
            }
            // Ignore display-only codes that occasionally
            // appear without bitfield prefixes.
            _ => {
                let prefix = if bitfield.is_empty() {
                    String::new()
                } else {
                    bitfield.to_string()
                };
                unhandled.push(format!("%{prefix}{}", code as char));
            }
        }
    }
    (operands, unhandled)
}

fn parse_bitfield(s: &str) -> Option<(u8, u8)> {
    if s.is_empty() {
        return None;
    }
    if let Some((lo_s, hi_s)) = s.split_once('-') {
        let lo: u8 = lo_s.parse().ok()?;
        let hi: u8 = hi_s.parse().ok()?;
        // binutils convention: the field name is "lo-hi"
        // with lo <= hi, and the field is bits [hi:lo] of the
        // word.
        if lo > hi {
            return None;
        }
        Some((lo, hi))
    } else {
        let n: u8 = s.parse().ok()?;
        Some((n, n))
    }
}

fn extract_field(word: u32, lo: u8, hi: u8) -> u32 {
    let width = hi - lo + 1;
    let mask = if width >= 32 {
        u32::MAX
    } else {
        (1u32 << width) - 1
    };
    (word >> lo) & mask
}

fn sign_extend(value: i64, bits: u32) -> i64 {
    if bits == 0 || bits >= 64 {
        return value;
    }
    let shift = 64 - bits as i64;
    (value << shift) >> shift
}

fn register_operand(word: u32, lo: u8, hi: u8, class: RegisterClass) -> DecodedOperand {
    let v = extract_field(word, lo, hi) as u8;
    DecodedOperand::Register(Register { class, index: v })
}

/// Decode the BL / unconditional B.W target. Same logic as
/// the hand-written `Bl24` shape in `table.rs` — reproduced
/// here for the format-driven decoder.
fn decode_bl_b_target(word: u32, address: u64) -> u64 {
    let s = (word >> 26) & 0x1;
    let imm10 = (word >> 16) & 0x3ff;
    let j1 = (word >> 13) & 0x1;
    let j2 = (word >> 11) & 0x1;
    let imm11 = word & 0x7ff;
    let i1 = !(j1 ^ s) & 0x1;
    let i2 = !(j2 ^ s) & 0x1;
    let raw = (s << 24) | (i1 << 23) | (i2 << 22) | (imm10 << 12) | (imm11 << 1);
    let signed = ((raw << 7) as i32) >> 7;
    address.wrapping_add(4).wrapping_add(signed as u64)
}

/// Decode the conditional B.W (T3) target. 21-bit signed
/// displacement encoded as S:J2:J1:imm6:imm11:0.
fn decode_b_cond_wide_target(word: u32, address: u64) -> u64 {
    let s = (word >> 26) & 0x1;
    let j1 = (word >> 13) & 0x1;
    let j2 = (word >> 11) & 0x1;
    let imm6 = (word >> 16) & 0x3f;
    let imm11 = word & 0x7ff;
    // Concatenation: S:J2:J1:imm6:imm11:0  → 21 bits total.
    let raw =
        (s << 19) | (j2 << 18) | (j1 << 17) | (imm6 << 11) | imm11;
    let signed = sign_extend(raw as i64, 20).wrapping_mul(2);
    address.wrapping_add(4).wrapping_add(signed as u64)
}

/// ThumbExpandImm — expand a 12-bit Thumb-2 modified
/// immediate per ARM ARM A6.3.2.
///
/// Bits 11..10 select the expansion form:
///   00 → various replication patterns based on bits 9..8.
///   01..11 → 8-bit value rotated right by `unrotated_value`.
fn thumb_expand_imm(imm12: u32) -> u32 {
    let imm12 = imm12 & 0xfff;
    let high2 = (imm12 >> 10) & 0x3;
    let low8 = imm12 & 0xff;
    if high2 == 0 {
        let case = (imm12 >> 8) & 0x3;
        match case {
            0 => low8,
            1 => (low8 << 16) | low8,
            2 => (low8 << 24) | (low8 << 8),
            3 => (low8 << 24) | (low8 << 16) | (low8 << 8) | low8,
            _ => unreachable!(),
        }
    } else {
        // unrotated_value = 1:imm8 (top bit forced to 1) of
        // the lower 7 bits.
        let unrotated = 0x80 | (imm12 & 0x7f);
        let rot = (imm12 >> 7) & 0x1f;
        unrotated.rotate_right(rot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::operand::DecodedOperand as DO;

    #[test]
    fn parse_bitfield_handles_range_and_single() {
        assert_eq!(parse_bitfield("0-2"), Some((0, 2)));
        assert_eq!(parse_bitfield("8-10"), Some((8, 10)));
        assert_eq!(parse_bitfield("3"), Some((3, 3)));
        assert_eq!(parse_bitfield(""), None);
        assert_eq!(parse_bitfield("5-2"), None); // lo > hi rejected
    }

    #[test]
    fn extract_field_pulls_bits() {
        // word = 0b____1101_0010 = 0xd2. Bits [3:0] = 0010 = 2.
        // Bits [7:4] = 1101 = 13.
        assert_eq!(extract_field(0xd2, 0, 3), 2);
        assert_eq!(extract_field(0xd2, 4, 7), 13);
        assert_eq!(extract_field(0xd2, 0, 7), 0xd2);
    }

    #[test]
    fn sign_extend_extends_negative_values() {
        // 8-bit 0xff = -1
        assert_eq!(sign_extend(0xff, 8), -1);
        // 8-bit 0x7f = 127 (positive)
        assert_eq!(sign_extend(0x7f, 8), 127);
        // 11-bit 0x7ff = -1
        assert_eq!(sign_extend(0x7ff, 11), -1);
    }

    #[test]
    fn decodes_thumb1_add_three_low_registers() {
        // add r3, r4, r5 — encoding: 0001_1001_0110_0011 = 0x1963
        // Format: "add%C\t%0-2r, %3-5r, %6-8r"
        let (operands, unhandled) =
            decode_operands_from_format("add%C\t%0-2r, %3-5r, %6-8r", 0x1963, 0, 2);
        assert!(unhandled.is_empty(), "unexpected unhandled: {unhandled:?}");
        assert_eq!(operands.len(), 3);
        match (&operands[0], &operands[1], &operands[2]) {
            (DO::Register(rd), DO::Register(rn), DO::Register(rm)) => {
                assert_eq!(rd.index, 3);
                assert_eq!(rn.index, 4);
                assert_eq!(rm.index, 5);
            }
            _ => panic!("unexpected operand shape: {operands:?}"),
        }
    }

    #[test]
    fn decodes_thumb1_mov_immediate() {
        // mov r2, #0x37 → 0x2237. Format: "mov%C\t%8-10r, %{I:#%0-7d%}"
        let (operands, _) =
            decode_operands_from_format("mov%C\t%8-10r, %{I:#%0-7d%}", 0x2237, 0, 2);
        assert_eq!(operands.len(), 2);
        match (&operands[0], &operands[1]) {
            (DO::Register(rd), DO::Immediate(imm)) => {
                assert_eq!(rd.index, 2);
                assert_eq!(*imm, 0x37);
            }
            _ => panic!("unexpected: {operands:?}"),
        }
    }

    #[test]
    fn decodes_thumb1_ldr_word_scaled_offset() {
        // ldr r0, [r1, #4] → 0x6848. Format with %W *4 scaling.
        // binutils format: "ldr%c\t%0-2r, [%3-5r, %{I:#%6-10W%}]"
        let (operands, _) = decode_operands_from_format(
            "ldr%c\t%0-2r, [%3-5r, %{I:#%6-10W%}]",
            0x6848,
            0,
            2,
        );
        assert_eq!(operands.len(), 3);
        match (&operands[0], &operands[1], &operands[2]) {
            (DO::Register(rt), DO::Register(rn), DO::Immediate(imm)) => {
                assert_eq!(rt.index, 0);
                assert_eq!(rn.index, 1);
                assert_eq!(*imm, 4); // imm5=1, *4 = 4
            }
            _ => panic!("unexpected: {operands:?}"),
        }
    }

    #[test]
    fn decodes_thumb1_branch_target() {
        // beq +6: imm8=3 → word 0xd003. Format: "b%8-11c.n\t%0-7B%X"
        // Note: condition `c` is decoder-display; only the
        // branch target operand (`%0-7B`) is emitted.
        let (operands, _) = decode_operands_from_format(
            "b%8-11c.n\t%0-7B%X",
            0xd003,
            0x2000,
            2,
        );
        assert_eq!(operands.len(), 1);
        match &operands[0] {
            DO::BranchTarget(t) => assert_eq!(*t, 0x2000 + 4 + 6),
            _ => panic!("expected BranchTarget, got {operands:?}"),
        }
    }

    #[test]
    fn decodes_unconditional_branch_target_thumb1() {
        // b +8 (imm11=4) → 0xe004. Format: "b%c.n\t%0-10B%x"
        let (operands, _) =
            decode_operands_from_format("b%c.n\t%0-10B%x", 0xe004, 0x1000, 2);
        assert_eq!(operands.len(), 1);
        match &operands[0] {
            DO::BranchTarget(t) => assert_eq!(*t, 0x1000 + 4 + 8),
            _ => panic!("expected BranchTarget, got {operands:?}"),
        }
    }

    #[test]
    fn decodes_bl_target() {
        // BL with target +4: word 0xf000_f802. Format: "bl%c\t%B%x"
        let (operands, _) =
            decode_operands_from_format("bl%c\t%B%x", 0xf000_f802, 0x1000, 4);
        assert_eq!(operands.len(), 1);
        match &operands[0] {
            DO::BranchTarget(t) => assert_eq!(*t, 0x1000 + 4 + 4),
            _ => panic!("expected BranchTarget, got {operands:?}"),
        }
    }

    #[test]
    fn decodes_push_register_list() {
        // push {r0, lr} → 0xb501. Format: "push%c\t%N"
        let (operands, _) = decode_operands_from_format("push%c\t%N", 0xb501, 0, 2);
        assert_eq!(operands.len(), 1);
        match &operands[0] {
            // Bit 0 (r0) and bit 14 (LR placeholder) set.
            DO::RegisterList(mask) => {
                assert!(*mask & 0x1 != 0, "r0 should be in list");
                assert!(*mask & 0x4000 != 0, "LR bit (14) should be in list");
            }
            _ => panic!("expected RegisterList, got {operands:?}"),
        }
    }

    #[test]
    fn decodes_pop_register_list_with_pc() {
        // pop {r0, pc} → 0xbd01. Format: "pop%c\t%O"
        let (operands, _) = decode_operands_from_format("pop%c\t%O", 0xbd01, 0, 2);
        assert_eq!(operands.len(), 1);
        match &operands[0] {
            DO::RegisterList(mask) => {
                assert!(*mask & 0x1 != 0);
                assert!(*mask & 0x8000 != 0, "PC bit (15) should be in list");
            }
            _ => panic!("{operands:?}"),
        }
    }

    #[test]
    fn decodes_thumb2_mov_immediate_w_form() {
        // mov.w r0, #1 → 0xf04f_0001. Format:
        //   "mov%20's%c.w\t%8-11r, %M"
        // We expect Rd=0 (bits 11..8 of low halfword) and
        // immediate=1 (modified-imm form yields the constant 1).
        let (operands, unhandled) = decode_operands_from_format(
            "mov%20's%c.w\t%8-11r, %M",
            0xf04f_0001,
            0,
            4,
        );
        assert!(unhandled.is_empty(), "unhandled: {unhandled:?}");
        assert_eq!(operands.len(), 2);
        match (&operands[0], &operands[1]) {
            (DO::Register(rd), DO::Immediate(imm)) => {
                assert_eq!(rd.index, 0);
                assert_eq!(*imm, 1);
            }
            _ => panic!("unexpected: {operands:?}"),
        }
    }

    #[test]
    fn thumb_expand_imm_constant_form() {
        // imm12 = 0b00000_0000001 (case 0): just low8=1.
        assert_eq!(thumb_expand_imm(0b0000_0000_0001), 1);
        // imm12 = 0b0001_0000_0001 (case 1): 0x00010001
        assert_eq!(thumb_expand_imm(0b0001_0000_0001), 0x0001_0001);
    }

    #[test]
    fn thumb_expand_imm_rotation_form() {
        // High 2 bits non-zero → rotation. imm12 = 0x880 =
        // 0b1000_1000_0000. Bits [11:7] = 0b10001 = 17,
        // low 7 = 0 → unrotated = 0x80 (bit 7 forced).
        // 0x80 ROR 17 = bit 7 → bit (7-17) mod 32 = bit 22.
        assert_eq!(thumb_expand_imm(0x880), 0x0040_0000);
    }

    #[test]
    fn skips_display_only_codes_without_emitting_operands() {
        // Format with only %c, %x, %X — no operand fields.
        let (operands, unhandled) =
            decode_operands_from_format("nop%c%x%X", 0xbf00, 0, 2);
        assert!(operands.is_empty(), "expected no operands, got {operands:?}");
        assert!(unhandled.is_empty(), "unhandled: {unhandled:?}");
    }

    #[test]
    fn decodes_pcrel_word_load() {
        // ldr r2, [pc, #16]: format like "ldr%c\t%8-10r, [%{R:pc%}, %{I:#%0-7W%}]\t@ (%0-7a)"
        // For our purposes the %0-7a is the PC-relative form.
        // word with imm8=4 (so #16 offset, PC-aligned).
        // Encoding: 01001 ttt iiiiiiii. Rt=2, imm8=4 →
        // 0b0100_1010_0000_0100 = 0x4a04
        let (operands, _) = decode_operands_from_format(
            "ldr%c\t%8-10r, %{I:[%{R:pc%}, #%0-7W%]%}\t@ (%0-7a)",
            0x4a04,
            0x100,
            2,
        );
        // Expect Rt=2, an immediate (imm8*4=16), AND a
        // PcRelative pointer ((PC+4 & ~3) + 16 = 0x100+4 & ~3
        // + 16 = 0x100 + 16 = 0x110 since 0x100 is already
        // 4-aligned and PC reads as +4 → 0x104 & ~3 = 0x104).
        let pcrel = operands
            .iter()
            .find(|o| matches!(o, DO::PcRelative(_)))
            .expect("expected PcRelative operand");
        match pcrel {
            DO::PcRelative(target) => {
                // PC = (0x100 + 4) & !3 = 0x104. + (4*4 = 16) = 0x114.
                assert_eq!(*target, 0x114);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn decodes_thumb1_lsr_immediate_shift() {
        // lsr r0, r1, #5 → 0b0000_1_00101_001_000 = 0x0948.
        // Format: "lsr%C\t%0-2r, %3-5r, %s"
        let (operands, unhandled) =
            decode_operands_from_format("lsr%C\t%0-2r, %3-5r, %s", 0x0948, 0, 2);
        assert!(unhandled.is_empty(), "unhandled: {unhandled:?}");
        assert_eq!(operands.len(), 3);
        match &operands[2] {
            DO::Immediate(imm) => assert_eq!(*imm, 5),
            _ => panic!("expected immediate, got {operands:?}"),
        }
    }

    #[test]
    fn decodes_thumb1_lsr_zero_means_thirty_two() {
        // imm5=0 in lsr-imm encoding decodes as #32.
        // Format bits: 00001 imm5(10..6) rs(5..3) rd(2..0).
        // imm5=0, rs=1, rd=0 → 0b0000_1_00000_001_000 = 0x0808.
        let (operands, _) =
            decode_operands_from_format("lsr%C\t%0-2r, %3-5r, %s", 0x0808, 0, 2);
        match operands.last().unwrap() {
            DO::Immediate(imm) => assert_eq!(*imm, 32),
            _ => panic!(),
        }
    }

    #[test]
    fn decodes_thumb2_register_mask() {
        // ldmia.w r0, {r1, r3, r5} → reglist mask = 0b00101010 = 0x002a.
        // Word: hw1 = 0xe890, hw2 = 0x002a → 0xe8900_02a.
        let (operands, _) = decode_operands_from_format(
            "ldmia%c.w\t%16-19r%21'!, %m",
            0xe890_002a,
            0,
            4,
        );
        // Operands: Rn (0), reglist mask.
        assert!(operands.iter().any(|o| matches!(
            o, DO::RegisterList(m) if *m == 0x002a
        )), "operands: {operands:?}");
    }

    #[test]
    fn decodes_bfi_lsb_and_width() {
        // bfi r0, r1, #4, #8 → lsb=4, width=8, msb=11.
        // Encoding: hw1 = 0xf361, hw2 = imm3=001 imm2=00 + msb=01011 = 0x010b.
        // hw2 layout: imm3 (14:12) << 12 | xx (11) | imm2 (7:6) | msb (4:0).
        // imm3=1, imm2=0, msb=11 → hw2 = (1<<12) | (0<<6) | 11 = 0x100b.
        // Actually: imm3<<12 | imm2<<6 | msb = 0x1000 | 0 | 11 = 0x100b.
        let word: u32 = (0xf361u32 << 16) | 0x100b;
        let (operands, _) = decode_operands_from_format(
            "bfi%c\t%8-11r, %16-19r, %E",
            word,
            0,
            4,
        );
        // Operands: Rd, Rn, lsb, width.
        let imms: Vec<i64> = operands.iter().filter_map(|o| match o {
            DO::Immediate(v) => Some(*v),
            _ => None,
        }).collect();
        assert_eq!(&imms[..], &[4, 8], "imms: {imms:?}, ops: {operands:?}");
    }

    #[test]
    fn decodes_ubfx_lsb_and_width() {
        // ubfx r0, r1, #2, #5 → lsb=2, width=5 (encoded widthm1=4).
        // hw1 = 0xf3c1; hw2 = imm3=000 imm2=10 widthm1=00100 →
        // (0<<12) | (2<<6) | 4 = 0x84.
        let word: u32 = (0xf3c1u32 << 16) | 0x0084;
        let (operands, _) = decode_operands_from_format(
            "ubfx%c\t%8-11r, %16-19r, %F",
            word,
            0,
            4,
        );
        let imms: Vec<i64> = operands.iter().filter_map(|o| match o {
            DO::Immediate(v) => Some(*v),
            _ => None,
        }).collect();
        assert_eq!(&imms[..], &[2, 5]);
    }
}
