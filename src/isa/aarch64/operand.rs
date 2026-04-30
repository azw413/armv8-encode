use super::table::{Aarch64Opcode, Aarch64Opnd};
use super::Word;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Register {
    pub class: RegisterClass,
    pub index: u8,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VectorRegister {
    pub index: u8,
    pub arrangement: VectorArrangement,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VectorElement {
    pub index: u8,
    pub element: u8,
    pub size: VectorElementSize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VectorList {
    pub first: u8,
    pub count: u8,
    pub arrangement: VectorArrangement,
    pub element: Option<u8>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VectorArrangement {
    B8,
    B16,
    H4,
    H8,
    S2,
    S4,
    D1,
    D2,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VectorElementSize {
    B,
    H,
    S,
    D,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RegisterClass {
    B,
    H,
    S,
    D,
    W,
    X,
    WOrSp,
    XOrSp,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ShiftedRegister {
    pub register: Register,
    pub shift: Shift,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExtendedRegister {
    pub register: Register,
    pub extend: ExtendKind,
    pub amount: u8,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ExtendKind {
    Uxtb,
    Uxth,
    Uxtw,
    Uxtx,
    Sxtb,
    Sxth,
    Sxtw,
    Sxtx,
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
    VectorRegister(VectorRegister),
    VectorElement(VectorElement),
    VectorList(VectorList),
    ShiftedRegister(ShiftedRegister),
    ExtendedRegister(ExtendedRegister),
    Immediate(i64),
    UnsignedImmediate(u64),
    ShiftedImmediate(ShiftedImmediate),
    Memory(MemoryOperand),
    BranchTarget(u64),
    PageTarget(u64),
    System(String),
    Condition(&'static str),
    FloatImmediate(String),
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
    pub operand_index: usize,
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
            Aarch64Opnd::Rd => Ok(DecodedOperand::Register(rd_reg(ctx.word, ctx.opcode))),
            Aarch64Opnd::Rn => Ok(DecodedOperand::Register(rn_reg(ctx.word, ctx.opcode))),
            Aarch64Opnd::Rm => Ok(DecodedOperand::Register(rm_reg(ctx.word, ctx.opcode))),
            Aarch64Opnd::RmSft => Ok(DecodedOperand::ShiftedRegister(rm_shifted(ctx.word))),
            Aarch64Opnd::RmExt => Ok(DecodedOperand::ExtendedRegister(rm_extended(ctx.word))),
            Aarch64Opnd::RdSp => Ok(DecodedOperand::Register(gp_or_sp(rd(ctx.word), ctx.word))),
            Aarch64Opnd::RnSp => Ok(DecodedOperand::Register(gp_or_sp(rn(ctx.word), ctx.word))),
            Aarch64Opnd::Rt => Ok(DecodedOperand::Register(rt_reg(ctx.word, ctx.opcode))),
            Aarch64Opnd::Rt2 => Ok(DecodedOperand::Register(x_reg(rt2(ctx.word)))),
            Aarch64Opnd::Ra => Ok(DecodedOperand::Register(ra_reg(ctx.word, ctx.opcode))),
            Aarch64Opnd::Rs => Ok(DecodedOperand::Register(rs_reg(ctx.word, ctx.opcode))),
            Aarch64Opnd::Fd => Ok(DecodedOperand::Register(fp_reg(rd(ctx.word), ctx.word))),
            Aarch64Opnd::Fn => Ok(DecodedOperand::Register(fp_reg(rn(ctx.word), ctx.word))),
            Aarch64Opnd::Fm => Ok(DecodedOperand::Register(fp_reg(rm(ctx.word), ctx.word))),
            Aarch64Opnd::Fa => Ok(DecodedOperand::Register(fp_reg(ra(ctx.word), ctx.word))),
            Aarch64Opnd::Ft => Ok(DecodedOperand::Register(ft_reg(
                rt(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::Ft2 => Ok(DecodedOperand::Register(ft_reg(
                rt2(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::Vd => Ok(DecodedOperand::VectorRegister(vector_reg(
                rd(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::Vn => Ok(DecodedOperand::VectorRegister(vector_reg(
                rn(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::Vm => Ok(DecodedOperand::VectorRegister(vector_reg(
                rm(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::Sd => Ok(DecodedOperand::Register(simd_scalar_reg(
                rd(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::Sn => Ok(DecodedOperand::Register(simd_scalar_reg(
                rn(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::Sm => Ok(DecodedOperand::Register(simd_scalar_reg(
                rm(ctx.word),
                ctx.word,
                ctx.opcode,
            ))),
            Aarch64Opnd::VdD1 => Ok(DecodedOperand::VectorElement(VectorElement {
                index: rd(ctx.word),
                element: 1,
                size: VectorElementSize::D,
            })),
            Aarch64Opnd::VnD1 => Ok(DecodedOperand::VectorElement(VectorElement {
                index: rn(ctx.word),
                element: 1,
                size: VectorElementSize::D,
            })),
            Aarch64Opnd::Ed => Ok(DecodedOperand::VectorElement(ed(ctx.word))),
            Aarch64Opnd::En => Ok(DecodedOperand::VectorElement(en(ctx.word, ctx.opcode))),
            Aarch64Opnd::Em => Ok(DecodedOperand::VectorElement(em(ctx.word))),
            Aarch64Opnd::Lvn => Ok(DecodedOperand::VectorList(vector_list(ctx.word))),
            Aarch64Opnd::Lvt => Ok(DecodedOperand::VectorList(simd_ldst_list(ctx.word))),
            Aarch64Opnd::LvtAl => Ok(DecodedOperand::VectorList(simd_ldst_list(ctx.word))),
            Aarch64Opnd::Let => Ok(DecodedOperand::VectorList(simd_ldst_element_list(
                ctx.word, ctx.opcode,
            ))),
            Aarch64Opnd::Idx => Ok(DecodedOperand::Immediate(idx(ctx.word))),
            Aarch64Opnd::ImmVlsl => Ok(DecodedOperand::Immediate(imm_vlsl(ctx.word))),
            Aarch64Opnd::ImmVlsr => Ok(DecodedOperand::Immediate(imm_vlsr(ctx.word))),
            Aarch64Opnd::SimdImm => Ok(simd_imm(ctx.word)),
            Aarch64Opnd::SimdImmSft => Ok(DecodedOperand::ShiftedImmediate(simd_imm_sft(ctx.word))),
            Aarch64Opnd::SimdFpimm => Ok(DecodedOperand::FloatImmediate(simd_fpimm(ctx.word))),
            Aarch64Opnd::ShllImm => Ok(DecodedOperand::Immediate(shll_imm(ctx.word))),
            Aarch64Opnd::Imm0 => Ok(DecodedOperand::Immediate(0)),
            Aarch64Opnd::Immr => Ok(DecodedOperand::Immediate(immr(ctx.word))),
            Aarch64Opnd::Imms => Ok(DecodedOperand::Immediate(imms(ctx.word))),
            Aarch64Opnd::Aimm => Ok(DecodedOperand::Immediate(aimm(ctx.word))),
            Aarch64Opnd::Limm => Ok(DecodedOperand::Immediate(
                logical_immediate(ctx.word).ok_or(DecodeError::InvalidOperand("Limm"))?,
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
            Aarch64Opnd::Exc => Ok(DecodedOperand::Immediate(exc(ctx.word))),
            Aarch64Opnd::CcmpImm => Ok(DecodedOperand::Immediate(ccmp_imm(ctx.word))),
            Aarch64Opnd::Nzcv => Ok(DecodedOperand::Immediate(nzcv(ctx.word))),
            Aarch64Opnd::Uimm3Op1 => Ok(DecodedOperand::Immediate(uimm3_op1(ctx.word))),
            Aarch64Opnd::Uimm3Op2 => Ok(DecodedOperand::Immediate(uimm3_op2(ctx.word))),
            Aarch64Opnd::Uimm4 => Ok(DecodedOperand::Immediate(uimm4(ctx.word))),
            Aarch64Opnd::Uimm7 => Ok(DecodedOperand::Immediate(uimm7(ctx.word))),
            Aarch64Opnd::Cond => Ok(DecodedOperand::Condition(condition(ctx.word))),
            Aarch64Opnd::Cond1 => Ok(DecodedOperand::Condition(inverted_condition(ctx.word))),
            Aarch64Opnd::Fpimm0 => Ok(DecodedOperand::FloatImmediate("0.0".to_string())),
            Aarch64Opnd::Fpimm => Ok(DecodedOperand::FloatImmediate(fpimm(ctx.word))),
            Aarch64Opnd::Fbits => Ok(DecodedOperand::Immediate(fbits(ctx.word))),
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
            Aarch64Opnd::AddrAdrp => Ok(DecodedOperand::PageTarget(adrp_target(
                ctx.address,
                ctx.word,
            ))),
            Aarch64Opnd::SimdAddrSimple => Ok(DecodedOperand::Memory(MemoryOperand {
                base: x_or_sp(rn(ctx.word)),
                offset: MemoryOffset::None,
                mode: AddressingMode::Offset,
            })),
            Aarch64Opnd::SimdAddrPost => {
                Ok(DecodedOperand::Memory(simd_addr_post(ctx.word, ctx.opcode)))
            }
            Aarch64Opnd::Barrier => Ok(DecodedOperand::System(barrier(ctx.word))),
            Aarch64Opnd::BarrierIsb => Ok(DecodedOperand::System(barrier_isb(ctx.word))),
            Aarch64Opnd::BarrierPsb => Ok(DecodedOperand::System("csync".to_string())),
            Aarch64Opnd::Pstatefield => Ok(DecodedOperand::System(pstate_field(ctx.word))),
            Aarch64Opnd::Sysreg => Ok(DecodedOperand::System(sysreg(ctx.word))),
            Aarch64Opnd::SysregAt => Ok(DecodedOperand::System(sysreg_at(ctx.word))),
            Aarch64Opnd::SysregDc => Ok(DecodedOperand::System(sysreg_dc(ctx.word))),
            Aarch64Opnd::SysregIc => Ok(DecodedOperand::System(sysreg_ic(ctx.word))),
            Aarch64Opnd::SysregTlbi => Ok(DecodedOperand::System(sysreg_tlbi(ctx.word))),
            Aarch64Opnd::RtSys => Ok(DecodedOperand::Register(rt_sys_reg(ctx.word))),
            Aarch64Opnd::Cn => Ok(DecodedOperand::System(format!("c{}", cn(ctx.word)))),
            Aarch64Opnd::Cm => Ok(DecodedOperand::System(format!("c{}", cm(ctx.word)))),
            Aarch64Opnd::Prfop => Ok(DecodedOperand::System(prfop(ctx.word))),
            Aarch64Opnd::Pairreg => Ok(DecodedOperand::Register(pair_reg(
                ctx.word,
                ctx.opcode,
                ctx.operand_index,
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
    "RmExt",
    "RmSft",
    "RdSp",
    "RnSp",
    "Rt",
    "Rt2",
    "Ra",
    "Rs",
    "Fd",
    "Fn",
    "Fm",
    "Fa",
    "Ft",
    "Ft2",
    "Vd",
    "Vn",
    "Vm",
    "Sd",
    "Sn",
    "Sm",
    "VdD1",
    "VnD1",
    "Ed",
    "En",
    "Em",
    "Lvn",
    "Lvt",
    "LvtAl",
    "Let",
    "Idx",
    "ImmVlsl",
    "ImmVlsr",
    "SimdImm",
    "SimdImmSft",
    "SimdFpimm",
    "ShllImm",
    "Imm0",
    "Immr",
    "Imms",
    "Aimm",
    "Limm",
    "Half",
    "ImmMov",
    "Imm",
    "Width",
    "BitNum",
    "Exc",
    "CcmpImm",
    "Nzcv",
    "Uimm3Op1",
    "Uimm3Op2",
    "Uimm4",
    "Uimm7",
    "Cond",
    "Cond1",
    "Fpimm0",
    "Fpimm",
    "Fbits",
    "AddrSimm7",
    "AddrSimple",
    "AddrRegoff",
    "AddrSimm9",
    "AddrUimm12",
    "AddrAdrp",
    "AddrPcrel14",
    "AddrPcrel19",
    "AddrPcrel21",
    "AddrPcrel26",
    "SimdAddrSimple",
    "SimdAddrPost",
    "Barrier",
    "BarrierIsb",
    "BarrierPsb",
    "Cn",
    "Cm",
    "Pairreg",
    "Pstatefield",
    "Prfop",
    "RtSys",
    "Sysreg",
    "SysregAt",
    "SysregDc",
    "SysregIc",
    "SysregTlbi",
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

fn cn(word: Word) -> u8 {
    ((word >> 12) & 0xf) as u8
}

fn cm(word: Word) -> u8 {
    ((word >> 8) & 0xf) as u8
}

fn rt_reg(word: Word, opcode: &Aarch64Opcode) -> Register {
    match opcode.mnemonic() {
        mnemonic if is_casp(mnemonic) => lse_pair_reg(rt(word), word),
        "cbz" | "cbnz" => gp_reg(rt(word), word),
        "str" | "ldr" if ((word >> 26) & 0x3f) == 0b111101 => fp_reg(rt(word), word),
        "ldr" if (word >> 31) & 1 == 0 && (word >> 30) & 1 == 0 => w_reg(rt(word)),
        "str" | "ldr" if ((word >> 30) & 0x3) == 0b10 => w_reg(rt(word)),
        "strb" | "ldrb" | "strh" | "ldrh" | "sturb" | "ldurb" | "sturh" | "ldurh" => {
            w_reg(rt(word))
        }
        "stur" | "ldur" if ((word >> 30) & 0x3) == 0b10 => w_reg(rt(word)),
        _ => x_reg(rt(word)),
    }
}

fn rs_reg(word: Word, opcode: &Aarch64Opcode) -> Register {
    if is_casp(opcode.mnemonic()) {
        lse_pair_reg(rs(word), word)
    } else {
        w_reg(rs(word))
    }
}

fn rt_sys_reg(word: Word) -> Register {
    x_reg(rt(word))
}

fn rd_reg(word: Word, opcode: &Aarch64Opcode) -> Register {
    match opcode.mnemonic() {
        "adr" | "adrp" => x_reg(rd(word)),
        mnemonic if is_crc32(mnemonic) => w_reg(rd(word)),
        "mov" | "umov" | "smov" if opcode.class_name() == "Asimdins" => {
            match element_size_from_imm5((word >> 16) & 0x1f) {
                VectorElementSize::D => x_reg(rd(word)),
                _ => w_reg(rd(word)),
            }
        }
        _ => gp_reg(rd(word), word),
    }
}

fn rn_reg(word: Word, opcode: &Aarch64Opcode) -> Register {
    if has_32_bit_multiply_inputs(opcode.mnemonic()) || is_crc32(opcode.mnemonic()) {
        w_reg(rn(word))
    } else {
        gp_reg(rn(word), word)
    }
}

fn rm_reg(word: Word, opcode: &Aarch64Opcode) -> Register {
    if has_32_bit_multiply_inputs(opcode.mnemonic()) || is_32_bit_crc32_source(opcode.mnemonic()) {
        w_reg(rm(word))
    } else {
        gp_reg(rm(word), word)
    }
}

fn ra_reg(word: Word, _opcode: &Aarch64Opcode) -> Register {
    gp_reg(ra(word), word)
}

fn has_32_bit_multiply_inputs(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "smaddl" | "smsubl" | "smull" | "smnegl" | "umaddl" | "umsubl" | "umull" | "umnegl"
    )
}

fn is_crc32(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "crc32b" | "crc32h" | "crc32w" | "crc32x" | "crc32cb" | "crc32ch" | "crc32cw" | "crc32cx"
    )
}

fn is_32_bit_crc32_source(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "crc32b" | "crc32h" | "crc32w" | "crc32cb" | "crc32ch" | "crc32cw"
    )
}

fn is_casp(mnemonic: &str) -> bool {
    matches!(mnemonic, "casp" | "caspa" | "caspl" | "caspal")
}

fn pair_reg(word: Word, opcode: &Aarch64Opcode, operand_index: usize) -> Register {
    let even = match operand_index {
        1 => rs(word),
        3 => rt(word),
        _ => 31,
    };
    let _ = opcode.mnemonic();

    lse_pair_reg(even + 1, word)
}

fn lse_pair_reg(reg: u8, word: Word) -> Register {
    if (word >> 30) & 1 == 0 {
        w_reg(reg)
    } else {
        x_reg(reg)
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

fn ft_reg(reg: u8, word: Word, opcode: &Aarch64Opcode) -> Register {
    if matches!(
        opcode.class_name(),
        "LdstnapairOffs" | "LdstpairOff" | "LdstpairIndexed"
    ) {
        fp_load_store_reg(reg, word)
    } else {
        fp_reg(reg, word)
    }
}

fn vector_reg(reg: u8, word: Word, opcode: &Aarch64Opcode) -> VectorRegister {
    let arrangement = match opcode.class_name() {
        "Asimdimm" => simd_modified_immediate_arrangement(word),
        "Asimdins" if opcode.mnemonic() == "dup" => {
            arrangement_from_element_size_and_q(element_size_from_imm5((word >> 16) & 0x1f), word)
        }
        "Asimdshf" => vector_shift_arrangement(word),
        _ if opcode.mnemonic() == "shll" => shll_arrangement(word),
        _ => vector_arrangement(word),
    };

    VectorRegister {
        index: reg,
        arrangement,
    }
}

fn shll_arrangement(word: Word) -> VectorArrangement {
    match (word >> 22) & 0x3 {
        0 => VectorArrangement::H8,
        1 => VectorArrangement::S4,
        _ => VectorArrangement::D2,
    }
}

fn vector_list(word: Word) -> VectorList {
    VectorList {
        first: rn(word),
        count: (((word >> 13) & 0x3) + 1) as u8,
        arrangement: vector_arrangement(word),
        element: None,
    }
}

fn simd_ldst_list(word: Word) -> VectorList {
    let opcode = (word >> 12) & 0xf;
    VectorList {
        first: rt(word),
        count: if opcode == 0xa { 2 } else { 1 },
        arrangement: vector_arrangement_from_parts((word >> 30) & 0x1, (word >> 10) & 0x3),
        element: None,
    }
}

fn simd_ldst_element_list(word: Word, opcode: &Aarch64Opcode) -> VectorList {
    let (size, element) = simd_ldst_element(word);
    VectorList {
        first: rt(word),
        count: simd_ldst_element_count(opcode.mnemonic()),
        arrangement: arrangement_for_element_size(size),
        element: Some(element),
    }
}

fn simd_ldst_element_count(mnemonic: &str) -> u8 {
    match mnemonic {
        "ld2" | "st2" => 2,
        "ld3" | "st3" => 3,
        "ld4" | "st4" => 4,
        _ => 1,
    }
}

fn simd_ldst_element(word: Word) -> (VectorElementSize, u8) {
    let q = ((word >> 30) & 1) as u8;
    let s = ((word >> 12) & 1) as u8;
    let size = ((word >> 10) & 0x3) as u8;

    match size {
        3 => (
            VectorElementSize::B,
            (q << 3) | (s << 2) | (((word >> 10) & 0x3) as u8),
        ),
        2 => (
            VectorElementSize::H,
            (q << 2) | (s << 1) | (((word >> 11) & 1) as u8),
        ),
        0 => (VectorElementSize::S, (q << 1) | s),
        _ => (VectorElementSize::D, q),
    }
}

fn vector_arrangement(word: Word) -> VectorArrangement {
    vector_arrangement_from_parts((word >> 30) & 0x1, (word >> 22) & 0x3)
}

fn vector_arrangement_from_parts(q: u32, size: u32) -> VectorArrangement {
    match (q, size) {
        (0, 0) => VectorArrangement::B8,
        (1, 0) => VectorArrangement::B16,
        (0, 1) => VectorArrangement::H4,
        (1, 1) => VectorArrangement::H8,
        (0, 2) => VectorArrangement::S2,
        (1, 2) => VectorArrangement::S4,
        (0, 3) => VectorArrangement::D1,
        _ => VectorArrangement::D2,
    }
}

fn simd_modified_immediate_arrangement(word: Word) -> VectorArrangement {
    let q = (word >> 30) & 0x1;
    let op = (word >> 29) & 0x1;
    let cmode = (word >> 12) & 0xf;

    match cmode {
        0x8..=0xb => vector_arrangement_from_parts(q, 1),
        0xe if op == 0 => vector_arrangement_from_parts(q, 0),
        0xf if op == 1 => VectorArrangement::D2,
        _ => vector_arrangement_from_parts(q, 2),
    }
}

fn vector_shift_arrangement(word: Word) -> VectorArrangement {
    let q = (word >> 30) & 1;
    let immh = (word >> 19) & 0xf;
    let size = match immh {
        0b0001 => 0,
        0b0010 | 0b0011 => 1,
        0b0100..=0b0111 => 2,
        _ => 3,
    };

    vector_arrangement_from_parts(q, size)
}

fn simd_scalar_reg(reg: u8, word: Word, opcode: &Aarch64Opcode) -> Register {
    let class = if opcode.class_name() == "Asimdimm" {
        RegisterClass::D
    } else {
        match (word >> 22) & 0x3 {
            0 => RegisterClass::B,
            1 => RegisterClass::H,
            2 => RegisterClass::S,
            _ => RegisterClass::D,
        }
    };

    Register { class, index: reg }
}

fn ed(word: Word) -> VectorElement {
    element_from_imm5(rd(word), (word >> 16) & 0x1f)
}

fn en(word: Word, opcode: &Aarch64Opcode) -> VectorElement {
    let size = element_size_from_imm5((word >> 16) & 0x1f);
    let uses_source_imm4 = opcode.operands().first() == Some(&Aarch64Opnd::Ed);
    let element = if uses_source_imm4 {
        (((word >> 11) & 0xf) >> element_size_shift(size)) as u8
    } else if matches!(size, VectorElementSize::B) {
        (((word >> 16) & 0x1f) >> 1) as u8
    } else {
        (((word >> 16) & 0x1f) >> (element_size_shift(size) + 1)) as u8
    };

    VectorElement {
        index: rn(word),
        element,
        size,
    }
}

fn em(word: Word) -> VectorElement {
    let size = match (word >> 22) & 0x3 {
        1 => VectorElementSize::H,
        2 => VectorElementSize::S,
        _ => VectorElementSize::S,
    };
    let h = ((word >> 11) & 1) as u8;
    let l = ((word >> 21) & 1) as u8;
    let m = ((word >> 20) & 1) as u8;
    let element = match size {
        VectorElementSize::H => (h << 2) | (l << 1) | m,
        VectorElementSize::S => (h << 1) | l,
        VectorElementSize::D => h,
        VectorElementSize::B => 0,
    };

    VectorElement {
        index: rm(word),
        element,
        size,
    }
}

fn element_from_imm5(index: u8, imm5: u32) -> VectorElement {
    let size = element_size_from_imm5(imm5);
    VectorElement {
        index,
        element: (imm5 >> (element_size_shift(size) + 1)) as u8,
        size,
    }
}

fn element_size_from_imm5(imm5: u32) -> VectorElementSize {
    match imm5.trailing_zeros() {
        0 => VectorElementSize::B,
        1 => VectorElementSize::H,
        2 => VectorElementSize::S,
        _ => VectorElementSize::D,
    }
}

fn element_size_shift(size: VectorElementSize) -> u32 {
    match size {
        VectorElementSize::B => 0,
        VectorElementSize::H => 1,
        VectorElementSize::S => 2,
        VectorElementSize::D => 3,
    }
}

fn arrangement_for_element_size(size: VectorElementSize) -> VectorArrangement {
    match size {
        VectorElementSize::B => VectorArrangement::B8,
        VectorElementSize::H => VectorArrangement::H4,
        VectorElementSize::S => VectorArrangement::S2,
        VectorElementSize::D => VectorArrangement::D1,
    }
}

fn arrangement_from_element_size_and_q(size: VectorElementSize, word: Word) -> VectorArrangement {
    let q = (word >> 30) & 1 != 0;
    match (size, q) {
        (VectorElementSize::B, false) => VectorArrangement::B8,
        (VectorElementSize::B, true) => VectorArrangement::B16,
        (VectorElementSize::H, false) => VectorArrangement::H4,
        (VectorElementSize::H, true) => VectorArrangement::H8,
        (VectorElementSize::S, false) => VectorArrangement::S2,
        (VectorElementSize::S, true) => VectorArrangement::S4,
        (VectorElementSize::D, false) => VectorArrangement::D1,
        (VectorElementSize::D, true) => VectorArrangement::D2,
    }
}

fn fp_load_store_reg(reg: u8, word: Word) -> Register {
    let class = match (word >> 30) & 0x3 {
        0 => RegisterClass::S,
        _ => RegisterClass::D,
    };

    Register { class, index: reg }
}

fn fpimm(word: Word) -> String {
    format_fpimm8(((word >> 13) & 0xff) as u8)
}

fn format_fpimm8(imm8: u8) -> String {
    let sign = if imm8 & 0x80 == 0 { 1.0 } else { -1.0 };
    let high_exponent_bit = (imm8 >> 6) & 1;
    let low_exponent_bits = ((imm8 >> 4) & 0x3) as i32;
    let fraction = (imm8 & 0xf) as f64 / 16.0;
    let exponent = if high_exponent_bit == 0 {
        low_exponent_bits + 1
    } else {
        low_exponent_bits - 3
    };

    format!("{:.8}", sign * (1.0 + fraction) * 2f64.powi(exponent))
}

fn fbits(word: Word) -> i64 {
    64 - ((word >> 10) & 0x3f) as i64
}

fn immr(word: Word) -> i64 {
    ((word >> 16) & 0x3f) as i64
}

fn imms(word: Word) -> i64 {
    ((word >> 10) & 0x3f) as i64
}

fn bit_num(word: Word) -> i64 {
    ((((word >> 31) & 1) << 5) | ((word >> 19) & 0x1f)) as i64
}

fn exc(word: Word) -> i64 {
    ((word >> 5) & 0xffff) as i64
}

fn ccmp_imm(word: Word) -> i64 {
    ((word >> 16) & 0x1f) as i64
}

fn nzcv(word: Word) -> i64 {
    (word & 0xf) as i64
}

fn uimm3_op1(word: Word) -> i64 {
    ((word >> 16) & 0x7) as i64
}

fn uimm3_op2(word: Word) -> i64 {
    ((word >> 5) & 0x7) as i64
}

fn uimm4(word: Word) -> i64 {
    ((word >> 8) & 0xf) as i64
}

fn uimm7(word: Word) -> i64 {
    ((((word >> 8) & 0xf) << 3) | ((word >> 5) & 0x7)) as i64
}

fn shll_imm(word: Word) -> i64 {
    8 << ((word >> 22) & 0x3)
}

fn idx(word: Word) -> i64 {
    ((word >> 11) & if (word >> 30) & 1 == 0 { 0x7 } else { 0xf }) as i64
}

fn imm_vlsl(word: Word) -> i64 {
    let immh = (word >> 19) & 0xf;
    let immb = (word >> 16) & 0x7;
    let imm = (immh << 3) | immb;

    imm as i64 - vector_shift_element_width(immh) as i64
}

fn imm_vlsr(word: Word) -> i64 {
    let immh = (word >> 19) & 0xf;
    let immb = (word >> 16) & 0x7;
    let imm = (immh << 3) | immb;

    (vector_shift_element_width(immh) * 2) as i64 - imm as i64
}

fn vector_shift_element_width(immh: u32) -> u32 {
    match immh {
        0b0001 => 8,
        0b0010 | 0b0011 => 16,
        0b0100..=0b0111 => 32,
        _ => 64,
    }
}

fn simd_imm(word: Word) -> DecodedOperand {
    let imm8 = simd_imm8(word) as u64;
    let op = (word >> 29) & 1;
    let cmode = (word >> 12) & 0xf;

    if op == 1 && cmode == 0xe {
        let mut value = 0u64;
        for bit in 0..8 {
            if imm8 & (1 << bit) != 0 {
                value |= 0xffu64 << (bit * 8);
            }
        }
        DecodedOperand::UnsignedImmediate(value)
    } else {
        DecodedOperand::Immediate(imm8 as i64)
    }
}

fn simd_imm_sft(word: Word) -> ShiftedImmediate {
    let cmode = (word >> 12) & 0xf;
    let shift = match cmode {
        0x2 | 0x3 | 0xa | 0xb => 8,
        0x4 | 0x5 => 16,
        0x6 | 0x7 => 24,
        _ => 0,
    };

    ShiftedImmediate {
        value: simd_imm8(word) as i64,
        shift,
    }
}

fn simd_fpimm(word: Word) -> String {
    format_fpimm8(simd_imm8(word))
}

fn simd_imm8(word: Word) -> u8 {
    ((((word >> 16) & 0x7) << 5) | ((word >> 5) & 0x1f)) as u8
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
        register: gp_reg(rm(word), word),
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

fn rm_extended(word: Word) -> ExtendedRegister {
    let extend = match (word >> 13) & 0x7 {
        0 => ExtendKind::Uxtb,
        1 => ExtendKind::Uxth,
        2 => ExtendKind::Uxtw,
        3 => ExtendKind::Uxtx,
        4 => ExtendKind::Sxtb,
        5 => ExtendKind::Sxth,
        6 => ExtendKind::Sxtw,
        _ => ExtendKind::Sxtx,
    };
    let register = match extend {
        ExtendKind::Uxtx | ExtendKind::Sxtx => x_reg(rm(word)),
        _ => w_reg(rm(word)),
    };

    ExtendedRegister {
        register,
        extend,
        amount: ((word >> 10) & 0x7) as u8,
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
        0 if (word >> 31) & 1 == 0 => (((!shifted) & 0xffff_ffff) as i32) as i64,
        0 => !shifted,
        _ => shifted,
    }
}

fn bitfield_imm(word: Word, opcode: &Aarch64Opcode) -> i64 {
    let immr = ((word >> 16) & 0x3f) as i64;
    let imms = ((word >> 10) & 0x3f) as i64;
    let max = if (word >> 31) & 1 == 0 { 31 } else { 63 };

    match opcode.mnemonic() {
        "lsl" => max - imms,
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

fn logical_immediate(word: Word) -> Option<i64> {
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
    let value = replicate(rotated, size);
    if (word >> 31) & 1 == 0 {
        Some((value & 0xffff_ffff) as i64)
    } else {
        Some(value as i64)
    }
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

fn adrp_target(address: u64, word: Word) -> u64 {
    let page = address & !0xfff;
    page.wrapping_add_signed(imm21(word) << 12)
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

fn simd_addr_post(word: Word, opcode: &Aarch64Opcode) -> MemoryOperand {
    let post_register = rm(word);
    let offset = if post_register == 31 {
        MemoryOffset::Immediate(simd_post_index_immediate(word, opcode))
    } else {
        MemoryOffset::Register {
            register: x_reg(post_register),
            shift: None,
        }
    };

    MemoryOperand {
        base: x_or_sp(rn(word)),
        offset,
        mode: AddressingMode::PostIndex,
    }
}

fn simd_post_index_immediate(word: Word, opcode: &Aarch64Opcode) -> i64 {
    if opcode.operands().first() == Some(&Aarch64Opnd::Let) {
        let (size, _) = simd_ldst_element(word);
        return simd_ldst_element_count(opcode.mnemonic()) as i64 * element_size_bytes(size);
    }

    match (word >> 12) & 0xf {
        0xc => 1,
        0xa => 32,
        _ => 16,
    }
}

fn element_size_bytes(size: VectorElementSize) -> i64 {
    match size {
        VectorElementSize::B => 1,
        VectorElementSize::H => 2,
        VectorElementSize::S => 4,
        VectorElementSize::D => 8,
    }
}

fn barrier(word: Word) -> String {
    match uimm4(word) {
        0x1 => "oshld".to_string(),
        0x2 => "oshst".to_string(),
        0x3 => "osh".to_string(),
        0x5 => "nshld".to_string(),
        0x6 => "nshst".to_string(),
        0x7 => "nsh".to_string(),
        0x9 => "ishld".to_string(),
        0xa => "ishst".to_string(),
        0xb => "ish".to_string(),
        0xd => "ld".to_string(),
        0xe => "st".to_string(),
        0xf => "sy".to_string(),
        value => format!("#0x{value:x}"),
    }
}

fn barrier_isb(word: Word) -> String {
    match uimm4(word) {
        0xf => "sy".to_string(),
        value => format!("#{value}"),
    }
}

fn pstate_field(word: Word) -> String {
    match (((word >> 16) & 0x7), ((word >> 5) & 0x7)) {
        (3, 6) => "DAIFSet".to_string(),
        (3, 7) => "DAIFClr".to_string(),
        (op1, op2) => format!("pstate:{op1}:{op2}"),
    }
}

fn sysreg(word: Word) -> String {
    let op0 = (word >> 19) & 0x3;
    let op1 = (word >> 16) & 0x7;
    let crn = (word >> 12) & 0xf;
    let crm = (word >> 8) & 0xf;
    let op2 = (word >> 5) & 0x7;

    match (op0, op1, crn, crm, op2) {
        (3, 3, 4, 2, 0) => "NZCV".to_string(),
        (3, 3, 13, 0, 2) => "TPIDR_EL0".to_string(),
        (3, 3, 14, 0, 2) => "CNTVCT_EL0".to_string(),
        _ => format!("S{op0}_{op1}_C{crn}_C{crm}_{op2}"),
    }
}

fn sysreg_at(word: Word) -> String {
    match sys_op_fields(word) {
        (0, 7, 8, 0) => "s1e1r".to_string(),
        fields => format_sys_op("at", fields),
    }
}

fn sysreg_dc(word: Word) -> String {
    match sys_op_fields(word) {
        (3, 7, 4, 1) => "zva".to_string(),
        fields => format_sys_op("dc", fields),
    }
}

fn sysreg_ic(word: Word) -> String {
    match sys_op_fields(word) {
        (3, 7, 5, 1) => "ivau".to_string(),
        (0, 7, 5, 0) => "iallu".to_string(),
        fields => format_sys_op("ic", fields),
    }
}

fn sysreg_tlbi(word: Word) -> String {
    match sys_op_fields(word) {
        (0, 8, 7, 5) => "vale1".to_string(),
        (0, 8, 7, 0) => "vmalle1".to_string(),
        fields => format_sys_op("tlbi", fields),
    }
}

fn sys_op_fields(word: Word) -> (u32, u32, u32, u32) {
    (
        (word >> 16) & 0x7,
        (word >> 12) & 0xf,
        (word >> 8) & 0xf,
        (word >> 5) & 0x7,
    )
}

fn format_sys_op(prefix: &str, (op1, cn, cm, op2): (u32, u32, u32, u32)) -> String {
    format!("{prefix}:#{op1}:c{cn}:c{cm}:#{op2}")
}

fn prfop(word: Word) -> String {
    match rt(word) {
        0x00 => "pldl1keep".to_string(),
        0x01 => "pldl1strm".to_string(),
        0x02 => "pldl2keep".to_string(),
        0x03 => "pldl2strm".to_string(),
        0x04 => "pldl3keep".to_string(),
        0x05 => "pldl3strm".to_string(),
        0x08 => "plil1keep".to_string(),
        0x09 => "plil1strm".to_string(),
        0x0a => "plil2keep".to_string(),
        0x0b => "plil2strm".to_string(),
        0x0c => "plil3keep".to_string(),
        0x0d => "plil3strm".to_string(),
        0x10 => "pstl1keep".to_string(),
        0x11 => "pstl1strm".to_string(),
        0x12 => "pstl2keep".to_string(),
        0x13 => "pstl2strm".to_string(),
        0x14 => "pstl3keep".to_string(),
        0x15 => "pstl3strm".to_string(),
        value => format!("#0x{value:x}"),
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

fn gp_reg(reg: u8, word: Word) -> Register {
    if (word >> 31) & 1 == 0 {
        w_reg(reg)
    } else {
        x_reg(reg)
    }
}

fn gp_or_sp(reg: u8, word: Word) -> Register {
    if (word >> 31) & 1 == 0 {
        Register {
            class: RegisterClass::WOrSp,
            index: reg,
        }
    } else {
        x_or_sp(reg)
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
