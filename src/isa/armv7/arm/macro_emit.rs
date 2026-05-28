//! ARM-mode macro emission — expansion of fused
//! [`MacroOp`]s back into instruction bytes.
//!
//! Currently handles [`ArmMacroKind::MovwMovt`], the
//! 32-bit-address synthesis idiom:
//!
//! ```text
//! movw Rd, #lo16(target)
//! movt Rd, #hi16(target)
//! ```
//!
//! When `target` resolves to a symbol that needs a runtime
//! relocation, the emitter writes both halves with imm16 = 0
//! and emits the `R_ARM_MOVW_ABS_NC` / `R_ARM_MOVT_ABS`
//! reloc pair. Otherwise the imm16s are filled with the
//! low/high halves of the resolved address directly.

use super::isa_impl::{ArmIsa, ArmMacroKind};
use super::table_generated::{
    ArmMnemonicGenerated, ArmOpcodeGenerated, ARM_OPCODE_TABLE_GENERATED,
};
use crate::container::{Container, RelocationKind, SectionId, SymbolId, SymbolKind};
use crate::isa::armv7::operand::{DecodedOperand, EncodeError, Register};
use crate::isa::{MacroEmitError, MacroEmittedRelocation};
use crate::rewrite::ir::{MacroOp, Target};

pub(super) fn emit_macro(
    macro_op: &MacroOp<ArmIsa>,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    current_section: Option<SectionId>,
    bytes: &mut Vec<u8>,
    relocations: &mut Vec<MacroEmittedRelocation>,
) -> Result<(), MacroEmitError<EncodeError>> {
    match macro_op.kind {
        ArmMacroKind::MovwMovt => emit_movw_movt(
            macro_op,
            here,
            block_addresses,
            container,
            current_section,
            bytes,
            relocations,
        ),
    }
}

fn emit_movw_movt(
    macro_op: &MacroOp<ArmIsa>,
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
        let movw_offset = bytes.len() as u64;
        encode_movw(&macro_op.register, 0, bytes)?;
        relocations.push(MacroEmittedRelocation {
            offset: movw_offset,
            kind: RelocationKind::ArmMovwAbsNc,
            symbol: symbol_id,
            addend: 0,
        });
        let movt_offset = bytes.len() as u64;
        encode_movt(&macro_op.register, 0, bytes)?;
        relocations.push(MacroEmittedRelocation {
            offset: movt_offset,
            kind: RelocationKind::ArmMovtAbs,
            symbol: symbol_id,
            addend: 0,
        });
        return Ok(());
    }

    let target_address = resolve_target(macro_op.target, block_addresses, container)
        .ok_or(MacroEmitError::Malformed)?;
    let _ = here;
    let lo = (target_address & 0xffff) as u16;
    let hi = ((target_address >> 16) & 0xffff) as u16;
    encode_movw(&macro_op.register, lo, bytes)?;
    encode_movt(&macro_op.register, hi, bytes)?;
    Ok(())
}

fn encode_movw(rd: &Register, imm16: u16, bytes: &mut Vec<u8>) -> Result<(), MacroEmitError<EncodeError>> {
    let row = find_row(0x03000000, 0x0ff00000)
        .expect("ARM movw must be in the opcode table");
    // Format: movw%c %12-15R, %V — operands: Condition (AL=14),
    // Register Rd, Immediate imm16.
    let operands = vec![
        DecodedOperand::Condition(14),
        DecodedOperand::Register(rd.clone()),
        DecodedOperand::Immediate(imm16 as i64),
    ];
    let word = super::encode::encode_with_row(row, &operands, 0)
        .map_err(MacroEmitError::Encode)?;
    bytes.extend_from_slice(&word.to_le_bytes());
    Ok(())
}

fn encode_movt(rd: &Register, imm16: u16, bytes: &mut Vec<u8>) -> Result<(), MacroEmitError<EncodeError>> {
    let row = find_row(0x03400000, 0x0ff00000)
        .expect("ARM movt must be in the opcode table");
    let operands = vec![
        DecodedOperand::Condition(14),
        DecodedOperand::Register(rd.clone()),
        DecodedOperand::Immediate(imm16 as i64),
    ];
    let word = super::encode::encode_with_row(row, &operands, 0)
        .map_err(MacroEmitError::Encode)?;
    bytes.extend_from_slice(&word.to_le_bytes());
    Ok(())
}

fn find_row(opcode: u32, mask: u32) -> Option<&'static ArmOpcodeGenerated> {
    ARM_OPCODE_TABLE_GENERATED
        .iter()
        .find(|row| row.opcode == opcode && row.mask == mask)
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

fn symbol_needs_relocation(
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

#[allow(dead_code)]
fn _unused_mnemonic_import(_m: ArmMnemonicGenerated) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::arm::sweep::disassemble_bytes;
    use crate::isa::armv7::operand::{Register, RegisterClass};
    use crate::rewrite::ir::{MacroOp, Target};

    fn r(index: u8) -> Register {
        Register {
            class: RegisterClass::R,
            index,
        }
    }

    fn movw_movt_op(target_address: u64, rd_index: u8) -> MacroOp<ArmIsa> {
        MacroOp {
            kind: ArmMacroKind::MovwMovt,
            register: r(rd_index),
            target: Target::Absolute(target_address),
            original_instructions: Vec::new(),
            original_addresses: Vec::new(),
        }
    }

    #[test]
    fn movw_movt_absolute_resolves_to_pair_with_correct_halves() {
        let target = 0xdead_beefu64;
        let op = movw_movt_op(target, 4);
        let mut bytes = Vec::new();
        let mut relocs = Vec::new();
        emit_macro(&op, 0x1000, &[], None, None, &mut bytes, &mut relocs)
            .expect("emit movw/movt");
        assert!(relocs.is_empty(), "absolute target must not emit relocs");
        assert_eq!(bytes.len(), 8, "movw+movt must be 8 bytes");

        let decoded = disassemble_bytes(0x1000, &bytes).expect("decode pair");
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].mnemonic, ArmMnemonicGenerated::Movw);
        assert_eq!(decoded[1].mnemonic, ArmMnemonicGenerated::Movt);

        for instr in &decoded {
            let rd = instr.operands.iter().find_map(|o| match o {
                DecodedOperand::Register(reg) => Some(reg.index),
                _ => None,
            });
            assert_eq!(rd, Some(4));
        }

        let imm = |i: usize| {
            decoded[i].operands.iter().find_map(|o| match o {
                DecodedOperand::Immediate(v) => Some(*v as u32),
                _ => None,
            })
        };
        assert_eq!(imm(0), Some(0xbeef));
        assert_eq!(imm(1), Some(0xdead));
    }

    #[test]
    fn movw_movt_block_target_resolves_via_block_addresses() {
        use crate::mc::BasicBlockId;
        let block_addresses = vec![0x0, 0x1234_5678];
        let op = MacroOp {
            kind: ArmMacroKind::MovwMovt,
            register: r(1),
            target: Target::Block(BasicBlockId(1)),
            original_instructions: Vec::new(),
            original_addresses: Vec::new(),
        };
        let mut bytes = Vec::new();
        let mut relocs = Vec::new();
        emit_macro(&op, 0x100, &block_addresses, None, None, &mut bytes, &mut relocs)
            .expect("emit");
        assert!(relocs.is_empty());

        let decoded = disassemble_bytes(0x100, &bytes).expect("decode");
        let imm = |i: usize| {
            decoded[i].operands.iter().find_map(|o| match o {
                DecodedOperand::Immediate(v) => Some(*v as u32),
                _ => None,
            })
        };
        assert_eq!(imm(0), Some(0x5678));
        assert_eq!(imm(1), Some(0x1234));
    }

    #[test]
    fn movw_movt_constant_target_is_malformed() {
        use crate::rewrite::ir::ConstantId;
        let op = MacroOp {
            kind: ArmMacroKind::MovwMovt,
            register: r(0),
            target: Target::Constant(ConstantId(0)),
            original_instructions: Vec::new(),
            original_addresses: Vec::new(),
        };
        let mut bytes = Vec::new();
        let mut relocs = Vec::new();
        let result = emit_macro(&op, 0, &[], None, None, &mut bytes, &mut relocs);
        assert!(matches!(result, Err(MacroEmitError::Malformed)));
    }
}
