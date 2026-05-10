//! AArch64 macro emission — expansion of fused
//! [`MacroOp`]s back into instruction bytes.

use super::isa_impl::{Aarch64Isa, AarchMacroKind};
use super::{
    encode_instruction, Aarch64Mnemonic, DecodedOperand, EncodeError, InstructionTemplate,
    Register,
};
use crate::container::{Container, RelocationKind, SectionId, SymbolId, SymbolKind};
use crate::isa::{MacroEmitError, MacroEmittedRelocation};
use crate::rewrite::ir::{MacroOp, RewriteInstruction, RewriteOperand, Target};

pub(super) fn emit_macro(
    macro_op: &MacroOp<Aarch64Isa>,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    current_section: Option<SectionId>,
    bytes: &mut Vec<u8>,
    relocations: &mut Vec<MacroEmittedRelocation>,
) -> Result<(), MacroEmitError<EncodeError>> {
    match macro_op.kind {
        AarchMacroKind::LoadAddress => emit_load_address(
            macro_op,
            here,
            block_addresses,
            container,
            current_section,
            bytes,
            relocations,
        ),
        AarchMacroKind::AccessValue => {
            emit_access_value(macro_op, here, container, current_section, bytes, relocations)
        }
    }
}

fn emit_load_address(
    macro_op: &MacroOp<Aarch64Isa>,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    current_section: Option<SectionId>,
    bytes: &mut Vec<u8>,
    relocations: &mut Vec<MacroEmittedRelocation>,
) -> Result<(), MacroEmitError<EncodeError>> {
    let target_needs_relocation = match (macro_op.target, container) {
        (Target::Symbol(id), Some(container)) => {
            symbol_needs_relocation(container, id, current_section)
        }
        _ => false,
    };

    if target_needs_relocation {
        let symbol_id = match macro_op.target {
            Target::Symbol(id) => id,
            _ => unreachable!(),
        };
        let adrp_offset = bytes.len() as u64;
        let adrp_template = build_adrp_template(&macro_op.register, here & !0xfff, here);
        let adrp_word = encode_instruction(&adrp_template).map_err(MacroEmitError::Encode)?;
        bytes.extend_from_slice(&adrp_word.to_le_bytes());
        relocations.push(MacroEmittedRelocation {
            offset: adrp_offset,
            kind: RelocationKind::AdrpPage21,
            symbol: symbol_id,
            addend: 0,
        });
        let add_offset = bytes.len() as u64;
        let add_template = build_add_immediate_template(&macro_op.register, 0);
        let add_word = encode_instruction(&add_template).map_err(MacroEmitError::Encode)?;
        bytes.extend_from_slice(&add_word.to_le_bytes());
        relocations.push(MacroEmittedRelocation {
            offset: add_offset,
            kind: RelocationKind::AddPageOffset12,
            symbol: symbol_id,
            addend: 0,
        });
        return Ok(());
    }

    // Resolved target.
    let target_address = resolve_target(macro_op.target, block_addresses, container)
        .ok_or(MacroEmitError::Malformed)?;
    let target_page = target_address & !0xfff;
    let page_offset = (target_address & 0xfff) as i64;
    let adrp_template = build_adrp_template(&macro_op.register, target_page, here);
    let adrp_word = encode_instruction(&adrp_template).map_err(MacroEmitError::Encode)?;
    bytes.extend_from_slice(&adrp_word.to_le_bytes());
    let add_template = build_add_immediate_template(&macro_op.register, page_offset);
    let add_word = encode_instruction(&add_template).map_err(MacroEmitError::Encode)?;
    bytes.extend_from_slice(&add_word.to_le_bytes());
    Ok(())
}

fn emit_access_value(
    macro_op: &MacroOp<Aarch64Isa>,
    here: u64,
    container: Option<&Container>,
    current_section: Option<SectionId>,
    bytes: &mut Vec<u8>,
    relocations: &mut Vec<MacroEmittedRelocation>,
) -> Result<(), MacroEmitError<EncodeError>> {
    if macro_op.original_instructions.len() != 2 {
        return Err(MacroEmitError::Malformed);
    }
    let original_adrp = &macro_op.original_instructions[0];
    let original_companion = &macro_op.original_instructions[1];

    let target_needs_relocation = match (macro_op.target, container) {
        (Target::Symbol(id), Some(container)) => {
            symbol_needs_relocation(container, id, current_section)
        }
        _ => false,
    };

    if target_needs_relocation {
        let symbol_id = match macro_op.target {
            Target::Symbol(id) => id,
            _ => unreachable!(),
        };
        let adrp_offset = bytes.len() as u64;
        let adrp_template = build_adrp_template(&macro_op.register, here & !0xfff, here);
        let adrp_word = encode_instruction(&adrp_template).map_err(MacroEmitError::Encode)?;
        bytes.extend_from_slice(&adrp_word.to_le_bytes());
        relocations.push(MacroEmittedRelocation {
            offset: adrp_offset,
            kind: RelocationKind::AdrpPage21,
            symbol: symbol_id,
            addend: 0,
        });

        let companion_template = InstructionTemplate {
            address: here + 4,
            mnemonic: original_companion.mnemonic,
            operands: original_companion
                .operands
                .iter()
                .map(decoded_from_rewrite_operand)
                .collect(),
        };
        let companion_offset = bytes.len() as u64;
        let companion_word =
            encode_instruction(&companion_template).map_err(MacroEmitError::Encode)?;
        bytes.extend_from_slice(&companion_word.to_le_bytes());
        let access_width_bytes = ldst_access_width_bytes(original_companion)
            .ok_or(MacroEmitError::NoRelocationForMnemonic)?;
        relocations.push(MacroEmittedRelocation {
            offset: companion_offset,
            kind: RelocationKind::LoadStorePageOffset12 { access_width_bytes },
            symbol: symbol_id,
            addend: 0,
        });
        return Ok(());
    }

    // Foldable case: re-emit verbatim.
    let adrp_template = InstructionTemplate {
        address: here,
        mnemonic: original_adrp.mnemonic,
        operands: original_adrp
            .operands
            .iter()
            .map(decoded_from_rewrite_operand)
            .collect(),
    };
    let adrp_word = encode_instruction(&adrp_template).map_err(MacroEmitError::Encode)?;
    bytes.extend_from_slice(&adrp_word.to_le_bytes());
    let companion_template = InstructionTemplate {
        address: here + 4,
        mnemonic: original_companion.mnemonic,
        operands: original_companion
            .operands
            .iter()
            .map(decoded_from_rewrite_operand)
            .collect(),
    };
    let companion_word =
        encode_instruction(&companion_template).map_err(MacroEmitError::Encode)?;
    bytes.extend_from_slice(&companion_word.to_le_bytes());
    Ok(())
}

fn ldst_access_width_bytes(instruction: &RewriteInstruction<Aarch64Isa>) -> Option<u8> {
    use crate::isa::aarch64::RegisterClass;
    let register = match instruction.operands.first()? {
        RewriteOperand::Decoded(DecodedOperand::Register(reg)) => reg,
        _ => return None,
    };
    match register.class {
        RegisterClass::W => Some(4),
        RegisterClass::X => Some(8),
        _ => None,
    }
}

fn decoded_from_rewrite_operand(
    operand: &RewriteOperand<Aarch64Isa>,
) -> DecodedOperand {
    match operand {
        RewriteOperand::Decoded(d) => d.clone(),
        RewriteOperand::Branch(_) => DecodedOperand::BranchTarget(0),
        RewriteOperand::Page(_) => DecodedOperand::PageTarget(0),
    }
}

fn build_adrp_template(rd: &Register, target_page: u64, address: u64) -> InstructionTemplate {
    InstructionTemplate {
        address,
        mnemonic: Aarch64Mnemonic::Adrp,
        operands: vec![
            DecodedOperand::Register(rd.clone()),
            DecodedOperand::PageTarget(target_page),
        ],
    }
}

fn build_add_immediate_template(rd: &Register, immediate: i64) -> InstructionTemplate {
    InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Add,
        operands: vec![
            DecodedOperand::Register(rd.clone()),
            DecodedOperand::Register(rd.clone()),
            DecodedOperand::Immediate(immediate),
        ],
    }
}

fn resolve_target(
    target: Target,
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Option<u64> {
    match target {
        Target::Block(id) => block_addresses.get(id.0).copied(),
        Target::Absolute(addr) => Some(addr),
        Target::Symbol(id) => container.and_then(|c| c.callable_address_of_symbol(id)),
        Target::Constant(_) => None,
    }
}

pub(super) fn symbol_needs_relocation(
    container: &Container,
    id: SymbolId,
    current_section: Option<SectionId>,
) -> bool {
    let symbol = container.symbol(id);
    if symbol.is_undefined {
        let has_plt_stub = container
            .elf_image
            .as_ref()
            .map(|img| img.plt_stubs.contains_key(&id))
            .unwrap_or(false);
        return !has_plt_stub;
    }
    if symbol.kind == SymbolKind::Section {
        return true;
    }
    let Some(current) = current_section else {
        return false;
    };
    let Some(symbol_section) = symbol.section else {
        return false;
    };
    if symbol_section == current {
        return false;
    }
    let current_section_address = container
        .sections
        .iter()
        .find(|s| s.id == current)
        .map(|s| s.address)
        .unwrap_or(0);
    current_section_address == 0
}
