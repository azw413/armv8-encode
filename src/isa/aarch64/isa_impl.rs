//! [`Isa`] implementation for AArch64.

use super::{
    encode_instruction, invert_conditional_branch, pcrel_range_bytes, Aarch64Mnemonic,
    DecodedOperand, EncodeError, InstructionTemplate, Register,
};
use crate::container::{Container, RelocationKind, SectionId};
use crate::isa::{
    FusionRelocationInfo, Isa, IsaEncodeOutput, MacroEmitError, MacroEmittedRelocation, PcRelKind,
};
use crate::mc::BasicBlockId;
use crate::rewrite::ir::{
    MacroOp, RewriteBlock, RewriteInstruction, RewriteOp, RewriteOperand, Target,
};
use std::collections::HashMap;

/// Marker type implementing [`Isa`] for AArch64.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Aarch64Isa;

/// AArch64-specific macro-fusion kinds.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum AarchMacroKind {
    /// `adrp Rd, page; add Rd, Rd, #lo12` — compute the
    /// absolute address of `target`.
    LoadAddress,
    /// `adrp Rd, page; ldr/str Rd', [Rn, #lo12]` — load
    /// from or store to `target`.
    AccessValue,
}

impl Isa for Aarch64Isa {
    type Mnemonic = Aarch64Mnemonic;
    type Register = Register;
    type Operand = DecodedOperand;
    type EncodeError = EncodeError;
    type MacroKind = AarchMacroKind;

    fn pcrel_kind(operand: &Self::Operand) -> Option<(PcRelKind, u64)> {
        match operand {
            DecodedOperand::BranchTarget(addr) => Some((PcRelKind::Branch, *addr)),
            DecodedOperand::PageTarget(addr) => Some((PcRelKind::Page, *addr)),
            _ => None,
        }
    }

    fn substitute_pcrel(operand: &Self::Operand, address: u64) -> Self::Operand {
        match operand {
            DecodedOperand::BranchTarget(_) => DecodedOperand::BranchTarget(address),
            DecodedOperand::PageTarget(_) => DecodedOperand::PageTarget(address),
            other => other.clone(),
        }
    }

    fn make_pcrel_operand(kind: PcRelKind, address: u64) -> Self::Operand {
        match kind {
            PcRelKind::Branch => DecodedOperand::BranchTarget(address),
            PcRelKind::Page => DecodedOperand::PageTarget(address),
        }
    }

    fn pcrel_range_bytes(mnemonic: Self::Mnemonic) -> Option<i64> {
        pcrel_range_bytes(mnemonic)
    }

    fn invert_conditional_branch(mnemonic: Self::Mnemonic) -> Option<Self::Mnemonic> {
        invert_conditional_branch(mnemonic)
    }

    fn widened_conditional_size() -> u64 {
        8
    }

    fn widened_conditional_range() -> i64 {
        128 * 1024 * 1024
    }

    fn encode(
        mnemonic: Self::Mnemonic,
        operands: &[Self::Operand],
        address: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        let template = InstructionTemplate {
            address,
            mnemonic,
            operands: operands.to_vec(),
        };
        let word = encode_instruction(&template)?;
        Ok(IsaEncodeOutput {
            bytes: word.to_le_bytes().to_vec(),
        })
    }

    fn encode_widened_conditional(
        mnemonic: Self::Mnemonic,
        operands_template: &[Self::Operand],
        here: u64,
        far_target: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        let inverted = invert_conditional_branch(mnemonic)
            .expect("widening called on non-conditional branch");
        let skip = here.wrapping_add(8);
        // Build the inverted-cond instruction targeting skip.
        let mut inv_operands: Vec<DecodedOperand> = operands_template
            .iter()
            .map(|o| match o {
                DecodedOperand::BranchTarget(_) => DecodedOperand::BranchTarget(skip),
                other => other.clone(),
            })
            .collect();
        // If the original had no BranchTarget operand at all,
        // append one so the encoder has a target. (AArch64
        // conditional branches always carry one, so this is
        // defensive.)
        if !inv_operands
            .iter()
            .any(|o| matches!(o, DecodedOperand::BranchTarget(_)))
        {
            inv_operands.push(DecodedOperand::BranchTarget(skip));
        }
        let inverted_word = encode_instruction(&InstructionTemplate {
            address: here,
            mnemonic: inverted,
            operands: inv_operands,
        })?;
        let far_word = encode_instruction(&InstructionTemplate {
            address: here.wrapping_add(4),
            mnemonic: Aarch64Mnemonic::B,
            operands: vec![DecodedOperand::BranchTarget(far_target)],
        })?;
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&inverted_word.to_le_bytes());
        bytes.extend_from_slice(&far_word.to_le_bytes());
        Ok(IsaEncodeOutput { bytes })
    }

    fn relocation_kind_for(
        mnemonic: Self::Mnemonic,
        kind: PcRelKind,
    ) -> Option<RelocationKind> {
        match kind {
            PcRelKind::Page => match mnemonic {
                Aarch64Mnemonic::Adrp => Some(RelocationKind::AdrpPage21),
                _ => None,
            },
            PcRelKind::Branch => match mnemonic {
                Aarch64Mnemonic::B | Aarch64Mnemonic::Bl => {
                    Some(RelocationKind::Branch26)
                }
                Aarch64Mnemonic::Beq
                | Aarch64Mnemonic::Bne
                | Aarch64Mnemonic::Bcs
                | Aarch64Mnemonic::Bcc
                | Aarch64Mnemonic::Bmi
                | Aarch64Mnemonic::Bpl
                | Aarch64Mnemonic::Bvs
                | Aarch64Mnemonic::Bvc
                | Aarch64Mnemonic::Bhi
                | Aarch64Mnemonic::Bls
                | Aarch64Mnemonic::Bge
                | Aarch64Mnemonic::Blt
                | Aarch64Mnemonic::Bgt
                | Aarch64Mnemonic::Ble
                | Aarch64Mnemonic::Cbz
                | Aarch64Mnemonic::Cbnz => Some(RelocationKind::Branch19),
                Aarch64Mnemonic::Tbz | Aarch64Mnemonic::Tbnz => Some(RelocationKind::Branch14),
                _ => None,
            },
        }
    }

    fn is_pc_relative_relocation(kind: RelocationKind) -> bool {
        matches!(
            kind,
            RelocationKind::Branch26
                | RelocationKind::Branch19
                | RelocationKind::Branch14
                | RelocationKind::AdrpPage21
        )
    }

    fn is_lift_relevant_relocation(kind: RelocationKind) -> bool {
        matches!(
            kind,
            RelocationKind::Branch26
                | RelocationKind::Branch19
                | RelocationKind::Branch14
                | RelocationKind::AdrpPage21
                | RelocationKind::AddPageOffset12
                | RelocationKind::LoadStorePageOffset12 { .. }
        )
    }

    fn fuse_macros(
        instructions: Vec<RewriteInstruction<Self>>,
        block_at_address: &HashMap<u64, BasicBlockId>,
        container: Option<&Container>,
        relocations: &HashMap<u64, FusionRelocationInfo>,
    ) -> Vec<RewriteOp<Self>> {
        super::fusion::fuse_macros(instructions, block_at_address, container, relocations)
    }

    fn emit_macro(
        macro_op: &MacroOp<Self>,
        here: u64,
        block_addresses: &[u64],
        container: Option<&Container>,
        current_section: Option<SectionId>,
        bytes: &mut Vec<u8>,
        relocations: &mut Vec<MacroEmittedRelocation>,
    ) -> Result<(), MacroEmitError<Self::EncodeError>> {
        super::macro_emit::emit_macro(
            macro_op,
            here,
            block_addresses,
            container,
            current_section,
            bytes,
            relocations,
        )
    }

    // instruction_source_size + macro_source_size: defaults
    // are correct for AArch64 (4 bytes per instruction).
}

// Suppress unused-import lint for crate::rewrite::ir types
// the trait impl re-exports indirectly.
#[allow(dead_code)]
fn _phantom_use(
    _b: RewriteBlock<Aarch64Isa>,
    _o: RewriteOperand<Aarch64Isa>,
    _t: Target,
    _m: MacroOp<Aarch64Isa>,
) {
}
