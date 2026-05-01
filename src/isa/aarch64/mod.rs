//! AArch64 ISA support.
//!
//! This module is the home for AArch64-specific decoding and encoding. The
//! current implementation keeps the imported opcode table as the matching
//! foundation while the typed instruction and operand model is built out.

mod control_flow;
mod operand;
mod recursive;
mod sweep;
pub(crate) mod table;

pub use control_flow::{invert_conditional_branch, pcrel_range_bytes};
pub use recursive::{
    disassemble_recursive, DataRange, DataReason, Disassembly, TimelineEntry,
};
pub use sweep::{disassemble_bytes, DisassembleError};

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
    pub mnemonic: Aarch64Mnemonic,
    pub operands: Vec<DecodedOperand>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstructionTemplate {
    pub address: u64,
    pub mnemonic: Aarch64Mnemonic,
    pub operands: Vec<DecodedOperand>,
}

/// Decode one AArch64 instruction word using the opcode table.
pub fn decode_instruction(address: u64, word: Word) -> Result<DecodedInstruction, DecodeError> {
    decode_instruction_with_symbols(address, word, |_| None)
}

pub fn decode_instruction_with_symbols<F>(
    address: u64,
    word: Word,
    symbol_for_address: F,
) -> Result<DecodedInstruction, DecodeError>
where
    F: Fn(u64) -> Option<String>,
{
    let opcode = table::match_opcode(word).ok_or(DecodeError::NoMatchingOpcode { word })?;
    let mnemonic = Aarch64Mnemonic::parse(opcode.mnemonic());
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
        })
        .collect::<Result<Vec<_>, _>>()?;

    let _ = symbol_for_address;
    Ok(DecodedInstruction {
        address,
        word,
        mnemonic,
        operands,
    })
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
            mnemonic: instruction.mnemonic.as_str(),
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
            word |= alias_implicit_bits(instruction.mnemonic, &instruction.operands)?;

            // The operand encoders may legitimately produce bits that don't
            // satisfy the candidate row's mask — multiple opcode rows can
            // share a mnemonic and only one is the right form. Record the
            // mismatch as a fallback error and try the next candidate, so
            // that if *no* row matches, the caller gets an explicit error
            // instead of `NoMatchingForm` hiding a near-miss.
            let masked = word & opcode.mask();
            let expected = opcode.base_opcode() & opcode.mask();
            if masked != expected {
                last_error.get_or_insert(EncodeError::InvalidOperand { kind: "<mask>" });
                continue;
            }
            return Ok(word);
        }
    }

    Err(last_error.unwrap_or(EncodeError::NoMatchingForm {
        mnemonic: instruction.mnemonic.as_str(),
    }))
}

/// Bits that an alias mnemonic encodes implicitly — fields present in the
/// underlying instruction word but absent from the alias's operand list.
///
/// Example: `cinc Wd, Wn, cond` shares its base opcode with
/// `csinc Wd, Wn, Wm, !cond`. The alias drops `Wm` from the operand list and
/// implicitly sets `Wm = Wn`. The `Cond1` operand kind already encodes the
/// condition inversion, so the only implicit work here is filling in the `Rm`
/// field.
///
/// Adding `cset` / `cneg` will require new arms here. Keeping this function as
/// the single home for that fix-up means the alias relationship is in one
/// reviewable place rather than scattered across operand encoders.
fn alias_implicit_bits(
    mnemonic: Aarch64Mnemonic,
    operands: &[DecodedOperand],
) -> Result<Word, EncodeError> {
    match alias_fixup(mnemonic) {
        Some(AliasFixup::CopyRnToRm) => {
            let rn = gp_register_at(operands, 1, "Rn")?;
            Ok((rn as Word) << 16)
        }
        None => Ok(0),
    }
}

#[derive(Debug, Copy, Clone)]
enum AliasFixup {
    /// The alias drops the canonical `Rm` operand and the encoder must copy
    /// `Rn` into the `Rm` field. Used by `cinc` (vs. `csinc`).
    CopyRnToRm,
}

fn alias_fixup(mnemonic: Aarch64Mnemonic) -> Option<AliasFixup> {
    match mnemonic {
        Aarch64Mnemonic::Cinc => Some(AliasFixup::CopyRnToRm),
        _ => None,
    }
}

fn gp_register_at(
    operands: &[DecodedOperand],
    index: usize,
    kind: &'static str,
) -> Result<u8, EncodeError> {
    let Some(DecodedOperand::Register(register)) = operands.get(index) else {
        return Err(EncodeError::InvalidOperand { kind });
    };
    if !matches!(register.class, RegisterClass::W | RegisterClass::X) || register.index > 31 {
        return Err(EncodeError::InvalidOperand { kind });
    }
    Ok(register.index)
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
) -> Result<DecodedInstruction, DecodeError>
where
    F: Fn(u64) -> Option<String>,
{
    decode_instruction_with_symbols(address, word, symbol_for_address)
}

pub fn disassemble_at(address: u64, word: Word) -> Result<DecodedInstruction, DecodeError> {
    decode_instruction(address, word)
}

impl DecodedInstruction {
    pub fn format_mnemonic(&self) -> String {
        let raw = self.mnemonic.as_str();
        if let Some(alias) = self.mnemonic.display_alias() {
            return alias.to_string();
        }

        if let Some(DecodedOperand::VectorRegister(register)) = self.operands.first() {
            return format!(
                "{raw}.{}",
                format_vector_arrangement(register.arrangement)
            );
        }
        if let Some(DecodedOperand::VectorList(list)) = self.operands.first() {
            return if list.element.is_some() {
                format!(
                    "{raw}.{}",
                    format_vector_element_arrangement(list.arrangement)
                )
            } else {
                format!("{raw}.{}", format_vector_arrangement(list.arrangement))
            };
        }
        if let Some(size) = self.operands.iter().find_map(vector_element_size) {
            return format!("{raw}.{}", format_vector_element_size(size));
        }

        raw.to_string()
    }

    pub fn format_operands_with_symbols<F>(&self, symbol_for_address: F) -> String
    where
        F: Fn(u64) -> Option<String>,
    {
        let raw = self.mnemonic.as_str();
        if raw == "adrp" {
            return format_adrp_operands(self.address, &self.operands, &symbol_for_address);
        }
        format_operands(raw, &self.operands, symbol_for_address)
    }

    pub fn format_operands(&self) -> String {
        self.format_operands_with_symbols(|_| None)
    }
}

/// Declarative formatting style for an instruction's operand list.
#[derive(Debug, Copy, Clone)]
enum FormatStyle {
    /// Comma-separated operand list. `prefix` is prepended to immediate operands;
    /// `decimal` switches integer immediates to base-10 rendering.
    List {
        prefix: Option<&'static str>,
        decimal: bool,
    },
    /// One of a small set of mnemonic-shaped operand layouts that don't fit
    /// the simple list form.
    Special(Special),
}

#[derive(Debug, Copy, Clone)]
enum Special {
    Branch,
    TestBranch,
    Adrp,
    MovAuto,
    Ret,
    SimdLdSt,
    LdpStp,
    Sys,
    IcTlbi,
    Clrex,
    Isb,
    Dcps,
    Exception,
}

const HASH: Option<&str> = Some("#");

fn format_style_for(mnemonic: &str) -> FormatStyle {
    let list = |prefix, decimal| FormatStyle::List { prefix, decimal };
    let special = FormatStyle::Special;
    match mnemonic {
        "add" | "adds" | "cmn" | "sub" | "subs" | "cmp" => list(HASH, false),
        "adc" | "adcs" | "sbc" | "sbcs" => list(None, false),
        "and" | "eor" | "orr" | "ands" => list(HASH, false),
        "movk" | "movz" | "movn" => list(HASH, false),
        "ubfx" | "bfxil" | "lsl" | "lsr" | "asr" => list(HASH, true),
        "shll" => list(HASH, true),
        "sshr" | "shl" | "sqshrun" => list(HASH, false),
        "ext" | "extr" => list(HASH, false),
        "movi" | "mvni" => list(HASH, false),
        "adr" => list(HASH, true),
        "adrp" => special(Special::Adrp),
        "mov" => special(Special::MovAuto),
        "cbz" | "cbnz" => list(None, false),
        "b" | "bl" | "beq" | "bne" | "bcs" | "bcc" | "bmi" | "bpl" | "bvs" | "bvc" | "bhi"
        | "bls" | "bge" | "blt" | "bgt" | "ble" => special(Special::Branch),
        "tbz" | "tbnz" => special(Special::TestBranch),
        "br" | "blr" => list(None, false),
        "ldr" | "str" | "ldrsw" | "ldxr" | "stxr" | "ldur" | "stur" => list(None, false),
        "ld1" | "ld2" | "ld3" | "ld4" | "st1" | "st2" | "st3" | "st4" | "ld1r" => {
            special(Special::SimdLdSt)
        }
        "prfm" => list(None, false),
        "fadd" | "fsub" | "fmul" | "fdiv" | "fmadd" | "fmsub" | "fcsel" => list(None, false),
        "fcmp" => list(HASH, false),
        "cmeq" => special(Special::Exception),
        "fmov" => list(HASH, false),
        "scvtf" | "ucvtf" | "fcvtns" | "fcvtnu" | "fcvtas" | "fcvtau" | "fcvtps" | "fcvtpu"
        | "fcvtms" | "fcvtmu" | "fcvtzs" | "fcvtzu" => list(HASH, false),
        "svc" | "hvc" | "smc" | "brk" | "hlt" | "hint" => special(Special::Exception),
        "msr" => list(HASH, false),
        "sys" => special(Special::Sys),
        "sysl" => list(HASH, false),
        "at" | "dc" => list(None, false),
        "ic" | "tlbi" => special(Special::IcTlbi),
        "clrex" => special(Special::Clrex),
        "dsb" | "dmb" => list(None, false),
        "isb" => special(Special::Isb),
        "dcps1" | "dcps2" | "dcps3" => special(Special::Dcps),
        "ret" => special(Special::Ret),
        "csel" | "cinc" => list(None, false),
        "ccmp" | "ccmn" => list(HASH, false),
        "stp" | "ldp" => special(Special::LdpStp),
        _ => list(None, false),
    }
}

fn format_operands<F>(mnemonic: &str, operands: &[DecodedOperand], symbol_for_address: F) -> String
where
    F: Fn(u64) -> Option<String>,
{
    match format_style_for(mnemonic) {
        FormatStyle::List { prefix, decimal: false } => {
            format_operand_list(operands, &symbol_for_address, prefix)
        }
        FormatStyle::List { prefix, decimal: true } => {
            format_operand_list_decimal(operands, prefix)
        }
        FormatStyle::Special(special) => {
            format_special(special, operands, &symbol_for_address)
        }
    }
}

fn format_special<F>(
    special: Special,
    operands: &[DecodedOperand],
    symbol_for_address: &F,
) -> String
where
    F: Fn(u64) -> Option<String>,
{
    match special {
        Special::Adrp => format_adrp_operands(0, operands, symbol_for_address),
        Special::MovAuto => {
            let prefix = if operands.iter().any(is_immediate_operand) {
                HASH
            } else {
                None
            };
            format_operand_list(operands, symbol_for_address, prefix)
        }
        Special::Branch => operands
            .first()
            .map(|operand| format_operand_with_symbols(operand, symbol_for_address, None))
            .unwrap_or_default(),
        Special::TestBranch => format_test_branch_operands(operands, symbol_for_address),
        Special::SimdLdSt => format_simd_ldst_operands(operands),
        Special::LdpStp => format_ldp_stp_operands(operands, symbol_for_address),
        Special::Sys => format_sys_operands(operands),
        Special::IcTlbi => format_optional_default_register(operands, 31),
        Special::Clrex => format_optional_default_immediate(operands, 0xf),
        Special::Isb => format_isb_operands(operands),
        Special::Dcps => match operands.first() {
            Some(DecodedOperand::Immediate(0)) | None => String::new(),
            _ => format_exception_operands(operands),
        },
        Special::Exception => format_exception_operands(operands),
        Special::Ret => match operands.first() {
            Some(DecodedOperand::Register(register)) if register.index == 30 => String::new(),
            Some(operand) => format_operand_with_symbols(operand, symbol_for_address, None),
            None => String::new(),
        },
    }
}

fn format_ldp_stp_operands<F>(operands: &[DecodedOperand], symbol_for_address: &F) -> String
where
    F: Fn(u64) -> Option<String>,
{
    let Some((first, rest)) = operands.split_first() else {
        return String::new();
    };
    let Some((second, rest)) = rest.split_first() else {
        return format_operand_with_symbols(first, symbol_for_address, None);
    };
    let Some(memory) = rest.first() else {
        return format!(
            "{}, {}",
            format_operand_with_symbols(first, symbol_for_address, None),
            format_operand_with_symbols(second, symbol_for_address, None)
        );
    };

    format!(
        "{}, {}, {}",
        format_operand_with_symbols(first, symbol_for_address, None),
        format_operand_with_symbols(second, symbol_for_address, None),
        format_operand_with_symbols(memory, symbol_for_address, None)
    )
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
            .map(|operand| format_operand_with_symbols(operand, symbol_for_address, HASH)),
    );
    parts.join(", ")
}

fn format_exception_operands(operands: &[DecodedOperand]) -> String {
    operands
        .iter()
        .map(|operand| match operand {
            DecodedOperand::Immediate(0) => "#0".to_string(),
            _ => format_operand_with_symbols(operand, &|_| None, HASH),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_optional_default_immediate(operands: &[DecodedOperand], default: i64) -> String {
    match operands.first() {
        Some(DecodedOperand::Immediate(value)) if *value == default => String::new(),
        Some(operand) => format_operand_with_symbols(operand, &|_| None, HASH),
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

    format_operand_list(operands, &|_| None, None)
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

    format_operand_list(operands, &|_| None, HASH)
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

fn format_operand_list<F>(
    operands: &[DecodedOperand],
    symbol_for_address: &F,
    immediate_prefix: Option<&str>,
) -> String
where
    F: Fn(u64) -> Option<String>,
{
    operands
        .iter()
        .map(|operand| format_operand_with_symbols(operand, symbol_for_address, immediate_prefix))
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
