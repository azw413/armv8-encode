//! Small hand-written AArch64 encoder/decoder prototype.
//!
//! This is a placeholder for the future generated/validated ISA layer. It
//! covers only a few arithmetic forms and should not be treated as complete.

#[derive(Debug, PartialEq)]
pub enum Instruction {
    ADC {
        sf: bool,
        rm: u8,
        rn: u8,
        rd: u8,
    },
    ADCS {
        sf: bool,
        rm: u8,
        rn: u8,
        rd: u8,
    },
    ADDExt {
        sf: bool,
        rm: u8,
        rn: u8,
        rd: u8,
        option: u8,
        imm3: u8,
    },
    ADDImm {
        sf: bool,
        rn: u8,
        rd: u8,
        sh: bool,
        imm12: u16,
    },
    Unknown,
}

pub fn decode(instr: u32) -> Instruction {
    match instr >> 23 & 0x1FF {
        0b_011_0100_0 => Instruction::ADC {
            sf: (instr >> 31) & 1 != 0,
            rm: (instr >> 16) as u8 & 0x1F,
            rn: (instr >> 5) as u8 & 0x1F,
            rd: instr as u8 & 0x1F,
        },
        0b_011_0101_0 => Instruction::ADCS {
            sf: (instr >> 31) & 1 != 0,
            rm: (instr >> 16) as u8 & 0x1F,
            rn: (instr >> 5) as u8 & 0x1F,
            rd: instr as u8 & 0x1F,
        },
        0b_001_0110_0 => Instruction::ADDExt {
            sf: (instr >> 31) & 1 != 0,
            rm: (instr >> 16) as u8 & 0x1F,
            rn: (instr >> 5) as u8 & 0x1F,
            rd: instr as u8 & 0x1F,
            option: (instr >> 13) as u8 & 0x7,
            imm3: (instr >> 10) as u8 & 0x7,
        },
        0b_100_1000_0 => Instruction::ADDImm {
            sf: (instr >> 31) & 1 != 0,
            rn: (instr >> 5) as u8 & 0x1F,
            rd: instr as u8 & 0x1F,
            sh: (instr >> 22) & 1 != 0,
            imm12: (instr >> 10) as u16 & 0xFFF,
        },
        _ => Instruction::Unknown,
    }
}

pub fn encode(instruction: &Instruction) -> Option<u32> {
    match instruction {
        Instruction::ADC { sf, rm, rn, rd } => {
            let op = 0;
            let s = 0;
            Some(
                ((*sf as u32) << 31)
                    | (0b01101000 << 23)
                    | ((*rm as u32) << 16)
                    | (op << 15)
                    | ((*rn as u32) << 5)
                    | (*rd as u32)
                    | (s << 29),
            )
        }
        Instruction::ADCS { sf, rm, rn, rd } => {
            let op = 0;
            let s = 1;
            Some(
                ((*sf as u32) << 31)
                    | (0b01101000 << 23)
                    | ((*rm as u32) << 16)
                    | (op << 15)
                    | ((*rn as u32) << 5)
                    | (*rd as u32)
                    | (s << 29),
            )
        }
        Instruction::ADDExt {
            sf,
            rm,
            option,
            imm3,
            rn,
            rd,
        } => {
            let op = 0;
            let s = 0;
            Some(
                ((*sf as u32) << 31)
                    | (0b01011001 << 23)
                    | ((*rm as u32) << 16)
                    | ((*option as u32) << 13)
                    | ((*imm3 as u32) << 10)
                    | ((*rn as u32) << 5)
                    | (*rd as u32)
                    | (op << 15)
                    | (s << 29),
            )
        }
        Instruction::ADDImm {
            sf,
            sh,
            imm12,
            rn,
            rd,
        } => {
            let op = 0;
            let s = 0;
            Some(
                ((*sf as u32) << 31)
                    | (0b10010000 << 23)
                    | ((*sh as u32) << 22)
                    | ((*imm12 as u32) << 10)
                    | ((*rn as u32) << 5)
                    | (*rd as u32)
                    | (op << 15)
                    | (s << 29),
            )
        }
        _ => None,
    }
}
