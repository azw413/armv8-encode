//! ARM-mode format-driven operand decoder.
//!
//! Mirror of [`super::super::format_decode`] but specialised
//! for ARM mode's bit layouts and unique format codes.
//!
//! Notable differences from Thumb:
//!
//! - `%o` (operand2) is ARM-specific. It encodes either an
//!   immediate with rotate (bit 25 = 1, imm8 in [0..7], rot
//!   in [8..11]) or a register with shift (bit 25 = 0).
//! - `%a` is a load/store address [Rn, ±offset] with a richer
//!   set of forms than Thumb's.
//! - `%b` is the 24-bit signed branch offset shifted left by 2.
//! - All ARM instructions are conditional via bits 28..31; the
//!   `%c` code is always present and is treated as display-only
//!   here (the condition bits stay in the encoded word).

use crate::isa::armv7::operand::{DecodedOperand, Register, RegisterClass};

/// Decode operands by walking the format string.
pub fn decode_operands_from_format(
    format: &str,
    word: u32,
    address: u64,
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
            let (mut inner_ops, mut inner_unh) =
                decode_operands_from_format(inner_str, word, address);
            operands.append(&mut inner_ops);
            unhandled.append(&mut inner_unh);
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
        // Display-only ARM codes: %c (condition), %x/%X
        // (warnings), %p (writes-to-PC marker), %t (T-bit
        // marker), %C (PSR sub-type print), %q (UAL hint),
        // %s (SPSR/CPSR mode in mrs).
        if matches!(code, b'c' | b'C' | b'x' | b'X' | b'%' | b'p' | b't' | b'q')
            && bitfield.is_empty()
        {
            continue;
        }
        match code {
            b'r' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    operands.push(register_operand(word, lo, hi));
                }
            }
            b'R' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    operands.push(register_operand(word, lo, hi));
                }
            }
            b'T' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi) as u8;
                    operands.push(DecodedOperand::Register(Register {
                        class: RegisterClass::R,
                        index: v.wrapping_add(1),
                    }));
                }
            }
            b'd' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate(v as i64));
                }
            }
            b'W' => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate(v as i64 + 1));
                }
            }
            b'x' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate(v as i64));
                }
            }
            b'X' if !bitfield.is_empty() => {
                if let Some((lo, hi)) = parse_bitfield(bitfield) {
                    let v = extract_field(word, lo, hi);
                    operands.push(DecodedOperand::Immediate(v as i64));
                }
            }
            b'b' if bitfield.is_empty() => {
                // 24-bit signed branch offset shifted left by 2.
                let raw = (word & 0x00ff_ffff) as i32;
                // Sign-extend from 24 bits.
                let signed = (raw << 8) >> 8;
                let offset = (signed as i64) * 4;
                let target = (address as i64).wrapping_add(8).wrapping_add(offset);
                operands.push(DecodedOperand::BranchTarget(target as u64));
            }
            b'B' if bitfield.is_empty() => {
                // BLX(1): same 24-bit imm as B, plus H bit
                // (bit 24) for ±2 byte half-word offset to
                // switch into Thumb mode.
                let raw = (word & 0x00ff_ffff) as i32;
                let signed = (raw << 8) >> 8;
                let h = ((word >> 24) & 0x1) as i64;
                let offset = (signed as i64) * 4 + h * 2;
                let target = (address as i64).wrapping_add(8).wrapping_add(offset);
                operands.push(DecodedOperand::BranchTarget(target as u64));
            }
            b'm' if bitfield.is_empty() => {
                // ldm/stm reglist: low 16 bits.
                let mask = (word & 0xffff) as u16;
                operands.push(DecodedOperand::RegisterList(mask));
            }
            b'o' if bitfield.is_empty() => {
                // Operand2: bit 25 selects immediate (1) or
                // register-with-shift (0).
                if (word >> 25) & 0x1 == 1 {
                    let imm8 = word & 0xff;
                    let rot = (word >> 8) & 0xf;
                    let value = (imm8).rotate_right(rot * 2);
                    operands.push(DecodedOperand::Immediate(value as i64));
                } else {
                    // Register form. We emit the Rm; the
                    // shift type / amount stay in opcode bits
                    // for now (the rewriter normally only
                    // cares about the register).
                    let rm = (word & 0xf) as u8;
                    operands.push(DecodedOperand::Register(Register {
                        class: RegisterClass::R,
                        index: rm,
                    }));
                }
            }
            b'a' if bitfield.is_empty() => {
                // ldr/str address: [Rn, ±offset]. Rn = bits
                // 16..19. P/U/W bits = 24/23/21. I-bit (25)
                // = immediate (0) vs register-shifted (1).
                let rn = ((word >> 16) & 0xf) as u8;
                operands.push(DecodedOperand::Register(Register {
                    class: RegisterClass::R,
                    index: rn,
                }));
                let u = (word >> 23) & 0x1;
                let i_bit = (word >> 25) & 0x1;
                if i_bit == 0 {
                    let imm12 = (word & 0xfff) as i64;
                    let signed = if u == 1 { imm12 } else { -imm12 };
                    if signed != 0 || u == 1 {
                        operands.push(DecodedOperand::Immediate(signed));
                    }
                } else {
                    let rm = (word & 0xf) as u8;
                    operands.push(DecodedOperand::Register(Register {
                        class: RegisterClass::R,
                        index: rm,
                    }));
                }
                // PC-relative: Rn=15 → emit a PcRelative
                // pointing at (PC+8) ± imm. For ARM mode,
                // PC reads as the address of the current
                // instruction + 8.
                if rn == 15 && i_bit == 0 {
                    let imm12 = (word & 0xfff) as i64;
                    let signed = if u == 1 { imm12 } else { -imm12 };
                    let target = (address as i64).wrapping_add(8).wrapping_add(signed);
                    operands.push(DecodedOperand::PcRelative(target as u64));
                }
            }
            b's' if bitfield.is_empty() => {
                // %s — ldr/str halfword/signextend address.
                // Two forms: bit 22 = 1 → 8-bit immediate
                // split across [11:8] and [3:0]; bit 22 = 0
                // → register Rm in [3:0]. Plus PUW bits.
                let rn = ((word >> 16) & 0xf) as u8;
                operands.push(DecodedOperand::Register(Register {
                    class: RegisterClass::R,
                    index: rn,
                }));
                let u = (word >> 23) & 0x1;
                if (word >> 22) & 0x1 == 1 {
                    let imm = (((word >> 8) & 0xf) << 4) | (word & 0xf);
                    let signed = if u == 1 { imm as i64 } else { -(imm as i64) };
                    if signed != 0 || u == 1 {
                        operands.push(DecodedOperand::Immediate(signed));
                    }
                } else {
                    let rm = (word & 0xf) as u8;
                    operands.push(DecodedOperand::Register(Register {
                        class: RegisterClass::R,
                        index: rm,
                    }));
                }
            }
            b'V' if bitfield.is_empty() => {
                // MOVT/MOVW: 16-bit immediate from bits
                // [19:16]:[11:0].
                let hi4 = (word >> 16) & 0xf;
                let lo12 = word & 0xfff;
                let imm = (hi4 << 12) | lo12;
                operands.push(DecodedOperand::Immediate(imm as i64));
            }
            b'E' if bitfield.is_empty() => {
                // BFI/BFC: lsb (bits 7..11), msb (bits 16..20).
                let lsb = (word >> 7) & 0x1f;
                let msb = (word >> 16) & 0x1f;
                let width = msb.saturating_sub(lsb).wrapping_add(1);
                operands.push(DecodedOperand::Immediate(lsb as i64));
                operands.push(DecodedOperand::Immediate(width as i64));
            }
            b'e' if bitfield.is_empty() => {
                // SMI immediate: bits [0..7]:[8..19].
                let lo = word & 0xff;
                let hi = (word >> 8) & 0xfff;
                let imm = (hi << 8) | lo;
                operands.push(DecodedOperand::Immediate(imm as i64));
            }
            b'U' if bitfield.is_empty() => {
                // Barrier type at bits [0..3].
                operands.push(DecodedOperand::Immediate((word & 0xf) as i64));
            }
            // Unrecognised — record for visibility.
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

fn register_operand(word: u32, lo: u8, hi: u8) -> DecodedOperand {
    DecodedOperand::Register(Register {
        class: RegisterClass::R,
        index: extract_field(word, lo, hi) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_arm_bx_lr() {
        // bx lr → e12fff1e. Format: "bx%c\t%0-3r"
        let (ops, unh) = decode_operands_from_format("bx%c\t%0-3r", 0xe12f_ff1e, 0);
        assert!(unh.is_empty());
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            DecodedOperand::Register(r) => assert_eq!(r.index, 14),
            _ => panic!(),
        }
    }

    #[test]
    fn decodes_arm_b_target() {
        // b +0 (offset 0) at address 0x100 → e_a000000. Format: "b%c\t%b"
        // Encoded: 0xea00_0000 (signed imm24 = 0).
        let (ops, _) = decode_operands_from_format("b%c\t%b", 0xea00_0000, 0x100);
        match &ops[0] {
            DecodedOperand::BranchTarget(t) => assert_eq!(*t, 0x100 + 8),
            _ => panic!("{ops:?}"),
        }
    }

    #[test]
    fn decodes_arm_push_lr() {
        // push {lr} → e52de004. Format from binutils:
        //   "str%c\t%12-15R, %a"
        // (binutils canonicalizes push{lr} as str lr, [sp, #-4]!)
        let (ops, _) = decode_operands_from_format(
            "str%c\t%12-15R, %a",
            0xe52d_e004,
            0,
        );
        // Expect: Rt=14 (lr), Rn=13 (sp), and an immediate.
        assert!(ops
            .iter()
            .any(|o| matches!(o, DecodedOperand::Register(r) if r.index == 14)));
        assert!(ops
            .iter()
            .any(|o| matches!(o, DecodedOperand::Register(r) if r.index == 13)));
        assert!(ops
            .iter()
            .any(|o| matches!(o, DecodedOperand::Immediate(_))));
    }

    #[test]
    fn decodes_arm_movw_immediate() {
        // movw r0, #0x1234 → e3010234. Format: "movw%c\t%12-15r, %V"
        let (ops, _) =
            decode_operands_from_format("movw%c\t%12-15r, %V", 0xe301_0234, 0);
        let imm: Vec<i64> = ops.iter().filter_map(|o| match o {
            DecodedOperand::Immediate(v) => Some(*v),
            _ => None,
        }).collect();
        assert_eq!(imm, vec![0x1234]);
    }

    #[test]
    fn decodes_arm_operand2_immediate() {
        // mov r0, #1 → e3a00001. Format includes %o.
        let (ops, _) = decode_operands_from_format(
            "mov%20's%c\t%12-15r, %o",
            0xe3a0_0001,
            0,
        );
        // Rd=0 + immediate=1.
        assert!(ops.iter().any(|o| matches!(o, DecodedOperand::Immediate(1))));
    }
}
