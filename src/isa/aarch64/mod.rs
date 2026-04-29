//! AArch64 ISA support.
//!
//! This module is the home for AArch64-specific decoding and encoding. The
//! current implementation keeps the imported opcode table as the matching
//! foundation while the typed instruction and operand model is built out.

mod operand;
pub(crate) mod table;

use operand::{decode_operand, w_reg, DecodeContext, IMPLEMENTED_OPERAND_KINDS};
pub use operand::{
    AddressingMode, DecodeError, DecodedOperand, EncodeError, MemoryOffset, MemoryOperand,
    Register, RegisterClass, Shift, ShiftKind, ShiftedImmediate, ShiftedRegister,
};

/// Raw AArch64 instruction word.
pub type Word = u32;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OperandKindCoverage {
    pub table_operand_kinds: Vec<&'static str>,
    pub implemented: Vec<&'static str>,
    pub fixture_covered: Vec<&'static str>,
    pub unimplemented: Vec<&'static str>,
    pub implemented_but_uncovered: Vec<&'static str>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OpcodeClassSummary {
    pub row_count: usize,
    pub mnemonics: Vec<&'static str>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DecodedInstruction {
    pub address: u64,
    pub word: Word,
    pub mnemonic: &'static str,
    pub operands: Vec<DecodedOperand>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstructionTemplate {
    pub mnemonic: &'static str,
    pub operands: Vec<DecodedOperand>,
}

/// Decode one AArch64 instruction word using the opcode table.
pub fn decode_instruction(address: u64, word: Word) -> Option<DecodedInstruction> {
    decode_instruction_with_symbols(address, word, |_| None)
}

pub fn decode_instruction_with_symbols<F>(
    address: u64,
    word: Word,
    symbol_for_address: F,
) -> Option<DecodedInstruction>
where
    F: Fn(u64) -> Option<String>,
{
    let opcode = table::match_opcode(word)?;
    let mnemonic = opcode.mnemonic();
    let operands = opcode
        .operands()
        .into_iter()
        .map(|kind| {
            decode_operand(
                kind,
                DecodeContext {
                    word,
                    address,
                    opcode: &opcode,
                },
            )
            .unwrap_or(DecodedOperand::Unimplemented {
                kind: "decode-error",
            })
        })
        .collect();

    let decoded = DecodedInstruction {
        address,
        word,
        mnemonic,
        operands,
    };

    let _ = symbol_for_address;
    Some(decoded)
}

/// Encode one AArch64 instruction.
///
/// The public API shape exists so the encoder can grow alongside operand
/// codecs. Actual table-driven encoding is not implemented yet.
pub fn encode_instruction(_instruction: &InstructionTemplate) -> Result<Word, EncodeError> {
    Err(EncodeError::Unimplemented {
        kind: "instruction",
    })
}

/// Match raw AArch64 instruction words against the opcode table.
///
/// This is deliberately crate-visible for now: it is useful while validating
/// the table, but it is not yet a complete disassembler API because operands
/// are not decoded.
#[allow(dead_code)]
pub(crate) fn match_table(words: &[Word]) -> Vec<table::Aarch64Opcode> {
    table::disassemble(words)
}

#[allow(dead_code)]
pub(crate) fn match_opcode(word: Word) -> Option<table::Aarch64Opcode> {
    table::match_opcode(word)
}

pub fn operand_kind_coverage(fixture_words: &[Word]) -> OperandKindCoverage {
    let table_operand_kinds = sorted_unique(
        table::operand_kinds()
            .into_iter()
            .map(|kind| kind.name())
            .collect(),
    );
    let implemented = sorted_unique(IMPLEMENTED_OPERAND_KINDS.to_vec());
    let fixture_covered = sorted_unique(
        fixture_words
            .iter()
            .filter_map(|word| table::match_opcode(*word))
            .flat_map(|opcode| opcode.operands().into_iter().map(|kind| kind.name()))
            .collect(),
    );
    let unimplemented = table_operand_kinds
        .iter()
        .copied()
        .filter(|kind| !implemented.contains(kind))
        .collect();
    let implemented_but_uncovered = implemented
        .iter()
        .copied()
        .filter(|kind| !fixture_covered.contains(kind))
        .collect();

    OperandKindCoverage {
        table_operand_kinds,
        implemented,
        fixture_covered,
        unimplemented,
        implemented_but_uncovered,
    }
}

pub fn opcode_class_summary(class_name: &str) -> OpcodeClassSummary {
    OpcodeClassSummary {
        row_count: table::opcode_class_row_count(class_name),
        mnemonics: sorted_unique(table::opcode_class_mnemonics(class_name)),
    }
}

pub fn disassemble_at_with_symbols<F>(
    address: u64,
    word: Word,
    symbol_for_address: F,
) -> Option<DecodedInstruction>
where
    F: Fn(u64) -> Option<String>,
{
    decode_instruction_with_symbols(address, word, symbol_for_address)
}

pub fn disassemble_at(address: u64, word: Word) -> Option<DecodedInstruction> {
    decode_instruction(address, word)
}

impl DecodedInstruction {
    pub fn format_mnemonic(&self) -> String {
        match self.mnemonic {
            "beq" => "b.eq".to_string(),
            "bne" => "b.ne".to_string(),
            "bcs" => "b.hs".to_string(),
            "bcc" => "b.lo".to_string(),
            "bmi" => "b.mi".to_string(),
            "bpl" => "b.pl".to_string(),
            "bvs" => "b.vs".to_string(),
            "bvc" => "b.vc".to_string(),
            "bhi" => "b.hi".to_string(),
            "bls" => "b.ls".to_string(),
            "bge" => "b.ge".to_string(),
            "blt" => "b.lt".to_string(),
            "bgt" => "b.gt".to_string(),
            "ble" => "b.le".to_string(),
            mnemonic => mnemonic.to_string(),
        }
    }

    pub fn format_operands_with_symbols<F>(&self, symbol_for_address: F) -> String
    where
        F: Fn(u64) -> Option<String>,
    {
        format_operands(self.mnemonic, &self.operands, symbol_for_address)
    }

    pub fn format_operands(&self) -> String {
        self.format_operands_with_symbols(|_| None)
    }
}

fn format_operands<F>(mnemonic: &str, operands: &[DecodedOperand], symbol_for_address: F) -> String
where
    F: Fn(u64) -> Option<String>,
{
    match mnemonic {
        "add" | "sub" => format_operand_list(operands, Some("#")),
        "adc" | "adcs" | "sbc" | "sbcs" => format_operand_list(operands, None),
        "and" | "eor" | "orr" | "ands" => format_operand_list(operands, Some("#")),
        "movk" | "movz" | "movn" => format_operand_list(operands, Some("#")),
        "ubfx" | "bfxil" | "lsl" | "lsr" | "asr" => {
            format_operand_list_decimal(operands, Some("#"))
        }
        "adr" => format_operand_list_decimal(operands, Some("#")),
        "mov" if operands.iter().any(is_immediate_operand) => {
            format_operand_list(operands, Some("#"))
        }
        "mov" => format_operand_list(operands, None),
        "cbz" => format_operand_list_with_symbols(operands, &symbol_for_address),
        "b" | "beq" | "bne" | "bcs" | "bcc" | "bmi" | "bpl" | "bvs" | "bvc" | "bhi" | "bls"
        | "bge" | "blt" | "bgt" | "ble" | "bl" => operands
            .first()
            .map(|operand| format_operand_with_symbols(operand, &symbol_for_address, None))
            .unwrap_or_default(),
        "tbz" | "tbnz" => format_test_branch_operands(operands, &symbol_for_address),
        "br" | "blr" => format_operand_list(operands, None),
        "ldr" | "str" | "ldrsw" | "ldxr" | "stxr" | "ldur" | "stur" => {
            format_operand_list(operands, None)
        }
        "fadd" | "fsub" | "fmul" | "fdiv" | "fmadd" | "fmsub" | "fcsel" => {
            format_operand_list(operands, None)
        }
        "fcmp" => format_operand_list(operands, Some("#")),
        "fmov" => format_operand_list(operands, Some("#")),
        "scvtf" | "ucvtf" | "fcvtns" | "fcvtnu" | "fcvtas" | "fcvtau" | "fcvtps" | "fcvtpu"
        | "fcvtms" | "fcvtmu" | "fcvtzs" | "fcvtzu" => format_operand_list(operands, Some("#")),
        "svc" | "hvc" | "smc" | "brk" | "hlt" => format_exception_operands(operands),
        "dcps1" | "dcps2" | "dcps3" => match operands.first() {
            Some(DecodedOperand::Immediate(0)) | None => String::new(),
            _ => format_exception_operands(operands),
        },
        "ret" => match operands.first() {
            Some(DecodedOperand::Register(register)) if register.index == 30 => String::new(),
            Some(operand) => format_operand_with_symbols(operand, &symbol_for_address, None),
            None => String::new(),
        },
        "csel" | "cinc" => format_operand_list(operands, None),
        "ccmp" | "ccmn" => format_operand_list(operands, Some("#")),
        "stp" | "ldp" => {
            let Some((first, rest)) = operands.split_first() else {
                return String::new();
            };
            let Some((second, rest)) = rest.split_first() else {
                return format_operand_with_symbols(first, &symbol_for_address, None);
            };
            let Some(memory) = rest.first() else {
                return format!(
                    "{}, {}",
                    format_operand_with_symbols(first, &symbol_for_address, None),
                    format_operand_with_symbols(second, &symbol_for_address, None)
                );
            };

            format!(
                "{}, {}, {}",
                format_operand_with_symbols(first, &symbol_for_address, None),
                format_operand_with_symbols(second, &symbol_for_address, None),
                format_operand_with_symbols(memory, &symbol_for_address, None)
            )
        }
        _ => format_operand_list_with_symbols(operands, &symbol_for_address),
    }
}

fn format_test_branch_operands<F>(operands: &[DecodedOperand], symbol_for_address: &F) -> String
where
    F: Fn(u64) -> Option<String>,
{
    let Some((first, rest)) = operands.split_first() else {
        return String::new();
    };
    let formatted_first = match first {
        DecodedOperand::Register(register) if register.index < 32 => {
            format_register(&w_reg(register.index))
        }
        _ => format_operand_with_symbols(first, symbol_for_address, None),
    };

    let mut parts = vec![formatted_first];
    parts.extend(
        rest.iter()
            .map(|operand| format_operand_with_symbols(operand, symbol_for_address, Some("#"))),
    );
    parts.join(", ")
}

fn format_exception_operands(operands: &[DecodedOperand]) -> String {
    operands
        .iter()
        .map(|operand| match operand {
            DecodedOperand::Immediate(0) => "#0".to_string(),
            _ => format_operand_with_symbols(operand, &|_| None, Some("#")),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_operand_list(operands: &[DecodedOperand], immediate_prefix: Option<&str>) -> String {
    operands
        .iter()
        .map(|operand| format_operand_with_symbols(operand, &|_| None, immediate_prefix))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_operand_list_decimal(
    operands: &[DecodedOperand],
    immediate_prefix: Option<&str>,
) -> String {
    operands
        .iter()
        .map(|operand| format_operand_decimal(operand, immediate_prefix))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_immediate_operand(operand: &DecodedOperand) -> bool {
    matches!(
        operand,
        DecodedOperand::Immediate(_) | DecodedOperand::ShiftedImmediate(_)
    )
}

fn format_operand_list_with_symbols<F>(
    operands: &[DecodedOperand],
    symbol_for_address: &F,
) -> String
where
    F: Fn(u64) -> Option<String>,
{
    operands
        .iter()
        .map(|operand| format_operand_with_symbols(operand, symbol_for_address, None))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_operand_with_symbols<F>(
    operand: &DecodedOperand,
    symbol_for_address: &F,
    immediate_prefix: Option<&str>,
) -> String
where
    F: Fn(u64) -> Option<String>,
{
    match operand {
        DecodedOperand::Register(register) => format_register(register),
        DecodedOperand::ShiftedRegister(shifted) => {
            let register = format_register(&shifted.register);
            if shifted.shift.amount == 0 {
                register
            } else {
                format!(
                    "{register}, {} #{}",
                    format_shift_kind(shifted.shift.kind),
                    shifted.shift.amount
                )
            }
        }
        DecodedOperand::Immediate(value) => format!(
            "{}{}",
            immediate_prefix.unwrap_or_default(),
            format_hex(*value)
        ),
        DecodedOperand::ShiftedImmediate(immediate) => {
            let value = format!(
                "{}{}",
                immediate_prefix.unwrap_or_default(),
                format_hex(immediate.value)
            );
            if immediate.shift == 0 {
                value
            } else {
                format!("{value}, lsl #{}", immediate.shift)
            }
        }
        DecodedOperand::BranchTarget(target) => {
            symbol_for_address(*target).unwrap_or_else(|| format!("0x{target:x}"))
        }
        DecodedOperand::Condition(condition) => condition.to_string(),
        DecodedOperand::FloatImmediate(value) => {
            format!("{}{}", immediate_prefix.unwrap_or_default(), value)
        }
        DecodedOperand::Memory(memory) => {
            let base = format_register(&memory.base);
            match (&memory.offset, memory.mode) {
                (MemoryOffset::None, _) => format!("[{base}]"),
                (MemoryOffset::Immediate(offset), AddressingMode::Offset) => {
                    format!("[{base}, #{}]", format_hex(*offset))
                }
                (MemoryOffset::Immediate(offset), AddressingMode::PreIndex) => {
                    format!("[{base}, #{}]!", format_hex(*offset))
                }
                (MemoryOffset::Immediate(offset), AddressingMode::PostIndex) => {
                    format!("[{base}], #{}", format_hex(*offset))
                }
                (MemoryOffset::Register { register, shift }, _) => {
                    let register = format_register(register);
                    match shift {
                        Some(shift) => format!(
                            "[{base}, {register}, {} #{}]",
                            format_shift_kind(shift.kind),
                            shift.amount
                        ),
                        None => format!("[{base}, {register}]"),
                    }
                }
            }
        }
        DecodedOperand::Unimplemented { kind } => format!("<unimplemented:{kind}>"),
    }
}

fn format_operand_decimal(operand: &DecodedOperand, immediate_prefix: Option<&str>) -> String {
    match operand {
        DecodedOperand::Immediate(value) => {
            format!("{}{}", immediate_prefix.unwrap_or_default(), value)
        }
        _ => format_operand_with_symbols(operand, &|_| None, immediate_prefix),
    }
}

fn format_shift_kind(kind: ShiftKind) -> &'static str {
    match kind {
        ShiftKind::Lsl => "lsl",
        ShiftKind::Lsr => "lsr",
        ShiftKind::Asr => "asr",
        ShiftKind::Ror => "ror",
    }
}

fn format_register(register: &Register) -> String {
    match register.class {
        RegisterClass::S => format!("s{}", register.index),
        RegisterClass::D => format!("d{}", register.index),
        RegisterClass::W => format!("w{}", register.index),
        RegisterClass::X => format!("x{}", register.index),
        RegisterClass::XOrSp if register.index == 31 => "sp".to_string(),
        RegisterClass::XOrSp => format!("x{}", register.index),
    }
}

fn format_hex(value: i64) -> String {
    if value < 0 {
        format!("-0x{:x}", value.unsigned_abs())
    } else {
        format!("0x{value:x}")
    }
}

fn sorted_unique(mut values: Vec<&'static str>) -> Vec<&'static str> {
    values.sort_unstable();
    values.dedup();
    values
}
