pub mod armv8;



use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::OpCode::B;
use crate::Operands::ADR26;
use crate::Sf::SF64;

mod tests;
mod aarch64;

type Register = u8;
type AddrPCRel26 = i32;
type AddrPCRel19 = i32;

#[derive(Debug)]
enum Sf {
    SF32,
    SF64
}

#[derive(Debug, FromPrimitive)]
enum Condition {
    EQ = 0x00,
    NE = 0x01,
    LT = 0x0b,
    LE = 0x0d,
    GT = 0x0c,
    GE = 0x0a,
}

#[derive(Debug)]
enum OpCode {
    ABS, ADC, ADCS, ADDHN2, ADDHN, ADDP, ADD, ADDS, ADDV, ADRP, ADR, AESD, AESE, AESIMC, AESMC, AND, ANDS, ASRV,
    B, B_C, BFM, BIC, BICS, BIF, BIT, BL, BLR, BRK, BR, BSL,
    CBNZ, CBZ, CCMN, CCMP, CLREX, CLS, CLZ, CMEQ, CMGE, CMGT, CMHI, CMHS, CMLE, CMLT, CMTST, CNT, CSEL, CSINC, CSINV, CSNEG,
    DCPS1, DCPS2, DCPS3, DMB, DRPS, DSB, DUP, EON, EOR, ERET, EXTR, EXT,
    FABD, FABS, FACGE, FACGT, FADD, FADDP, FCCMPE, FCCMP, FCMEQ, FCMGE, FCMGT, FCMLE, FCMLT, FCMPE, FCMP, FCSEL, FCVTAS, FCVTAU,
    FCVT, FCVTL2, FCVTL, FCVTMS, FCVTMU, FCVTN2, FCVTNS, FCVTNU, FCVTN, FCVTPS, FCVTPU, FCVTXN2, FCVTXN, FCVTZS, FCVTZU,
    FDIV, FMADD, FMAX, FMAXNM, FMAXNMP, FMAXNMV, FMAXP, FMAXV, FMIN, FMINNM, FMINNMP, FMINNMV, FMINP, FMINV, FMLA, FMLS,
    FMOV, FMSUB, FMUL, FMULX, FNEG, FNMADD, FNMSUB, FNMUL, FRECPE, FRECPS, FRECPX, FRINTA, FRINTI, FRINTM, FRINTN, FRINTP, FRINTX, FRINTZ,
    FRSQRTE, FRSQRTS, FSQRT, FSUB, HINT, HLT, HVC, INS, ISB,
    LD1, LD2, LD3, LD4, LDARB, LDARH, LDAR, LDAXP, LDAXRB, LDAXRH, LDAXR, LDNP, LDP, LDPSW, LDRB, LDR, LDRH, LDRSB, LDRSH, LDRSW, LDTRB,
    LDTRH, LDTR, LDTRSB, LDTRSH, LDTRSW, LDURB, LDUR, LDURH, LDURSB, LDURSH, LDURSW, LDXP, LDXRB, LDXRH, LDXR, LSLV, LSRV,
    MADD, MLA, MLS, MOVI, MOVK, MOVN, MOVZ, MRS, MSR, MSUB, MUL, MVNI, NEG, NOT, ORN, ORR,
    PMUL, PMULL2, PMULL, PRFM, PRFUM, RADDHN2, RADDHN, RBIT, RET, REV16, REV32, REV64, REV, RORV, RSHRN2, RSHRN, RSUBHN2, RSUBHN,
    SABAL2, SABAL, SABA, SABDL2, SABDL, SABD, SADALP, SADDL2, SADDLP, SADDL, SADDLV, SADDW2, SADDW, SBC, SBCS, SBFM,
    SCVTF, SDIV, SHA1C, SHA1H, SHA1M, SHA1P, SHA1SU0, SHA1SU1, SHA256H2, SHA256H, SHA256SU0, SHA256SU1, SHADD, SHLL2, SHLL, SHL, SHSUB,
    SLI, SMADDL, SMAXP, SMAX, SMAXV, SMC, SMINP, SMIN, SMINV, SMLAL2, SMLAL, SMLSL2, SMLSL, SMOV, SMSUBL, SMULH, SMULL2, SMULL,
    SQABS, SQADD, SQDMLAL2, SQDMLAL, SQDMLSL2, SQDMLSL, SQDMULH, SQDMULL2, SQDMULL, SQNEG, SQRDMULH, SQRSHL, SQRSHRN2, SQRSHRN,
    SQRSHRUN2, SQRSHRUN, SQSHL, SQSHLU, SQSHRN, SQSHRUN, SQSUB, SQXTN2, SQXTN, SQXTUN2, SQXTUN, SRHADD, SRI, SRSHL, SRSHR, SRSRA,
    SSHL, SSHR, SSRA, SSUBL2, SSUBL, SSUBW2, SSUBW, ST1, ST2, ST3, ST4, STLRB, STLRH, STLR, STLXP, STLXRB, STLXRH, STLXR, STNP,
    STP, STRB, STR, STRH, STTRB, STTRH, STTR, STURB, STUR, STURH, STXP, STXRB, STXRH, STXR, SUBHN2, SUBHN, SUB, SUBS,
    SUQADD, SVC, SYSL, SYS, TBL, TBNZ, TBX, TBZ, TRN1, TRN2,
    UABAL2, UABAL, UABA, UABDL2, UABDL, UABD, UADALP, UADDL2, UADDL, UADLV, UADDW2, UADDW, UBFM, UCVTF, UDIV, UHADD, UHSUB, UMADDL, UMAXP,
    UMAX, UMAXV, UMINP, UMIN, UMINV, UMLAL2, UMLAL, UMLSL2, UMLSL, UMOV, UMSUBL, UMULH, UMULL2, UMULL, UQADD, UQRSHL, UQRSHRN2,
    UQRSHRN, UQSHL, UQSHRN, UQSUB, UQXTN2, UQXTN, URECPE, URHADD, URSHL, URSHR, URSQRTE, URSRA, USHL, USHR, USQADD, USRA,
    USUBL2, USUBL, USUBW2, USUBW, UZP1, UZP2, XTN2, XTN, ZIP1, ZIP2, BAD
}

#[derive(Debug)]
enum Operands {
    NONE,
    R(Register),
    R_R(Register, Register),
    R_R_R(Register, Register, Register),
    ADR26(AddrPCRel26),
    ADR19(AddrPCRel19),
}

#[derive(Debug)]
struct Instruction {
    opcode: OpCode,
    operands: Operands,
    sf: Option<Sf>,
    condition: Option<Condition>,
}

fn Instruction(opcode: OpCode, operands: Operands) -> Instruction {
    Instruction { opcode, operands, sf: None, condition: None}
}

struct DisassemblyError {
    msg: String
}

impl Instruction {
    fn fromUInt32(i: u32) -> Result<Instruction, DisassemblyError>
    {
        let b = i.to_le_bytes();
        let op: u16 = (b[0] as u16) << 1 | (b[1] as u16) >> 7 ;
        print!("Bits 31 - 23:  {:#09b}   ", op);

        let mut operands: Operands = Operands::NONE;
        let mut condition = None;
        let mut sf: Option<Sf> = None;

        let opcode = match op {
            // Branches
            0b000101000 ..= 0b000101111 => OpCode::B,
            0b100101000 ..= 0b100101111 => OpCode::BL,
            0b010101000 | 0b010101001 => {  // Branch with condition
                let x = ((b[1] as AddrPCRel19) << 16) | ((b[2] as AddrPCRel19) << 8) | (((b[3] as AddrPCRel19) & 0xf0) >> 4) * 2;
                println!("{:06x} = {:02x} {:02x} {:02x}", x, b[1], b[2], b[3]);
                operands = Operands::ADR19(x);
                condition = Some(FromPrimitive::from_u8(b[3] & 0xf).expect("Invalid condition"));
                OpCode::B_C
            }
            0b110101100               => OpCode::BR,

            0b001101000 | 0b001101001 => OpCode::CBZ,
            0b001101010 | 0b001101011 => OpCode::CBNZ,
            0b101101000 | 0b101101001 => OpCode::CBZ,
            0b101101010 | 0b101101011 => OpCode::CBNZ,

            0b000100010 | 0b000100011 => OpCode::ADD,   // Immediate W
            0b100100010 | 0b100100011 => OpCode::ADD,   // Immediate X
            0b000010110 | 0b000010111 => OpCode::ADD,   // Shifted register W
            0b100010110 | 0b100010111 => OpCode::ADD,   // Shifted register X
            0b000010110               => OpCode::ADD,   // Extended register

            0b001111000 => OpCode::STR,
            _ => OpCode::BAD
        };

        Ok(Instruction { opcode, operands, sf: None, condition })
    }
}


type Block = Vec<Instruction>;







