use super::table::{Aarch64Opcode, Aarch64Opnd};
use super::Word;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Register {
    pub class: RegisterClass,
    pub index: u8,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RegisterClass {
    X,
    XOrSp,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemoryOperand {
    pub base: Register,
    pub offset: i64,
    pub mode: AddressingMode,
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
    Immediate(i64),
    Memory(MemoryOperand),
    BranchTarget(u64),
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
            Aarch64Opnd::RdSp => Ok(DecodedOperand::Register(x_or_sp(rd(ctx.word)))),
            Aarch64Opnd::RnSp => Ok(DecodedOperand::Register(x_or_sp(rn(ctx.word)))),
            Aarch64Opnd::Rt => Ok(DecodedOperand::Register(x_reg(rt(ctx.word)))),
            Aarch64Opnd::Rt2 => Ok(DecodedOperand::Register(x_reg(rt2(ctx.word)))),
            Aarch64Opnd::Aimm => Ok(DecodedOperand::Immediate(aimm(ctx.word))),
            Aarch64Opnd::AddrSimm7 => Ok(DecodedOperand::Memory(MemoryOperand {
                base: x_or_sp(rn(ctx.word)),
                offset: simm7_pair_offset(ctx.word),
                mode: pair_addressing_mode(ctx.word),
            })),
            Aarch64Opnd::AddrPcrel19 => Ok(DecodedOperand::BranchTarget(branch_target(
                ctx.address,
                imm19(ctx.word),
            ))),
            Aarch64Opnd::AddrPcrel26 => Ok(DecodedOperand::BranchTarget(branch_target(
                ctx.address,
                imm26(ctx.word),
            ))),
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
    "RdSp",
    "RnSp",
    "Rt",
    "Rt2",
    "Aimm",
    "AddrSimm7",
    "AddrPcrel19",
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

fn aimm(word: Word) -> i64 {
    let imm = ((word >> 10) & 0xfff) as i64;
    if (word >> 22) & 1 == 0 {
        imm
    } else {
        imm << 12
    }
}

fn imm19(word: Word) -> i64 {
    sign_extend(((word >> 5) & 0x7ffff) as i64, 19) << 2
}

fn imm26(word: Word) -> i64 {
    sign_extend((word & 0x03ff_ffff) as i64, 26) << 2
}

fn simm7_pair_offset(word: Word) -> i64 {
    sign_extend(((word >> 15) & 0x7f) as i64, 7) << 3
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

fn x_or_sp(reg: u8) -> Register {
    Register {
        class: RegisterClass::XOrSp,
        index: reg,
    }
}
