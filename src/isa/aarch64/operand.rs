use super::table::{Aarch64Opcode, Aarch64Opnd};
use super::Word;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Register {
    pub class: RegisterClass,
    pub index: u8,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RegisterClass {
    S,
    D,
    W,
    X,
    XOrSp,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ShiftedRegister {
    pub register: Register,
    pub shift: Shift,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Shift {
    pub kind: ShiftKind,
    pub amount: u8,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ShiftKind {
    Lsl,
    Lsr,
    Asr,
    Ror,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ShiftedImmediate {
    pub value: i64,
    pub shift: u8,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemoryOperand {
    pub base: Register,
    pub offset: MemoryOffset,
    pub mode: AddressingMode,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MemoryOffset {
    None,
    Immediate(i64),
    Register {
        register: Register,
        shift: Option<Shift>,
    },
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AddressingMode {
    Offset,
    PreIndex,
    PostIndex,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DecodedOperand {
    Register(Register),
    ShiftedRegister(ShiftedRegister),
    Immediate(i64),
    ShiftedImmediate(ShiftedImmediate),
    Memory(MemoryOperand),
    BranchTarget(u64),
    Condition(&'static str),
    FloatImmediate(&'static str),
    Unimplemented { kind: &'static str },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DecodeError {
    InvalidOperand(&'static str),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EncodeError {
    Unimplemented { kind: &'static str },
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct DecodeContext<'a> {
    pub word: Word,
    pub address: u64,
    pub opcode: &'a Aarch64Opcode,
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub(crate) struct EncodeContext<'a> {
    pub base_word: Word,
    pub address: u64,
    pub opcode: &'a Aarch64Opcode,
}

pub(crate) trait OperandCodec {
    fn decode(self, ctx: DecodeContext<'_>) -> Result<DecodedOperand, DecodeError>;

    #[allow(dead_code)]
    fn encode(
        self,
        _operand: &DecodedOperand,
        _ctx: EncodeContext<'_>,
    ) -> Result<Word, EncodeError>;
}

impl OperandCodec for Aarch64Opnd {
    fn decode(self, ctx: DecodeContext<'_>) -> Result<DecodedOperand, DecodeError> {
        let _ = ctx.opcode.mnemonic();

        match self {
            Aarch64Opnd::Rd => Ok(DecodedOperand::Register(x_reg(rd(ctx.word)))),
            Aarch64Opnd::Rn => Ok(DecodedOperand::Register(x_reg(rn(ctx.word)))),
            Aarch64Opnd::Rm => Ok(DecodedOperand::Register(x_reg(rm(ctx.word)))),
            Aarch64Opnd::RmSft => Ok(DecodedOperand::ShiftedRegister(rm_shifted(ctx.word))),
            Aarch64Opnd::RdSp => Ok(DecodedOperand::Register(x_or_sp(rd(ctx.word)))),
            Aarch64Opnd::RnSp => Ok(DecodedOperand::Register(x_or_sp(rn(ctx.word)))),
            Aarch64Opnd::Rt => Ok(DecodedOperand::Register(rt_reg(ctx.word, ctx.opcode))),
            Aarch64Opnd::Rt2 => Ok(DecodedOperand::Register(x_reg(rt2(ctx.word)))),
            Aarch64Opnd::Rs => Ok(DecodedOperand::Register(w_reg(rs(ctx.word)))),
            Aarch64Opnd::Fd => Ok(DecodedOperand::Register(fp_reg(rd(ctx.word), ctx.word))),
            Aarch64Opnd::Fn => Ok(DecodedOperand::Register(fp_reg(rn(ctx.word), ctx.word))),
            Aarch64Opnd::Fm => Ok(DecodedOperand::Register(fp_reg(rm(ctx.word), ctx.word))),
            Aarch64Opnd::Fa => Ok(DecodedOperand::Register(fp_reg(ra(ctx.word), ctx.word))),
            Aarch64Opnd::Ft => Ok(DecodedOperand::Register(fp_reg(rt(ctx.word), ctx.word))),
            Aarch64Opnd::Ft2 => Ok(DecodedOperand::Register(fp_reg(rt2(ctx.word), ctx.word))),
            Aarch64Opnd::Aimm => Ok(DecodedOperand::Immediate(aimm(ctx.word))),
            Aarch64Opnd::Limm => Ok(DecodedOperand::Immediate(
                logical_immediate(ctx.word).ok_or(DecodeError::InvalidOperand("Limm"))? as i64,
            )),
            Aarch64Opnd::Half => Ok(DecodedOperand::ShiftedImmediate(half(ctx.word))),
            Aarch64Opnd::ImmMov => Ok(DecodedOperand::Immediate(imm_mov(ctx.word))),
            Aarch64Opnd::Imm => Ok(DecodedOperand::Immediate(bitfield_imm(
                ctx.word, ctx.opcode,
            ))),
            Aarch64Opnd::Width => Ok(DecodedOperand::Immediate(bitfield_width(
                ctx.word, ctx.opcode,
            ))),
            Aarch64Opnd::BitNum => Ok(DecodedOperand::Immediate(bit_num(ctx.word))),
            Aarch64Opnd::CcmpImm => Ok(DecodedOperand::Immediate(ccmp_imm(ctx.word))),
            Aarch64Opnd::Nzcv => Ok(DecodedOperand::Immediate(nzcv(ctx.word))),
            Aarch64Opnd::Cond => Ok(DecodedOperand::Condition(condition(ctx.word))),
            Aarch64Opnd::Cond1 => Ok(DecodedOperand::Condition(inverted_condition(ctx.word))),
            Aarch64Opnd::Fpimm0 => Ok(DecodedOperand::FloatImmediate("0.0")),
            Aarch64Opnd::Fpimm => Ok(DecodedOperand::FloatImmediate(fpimm(ctx.word))),
            Aarch64Opnd::AddrSimm7 => Ok(DecodedOperand::Memory(MemoryOperand {
                base: x_or_sp(rn(ctx.word)),
                offset: MemoryOffset::Immediate(simm7_pair_offset(ctx.word)),
                mode: pair_addressing_mode(ctx.word),
            })),
            Aarch64Opnd::AddrSimple => Ok(DecodedOperand::Memory(MemoryOperand {
                base: x_or_sp(rn(ctx.word)),
                offset: MemoryOffset::None,
                mode: AddressingMode::Offset,
            })),
            Aarch64Opnd::AddrRegoff => Ok(DecodedOperand::Memory(MemoryOperand {
                base: x_or_sp(rn(ctx.word)),
                offset: MemoryOffset::Register {
                    register: x_reg(rm(ctx.word)),
                    shift: regoff_shift(ctx.word),
                },
                mode: AddressingMode::Offset,
            })),
            Aarch64Opnd::AddrSimm9 => Ok(DecodedOperand::Memory(MemoryOperand {
                base: x_or_sp(rn(ctx.word)),
                offset: MemoryOffset::Immediate(simm9_offset(ctx.word)),
                mode: simm9_addressing_mode(ctx.word),
            })),
            Aarch64Opnd::AddrUimm12 => Ok(DecodedOperand::Memory(MemoryOperand {
                base: x_or_sp(rn(ctx.word)),
                offset: MemoryOffset::Immediate(uimm12_offset(ctx.word)),
                mode: AddressingMode::Offset,
            })),
            Aarch64Opnd::AddrPcrel14 => Ok(DecodedOperand::BranchTarget(branch_target(
                ctx.address,
                imm14(ctx.word),
            ))),
            Aarch64Opnd::AddrPcrel19 => Ok(DecodedOperand::BranchTarget(branch_target(
                ctx.address,
                imm19(ctx.word),
            ))),
            Aarch64Opnd::AddrPcrel26 => Ok(DecodedOperand::BranchTarget(branch_target(
                ctx.address,
                imm26(ctx.word),
            ))),
            Aarch64Opnd::AddrPcrel21 => Ok(DecodedOperand::Immediate(imm21(ctx.word))),
            _ => Ok(DecodedOperand::Unimplemented { kind: self.name() }),
        }
    }

    fn encode(
        self,
        _operand: &DecodedOperand,
        ctx: EncodeContext<'_>,
    ) -> Result<Word, EncodeError> {
        let _ = (ctx.base_word, ctx.address, ctx.opcode.mnemonic());
        Err(EncodeError::Unimplemented { kind: self.name() })
    }
}

pub(crate) fn decode_operand(
    kind: Aarch64Opnd,
    ctx: DecodeContext<'_>,
) -> Result<DecodedOperand, DecodeError> {
    kind.decode(ctx)
}

impl Aarch64Opnd {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Aarch64Opnd::Nil => "Nil",
            Aarch64Opnd::Rd => "Rd",
            Aarch64Opnd::Rn => "Rn",
            Aarch64Opnd::Rm => "Rm",
            Aarch64Opnd::Rt => "Rt",
            Aarch64Opnd::Rt2 => "Rt2",
            Aarch64Opnd::Rs => "Rs",
            Aarch64Opnd::Ra => "Ra",
            Aarch64Opnd::RtSys => "RtSys",
            Aarch64Opnd::RdSp => "RdSp",
            Aarch64Opnd::RnSp => "RnSp",
            Aarch64Opnd::Pairreg => "Pairreg",
            Aarch64Opnd::RmExt => "RmExt",
            Aarch64Opnd::RmSft => "RmSft",
            Aarch64Opnd::Fd => "Fd",
            Aarch64Opnd::Fn => "Fn",
            Aarch64Opnd::Fm => "Fm",
            Aarch64Opnd::Fa => "Fa",
            Aarch64Opnd::Ft => "Ft",
            Aarch64Opnd::Ft2 => "Ft2",
            Aarch64Opnd::Sd => "Sd",
            Aarch64Opnd::Sn => "Sn",
            Aarch64Opnd::Sm => "Sm",
            Aarch64Opnd::Vd => "Vd",
            Aarch64Opnd::Vn => "Vn",
            Aarch64Opnd::Vm => "Vm",
            Aarch64Opnd::VdD1 => "VdD1",
            Aarch64Opnd::VnD1 => "VnD1",
            Aarch64Opnd::Ed => "Ed",
            Aarch64Opnd::En => "En",
            Aarch64Opnd::Em => "Em",
            Aarch64Opnd::Lvn => "Lvn",
            Aarch64Opnd::Lvt => "Lvt",
            Aarch64Opnd::LvtAl => "LvtAl",
            Aarch64Opnd::Let => "Let",
            Aarch64Opnd::Cn => "Cn",
            Aarch64Opnd::Cm => "Cm",
            Aarch64Opnd::Idx => "Idx",
            Aarch64Opnd::ImmVlsl => "ImmVlsl",
            Aarch64Opnd::ImmVlsr => "ImmVlsr",
            Aarch64Opnd::SimdImm => "SimdImm",
            Aarch64Opnd::SimdImmSft => "SimdImmSft",
            Aarch64Opnd::SimdFpimm => "SimdFpimm",
            Aarch64Opnd::ShllImm => "ShllImm",
            Aarch64Opnd::Imm0 => "Imm0",
            Aarch64Opnd::Fpimm0 => "Fpimm0",
            Aarch64Opnd::Fpimm => "Fpimm",
            Aarch64Opnd::Immr => "Immr",
            Aarch64Opnd::Imms => "Imms",
            Aarch64Opnd::Width => "Width",
            Aarch64Opnd::Imm => "Imm",
            Aarch64Opnd::Uimm3Op1 => "Uimm3Op1",
            Aarch64Opnd::Uimm3Op2 => "Uimm3Op2",
            Aarch64Opnd::Uimm4 => "Uimm4",
            Aarch64Opnd::Uimm7 => "Uimm7",
            Aarch64Opnd::BitNum => "BitNum",
            Aarch64Opnd::Exc => "Exc",
            Aarch64Opnd::CcmpImm => "CcmpImm",
            Aarch64Opnd::Nzcv => "Nzcv",
            Aarch64Opnd::Limm => "Limm",
            Aarch64Opnd::Aimm => "Aimm",
            Aarch64Opnd::Half => "Half",
            Aarch64Opnd::Fbits => "Fbits",
            Aarch64Opnd::ImmMov => "ImmMov",
            Aarch64Opnd::Cond => "Cond",
            Aarch64Opnd::Cond1 => "Cond1",
            Aarch64Opnd::AddrAdrp => "AddrAdrp",
            Aarch64Opnd::AddrPcrel14 => "AddrPcrel14",
            Aarch64Opnd::AddrPcrel19 => "AddrPcrel19",
            Aarch64Opnd::AddrPcrel21 => "AddrPcrel21",
            Aarch64Opnd::AddrPcrel26 => "AddrPcrel26",
            Aarch64Opnd::AddrSimple => "AddrSimple",
            Aarch64Opnd::AddrRegoff => "AddrRegoff",
            Aarch64Opnd::AddrSimm7 => "AddrSimm7",
            Aarch64Opnd::AddrSimm9 => "AddrSimm9",
            Aarch64Opnd::AddrSimm92 => "AddrSimm92",
            Aarch64Opnd::AddrUimm12 => "AddrUimm12",
            Aarch64Opnd::SimdAddrSimple => "SimdAddrSimple",
            Aarch64Opnd::SimdAddrPost => "SimdAddrPost",
            Aarch64Opnd::Sysreg => "Sysreg",
            Aarch64Opnd::Pstatefield => "Pstatefield",
            Aarch64Opnd::SysregAt => "SysregAt",
            Aarch64Opnd::SysregDc => "SysregDc",
            Aarch64Opnd::SysregIc => "SysregIc",
            Aarch64Opnd::SysregTlbi => "SysregTlbi",
            Aarch64Opnd::Barrier => "Barrier",
            Aarch64Opnd::BarrierIsb => "BarrierIsb",
            Aarch64Opnd::Prfop => "Prfop",
            Aarch64Opnd::BarrierPsb => "BarrierPsb",
        }
    }
}

pub(crate) const IMPLEMENTED_OPERAND_KINDS: &[&str] = &[
    "Rd",
    "Rn",
    "Rm",
    "RmSft",
    "RdSp",
    "RnSp",
    "Rt",
    "Rt2",
    "Rs",
    "Fd",
    "Fn",
    "Fm",
    "Fa",
    "Ft",
    "Aimm",
    "Limm",
    "Half",
    "ImmMov",
    "Imm",
    "Width",
    "BitNum",
    "CcmpImm",
    "Nzcv",
    "Cond",
    "Cond1",
    "Fpimm0",
    "Fpimm",
    "AddrSimm7",
    "AddrSimple",
    "AddrRegoff",
    "AddrSimm9",
    "AddrUimm12",
    "AddrPcrel14",
    "AddrPcrel19",
    "AddrPcrel21",
    "AddrPcrel26",
];

fn rd(word: Word) -> u8 {
    (word & 0x1f) as u8
}

fn rn(word: Word) -> u8 {
    ((word >> 5) & 0x1f) as u8
}

fn rm(word: Word) -> u8 {
    ((word >> 16) & 0x1f) as u8
}

fn rt(word: Word) -> u8 {
    rd(word)
}

fn rt2(word: Word) -> u8 {
    ((word >> 10) & 0x1f) as u8
}

fn rs(word: Word) -> u8 {
    ((word >> 16) & 0x1f) as u8
}

fn ra(word: Word) -> u8 {
    ((word >> 10) & 0x1f) as u8
}

fn rt_reg(word: Word, opcode: &Aarch64Opcode) -> Register {
    match opcode.mnemonic() {
        "str" | "ldr" if ((word >> 26) & 0x3f) == 0b111101 => fp_reg(rt(word), word),
        "ldr" if (word >> 31) & 1 == 0 && (word >> 30) & 1 == 0 => w_reg(rt(word)),
        "str" | "ldr" if ((word >> 30) & 0x3) == 0b10 => w_reg(rt(word)),
        "strb" | "ldrb" | "strh" | "ldrh" => w_reg(rt(word)),
        _ => x_reg(rt(word)),
    }
}

fn fp_reg(reg: u8, word: Word) -> Register {
    let class = if ((word >> 22) & 0x3) == 1 {
        RegisterClass::D
    } else {
        RegisterClass::S
    };

    Register { class, index: reg }
}

fn fpimm(word: Word) -> &'static str {
    match (word >> 13) & 0xff {
        0x70 => "1.00000000",
        _ => "<fpimm>",
    }
}

fn bit_num(word: Word) -> i64 {
    ((((word >> 31) & 1) << 5) | ((word >> 19) & 0x1f)) as i64
}

fn ccmp_imm(word: Word) -> i64 {
    ((word >> 16) & 0x1f) as i64
}

fn nzcv(word: Word) -> i64 {
    (word & 0xf) as i64
}

fn condition(word: Word) -> &'static str {
    condition_name(((word >> 12) & 0xf) as u8)
}

fn inverted_condition(word: Word) -> &'static str {
    condition_name((((word >> 12) & 0xf) as u8) ^ 1)
}

fn condition_name(condition: u8) -> &'static str {
    match condition {
        0x0 => "eq",
        0x1 => "ne",
        0x2 => "hs",
        0x3 => "lo",
        0x4 => "mi",
        0x5 => "pl",
        0x6 => "vs",
        0x7 => "vc",
        0x8 => "hi",
        0x9 => "ls",
        0xa => "ge",
        0xb => "lt",
        0xc => "gt",
        0xd => "le",
        0xe => "al",
        _ => "nv",
    }
}

fn aimm(word: Word) -> i64 {
    let imm = ((word >> 10) & 0xfff) as i64;
    if (word >> 22) & 1 == 0 {
        imm
    } else {
        imm << 12
    }
}

fn rm_shifted(word: Word) -> ShiftedRegister {
    ShiftedRegister {
        register: x_reg(rm(word)),
        shift: Shift {
            kind: match (word >> 22) & 0x3 {
                0 => ShiftKind::Lsl,
                1 => ShiftKind::Lsr,
                2 => ShiftKind::Asr,
                _ => ShiftKind::Ror,
            },
            amount: ((word >> 10) & 0x3f) as u8,
        },
    }
}

fn half(word: Word) -> ShiftedImmediate {
    ShiftedImmediate {
        value: ((word >> 5) & 0xffff) as i64,
        shift: (((word >> 21) & 0x3) * 16) as u8,
    }
}

fn imm_mov(word: Word) -> i64 {
    let value = ((word >> 5) & 0xffff) as i64;
    let shift = (((word >> 21) & 0x3) * 16) as u8;
    let shifted = value << shift;

    match (word >> 29) & 0x3 {
        0 => !shifted,
        _ => shifted,
    }
}

fn bitfield_imm(word: Word, opcode: &Aarch64Opcode) -> i64 {
    let immr = ((word >> 16) & 0x3f) as i64;
    let imms = ((word >> 10) & 0x3f) as i64;

    match opcode.mnemonic() {
        "lsl" => 63 - imms,
        "lsr" | "asr" => immr,
        "ubfx" | "bfxil" => immr,
        _ => immr,
    }
}

fn bitfield_width(word: Word, opcode: &Aarch64Opcode) -> i64 {
    let immr = ((word >> 16) & 0x3f) as i64;
    let imms = ((word >> 10) & 0x3f) as i64;

    match opcode.mnemonic() {
        "ubfx" | "bfxil" => imms - immr + 1,
        _ => imms + 1,
    }
}

fn logical_immediate(word: Word) -> Option<u64> {
    let n = ((word >> 22) & 1) as u8;
    let immr = ((word >> 16) & 0x3f) as u8;
    let imms = ((word >> 10) & 0x3f) as u8;
    let value = ((n as u8) << 6) | (!imms & 0x3f);
    let len = highest_set_bit(value)?;
    let size = 1u32 << len;
    let levels = size - 1;
    let s = (imms as u32) & levels;
    let r = (immr as u32) & levels;

    if s == levels {
        return None;
    }

    let pattern = ones(s + 1);
    let rotated = rotate_right_with_width(pattern, r, size);
    Some(replicate(rotated, size))
}

fn highest_set_bit(value: u8) -> Option<u32> {
    (0..=6).rev().find(|bit| value & (1 << bit) != 0)
}

fn ones(width: u32) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

fn rotate_right_with_width(value: u64, rotate: u32, width: u32) -> u64 {
    let mask = ones(width);
    let rotate = rotate % width;
    if rotate == 0 {
        return value & mask;
    }

    ((value >> rotate) | (value << (width - rotate))) & mask
}

fn replicate(value: u64, width: u32) -> u64 {
    let mut result = 0;
    let mut shift = 0;

    while shift < 64 {
        result |= value << shift;
        shift += width;
    }

    result
}

fn imm19(word: Word) -> i64 {
    sign_extend(((word >> 5) & 0x7ffff) as i64, 19) << 2
}

fn imm14(word: Word) -> i64 {
    sign_extend(((word >> 5) & 0x3fff) as i64, 14) << 2
}

fn imm21(word: Word) -> i64 {
    let immlo = ((word >> 29) & 0x3) as i64;
    let immhi = ((word >> 5) & 0x7ffff) as i64;
    sign_extend((immhi << 2) | immlo, 21)
}

fn imm26(word: Word) -> i64 {
    sign_extend((word & 0x03ff_ffff) as i64, 26) << 2
}

fn simm7_pair_offset(word: Word) -> i64 {
    sign_extend(((word >> 15) & 0x7f) as i64, 7) << 3
}

fn simm9_offset(word: Word) -> i64 {
    sign_extend(((word >> 12) & 0x1ff) as i64, 9)
}

fn simm9_addressing_mode(word: Word) -> AddressingMode {
    match (word >> 10) & 0x3 {
        0b01 => AddressingMode::PostIndex,
        0b11 => AddressingMode::PreIndex,
        _ => AddressingMode::Offset,
    }
}

fn uimm12_offset(word: Word) -> i64 {
    let size = ((word >> 30) & 0x3) as i64;
    let imm = ((word >> 10) & 0xfff) as i64;
    imm << size
}

fn regoff_shift(word: Word) -> Option<Shift> {
    let amount = if (word >> 12) & 1 == 0 {
        0
    } else {
        ((word >> 30) & 0x3) as u8
    };

    if amount == 0 {
        None
    } else {
        Some(Shift {
            kind: ShiftKind::Lsl,
            amount,
        })
    }
}

fn pair_addressing_mode(word: Word) -> AddressingMode {
    match (word >> 23) & 0x3 {
        0b01 => AddressingMode::PostIndex,
        0b11 => AddressingMode::PreIndex,
        _ => AddressingMode::Offset,
    }
}

fn sign_extend(value: i64, bits: u8) -> i64 {
    let shift = 64 - bits;
    (value << shift) >> shift
}

fn branch_target(address: u64, offset: i64) -> u64 {
    address.wrapping_add_signed(offset)
}

fn x_reg(reg: u8) -> Register {
    Register {
        class: RegisterClass::X,
        index: reg,
    }
}

pub(crate) fn w_reg(reg: u8) -> Register {
    Register {
        class: RegisterClass::W,
        index: reg,
    }
}

fn x_or_sp(reg: u8) -> Register {
    Register {
        class: RegisterClass::XOrSp,
        index: reg,
    }
}
