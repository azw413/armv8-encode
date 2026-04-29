//! AArch64 ISA support.
//!
//! This module is the home for AArch64-specific decoding and encoding. The
//! current implementation keeps the imported opcode table as the matching
//! foundation while the typed instruction and operand model is built out.

mod operand;
mod simple;
pub(crate) mod table;

use operand::{decode_operand, DecodeContext, IMPLEMENTED_OPERAND_KINDS};
pub use operand::{
    AddressingMode, DecodeError, DecodedOperand, EncodeError, MemoryOperand, Register,
    RegisterClass,
};
pub use simple::{decode, encode, Instruction};

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
pub struct DecodedInstruction {
    pub address: u64,
    pub word: Word,
    pub mnemonic: &'static str,
    pub operands: Vec<DecodedOperand>,
}

/// Decode one raw AArch64 instruction word.
pub fn decode_word(word: Word) -> Instruction {
    decode(word)
}

/// Encode one placeholder AArch64 instruction.
pub fn encode_word(instruction: &Instruction) -> Option<Word> {
    encode(instruction)
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

pub fn disassemble_at_with_symbols<F>(
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

pub fn disassemble_at(address: u64, word: Word) -> Option<DecodedInstruction> {
    disassemble_at_with_symbols(address, word, |_| None)
}

impl DecodedInstruction {
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
        "mov" => format_operand_list(operands, None),
        "cbz" => format_operand_list_with_symbols(operands, &symbol_for_address),
        "bl" => operands
            .first()
            .map(|operand| format_operand_with_symbols(operand, &symbol_for_address, None))
            .unwrap_or_default(),
        "ret" => match operands.first() {
            Some(DecodedOperand::Register(register)) if register.index == 30 => String::new(),
            Some(operand) => format_operand_with_symbols(operand, &symbol_for_address, None),
            None => String::new(),
        },
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

fn format_operand_list(operands: &[DecodedOperand], immediate_prefix: Option<&str>) -> String {
    operands
        .iter()
        .map(|operand| format_operand_with_symbols(operand, &|_| None, immediate_prefix))
        .collect::<Vec<_>>()
        .join(", ")
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
        DecodedOperand::Immediate(value) => format!(
            "{}{}",
            immediate_prefix.unwrap_or_default(),
            format_hex(*value)
        ),
        DecodedOperand::BranchTarget(target) => {
            symbol_for_address(*target).unwrap_or_else(|| format!("0x{target:x}"))
        }
        DecodedOperand::Memory(memory) => {
            let base = format_register(&memory.base);
            let offset = format_hex(memory.offset);
            match memory.mode {
                AddressingMode::Offset => format!("[{base}, #{offset}]"),
                AddressingMode::PreIndex => format!("[{base}, #{offset}]!"),
                AddressingMode::PostIndex => format!("[{base}], #{offset}"),
            }
        }
        DecodedOperand::Unimplemented { kind } => format!("<unimplemented:{kind}>"),
    }
}

fn format_register(register: &Register) -> String {
    match register.class {
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
