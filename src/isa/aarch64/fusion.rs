//! AArch64 macro-fusion logic.
//!
//! Recognises `adrp + add` and `adrp + ldr/str` adjacency
//! patterns at lift time and replaces them with
//! [`MacroOp`] entries in the rewrite IR.

use super::isa_impl::{Aarch64Isa, AarchMacroKind};
use super::{Aarch64Mnemonic, DecodedOperand, Register};
use crate::container::{Container, RelocationKind};
use crate::isa::FusionRelocationInfo;
use crate::mc::BasicBlockId;
use crate::rewrite::ir::{
    MacroOp, RewriteInstruction, RewriteOp, RewriteOperand, Target,
};
use std::collections::HashMap;

/// Walk a flat list of singleton instructions and fuse
/// recognised multi-instruction idioms into [`MacroOp`]s.
pub(super) fn fuse_macros(
    instructions: Vec<RewriteInstruction<Aarch64Isa>>,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
    relocations: &HashMap<u64, FusionRelocationInfo>,
) -> Vec<RewriteOp<Aarch64Isa>> {
    let mut ops = Vec::with_capacity(instructions.len());
    let mut iter = instructions.into_iter().peekable();

    while let Some(current) = iter.next() {
        if matches!(current.mnemonic, Aarch64Mnemonic::Adrp) {
            if let Some(next) = iter.peek() {
                if let Some(macro_op) = try_fuse_adrp_add(
                    &current,
                    next,
                    block_at_address,
                    container,
                    relocations,
                ) {
                    iter.next();
                    ops.push(RewriteOp::Macro(macro_op));
                    continue;
                }
                if let Some(macro_op) = try_fuse_adrp_ldst(&current, next, relocations) {
                    iter.next();
                    ops.push(RewriteOp::Macro(macro_op));
                    continue;
                }
            }
        }
        ops.push(RewriteOp::Instruction(current));
    }

    ops
}

fn try_fuse_adrp_ldst(
    adrp: &RewriteInstruction<Aarch64Isa>,
    companion: &RewriteInstruction<Aarch64Isa>,
    relocations: &HashMap<u64, FusionRelocationInfo>,
) -> Option<MacroOp<Aarch64Isa>> {
    if !matches!(
        companion.mnemonic,
        Aarch64Mnemonic::Ldr | Aarch64Mnemonic::Str
    ) {
        return None;
    }
    let adrp_rd = match adrp.operands.as_slice() {
        [RewriteOperand::Decoded(DecodedOperand::Register(rd)), RewriteOperand::Page(_)] => rd,
        _ => return None,
    };
    let companion_base = match companion.operands.as_slice() {
        [
            RewriteOperand::Decoded(DecodedOperand::Register(_)),
            RewriteOperand::Decoded(DecodedOperand::Memory(memory)),
        ] => &memory.base,
        _ => return None,
    };
    if !same_register(adrp_rd, companion_base) {
        return None;
    }
    let target = macro_target_from_relocations(
        adrp,
        companion,
        relocations,
        CompanionRelocKind::LoadStore,
    )?;
    Some(MacroOp {
        kind: AarchMacroKind::AccessValue,
        register: adrp_rd.clone(),
        target,
        original_addresses: [adrp.original_address, companion.original_address]
            .into_iter()
            .flatten()
            .collect(),
        original_instructions: vec![adrp.clone(), companion.clone()],
    })
}

fn try_fuse_adrp_add(
    adrp: &RewriteInstruction<Aarch64Isa>,
    add: &RewriteInstruction<Aarch64Isa>,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
    relocations: &HashMap<u64, FusionRelocationInfo>,
) -> Option<MacroOp<Aarch64Isa>> {
    let adrp_rd = match adrp.operands.as_slice() {
        [RewriteOperand::Decoded(DecodedOperand::Register(rd)), RewriteOperand::Page(_)] => rd,
        _ => return None,
    };
    let adrp_target = match &adrp.operands[1] {
        RewriteOperand::Page(target) => *target,
        _ => return None,
    };

    let (add_rd, add_rn, add_imm) = match (add.mnemonic, add.operands.as_slice()) {
        (
            Aarch64Mnemonic::Add,
            [
                RewriteOperand::Decoded(DecodedOperand::Register(rd)),
                RewriteOperand::Decoded(DecodedOperand::Register(rn)),
                RewriteOperand::Decoded(DecodedOperand::Immediate(imm)),
            ],
        ) => (rd, rn, *imm),
        (
            Aarch64Mnemonic::Mov,
            [
                RewriteOperand::Decoded(DecodedOperand::Register(rd)),
                RewriteOperand::Decoded(DecodedOperand::Register(rn)),
            ],
        ) => (rd, rn, 0_i64),
        _ => return None,
    };

    if !same_register(adrp_rd, add_rd) || !same_register(adrp_rd, add_rn) {
        return None;
    }
    if add_imm < 0 || add_imm > 0xfff {
        return None;
    }

    if matches!(adrp_target, Target::Symbol(_)) && add_imm == 0 {
        return Some(MacroOp {
            kind: AarchMacroKind::LoadAddress,
            register: adrp_rd.clone(),
            target: adrp_target,
            original_addresses: [adrp.original_address, add.original_address]
                .into_iter()
                .flatten()
                .collect(),
            original_instructions: vec![adrp.clone(), add.clone()],
        });
    }

    let target = match macro_target_from_relocations(
        adrp,
        add,
        relocations,
        CompanionRelocKind::Add,
    ) {
        Some(t) => t,
        None => {
            let combined_address = match adrp_target {
                Target::Absolute(page) => page.wrapping_add(add_imm as u64),
                Target::Block(_) | Target::Symbol(_) | Target::Constant(_) => {
                    return None;
                }
            };
            resolve_address(combined_address, block_at_address, container)
        }
    };

    Some(MacroOp {
        kind: AarchMacroKind::LoadAddress,
        register: adrp_rd.clone(),
        target,
        original_addresses: [adrp.original_address, add.original_address]
            .into_iter()
            .flatten()
            .collect(),
        original_instructions: vec![adrp.clone(), add.clone()],
    })
}

fn macro_target_from_relocations(
    adrp: &RewriteInstruction<Aarch64Isa>,
    companion: &RewriteInstruction<Aarch64Isa>,
    relocations: &HashMap<u64, FusionRelocationInfo>,
    expected_companion_kind: CompanionRelocKind,
) -> Option<Target> {
    let adrp_addr = adrp.original_address?;
    let companion_addr = companion.original_address?;
    let adrp_reloc = relocations.get(&adrp_addr)?;
    let companion_reloc = relocations.get(&companion_addr)?;
    if adrp_reloc.kind != RelocationKind::AdrpPage21 {
        return None;
    }
    if !companion_reloc_matches(companion_reloc.kind, expected_companion_kind) {
        return None;
    }
    let adrp_sym = adrp_reloc.symbol?;
    let companion_sym = companion_reloc.symbol?;
    if adrp_sym != companion_sym {
        return None;
    }
    Some(Target::Symbol(adrp_sym))
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum CompanionRelocKind {
    Add,
    LoadStore,
}

fn companion_reloc_matches(actual: RelocationKind, expected: CompanionRelocKind) -> bool {
    match expected {
        CompanionRelocKind::Add => matches!(actual, RelocationKind::AddPageOffset12),
        CompanionRelocKind::LoadStore => {
            matches!(actual, RelocationKind::LoadStorePageOffset12 { .. })
        }
    }
}

fn same_register(a: &Register, b: &Register) -> bool {
    use crate::isa::aarch64::RegisterClass::{W, WOrSp, X, XOrSp};
    if a.index != b.index {
        return false;
    }
    let lhs_64 = matches!(a.class, X | XOrSp);
    let rhs_64 = matches!(b.class, X | XOrSp);
    let lhs_32 = matches!(a.class, W | WOrSp);
    let rhs_32 = matches!(b.class, W | WOrSp);
    (lhs_64 && rhs_64) || (lhs_32 && rhs_32)
}

fn resolve_address(
    address: u64,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
) -> Target {
    if let Some(&id) = block_at_address.get(&address) {
        return Target::Block(id);
    }
    if let Some(container) = container {
        if let Some(symbol) = container.symbol_at_address(address) {
            return Target::Symbol(symbol.id);
        }
    }
    Target::Absolute(address)
}
