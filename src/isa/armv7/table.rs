#![allow(dead_code)]
//! ARMv7 Thumb opcode table.
//!
//! Bootstrap iteration: a small seed set of instructions
//! spanning the major Thumb shape categories. Designed so
//! adding new entries is mechanical: pick a mnemonic, transcribe
//! the bit pattern + mask from the ARM Architecture Reference
//! Manual (DDI 0406C "ARMv7-A and ARMv7-R"), and append a
//! `ThumbOpcode` row.
//!
//! ## Coverage so far
//!
//! - 16-bit data-processing (`add`, `sub`, `mov`, `cmp`):
//!   shifted-register and small-immediate forms.
//! - 16-bit memory (`ldr`, `str`): immediate-offset forms.
//! - 16-bit branches (`b`, `b<cond>`, `bx`).
//! - 16-bit stack (`push`, `pop`).
//! - 32-bit branches (`b.w`, `bl`).
//! - 32-bit data-processing (`add` with 12-bit immediate, as a
//!   representative Thumb-2 32-bit data-proc encoding).
//!
//! Each entry is hand-transcribed from the ARM ARM. The entry
//! count grows as the rewriter and disassembler need more.

use super::operand::{DecodedOperand, DecodeError, Register, RegisterClass};

/// Width of a Thumb instruction. Drives how many bytes the
/// matcher consumes from the input.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ThumbWidth {
    /// 16-bit (Thumb-1, plus original 16-bit Thumb-2).
    Halfword,
    /// 32-bit (Thumb-2 wide encoding).
    Word,
}

/// Mnemonic identifier. Bootstrap set — extend as the table
/// grows. Conditional branches are split into per-condition
/// variants to mirror the AArch64 convention (`Beq`, `Bne`,
/// etc.); the unified `B` covers the unconditional 16-bit
/// short branch.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ThumbMnemonic {
    // Data-processing
    Add,
    Sub,
    Mov,
    Cmp,
    // Loads / stores
    Ldr,
    Str,
    // Branches
    B,
    Bl,
    Bx,
    Beq,
    Bne,
    Bcs,
    Bcc,
    Bmi,
    Bpl,
    Bvs,
    Bvc,
    Bhi,
    Bls,
    Bge,
    Blt,
    Bgt,
    Ble,
    // Stack
    Push,
    Pop,
    // No-op / unknown — used for instructions the table doesn't
    // recognise yet.
    Unknown,
}

impl ThumbMnemonic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mov => "mov",
            Self::Cmp => "cmp",
            Self::Ldr => "ldr",
            Self::Str => "str",
            Self::B => "b",
            Self::Bl => "bl",
            Self::Bx => "bx",
            Self::Beq => "beq",
            Self::Bne => "bne",
            Self::Bcs => "bcs",
            Self::Bcc => "bcc",
            Self::Bmi => "bmi",
            Self::Bpl => "bpl",
            Self::Bvs => "bvs",
            Self::Bvc => "bvc",
            Self::Bhi => "bhi",
            Self::Bls => "bls",
            Self::Bge => "bge",
            Self::Blt => "blt",
            Self::Bgt => "bgt",
            Self::Ble => "ble",
            Self::Push => "push",
            Self::Pop => "pop",
            Self::Unknown => "<unknown>",
        }
    }
}

/// Operand-shape tag attached to each opcode entry. The
/// decoder uses it to dispatch to the right field-extraction
/// routine. Each variant encodes the operand layout for one
/// encoding form, NOT the operand types themselves — that's
/// what [`DecodedOperand`] expresses post-decode.
///
/// New encoding forms become new variants; the decoder gains
/// a match arm for each. Keeping the tag small + per-form
/// rather than per-operand means the table stays compact and
/// the decoder logic stays explicit (no field-by-field
/// metadata interpretation at runtime).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum OperandShape {
    /// `mnemonic Rd, Rn, Rm` — three low registers in
    /// fields [2:0], [5:3], [8:6]. Used by Thumb-1
    /// add/sub-register.
    LowLowLow,
    /// `mnemonic Rd, #imm8` — low register in [10:8],
    /// 8-bit unsigned immediate in [7:0]. Used by mov-imm,
    /// cmp-imm, add-imm-T2, sub-imm-T2.
    LowImm8,
    /// `mnemonic Rt, [Rn, #imm5*4]` — three fields:
    /// Rt [2:0], Rn [5:3], imm5 [10:6] (scaled by 4 for
    /// word-sized loads/stores).
    LowLowImm5W,
    /// `mnemonic <pcrel11>` — 11-bit signed PC-relative
    /// branch (16-bit unconditional `b`).
    Pcrel11,
    /// `mnemonic <pcrel8>` — 8-bit signed PC-relative
    /// branch (16-bit conditional `b<cond>`). The condition
    /// itself is encoded in the mnemonic, not extracted.
    Pcrel8,
    /// `bx Rm` — single full-range register in [6:3].
    BxReg,
    /// `push {register_list}` / `pop {register_list}`. The
    /// 9-bit field [8:0] = high-bit-for-LR-or-PC : reglist[7:0].
    PushPop,
    /// 32-bit `bl <pcrel24>` — sign-extended 24-bit
    /// PC-relative target encoded across two halfwords with
    /// the J1/J2 sign-magic.
    Bl24,
    /// 32-bit `b.w <pcrel20>` for conditional encodings, or
    /// `b.w <pcrel24>` for unconditional. Handled
    /// separately from `Bl24` because the immediate
    /// extraction for the conditional form is different.
    BWide,
    /// 32-bit `add Rd, Rn, #imm12` — Thumb-2 data-proc
    /// immediate, T3 encoding.
    AddImm12T3,
    /// Catch-all for entries the decoder doesn't yet
    /// translate operands for; emits an empty operand
    /// vector. Useful while the table grows so partial rows
    /// at least classify the mnemonic.
    Unspecified,
}

#[derive(Debug, Copy, Clone)]
pub struct ThumbOpcode {
    /// Mnemonic for this row.
    pub mnemonic: ThumbMnemonic,
    /// Bit pattern of the encoding. For 16-bit encodings,
    /// only the low 16 bits are meaningful (high 16 must be
    /// zero). For 32-bit encodings, the FIRST halfword
    /// occupies bits 31..16 and the second occupies 15..0,
    /// so the field can be transcribed directly from the
    /// ARM ARM where 32-bit encodings are typically printed
    /// "first halfword | second halfword".
    pub opcode: u32,
    /// Mask: 1 bits indicate fields that must match
    /// `opcode`; 0 bits are operand fields that can be
    /// anything. Same width semantics as `opcode`.
    pub mask: u32,
    pub width: ThumbWidth,
    pub shape: OperandShape,
}

/// Hand-transcribed seed table. Order matters only for
/// disambiguation when multiple rows would otherwise match
/// the same input — the matcher returns the first hit, so
/// more-specific encodings should appear before less-specific
/// ones.
pub static THUMB_OPCODE_TABLE: &[ThumbOpcode] = &[
    // ---- 16-bit data-processing ----
    // ADD (register, T1): Rd, Rn, Rm — three low registers.
    // Encoding: 0001100 mmm nnn ddd
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Add,
        opcode: 0b0001_1000_0000_0000,
        mask: 0b1111_1110_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowLowLow,
    },
    // SUB (register, T1): same shape as ADD but
    // 0001101 mmm nnn ddd.
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Sub,
        opcode: 0b0001_1010_0000_0000,
        mask: 0b1111_1110_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowLowLow,
    },
    // MOV (immediate, T1): Rd, #imm8.
    // Encoding: 00100 ddd iiiiiiii
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Mov,
        opcode: 0b0010_0000_0000_0000,
        mask: 0b1111_1000_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowImm8,
    },
    // CMP (immediate, T1): Rn, #imm8.
    // Encoding: 00101 nnn iiiiiiii
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Cmp,
        opcode: 0b0010_1000_0000_0000,
        mask: 0b1111_1000_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowImm8,
    },
    // ADD (immediate, T2): Rdn, #imm8.
    // Encoding: 00110 ddd iiiiiiii — Rd and Rn are the same.
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Add,
        opcode: 0b0011_0000_0000_0000,
        mask: 0b1111_1000_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowImm8,
    },
    // SUB (immediate, T2): Rdn, #imm8.
    // Encoding: 00111 ddd iiiiiiii.
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Sub,
        opcode: 0b0011_1000_0000_0000,
        mask: 0b1111_1000_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowImm8,
    },
    // ---- 16-bit memory ----
    // LDR (immediate, T1): Rt, [Rn, #imm5 * 4]. Word load.
    // Encoding: 01101 iiiii nnn ttt
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Ldr,
        opcode: 0b0110_1000_0000_0000,
        mask: 0b1111_1000_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowLowImm5W,
    },
    // STR (immediate, T1): Rt, [Rn, #imm5 * 4]. Word store.
    // Encoding: 01100 iiiii nnn ttt
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Str,
        opcode: 0b0110_0000_0000_0000,
        mask: 0b1111_1000_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::LowLowImm5W,
    },
    // ---- 16-bit branches ----
    // B<cond> (T1): conditional 8-bit signed PC-relative.
    // Encoding: 1101 cccc iiiiiiii where cond ≠ 1110/1111.
    // We emit one row per condition for the bootstrap so
    // the matcher returns a specific mnemonic without
    // needing a field-decode pass.
    branch_cond(0b0000, ThumbMnemonic::Beq),
    branch_cond(0b0001, ThumbMnemonic::Bne),
    branch_cond(0b0010, ThumbMnemonic::Bcs),
    branch_cond(0b0011, ThumbMnemonic::Bcc),
    branch_cond(0b0100, ThumbMnemonic::Bmi),
    branch_cond(0b0101, ThumbMnemonic::Bpl),
    branch_cond(0b0110, ThumbMnemonic::Bvs),
    branch_cond(0b0111, ThumbMnemonic::Bvc),
    branch_cond(0b1000, ThumbMnemonic::Bhi),
    branch_cond(0b1001, ThumbMnemonic::Bls),
    branch_cond(0b1010, ThumbMnemonic::Bge),
    branch_cond(0b1011, ThumbMnemonic::Blt),
    branch_cond(0b1100, ThumbMnemonic::Bgt),
    branch_cond(0b1101, ThumbMnemonic::Ble),
    // B (T2): unconditional 11-bit signed PC-relative.
    // Encoding: 11100 iiiiiiiiiii
    ThumbOpcode {
        mnemonic: ThumbMnemonic::B,
        opcode: 0b1110_0000_0000_0000,
        mask: 0b1111_1000_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::Pcrel11,
    },
    // BX (T1): branch and exchange. Encoding:
    // 010001110 mmmm 000.
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Bx,
        opcode: 0b0100_0111_0000_0000,
        mask: 0b1111_1111_1000_0111,
        width: ThumbWidth::Halfword,
        shape: OperandShape::BxReg,
    },
    // ---- 16-bit stack ----
    // PUSH: 1011 010 M reglist8.  M=1 means LR is in the list.
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Push,
        opcode: 0b1011_0100_0000_0000,
        mask: 0b1111_1110_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::PushPop,
    },
    // POP: 1011 110 P reglist8.  P=1 means PC is in the list.
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Pop,
        opcode: 0b1011_1100_0000_0000,
        mask: 0b1111_1110_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::PushPop,
    },
    // ---- 32-bit branches (Thumb-2) ----
    // BL (T1): 11110 S imm10  11 J1 1 J2 imm11.
    // The opcode pattern below pins the leading bits;
    // operands fill imm10 (bits 25..16), J1 (bit 13), J2
    // (bit 11), imm11 (bits 10..0), S (bit 26).
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Bl,
        opcode: 0b1111_0000_0000_0000__1101_0000_0000_0000,
        mask: 0b1111_1000_0000_0000__1101_0000_0000_0000,
        width: ThumbWidth::Word,
        shape: OperandShape::Bl24,
    },
    // B.W (T4, unconditional 24-bit): 11110 S imm10  10 J1 1 J2 imm11.
    ThumbOpcode {
        mnemonic: ThumbMnemonic::B,
        opcode: 0b1111_0000_0000_0000__1001_0000_0000_0000,
        mask: 0b1111_1000_0000_0000__1101_0000_0000_0000,
        width: ThumbWidth::Word,
        shape: OperandShape::BWide,
    },
    // ---- 32-bit data-processing ----
    // ADD (immediate, T3): Rd, Rn, #imm12.
    // Encoding: 11110 i 0 1000 0 nnnn  0 iii dddd iiiiiiii
    ThumbOpcode {
        mnemonic: ThumbMnemonic::Add,
        opcode: 0b1111_0000_0000_0000__0000_0000_0000_0000 | (0b01000 << 20),
        mask: 0b1111_1011_1111_0000__1000_0000_0000_0000,
        width: ThumbWidth::Word,
        shape: OperandShape::AddImm12T3,
    },
];

const fn branch_cond(cond: u16, mnemonic: ThumbMnemonic) -> ThumbOpcode {
    // Conditional 16-bit branch:
    //   1101 cccc iiiiiiii  (cond ≠ 1110, 1111)
    ThumbOpcode {
        mnemonic,
        opcode: 0b1101_0000_0000_0000 | ((cond as u32) << 8),
        mask: 0b1111_1111_0000_0000,
        width: ThumbWidth::Halfword,
        shape: OperandShape::Pcrel8,
    }
}

/// Find the first table entry that matches `word` for the
/// given width. Linear scan; the table is small (<100 rows
/// today) and this is sufficient until benchmarking says
/// otherwise.
pub fn match_opcode(word: u32, width: ThumbWidth) -> Option<&'static ThumbOpcode> {
    THUMB_OPCODE_TABLE
        .iter()
        .find(|row| row.width == width && (word & row.mask) == row.opcode)
}

/// Decode operands for a single matched opcode. Bootstrap
/// support is partial — the variants we need most for the
/// rewriter (LowImm8, Pcrel8, Pcrel11, Bl24) are wired up;
/// others return an empty operand vector with a comment for
/// the next pass to flesh out.
pub fn decode_operands(
    opcode: &ThumbOpcode,
    word: u32,
    address: u64,
) -> Result<Vec<DecodedOperand>, DecodeError> {
    match opcode.shape {
        OperandShape::LowLowLow => {
            // [8:6] m, [5:3] n, [2:0] d
            let rd = ((word >> 0) & 0x7) as u8;
            let rn = ((word >> 3) & 0x7) as u8;
            let rm = ((word >> 6) & 0x7) as u8;
            Ok(vec![
                DecodedOperand::Register(low_reg(rd)),
                DecodedOperand::Register(low_reg(rn)),
                DecodedOperand::Register(low_reg(rm)),
            ])
        }
        OperandShape::LowImm8 => {
            // [10:8] d, [7:0] imm8
            let rd = ((word >> 8) & 0x7) as u8;
            let imm = (word & 0xff) as i64;
            Ok(vec![
                DecodedOperand::Register(low_reg(rd)),
                DecodedOperand::Immediate(imm),
            ])
        }
        OperandShape::LowLowImm5W => {
            // [10:6] imm5, [5:3] n, [2:0] t
            let rt = ((word >> 0) & 0x7) as u8;
            let rn = ((word >> 3) & 0x7) as u8;
            let imm5 = ((word >> 6) & 0x1f) as i64;
            // Word-scaled: actual byte offset = imm5 * 4.
            Ok(vec![
                DecodedOperand::Register(low_reg(rt)),
                DecodedOperand::Register(low_reg(rn)),
                DecodedOperand::Immediate(imm5 * 4),
            ])
        }
        OperandShape::Pcrel8 => {
            // imm8 << 1, sign-extended. Branch target =
            // PC + 4 + offset (Thumb's PC convention: PC
            // reads as the address of the current instruction
            // + 4).
            let imm8 = (word & 0xff) as u8;
            let signed = ((imm8 as i8) as i32) << 1; // *2, signed
            let target = address.wrapping_add(4).wrapping_add(signed as u64);
            Ok(vec![DecodedOperand::BranchTarget(target)])
        }
        OperandShape::Pcrel11 => {
            // imm11 << 1, sign-extended.
            let imm11 = word & 0x7ff;
            // Sign-extend 11 bits to i32.
            let signed = (((imm11 << 21) as i32) >> 21) << 1;
            let target = address.wrapping_add(4).wrapping_add(signed as u64);
            Ok(vec![DecodedOperand::BranchTarget(target)])
        }
        OperandShape::BxReg => {
            // [6:3] m — full register class.
            let rm = ((word >> 3) & 0xf) as u8;
            Ok(vec![DecodedOperand::Register(Register {
                class: RegisterClass::R,
                index: rm,
            })])
        }
        OperandShape::PushPop => {
            // [8:0] = M:reglist  (push: M=LR, pop: M=PC)
            let reglist = (word & 0xff) as u16;
            let extra_bit = ((word >> 8) & 0x1) as u16;
            // For push, extra_bit means "include LR" (bit 14
            // of the full 16-bit reglist representation).
            // For pop, "include PC" (bit 15). We always
            // attach the extra bit at position 14 here and
            // let the caller distinguish via the mnemonic;
            // a future pass can split into two operand
            // shapes if a caller needs the precise meaning
            // without checking the mnemonic.
            let combined = reglist | ((extra_bit) << 14);
            Ok(vec![DecodedOperand::RegisterList(combined)])
        }
        OperandShape::Bl24 => {
            // BL T1: 11110 S imm10  11 J1 1 J2 imm11
            //   target = PC + 4 + sign_extend(I1:I2:imm10:imm11:0, 25)
            //   I1 = NOT(J1 XOR S)
            //   I2 = NOT(J2 XOR S)
            // Word layout in our u32: bits 31..16 = first
            // halfword, 15..0 = second.
            let s = (word >> 26) & 0x1;
            let imm10 = (word >> 16) & 0x3ff;
            let j1 = (word >> 13) & 0x1;
            let j2 = (word >> 11) & 0x1;
            let imm11 = word & 0x7ff;
            let i1 = !(j1 ^ s) & 0x1;
            let i2 = !(j2 ^ s) & 0x1;
            // Build 25-bit unsigned: S(24) I1(23) I2(22) imm10(21..12) imm11(11..1) 0(0)
            let raw = (s << 24)
                | (i1 << 23)
                | (i2 << 22)
                | (imm10 << 12)
                | (imm11 << 1);
            // Sign-extend from 25 bits.
            let signed = ((raw << 7) as i32) >> 7;
            let target = address.wrapping_add(4).wrapping_add(signed as u64);
            Ok(vec![DecodedOperand::BranchTarget(target)])
        }
        OperandShape::BWide | OperandShape::AddImm12T3 | OperandShape::Unspecified => {
            // Bootstrap: not yet decoded. Returning an empty
            // operand list lets the matcher classify the
            // mnemonic without claiming structured operands.
            Ok(Vec::new())
        }
    }
}

fn low_reg(index: u8) -> Register {
    Register {
        class: RegisterClass::Low,
        index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_opcode_finds_add_register_t1() {
        // add r0, r1, r2 → 0b0001_1000_1000_1000 = 0x1888
        let word = 0x1888;
        let op = match_opcode(word, ThumbWidth::Halfword).expect("match add T1");
        assert_eq!(op.mnemonic, ThumbMnemonic::Add);
        assert_eq!(op.shape, OperandShape::LowLowLow);
    }

    #[test]
    fn match_opcode_distinguishes_add_t1_from_sub_t1() {
        // sub r0, r1, r2 → 0b0001_1010_1000_1000 = 0x1a88
        let op = match_opcode(0x1a88, ThumbWidth::Halfword).expect("match sub T1");
        assert_eq!(op.mnemonic, ThumbMnemonic::Sub);
    }

    #[test]
    fn match_opcode_finds_movs_imm() {
        // mov r0, #42 → 0b0010_0000_0010_1010 = 0x202a
        let op = match_opcode(0x202a, ThumbWidth::Halfword).expect("match mov imm T1");
        assert_eq!(op.mnemonic, ThumbMnemonic::Mov);
    }

    #[test]
    fn match_opcode_returns_none_for_unmatched_input() {
        // 0xffff is currently not in the table — verify the
        // matcher cleanly returns None instead of an
        // arbitrary partial match.
        assert!(match_opcode(0xffff, ThumbWidth::Halfword).is_none());
    }

    #[test]
    fn match_opcode_returns_word_row_for_32bit_pattern() {
        // BL pattern with all-zero immediate fields. The
        // matcher must classify it as the 32-bit BL row when
        // asked for `Word` width. (Note: the same low-16-bit
        // bits 0xd000 happen to look like a valid 16-bit
        // beq encoding, so the matcher's width discrimination
        // is what keeps callers from confusing the two —
        // callers must ask with the right width based on
        // `read_instruction`'s detection.)
        let bl_word = 0xf000_d000;
        let op = match_opcode(bl_word, ThumbWidth::Word).expect("match bl");
        assert_eq!(op.mnemonic, ThumbMnemonic::Bl);
        assert_eq!(op.width, ThumbWidth::Word);
    }

    #[test]
    fn decode_low_low_low_extracts_three_low_registers() {
        // add r3, r4, r5 → bits: rm=5, rn=4, rd=3
        // 0001_1001_0110_0011 = 0x1963 (m=5 [8:6], n=4 [5:3], d=3 [2:0])
        let word = 0b0001_1001_0110_0011;
        let op = match_opcode(word, ThumbWidth::Halfword).unwrap();
        let operands = decode_operands(op, word, 0).unwrap();
        assert_eq!(operands.len(), 3);
        match (&operands[0], &operands[1], &operands[2]) {
            (
                DecodedOperand::Register(rd),
                DecodedOperand::Register(rn),
                DecodedOperand::Register(rm),
            ) => {
                assert_eq!(rd.index, 3);
                assert_eq!(rn.index, 4);
                assert_eq!(rm.index, 5);
                assert!(matches!(rd.class, RegisterClass::Low));
            }
            _ => panic!("expected three Register operands, got {operands:?}"),
        }
    }

    #[test]
    fn decode_low_imm8_extracts_register_and_immediate() {
        // mov r2, #0x37 → 0b0010_0010_0011_0111 = 0x2237
        let word = 0x2237;
        let op = match_opcode(word, ThumbWidth::Halfword).unwrap();
        let operands = decode_operands(op, word, 0).unwrap();
        assert_eq!(operands.len(), 2);
        match (&operands[0], &operands[1]) {
            (DecodedOperand::Register(rd), DecodedOperand::Immediate(imm)) => {
                assert_eq!(rd.index, 2);
                assert_eq!(*imm, 0x37);
            }
            _ => panic!("unexpected operand shape: {operands:?}"),
        }
    }

    #[test]
    fn decode_pcrel11_unconditional_branch_resolves_address() {
        // b +8 (ahead of PC+4): imm11 = 4 → word = 0xe004.
        // Address arbitrary; target = address + 4 + 8.
        let word = 0xe004u32;
        let op = match_opcode(word, ThumbWidth::Halfword).unwrap();
        assert_eq!(op.mnemonic, ThumbMnemonic::B);
        let operands = decode_operands(op, word, 0x1000).unwrap();
        match operands.first() {
            Some(DecodedOperand::BranchTarget(t)) => {
                assert_eq!(*t, 0x1000 + 4 + 8);
            }
            _ => panic!("expected BranchTarget, got {operands:?}"),
        }
    }

    #[test]
    fn decode_pcrel8_conditional_branch_resolves_address() {
        // beq +6: imm8 = 3 → word = 0b1101_0000_0000_0011 = 0xd003.
        let word = 0xd003u32;
        let op = match_opcode(word, ThumbWidth::Halfword).unwrap();
        assert_eq!(op.mnemonic, ThumbMnemonic::Beq);
        let operands = decode_operands(op, word, 0x2000).unwrap();
        match operands.first() {
            Some(DecodedOperand::BranchTarget(t)) => {
                assert_eq!(*t, 0x2000 + 4 + 6);
            }
            _ => panic!("expected BranchTarget, got {operands:?}"),
        }
    }

    #[test]
    fn decode_pcrel8_handles_negative_offsets() {
        // bne -4: imm8 = 0xfe (i.e. -2 signed) → offset = -4.
        // 0b1101_0001_1111_1110 = 0xd1fe
        let word = 0xd1fe;
        let op = match_opcode(word, ThumbWidth::Halfword).unwrap();
        assert_eq!(op.mnemonic, ThumbMnemonic::Bne);
        let operands = decode_operands(op, word, 0x2000).unwrap();
        match operands.first() {
            Some(DecodedOperand::BranchTarget(t)) => {
                assert_eq!(*t, 0x2000u64.wrapping_add(4).wrapping_sub(4));
            }
            _ => panic!("expected BranchTarget, got {operands:?}"),
        }
    }

    #[test]
    fn match_opcode_finds_bx() {
        // bx lr → 0b0100_0111_0111_0000 = 0x4770
        let op = match_opcode(0x4770, ThumbWidth::Halfword).expect("match bx");
        assert_eq!(op.mnemonic, ThumbMnemonic::Bx);
        let operands = decode_operands(op, 0x4770, 0).unwrap();
        match operands.first() {
            Some(DecodedOperand::Register(reg)) => {
                assert_eq!(reg.index, 14); // lr
                assert!(matches!(reg.class, RegisterClass::R));
            }
            _ => panic!("expected Register, got {operands:?}"),
        }
    }

    #[test]
    fn match_opcode_finds_push_and_pop() {
        // push {r0, lr} → 0b1011_0101_0000_0001 = 0xb501
        // pop {r0, pc} →  0b1011_1101_0000_0001 = 0xbd01
        let push_op = match_opcode(0xb501, ThumbWidth::Halfword).expect("push");
        let pop_op = match_opcode(0xbd01, ThumbWidth::Halfword).expect("pop");
        assert_eq!(push_op.mnemonic, ThumbMnemonic::Push);
        assert_eq!(pop_op.mnemonic, ThumbMnemonic::Pop);
    }

    #[test]
    fn decode_bl24_round_trip_short_target() {
        // BL with target 4 bytes ahead.
        // For target = PC+4 + 4: imm24 (post-S/I1/I2 fold) = 4.
        // The cleanest seed: encode S=0, I1=I2=1, imm10=0,
        // imm11 = 2 (≪1 = 4) → J1=J2=1.
        // First halfword: 11110 S imm10 = 11110_0_0000000000 = 0xf000
        // Second halfword: 11 J1 1 J2 imm11 = 11_1_1_1_00000000010 = 0xf802
        let word = 0xf000_f802u32;
        let op = match_opcode(word, ThumbWidth::Word).expect("match bl");
        assert_eq!(op.mnemonic, ThumbMnemonic::Bl);
        let operands = decode_operands(op, word, 0x1000).unwrap();
        match operands.first() {
            Some(DecodedOperand::BranchTarget(t)) => {
                assert_eq!(*t, 0x1000 + 4 + 4);
            }
            _ => panic!("expected BranchTarget, got {operands:?}"),
        }
    }
}
