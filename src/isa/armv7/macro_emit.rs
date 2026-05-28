//! Thumb macro emission — expansion of fused [`MacroOp`]s
//! back into instruction bytes.
//!
//! Currently handles [`ThumbMacroKind::MovwMovt`], the
//! 32-bit-address synthesis idiom:
//!
//! ```text
//! movw Rd, #lo16(target)
//! movt Rd, #hi16(target)
//! ```
//!
//! When `target` resolves to a symbol that needs a runtime
//! relocation, the emitter writes both halves with imm16 = 0
//! and emits the `R_ARM_THM_MOVW_ABS_NC` /
//! `R_ARM_THM_MOVT_ABS` reloc pair. Otherwise the imm16s are
//! filled with the low/high halves of the resolved address
//! directly.

use super::isa_impl::{ThumbIsa, ThumbMacroKind};
use super::operand::{DecodedOperand, EncodeError, Register};
use super::table_generated::{ThumbMnemonicGenerated, THUMB_OPCODE_TABLE_GENERATED};
use crate::container::{Container, RelocationKind, SectionId, SymbolId, SymbolKind};
use crate::isa::{MacroEmitError, MacroEmittedRelocation};
use crate::rewrite::ir::{MacroOp, Target};

pub(super) fn emit_macro(
    macro_op: &MacroOp<ThumbIsa>,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    current_section: Option<SectionId>,
    bytes: &mut Vec<u8>,
    relocations: &mut Vec<MacroEmittedRelocation>,
) -> Result<(), MacroEmitError<EncodeError>> {
    match macro_op.kind {
        ThumbMacroKind::MovwMovt => emit_movw_movt(
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
    macro_op: &MacroOp<ThumbIsa>,
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
            kind: RelocationKind::ThumbMovwAbsNc,
            symbol: symbol_id,
            addend: 0,
        });
        let movt_offset = bytes.len() as u64;
        encode_movt(&macro_op.register, 0, bytes)?;
        relocations.push(MacroEmittedRelocation {
            offset: movt_offset,
            kind: RelocationKind::ThumbMovtAbs,
            symbol: symbol_id,
            addend: 0,
        });
        return Ok(());
    }

    let target_address = resolve_target(macro_op.target, block_addresses, container)
        .ok_or(MacroEmitError::Malformed)?;
    let _ = here; // `here` is unused for the resolved-target case (the
                  // encoded movw/movt don't depend on the instruction
                  // address — they're absolute, not PC-relative).
    let lo = (target_address & 0xffff) as u16;
    let hi = ((target_address >> 16) & 0xffff) as u16;
    encode_movw(&macro_op.register, lo, bytes)?;
    encode_movt(&macro_op.register, hi, bytes)?;
    Ok(())
}

fn encode_movw(rd: &Register, imm16: u16, bytes: &mut Vec<u8>) -> Result<(), MacroEmitError<EncodeError>> {
    let row = find_row(0xf2400000, 0xfbf08000)
        .expect("Thumb movw (T3) must be in the opcode table");
    let operands = vec![
        DecodedOperand::Register(rd.clone()),
        DecodedOperand::Immediate(imm16 as i64),
    ];
    let (word, _w) = super::encode::encode_with_row(row, &operands, 0)
        .map_err(MacroEmitError::Encode)?;
    push_word_thumb(bytes, word);
    Ok(())
}

fn encode_movt(rd: &Register, imm16: u16, bytes: &mut Vec<u8>) -> Result<(), MacroEmitError<EncodeError>> {
    let row = find_row(0xf2c00000, 0xfbf08000)
        .expect("Thumb movt (T1) must be in the opcode table");
    let operands = vec![
        DecodedOperand::Register(rd.clone()),
        DecodedOperand::Immediate(imm16 as i64),
    ];
    let (word, _w) = super::encode::encode_with_row(row, &operands, 0)
        .map_err(MacroEmitError::Encode)?;
    push_word_thumb(bytes, word);
    Ok(())
}

fn find_row(
    opcode: u32,
    mask: u32,
) -> Option<&'static super::table_generated::ThumbOpcodeGenerated> {
    THUMB_OPCODE_TABLE_GENERATED
        .iter()
        .find(|row| row.opcode == opcode && row.mask == mask)
}

fn push_word_thumb(bytes: &mut Vec<u8>, word: u32) {
    let hw1 = ((word >> 16) & 0xffff) as u16;
    let hw2 = (word & 0xffff) as u16;
    bytes.extend_from_slice(&hw1.to_le_bytes());
    bytes.extend_from_slice(&hw2.to_le_bytes());
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

/// Mirror of the aarch64 helper in `aarch64/macro_emit.rs`.
/// A symbol needs a relocation when it's undefined (and not
/// resolved via a PLT stub), when it's a section symbol, or
/// when it lives in a different section whose final address
/// isn't known at macro-emit time.
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

// Suppress unused-name warning if ThumbMnemonicGenerated isn't
// consulted directly here (it's still required by the
// `find_row` lookups via the opcode/mask pair).
#[allow(dead_code)]
fn _unused_mnemonic_import(_m: ThumbMnemonicGenerated) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::operand::{Register, RegisterClass};
    use crate::isa::armv7::sweep::disassemble_bytes;
    use crate::rewrite::ir::{MacroOp, Target};

    fn r(index: u8) -> Register {
        Register {
            class: RegisterClass::R,
            index,
        }
    }

    fn movw_movt_op(target_address: u64, rd_index: u8) -> MacroOp<ThumbIsa> {
        MacroOp {
            kind: ThumbMacroKind::MovwMovt,
            register: r(rd_index),
            target: Target::Absolute(target_address),
            original_instructions: Vec::new(),
            original_addresses: Vec::new(),
        }
    }

    #[test]
    fn movw_movt_absolute_resolves_to_pair_with_correct_halves() {
        // Build a MacroOp targeting 0xdead_beef and confirm the
        // emitted bytes decode back to movw r2, #0xbeef + movt
        // r2, #0xdead.
        let target = 0xdead_beefu64;
        let op = movw_movt_op(target, 2);
        let mut bytes = Vec::new();
        let mut relocs = Vec::new();
        emit_macro(&op, 0x1000, &[], None, None, &mut bytes, &mut relocs)
            .expect("emit movw/movt");
        assert!(relocs.is_empty(), "absolute target must not emit relocs");
        assert_eq!(bytes.len(), 8, "movw+movt must be 8 bytes");

        let decoded = disassemble_bytes(0x1000, &bytes).expect("decode pair");
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].mnemonic, ThumbMnemonicGenerated::Movw);
        assert_eq!(decoded[1].mnemonic, ThumbMnemonicGenerated::Movt);

        // Both must target r2.
        for instr in &decoded {
            let rd = instr.operands.iter().find_map(|o| match o {
                DecodedOperand::Register(reg) => Some(reg.index),
                _ => None,
            });
            assert_eq!(rd, Some(2));
        }

        // Movw immediate = lo16 = 0xbeef; Movt = hi16 = 0xdead.
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
            kind: ThumbMacroKind::MovwMovt,
            register: r(0),
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
    fn movw_movt_constant_target_is_unsupported() {
        use crate::rewrite::ir::ConstantId;
        let op = MacroOp {
            kind: ThumbMacroKind::MovwMovt,
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
