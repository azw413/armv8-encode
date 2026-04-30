//! AArch64 ISA support.
//!
//! This module is the home for AArch64-specific decoding and encoding. The
//! current implementation keeps the imported opcode table as the matching
//! foundation while the typed instruction and operand model is built out.

mod operand;
pub(crate) mod table;

use operand::{
    decode_operand, encode_operand, w_reg, DecodeContext, EncodeContext, IMPLEMENTED_OPERAND_KINDS,
};
pub use operand::{
    AddressingMode, DecodeError, DecodedOperand, EncodeError, ExtendKind, ExtendedRegister,
    MemoryOffset, MemoryOperand, Register, RegisterClass, Shift, ShiftKind, ShiftedImmediate,
    ShiftedRegister, VectorArrangement, VectorElement, VectorElementSize, VectorList,
    VectorRegister,
};
pub use table::Aarch64Mnemonic;

/// Raw AArch64 instruction word.
pub type Word = u32;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OperandKindCoverage {
    pub table_operand_kinds: Vec<&'static str>,
    pub implemented: Vec<&'static str>,
    pub direct_tested: Vec<&'static str>,
    pub fixture_covered: Vec<&'static str>,
    pub unimplemented: Vec<&'static str>,
    pub implemented_but_not_direct_tested: Vec<&'static str>,
    pub implemented_but_uncovered: Vec<&'static str>,
    pub direct_tested_but_unimplemented: Vec<&'static str>,
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
    pub address: u64,
    pub mnemonic: Aarch64Mnemonic,
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
        .enumerate()
        .map(|(operand_index, kind)| {
            decode_operand(
                kind,
                DecodeContext {
                    word,
                    address,
                    opcode: &opcode,
                    operand_index,
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
/// Encoding is table-driven, but operand encoders are still being built out.
/// At this stage the encoder indexes candidate rows by mnemonic and reaches
/// the relevant operand codec, which may still report `Unimplemented`.
pub fn encode_instruction(instruction: &InstructionTemplate) -> Result<Word, EncodeError> {
    let candidates = table::opcodes_for_mnemonic(instruction.mnemonic);

    if candidates.is_empty() {
        return Err(EncodeError::UnknownMnemonic {
            mnemonic: instruction.mnemonic.table_name(),
        });
    }

    let mut last_error = None;
    for opcode in candidates {
        let operand_kinds = opcode.operands();
        if operand_kinds.len() != instruction.operands.len() {
            continue;
        }

        let mut word = opcode.base_opcode();
        let mut matched = true;
        for (kind, operand) in operand_kinds.into_iter().zip(&instruction.operands) {
            match encode_operand(
                kind,
                operand,
                EncodeContext {
                    base_word: word,
                    address: instruction.address,
                    opcode: &opcode,
                },
            ) {
                Ok(bits) => word |= bits,
                Err(EncodeError::InvalidOperand { kind }) => {
                    last_error = Some(EncodeError::InvalidOperand { kind });
                    matched = false;
                    break;
                }
                Err(EncodeError::Unimplemented { kind }) => {
                    last_error = Some(EncodeError::Unimplemented { kind });
                    matched = false;
                    break;
                }
                Err(error) => return Err(error),
            }
        }

        if matched {
            word |= encode_implicit_alias_operands(&opcode, &instruction.operands)?;
            if word & opcode.mask() != opcode.base_opcode() & opcode.mask() {
                continue;
            }
            return Ok(word);
        }
    }

    Err(last_error.unwrap_or(EncodeError::NoMatchingForm {
        mnemonic: instruction.mnemonic.table_name(),
    }))
}

fn encode_implicit_alias_operands(
    opcode: &table::Aarch64Opcode,
    operands: &[DecodedOperand],
) -> Result<Word, EncodeError> {
    match opcode.mnemonic() {
        "cinc" => {
            let Some(DecodedOperand::Register(rn)) = operands.get(1) else {
                return Err(EncodeError::InvalidOperand { kind: "Rn" });
            };
            if !matches!(rn.class, RegisterClass::W | RegisterClass::X) || rn.index > 31 {
                return Err(EncodeError::InvalidOperand { kind: "Rn" });
            }

            Ok((rn.index as Word) << 16)
        }
        _ => Ok(0),
    }
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

pub fn operand_kind_coverage(
    fixture_words: &[Word],
    direct_tested_kinds: &[&'static str],
) -> OperandKindCoverage {
    let table_operand_kinds = sorted_unique(
        table::operand_kinds()
            .into_iter()
            .map(|kind| kind.name())
            .collect(),
    );
    let implemented = sorted_unique(IMPLEMENTED_OPERAND_KINDS.to_vec());
    let direct_tested = sorted_unique(direct_tested_kinds.to_vec());
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
    let implemented_but_not_direct_tested = implemented
        .iter()
        .copied()
        .filter(|kind| !direct_tested.contains(kind))
        .collect();
    let implemented_but_uncovered = implemented
        .iter()
        .copied()
        .filter(|kind| !fixture_covered.contains(kind) && !direct_tested.contains(kind))
        .collect();
    let direct_tested_but_unimplemented = direct_tested
        .iter()
        .copied()
        .filter(|kind| !implemented.contains(kind))
        .collect();

    OperandKindCoverage {
        table_operand_kinds,
        implemented,
        direct_tested,
        fixture_covered,
        unimplemented,
        implemented_but_not_direct_tested,
        implemented_but_uncovered,
        direct_tested_but_unimplemented,
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
            mnemonic => {
                if let Some(DecodedOperand::VectorRegister(register)) = self.operands.first() {
                    format!(
                        "{mnemonic}.{}",
                        format_vector_arrangement(register.arrangement)
                    )
                } else if let Some(DecodedOperand::VectorList(list)) = self.operands.first() {
                    if list.element.is_some() {
                        format!(
                            "{mnemonic}.{}",
                            format_vector_element_arrangement(list.arrangement)
                        )
                    } else {
                        format!("{mnemonic}.{}", format_vector_arrangement(list.arrangement))
                    }
                } else if let Some(size) = self.operands.iter().find_map(vector_element_size) {
                    format!("{mnemonic}.{}", format_vector_element_size(size))
                } else {
                    mnemonic.to_string()
                }
            }
        }
    }

    pub fn format_operands_with_symbols<F>(&self, symbol_for_address: F) -> String
    where
        F: Fn(u64) -> Option<String>,
    {
        if self.mnemonic == "adrp" {
            return format_adrp_operands(self.address, &self.operands, &symbol_for_address);
        }

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
        "add" | "adds" | "cmn" | "sub" | "subs" | "cmp" => format_operand_list(operands, Some("#")),
        "adc" | "adcs" | "sbc" | "sbcs" => format_operand_list(operands, None),
        "and" | "eor" | "orr" | "ands" => format_operand_list(operands, Some("#")),
        "movk" | "movz" | "movn" => format_operand_list(operands, Some("#")),
        "ubfx" | "bfxil" | "lsl" | "lsr" | "asr" => {
            format_operand_list_decimal(operands, Some("#"))
        }
        "shll" => format_operand_list_decimal(operands, Some("#")),
        "sshr" | "shl" | "sqshrun" => format_operand_list(operands, Some("#")),
        "ext" | "extr" => format_operand_list(operands, Some("#")),
        "movi" | "mvni" => format_operand_list(operands, Some("#")),
        "adr" => format_operand_list_decimal(operands, Some("#")),
        "adrp" => format_adrp_operands(0, operands, &symbol_for_address),
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
        "ld1" | "ld2" | "ld3" | "ld4" | "st1" | "st2" | "st3" | "st4" | "ld1r" => {
            format_simd_ldst_operands(operands)
        }
        "prfm" => format_operand_list(operands, None),
        "fadd" | "fsub" | "fmul" | "fdiv" | "fmadd" | "fmsub" | "fcsel" => {
            format_operand_list(operands, None)
        }
        "fcmp" => format_operand_list(operands, Some("#")),
        "cmeq" => format_exception_operands(operands),
        "fmov" => format_operand_list(operands, Some("#")),
        "scvtf" | "ucvtf" | "fcvtns" | "fcvtnu" | "fcvtas" | "fcvtau" | "fcvtps" | "fcvtpu"
        | "fcvtms" | "fcvtmu" | "fcvtzs" | "fcvtzu" => format_operand_list(operands, Some("#")),
        "svc" | "hvc" | "smc" | "brk" | "hlt" | "hint" => format_exception_operands(operands),
        "msr" => format_operand_list(operands, Some("#")),
        "sys" => format_sys_operands(operands),
        "sysl" => format_operand_list(operands, Some("#")),
        "at" | "dc" => format_operand_list(operands, None),
        "ic" | "tlbi" => format_optional_default_register(operands, 31),
        "clrex" => format_optional_default_immediate(operands, 0xf),
        "dsb" | "dmb" => format_operand_list(operands, None),
        "isb" => format_isb_operands(operands),
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
    let bit_num = rest.first();
    let formatted_first = match (first, bit_num) {
        (DecodedOperand::Register(register), Some(DecodedOperand::Immediate(bit_num)))
            if register.index < 32 && *bit_num < 32 =>
        {
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

fn format_optional_default_immediate(operands: &[DecodedOperand], default: i64) -> String {
    match operands.first() {
        Some(DecodedOperand::Immediate(value)) if *value == default => String::new(),
        Some(operand) => format_operand_with_symbols(operand, &|_| None, Some("#")),
        None => String::new(),
    }
}

fn format_optional_default_register(operands: &[DecodedOperand], default: u8) -> String {
    let operands = match operands.last() {
        Some(DecodedOperand::Register(register))
            if register.class == RegisterClass::X && register.index == default =>
        {
            &operands[..operands.len() - 1]
        }
        _ => operands,
    };

    format_operand_list(operands, None)
}

fn format_isb_operands(operands: &[DecodedOperand]) -> String {
    match operands.first() {
        Some(DecodedOperand::System(value)) if value == "sy" => String::new(),
        Some(operand) => format_operand_with_symbols(operand, &|_| None, None),
        None => String::new(),
    }
}

fn format_sys_operands(operands: &[DecodedOperand]) -> String {
    let operands = match operands.last() {
        Some(DecodedOperand::Register(register))
            if register.class == RegisterClass::X && register.index == 31 =>
        {
            &operands[..operands.len() - 1]
        }
        _ => operands,
    };

    format_operand_list(operands, Some("#"))
}

fn format_simd_ldst_operands(operands: &[DecodedOperand]) -> String {
    operands
        .iter()
        .map(|operand| match operand {
            DecodedOperand::Memory(memory) => format_simd_memory(memory),
            _ => format_operand_with_symbols(operand, &|_| None, None),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_simd_memory(memory: &MemoryOperand) -> String {
    let base = format_register(&memory.base);
    match (&memory.offset, memory.mode) {
        (MemoryOffset::None, _) => format!("[{base}]"),
        (MemoryOffset::Immediate(offset), AddressingMode::PostIndex) => {
            format!("[{base}], #{offset}")
        }
        (
            MemoryOffset::Register {
                register,
                shift: None,
            },
            AddressingMode::PostIndex,
        ) => {
            format!("[{base}], {}", format_register(register))
        }
        _ => format_operand_with_symbols(&DecodedOperand::Memory(memory.clone()), &|_| None, None),
    }
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
        DecodedOperand::Immediate(_)
            | DecodedOperand::ShiftedImmediate(_)
            | DecodedOperand::UnsignedImmediate(_)
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
        DecodedOperand::VectorRegister(register) => format!("v{}", register.index),
        DecodedOperand::VectorElement(element) => {
            format!("v{}[{}]", element.index, element.element)
        }
        DecodedOperand::VectorList(list) => format_vector_list(list),
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
        DecodedOperand::ExtendedRegister(extended) => {
            let register = format_register(&extended.register);
            let extend = format_extend_kind(extended.extend);
            if extended.amount == 0 {
                format!("{register}, {extend}")
            } else {
                format!("{register}, {extend} #{}", extended.amount)
            }
        }
        DecodedOperand::Immediate(value) => format!(
            "{}{}",
            immediate_prefix.unwrap_or_default(),
            format_hex(*value)
        ),
        DecodedOperand::UnsignedImmediate(value) => {
            format!("{}0x{value:x}", immediate_prefix.unwrap_or_default(),)
        }
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
        DecodedOperand::PageTarget(target) => {
            symbol_for_address(*target).unwrap_or_else(|| format_page_target(*target))
        }
        DecodedOperand::System(value) => value.clone(),
        DecodedOperand::Condition(condition) => condition.to_string(),
        DecodedOperand::FloatImmediate(value) => {
            format!("{}{}", immediate_prefix.unwrap_or_default(), value)
        }
        DecodedOperand::Memory(memory) => {
            let base = format_register(&memory.base);
            match (&memory.offset, memory.mode) {
                (MemoryOffset::None, _) => format!("[{base}]"),
                (MemoryOffset::Immediate(0), AddressingMode::Offset) => format!("[{base}]"),
                (MemoryOffset::Immediate(offset), AddressingMode::Offset) => {
                    format!("[{base}, #{}]", format_hex(*offset))
                }
                (MemoryOffset::Immediate(offset), AddressingMode::PreIndex) => {
                    format!("[{base}, #{}]!", format_hex(*offset))
                }
                (MemoryOffset::Immediate(offset), AddressingMode::PostIndex) => {
                    format!("[{base}], #{}", format_hex(*offset))
                }
                (MemoryOffset::Register { register, shift }, AddressingMode::PostIndex) => {
                    let register = format_register(register);
                    match shift {
                        Some(shift) => format!(
                            "[{base}], {register}, {} #{}",
                            format_shift_kind(shift.kind),
                            shift.amount
                        ),
                        None => format!("[{base}], {register}"),
                    }
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

fn format_adrp_operands<F>(
    address: u64,
    operands: &[DecodedOperand],
    symbol_for_address: &F,
) -> String
where
    F: Fn(u64) -> Option<String>,
{
    let Some((first, rest)) = operands.split_first() else {
        return String::new();
    };
    let Some(second) = rest.first() else {
        return format_operand_with_symbols(first, symbol_for_address, None);
    };

    format!(
        "{}, {}",
        format_operand_with_symbols(first, symbol_for_address, None),
        format_adrp_target(address, second, symbol_for_address)
    )
}

fn format_adrp_target<F>(address: u64, operand: &DecodedOperand, symbol_for_address: &F) -> String
where
    F: Fn(u64) -> Option<String>,
{
    match operand {
        DecodedOperand::PageTarget(target) => symbol_for_address(*target)
            .unwrap_or_else(|| format_page_target_from_address(address, *target)),
        _ => format_operand_with_symbols(operand, symbol_for_address, None),
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

fn format_extend_kind(kind: ExtendKind) -> &'static str {
    match kind {
        ExtendKind::Uxtb => "uxtb",
        ExtendKind::Uxth => "uxth",
        ExtendKind::Uxtw => "uxtw",
        ExtendKind::Uxtx => "uxtx",
        ExtendKind::Sxtb => "sxtb",
        ExtendKind::Sxth => "sxth",
        ExtendKind::Sxtw => "sxtw",
        ExtendKind::Sxtx => "sxtx",
    }
}

fn format_vector_arrangement(arrangement: VectorArrangement) -> &'static str {
    match arrangement {
        VectorArrangement::B8 => "8b",
        VectorArrangement::B16 => "16b",
        VectorArrangement::H4 => "4h",
        VectorArrangement::H8 => "8h",
        VectorArrangement::S2 => "2s",
        VectorArrangement::S4 => "4s",
        VectorArrangement::D1 => "1d",
        VectorArrangement::D2 => "2d",
    }
}

fn format_vector_element_arrangement(arrangement: VectorArrangement) -> &'static str {
    match arrangement {
        VectorArrangement::B8 | VectorArrangement::B16 => "b",
        VectorArrangement::H4 | VectorArrangement::H8 => "h",
        VectorArrangement::S2 | VectorArrangement::S4 => "s",
        VectorArrangement::D1 | VectorArrangement::D2 => "d",
    }
}

fn format_vector_element_size(size: VectorElementSize) -> &'static str {
    match size {
        VectorElementSize::B => "b",
        VectorElementSize::H => "h",
        VectorElementSize::S => "s",
        VectorElementSize::D => "d",
    }
}

fn vector_element_size(operand: &DecodedOperand) -> Option<VectorElementSize> {
    match operand {
        DecodedOperand::VectorElement(element) => Some(element.size),
        _ => None,
    }
}

fn format_vector_list(list: &VectorList) -> String {
    let registers = (0..list.count)
        .map(|offset| format!("v{}", (list.first + offset) & 0x1f))
        .collect::<Vec<_>>()
        .join(", ");
    match list.element {
        Some(element) => format!("{{ {registers} }}[{element}]"),
        None => format!("{{ {registers} }}"),
    }
}

fn format_page_target(target: u64) -> String {
    let signed_target = target as i64;
    format!("{} ; 0x{target:x}", signed_target >> 12)
}

fn format_page_target_from_address(address: u64, target: u64) -> String {
    let signed_delta = target.wrapping_sub(address & !0xfff) as i64;
    format!("{} ; 0x{target:x}", signed_delta >> 12)
}

fn format_register(register: &Register) -> String {
    match register.class {
        RegisterClass::B => format!("b{}", register.index),
        RegisterClass::H => format!("h{}", register.index),
        RegisterClass::S => format!("s{}", register.index),
        RegisterClass::D => format!("d{}", register.index),
        RegisterClass::W if register.index == 31 => "wzr".to_string(),
        RegisterClass::W => format!("w{}", register.index),
        RegisterClass::X if register.index == 31 => "xzr".to_string(),
        RegisterClass::X => format!("x{}", register.index),
        RegisterClass::WOrSp if register.index == 31 => "sp".to_string(),
        RegisterClass::WOrSp => format!("w{}", register.index),
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
