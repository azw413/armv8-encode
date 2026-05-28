//! [`Isa`] implementation for ARMv7 ARM-mode (A32).
//!
//! Stub — Stage B. Type associations are wired so the rewrite
//! layer admits ARMv7-ARM at the type level. Methods that
//! require real branch encoding / fusion / emission detail
//! `unimplemented!()` until Stage C.

use super::table_generated::{
    ArmMnemonicGenerated, ArmOpcodeGenerated, ARM_OPCODE_TABLE_GENERATED,
};
use crate::container::RelocationKind;
use crate::isa::armv7::operand::{DecodedOperand, EncodeError, Register};
use crate::isa::{
    FusionRelocationInfo, Isa, IsaEncodeOutput, MacroEmitError, MacroEmittedRelocation, PcRelKind,
};
use crate::mc::BasicBlockId;
use crate::rewrite::ir::{MacroOp, RewriteInstruction, RewriteOp};
use std::collections::HashMap;

/// Marker type implementing [`Isa`] for ARMv7 ARM-mode.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ArmIsa;

/// ARM-mode macro-fusion kinds. Initially empty — patterns
/// like `movw + movt` (load-immediate-32) will be added in
/// Stage C as the fuser is built.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ArmMacroKind {
    /// `movw Rd, #lo16 ; movt Rd, #hi16` — synthesise a
    /// 32-bit constant address. Concrete fusion lands in
    /// Stage C.
    MovwMovt,
}

impl Isa for ArmIsa {
    type Mnemonic = ArmMnemonicGenerated;
    type Register = Register;
    type Operand = DecodedOperand;
    type EncodeError = EncodeError;
    type MacroKind = ArmMacroKind;

    fn pcrel_kind(operand: &Self::Operand) -> Option<(PcRelKind, u64)> {
        match operand {
            DecodedOperand::BranchTarget(addr) => Some((PcRelKind::Branch, *addr)),
            // ARM has no Page operand; PC-relative loads use
            // `PcRelative`, which the rewriter doesn't yet
            // treat as a layout-pass concern.
            _ => None,
        }
    }

    fn substitute_pcrel(operand: &Self::Operand, address: u64) -> Self::Operand {
        match operand {
            DecodedOperand::BranchTarget(_) => DecodedOperand::BranchTarget(address),
            other => other.clone(),
        }
    }

    fn make_pcrel_operand(kind: PcRelKind, address: u64) -> Self::Operand {
        match kind {
            PcRelKind::Branch => DecodedOperand::BranchTarget(address),
            PcRelKind::Page => unimplemented!("ARM mode has no Page operand"),
        }
    }

    fn pcrel_range_bytes(mnemonic: Self::Mnemonic) -> Option<i64> {
        // ARM mode: the only branch row in the imported
        // table is `B` (binutils encodes both unconditional
        // B and conditional B<cond> in the same row, with
        // the cond field encoded in word bits 28..31). The
        // displacement is `imm24 << 2`, sign-extended →
        // ±(2^25) bytes = ±32 MiB.
        //
        // BL shares this row too — binutils' format string is
        // `b%24'l%c\t%b`, where bit 24 chooses B vs BL.
        match mnemonic {
            ArmMnemonicGenerated::B => Some(32 * 1024 * 1024),
            // BLX (immediate) — same 24-bit reach plus an
            // extra half-word offset bit; range is the same
            // for layout-pass purposes.
            ArmMnemonicGenerated::Blx => Some(32 * 1024 * 1024),
            _ => None,
        }
    }

    fn invert_conditional_branch(mnemonic: Self::Mnemonic) -> Option<Self::Mnemonic> {
        // ARM mode stores the condition in the encoded word's
        // top 4 bits, which our decoder surfaces as a
        // `DecodedOperand::Condition(c)` slot. The trait can
        // only return a mnemonic here, so we return `Some(B)`
        // to signal "this is widenable" and let
        // `encode_widened_conditional` do the operand-side
        // cond inversion. Same approach as Thumb mode.
        //
        // For an unconditional B (Condition = AL / 14), the
        // widened sequence degenerates to a single B + NOP
        // pair, which `encode_widened_conditional` handles.
        match mnemonic {
            ArmMnemonicGenerated::B => Some(ArmMnemonicGenerated::B),
            _ => None,
        }
    }

    fn widened_conditional_size() -> u64 {
        8
    }

    fn widened_conditional_range() -> i64 {
        // ARM B has ±32 MiB reach (24-bit signed × 4).
        32 * 1024 * 1024
    }

    fn encode(
        mnemonic: Self::Mnemonic,
        operands: &[Self::Operand],
        address: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        let word = super::encode::encode(mnemonic, operands, address)?;
        Ok(IsaEncodeOutput {
            bytes: word.to_le_bytes().to_vec(),
        })
    }

    fn encode_widened_conditional(
        _mnemonic: Self::Mnemonic,
        operands_template: &[Self::Operand],
        here: u64,
        far_target: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        // ARM-mode widened conditional sequence:
        //   B<!cond>  here+8   (skip past the unconditional)
        //   B         far_target
        //
        // Each is 4 bytes; total 8 bytes — matches
        // widened_conditional_size. The B row shared by both
        // conditional and unconditional uses cond in the top 4
        // encoded bits, sourced from the `Condition` operand.
        //
        // If the source branch is unconditional (cond == AL),
        // inversion is vacuous: emit a single B to far_target
        // plus an ARM NOP (mov r0, r0) to preserve the 8-byte
        // size promise.
        let cond = operands_template.iter().find_map(|op| match op {
            DecodedOperand::Condition(c) => Some(*c),
            _ => None,
        });
        let is_conditional = matches!(cond, Some(c) if c < 14);

        let row_b = find_row(0x0a000000, 0x0e000000)
            .expect("ARM B row must be in the opcode table");

        let mut bytes = Vec::with_capacity(8);

        if is_conditional {
            let inverted_cond = (cond.unwrap() ^ 1) & 0xf;
            let skip = here.wrapping_add(8);
            let inv_operands = build_operands_with_overrides(
                operands_template,
                Some(inverted_cond),
                Some(skip),
            );
            let word = super::encode::encode_with_row(row_b, &inv_operands, here)?;
            bytes.extend_from_slice(&word.to_le_bytes());

            let far_operands = vec![
                DecodedOperand::Condition(14),
                DecodedOperand::BranchTarget(far_target),
            ];
            let word = super::encode::encode_with_row(
                row_b,
                &far_operands,
                here.wrapping_add(4),
            )?;
            bytes.extend_from_slice(&word.to_le_bytes());
        } else {
            // Unconditional source: emit B to far_target +
            // ARM NOP (0xe1a00000 = mov r0, r0, the v5 canonical
            // form).
            let far_operands = vec![
                DecodedOperand::Condition(14),
                DecodedOperand::BranchTarget(far_target),
            ];
            let word = super::encode::encode_with_row(row_b, &far_operands, here)?;
            bytes.extend_from_slice(&word.to_le_bytes());
            bytes.extend_from_slice(&0xe1a0_0000u32.to_le_bytes());
        }
        Ok(IsaEncodeOutput { bytes })
    }

    fn relocation_kind_for(
        mnemonic: Self::Mnemonic,
        kind: PcRelKind,
    ) -> Option<RelocationKind> {
        match (kind, mnemonic) {
            (PcRelKind::Branch, ArmMnemonicGenerated::B) => Some(RelocationKind::ArmCall),
            (PcRelKind::Branch, ArmMnemonicGenerated::Blx) => Some(RelocationKind::ArmCall),
            // ARM has no Page operand; rewriter shouldn't ask.
            _ => None,
        }
    }

    fn is_pc_relative_relocation(kind: RelocationKind) -> bool {
        matches!(
            kind,
            RelocationKind::ArmCall
                | RelocationKind::ArmJump24
                | RelocationKind::ArmPc24
        )
    }

    fn is_lift_relevant_relocation(kind: RelocationKind) -> bool {
        matches!(
            kind,
            RelocationKind::ArmCall
                | RelocationKind::ArmJump24
                | RelocationKind::ArmPc24
                | RelocationKind::ArmMovwAbsNc
                | RelocationKind::ArmMovtAbs
        )
    }

    fn fuse_macros(
        instructions: Vec<RewriteInstruction<Self>>,
        block_at_address: &HashMap<u64, BasicBlockId>,
        container: Option<&crate::container::Container>,
        relocations: &HashMap<u64, FusionRelocationInfo>,
    ) -> Vec<RewriteOp<Self>> {
        // Default behaviour: no fusion. Stage C adds
        // `movw + movt` recognition.
        let _ = (block_at_address, container, relocations);
        instructions
            .into_iter()
            .map(RewriteOp::Instruction)
            .collect()
    }

    fn emit_macro(
        macro_op: &MacroOp<Self>,
        here: u64,
        block_addresses: &[u64],
        container: Option<&crate::container::Container>,
        current_section: Option<crate::container::SectionId>,
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
    // are correct for ARM mode (4 bytes per instruction).
}

/// Find a row in the static ARM opcode table by exact
/// (opcode, mask) match. Used by `encode_widened_conditional`
/// to pin the specific B row.
fn find_row(opcode: u32, mask: u32) -> Option<&'static ArmOpcodeGenerated> {
    ARM_OPCODE_TABLE_GENERATED
        .iter()
        .find(|row| row.opcode == opcode && row.mask == mask)
}

/// Build an operand list for the inverted-conditional branch:
/// copy the original operands, overriding the `Condition` slot
/// with `cond_override` and the `BranchTarget` slot with
/// `branch_override`. If either slot is absent in the original,
/// append the override.
fn build_operands_with_overrides(
    template: &[DecodedOperand],
    cond_override: Option<u8>,
    branch_override: Option<u64>,
) -> Vec<DecodedOperand> {
    let mut overrode_cond = false;
    let mut overrode_branch = false;
    let mut out: Vec<DecodedOperand> = template
        .iter()
        .map(|op| match op {
            DecodedOperand::Condition(_) if cond_override.is_some() => {
                overrode_cond = true;
                DecodedOperand::Condition(cond_override.unwrap())
            }
            DecodedOperand::BranchTarget(_) if branch_override.is_some() => {
                overrode_branch = true;
                DecodedOperand::BranchTarget(branch_override.unwrap())
            }
            other => other.clone(),
        })
        .collect();
    if let Some(c) = cond_override {
        if !overrode_cond {
            out.push(DecodedOperand::Condition(c));
        }
    }
    if let Some(b) = branch_override {
        if !overrode_branch {
            out.push(DecodedOperand::BranchTarget(b));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::arm::sweep::disassemble_bytes;
    use crate::mc::build_cfg;
    use crate::rewrite::ir::{RewriteOp, RewriteOperand, Target};
    use crate::rewrite::plan::{DecodedRef, RewritePlan};

    fn _assert_isa<T: Isa>() {}

    #[test]
    fn arm_isa_satisfies_trait() {
        _assert_isa::<ArmIsa>();
    }

    #[test]
    fn widened_conditional_for_conditional_branch_decodes_back_as_pair() {
        let here = 0x1000u64;
        // Far target within ARM B reach (±32 MiB).
        let far_target = 0x80_0000u64; // 8 MiB
        let template = vec![
            DecodedOperand::Condition(0), // eq
            DecodedOperand::BranchTarget(here.wrapping_add(8)),
        ];
        let out = <ArmIsa as Isa>::encode_widened_conditional(
            ArmMnemonicGenerated::B,
            &template,
            here,
            far_target,
        )
        .expect("widening encode");
        assert_eq!(out.bytes.len(), 8, "widened conditional must be 8 bytes");

        let decoded =
            disassemble_bytes(here, &out.bytes).expect("decode widened pair");
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].mnemonic, ArmMnemonicGenerated::B);
        assert_eq!(decoded[1].mnemonic, ArmMnemonicGenerated::B);

        // First branch: inverted-cond (eq → ne = 1) to here + 8.
        let first_cond = decoded[0].operands.iter().find_map(|o| match o {
            DecodedOperand::Condition(c) => Some(*c),
            _ => None,
        });
        assert_eq!(first_cond, Some(1));
        let first_target = decoded[0].operands.iter().find_map(|o| match o {
            DecodedOperand::BranchTarget(a) => Some(*a),
            _ => None,
        });
        assert_eq!(first_target, Some(here.wrapping_add(8)));

        // Second branch: AL to far_target.
        let second_cond = decoded[1].operands.iter().find_map(|o| match o {
            DecodedOperand::Condition(c) => Some(*c),
            _ => None,
        });
        assert_eq!(second_cond, Some(14));
        let second_target = decoded[1].operands.iter().find_map(|o| match o {
            DecodedOperand::BranchTarget(a) => Some(*a),
            _ => None,
        });
        assert_eq!(second_target, Some(far_target));
    }

    #[test]
    fn widened_conditional_for_unconditional_branch_is_b_plus_nop() {
        let here = 0x1000u64;
        let far_target = 0x40_0000u64;
        let template = vec![
            DecodedOperand::Condition(14), // AL
            DecodedOperand::BranchTarget(here.wrapping_add(8)),
        ];
        let out = <ArmIsa as Isa>::encode_widened_conditional(
            ArmMnemonicGenerated::B,
            &template,
            here,
            far_target,
        )
        .expect("widening encode (unconditional)");
        assert_eq!(out.bytes.len(), 8);

        // First word: B AL to far_target. Second word:
        // 0xe1a00000 = mov r0, r0 (ARM NOP). The disassembler's
        // Nop row matches this exactly (table_generated.rs
        // line 653 has the canonical 0xe1a00000).
        let decoded = disassemble_bytes(here, &out.bytes).expect("decode");
        assert_eq!(decoded[0].mnemonic, ArmMnemonicGenerated::B);
        let target = decoded[0].operands.iter().find_map(|o| match o {
            DecodedOperand::BranchTarget(a) => Some(*a),
            _ => None,
        });
        assert_eq!(target, Some(far_target));
        assert_eq!(decoded[1].mnemonic, ArmMnemonicGenerated::Nop);
    }

    #[test]
    fn rewrite_ir_admits_arm_isa() {
        let _: Vec<crate::rewrite::ir::RewriteInstruction<ArmIsa>> = Vec::new();
        let _: Vec<RewriteOp<ArmIsa>> = Vec::new();
        let _: RewritePlan<ArmIsa> = RewritePlan::new();
    }

    #[test]
    fn lifts_arm_plt_stub_into_rewrite_plan() {
        // PLT stub from libtool-checker.so at 0xf84:
        //   e52de004    push {lr}
        //   e59fe004    ldr lr, [pc, #4]
        //   e08fe00e    add lr, pc, lr
        //   e5bef008    ldr pc, [lr, #8]!
        let bytes: &[u8] = &[
            0x04, 0xe0, 0x2d, 0xe5,
            0x04, 0xe0, 0x9f, 0xe5,
            0x0e, 0xe0, 0x8f, 0xe0,
            0x08, 0xf0, 0xbe, 0xe5,
        ];
        let base = 0xf84u64;
        let instructions = disassemble_bytes(base, bytes).expect("sweep");
        let cfg = build_cfg(&instructions);
        // Build DecodedRefs from the sweep result.
        let refs: Vec<DecodedRef<ArmIsa>> = instructions
            .iter()
            .map(|i| DecodedRef {
                address: i.address,
                mnemonic: i.mnemonic,
                operands: &i.operands,
            })
            .collect();
        let plan = RewritePlan::<ArmIsa>::lift_refs(&cfg, &refs);
        // Plan should have at least one block carrying all 4
        // instructions (the PLT stub is straight-line code
        // ending in an indirect jump that has no direct
        // target, so the CFG terminates the block but produces
        // exactly one block).
        assert!(!plan.blocks.is_empty());
        let total_ops: usize = plan.blocks.iter().map(|b| b.ops.len()).sum();
        assert_eq!(total_ops, 4);
        // No fusion configured for ARM yet, so every op
        // should be a singleton Instruction.
        for block in &plan.blocks {
            for op in &block.ops {
                assert!(matches!(op, RewriteOp::Instruction(_)));
            }
        }
        // Verify the lift produced no `Branch`/`Page` operands
        // (the stub has no PC-relative branch operands the
        // ARM format-decoder currently emits — only memory
        // addressing-mode targets).
        let symbolic_count: usize = plan
            .blocks
            .iter()
            .flat_map(|b| b.ops.iter())
            .filter_map(|op| match op {
                RewriteOp::Instruction(insn) => Some(insn),
                _ => None,
            })
            .flat_map(|i| i.operands.iter())
            .filter(|o| matches!(o, RewriteOperand::Branch(_) | RewriteOperand::Page(_)))
            .count();
        // PLT stubs reach the resolver via `ldr pc, [lr, #8]!`
        // which our format decoder doesn't classify as
        // PcRelKind::Branch — that's deliberate (it's an
        // indirect jump; the layout pass shouldn't try to
        // range-check it).
        assert_eq!(symbolic_count, 0);
        let _ = Target::Absolute(0); // silence unused import
    }
}
