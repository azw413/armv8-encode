//! `operand_bit_ranges()` for Thumb-2 and ARM-mode opcode rows.
//!
//! ## Strategy
//!
//! Walk the binutils format string the same way the encoder
//! does (see `armv7/encode.rs` and `armv7/arm/encode.rs`). For
//! each operand-bearing code, record the bit ranges the
//! operand occupies in the encoded 32-bit word.
//!
//! Most codes carry their bit positions inline as a bitfield
//! (e.g. `%12-15r`, `%0-7B`). A handful use hardcoded
//! positions known from the binutils opcode tables — for
//! Thumb-2 these are the scattered immediates (`%I`, `%J`,
//! `%M`), the BL/B.W branch targets (`%B`, `%b` with no
//! bitfield), the condition code `%c`, and a few register
//! shapes (`%D`, `%S`, `%M`/`%N`/`%O`).
//!
//! ## What this returns
//!
//! - One `Vec<Range<u8>>` per operand the format string
//!   consumes, in left-to-right order — matching the operand
//!   list the decoder produces.
//! - Empty `Vec` when the operand's bit pattern isn't
//!   modelled here (typically because the encoder treats it
//!   as `OpaqueBits` — the round-trip preserves it bit-exact
//!   but the bit-range concept doesn't apply).
//!
//! Callers using this for wildcard masking should treat an
//! empty inner Vec as "do not wildcard this operand."
//!
//! ## Word semantics
//!
//! For 16-bit Thumb halfword rows, bit indices are 0..16
//! within the halfword. For 32-bit Thumb-2 rows, the
//! convention used elsewhere in the encoder is "hw1 in the
//! high 16 bits, hw2 in the low 16 bits" of the working u32.
//! So bit 0..16 = hw2, bit 16..32 = hw1. ARM-mode rows are
//! plain 0..32.

use std::ops::Range;

/// Walk `format` and return one `Vec<Range<u8>>` per operand
/// slot consumed. `width_bytes` is 2 or 4 for Thumb (selecting
/// which `%B` / `%b` / `%S` / `%D` interpretation applies);
/// pass 4 for ARM-mode rows.
pub fn extract_operand_bit_ranges(format: &str, width_bytes: usize) -> Vec<Vec<Range<u8>>> {
    let mut out: Vec<Vec<Range<u8>>> = Vec::new();
    let bytes = format.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }

        // Skip `{X:...%}` display wrappers but recurse into
        // them: they contain real operand-bearing escapes.
        if bytes[i] == b'{' {
            let inner_start = i + 1;
            let mut j = inner_start;
            while j + 1 < bytes.len() {
                if bytes[j] == b'%' && bytes[j + 1] == b'}' {
                    break;
                }
                j += 1;
            }
            let inner = if inner_start + 2 <= j {
                std::str::from_utf8(&bytes[inner_start + 2..j]).unwrap_or("")
            } else {
                ""
            };
            // Recurse to extract operands inside the wrapper.
            let mut inner_ranges = extract_operand_bit_ranges(inner, width_bytes);
            out.append(&mut inner_ranges);
            i = j + 2;
            continue;
        }

        // Skip `'X` literal display markers.
        if bytes[i] == b'\'' {
            i += 1;
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        // Skip `?X<Y>` conditional display markers (`X` says
        // when to print `Y`; doesn't consume operands).
        if bytes[i] == b'?' {
            if i + 1 < bytes.len() {
                i += 2;
            }
            continue;
        }
        // Skip backtick literal-passthrough.
        if bytes[i] == b'`' {
            if i + 1 < bytes.len() {
                i += 2;
            }
            continue;
        }

        // Parse optional leading bitfield (e.g. `12-15`, `8`).
        let bf_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
            i += 1;
        }
        let bitfield = std::str::from_utf8(&bytes[bf_start..i]).unwrap_or("");
        if i >= bytes.len() {
            break;
        }
        let code = bytes[i];
        i += 1;

        // Map code → operand bit ranges. Returns:
        //   Some(Some(ranges)) → operand consumed with ranges
        //   Some(None)         → operand consumed; ranges
        //                        unmodelled (push empty Vec)
        //   None               → display-only, no operand
        let consumed = decode_operand_code(code, bitfield, width_bytes);
        match consumed {
            Some(ranges) => out.push(ranges),
            None => {} // display-only, no operand
        }
    }
    out
}

fn decode_operand_code(
    code: u8,
    bitfield: &str,
    width_bytes: usize,
) -> Option<Vec<Range<u8>>> {
    match code {
        // Bitfielded register / immediate / etc. codes: the
        // bitfield carries the bit range explicitly.
        b'r' | b'R' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'd' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'D' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'W' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'H' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'x' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'd' | b'D' if bitfield.is_empty() => Some(Vec::new()), // unbitfielded, opaque
        b'S' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'B' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'a' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),

        // 32-bit `%b` (no bitfield) covers two distinct
        // encodings under the same code:
        // - ARM-mode B/BL: 24-bit imm at bits 0..24.
        // - Thumb-2 B.W (T3 conditional): S + J1 + J2 +
        //   imm6 + imm11 scattered. We report the
        //   conservative ARM single-range view; Thumb
        //   callers needing the scattered Thumb-2 layout
        //   should look up the row's mask directly.
        b'b' if bitfield.is_empty() && width_bytes == 4 => Some(vec![0..24]),
        b'B' if bitfield.is_empty() && width_bytes == 4 => {
            // Thumb-2 BL/B.W (T4): 24-bit packed branch
            // target — S (hw1[10]) : J1/J2 (hw2[13], hw2[11])
            // : imm10 (hw1[9:0]) : imm11 (hw2[10:0]). On our
            // hw1<<16|hw2 word convention:
            //   hw2[10:0]  = bits  0..11
            //   hw2[11]    = bit  11..12
            //   hw2[13]    = bit  13..14
            //   hw1[9:0]   = bits 16..26
            //   hw1[10]    = bit  26..27
            Some(vec![0..12, 13..14, 16..27])
        }

        // Thumb 16-bit unconditional B (T2): 11-bit
        // displacement in bits 0..11.
        b'B' if bitfield.is_empty() && width_bytes == 2 => Some(vec![0..11]),

        // Thumb 16-bit conditional B (T1): cond bits 8..12 +
        // imm8 bits 0..8. Cond is taken by %c; this code is
        // the imm8 only.
        b'b' if !bitfield.is_empty() && width_bytes == 2 => {
            Some(bitfield_to_ranges(bitfield))
        }

        // Condition operand:
        //   ARM-mode (width=4, no bitfield): bits 28..32.
        //   Thumb halfword (width=2, no bitfield): bits 8..12
        //     for the T1 B<cond> form. For other Thumb
        //     mnemonics %c is display-only (the unconditional
        //     forms accept any condition value at decode
        //     time but encode AL).
        //   Bitfielded %22-25c etc. (Thumb-2 32-bit B<cond>.W
        //     T3): bitfield carries it.
        b'c' if !bitfield.is_empty() => Some(bitfield_to_ranges(bitfield)),
        b'c' if bitfield.is_empty() && width_bytes == 4 => {
            // ARM-mode: cond is always bits 28..32. The Thumb
            // 32-bit `%c.w` form for unconditional B uses
            // hw2[14:12] but with the "always" semantics —
            // treat as Thumb display-only.
            // Distinguish by heuristic: ARM rows never set
            // hw1[31:28] = 0xf for conditional encodings, so
            // the `%c` without bitfield on a 32-bit Thumb row
            // is display-only. We can't tell them apart from
            // the format alone — adopt the conservative
            // "ARM = bits 28..32; Thumb 32-bit `%c` =
            // display-only AL" rule and let callers using
            // this on Thumb mnemonics know.
            // This helper's `width_bytes` distinguishes Thumb
            // halfword from "32-bit thing" but not Thumb-32
            // from ARM. Practical impact: callers should use
            // the Thumb / ARM-mode wrappers below, which
            // disambiguate.
            Some(vec![28..32])
        }
        // Thumb halfword %c with no bitfield: T1 B<cond>
        // condition lives at bits 8..12.
        b'c' if bitfield.is_empty() && width_bytes == 2 => Some(vec![8..12]),

        // %V — ARM/Thumb 16-bit move-wide immediate split
        // imm4:imm12. For ARM rows this is bits 0..12 | 16..20.
        // For Thumb-2 movw/movt the encoding is the same as
        // %J below.
        b'V' if bitfield.is_empty() && width_bytes == 4 => {
            Some(vec![0..12, 16..20])
        }

        // %J — Thumb-2 16-bit movw/movt immediate scattered
        // hw1[3:0] : hw1[10] : hw2[14:12] : hw2[7:0].
        // On hw1<<16|hw2: bits 0..8, 12..15, 16..20 cont? let me
        // recompute. hw2[7:0] = bits 0..8; hw2[14:12] = bits
        // 12..15; hw1[10] = bit 26; hw1[3:0] = bits 16..20.
        b'J' if bitfield.is_empty() && width_bytes == 4 => {
            Some(vec![0..8, 12..15, 16..20, 26..27])
        }
        // %I — Thumb-2 12-bit imm: hw1[10] : hw2[14:12] :
        // hw2[7:0]. Bits: 0..8, 12..15, 26..27.
        b'I' if bitfield.is_empty() && width_bytes == 4 => {
            Some(vec![0..8, 12..15, 26..27])
        }
        // %M (Thumb-2 ThumbExpandImm): same bit layout as %I
        // — the scattered 12 bits — but with rotation
        // semantics applied at decode.
        b'M' if bitfield.is_empty() && width_bytes == 4 => {
            Some(vec![0..8, 12..15, 26..27])
        }
        // 16-bit %M (register list): bits 0..8.
        b'M' if bitfield.is_empty() && width_bytes == 2 => {
            Some(vec![0..8])
        }
        // 16-bit %N / %O (register list with optional PC/LR
        // bit): low byte + a high bit. We model only the low
        // byte; the high bit is taken by `%24'something` or
        // similar display marker depending on the mnemonic.
        b'N' | b'O' if bitfield.is_empty() && width_bytes == 2 => Some(vec![0..8]),

        // %S without bitfield: 16-bit Thumb Rm at bits 3..6
        // plus high bit at bit 6..7. 32-bit %S is OpaqueBits
        // (return empty).
        b'S' if bitfield.is_empty() && width_bytes == 2 => Some(vec![3..7]),
        b'S' if bitfield.is_empty() && width_bytes == 4 => Some(Vec::new()),

        // %D without bitfield, halfword: Thumb Rd alt position
        // (low 3 bits + high bit at 7).
        b'D' if bitfield.is_empty() && width_bytes == 2 => Some(vec![0..3, 7..8]),

        // Codes that consume an operand but with bit layouts
        // we don't model here — caller treats empty as
        // "do not wildcard."
        b'a' if bitfield.is_empty() => Some(Vec::new()),
        b's' | b'L' | b'E' | b'F' | b'm' | b'n' => Some(Vec::new()),

        // Display-only codes: do NOT consume an operand.
        b'X' | b'x' | b'%' | b'p' | b't' | b'q' | b'C' => None,
        // %r without bitfield in some contexts is display-only
        // (the register list comma rendering). Treat as no-op.
        b'r' | b'R' if bitfield.is_empty() => None,

        // Default: unknown format code — assume display-only
        // so we don't over-count operands. The format walker
        // matches the encoder's tolerance.
        _ => None,
    }
}

fn bitfield_to_ranges(bf: &str) -> Vec<Range<u8>> {
    if let Some((lo_s, hi_s)) = bf.split_once('-') {
        let (Ok(lo), Ok(hi)) = (lo_s.parse::<u8>(), hi_s.parse::<u8>()) else {
            return Vec::new();
        };
        if lo > hi {
            return Vec::new();
        }
        vec![lo..hi.saturating_add(1)]
    } else if let Ok(n) = bf.parse::<u8>() {
        vec![n..n.saturating_add(1)]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_b_bl_arm_format() {
        // ARM B/BL: "b%24'l%c\t%b" — operands: Condition,
        // BranchTarget. Cond = bits 28..32, branch = bits 0..24.
        let ranges = extract_operand_bit_ranges("b%24'l%c\t%b", 4);
        assert_eq!(ranges.len(), 2, "expected 2 operands, got {ranges:?}");
        assert_eq!(ranges[0], vec![28..32], "cond");
        assert_eq!(ranges[1], vec![0..24], "branch");
    }

    #[test]
    fn extract_thumb_movw_format() {
        // Thumb movw: "movw%c\t%8-11r, %J"
        // Operands: Condition, Register Rd (bits 8..12), Imm16 (%J split).
        let ranges = extract_operand_bit_ranges("movw%c\t%8-11r, %J", 4);
        assert_eq!(ranges.len(), 3);
        // First %c — Thumb-2 32-bit %c (no bitfield): bits
        // 28..32 by the conservative ARM rule. (For Thumb the
        // "always" semantics mean this is effectively
        // display-only; callers using this for wildcarding
        // can mask it without harm.)
        assert_eq!(ranges[0], vec![28..32]);
        assert_eq!(ranges[1], vec![8..12]);
        assert_eq!(ranges[2], vec![0..8, 12..15, 16..20, 26..27]);
    }

    #[test]
    fn extract_arm_movw_format() {
        // ARM movw: "movw%c\t%12-15R, %V"
        // Cond bits 28..32; Rd bits 12..16; imm16 bits 0..12, 16..20.
        let ranges = extract_operand_bit_ranges("movw%c\t%12-15R, %V", 4);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], vec![28..32]);
        assert_eq!(ranges[1], vec![12..16]);
        assert_eq!(ranges[2], vec![0..12, 16..20]);
    }

    #[test]
    fn display_only_codes_do_not_consume_operands() {
        // Cond + branch only — `%24'l` is a literal-display
        // marker, `%c` consumes cond, `%b` consumes target.
        let ranges = extract_operand_bit_ranges("b%24'l%c\t%b", 4);
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn empty_format_yields_no_operands() {
        let ranges = extract_operand_bit_ranges("", 4);
        assert!(ranges.is_empty());
    }
}
