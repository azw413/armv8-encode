//! [`Isa`] implementation for ARMv7 ARM-mode (A32).
//!
//! Stub — Stage B. Type associations are wired so the rewrite
//! layer admits ARMv7-ARM at the type level. Methods that
//! require real branch encoding / fusion / emission detail
//! `unimplemented!()` until Stage C.

use super::table_generated::ArmMnemonicGenerated;
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

    fn invert_conditional_branch(_mnemonic: Self::Mnemonic) -> Option<Self::Mnemonic> {
        // ARM-mode design issue: the condition lives in the
        // encoded word's top 4 bits, not the mnemonic.
        // Inverting a conditional branch is a bit-flip on the
        // word, not a mnemonic swap. The current trait shape
        // takes only `mnemonic` and returns a different
        // mnemonic — there's no way to express ARM-mode
        // inversion within it.
        //
        // Returning `None` here means the layout pass will
        // never widen ARM-mode conditional branches into the
        // <inverted> + far form; it'll surface
        // DisplacementTooLarge instead. That's acceptable for
        // now: cross-section ARM jumps are rare, and the
        // PLT (the main ARM-mode code in real binaries) uses
        // unconditional ldr-pc-style indirect jumps that
        // don't widen.
        //
        // A future trait revision could change the signature
        // to `(mnemonic, operands) -> Option<(mnemonic, operands)>`
        // so ARM mode can flip the cond bits within an
        // operand-side condition representation — but that's
        // a larger discussion than Stage C should resolve.
        None
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
        _operands_template: &[Self::Operand],
        _here: u64,
        _far_target: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        unimplemented!("ArmIsa::encode_widened_conditional — Stage C")
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
        _macro_op: &MacroOp<Self>,
        _here: u64,
        _block_addresses: &[u64],
        _container: Option<&crate::container::Container>,
        _current_section: Option<crate::container::SectionId>,
        _bytes: &mut Vec<u8>,
        _relocations: &mut Vec<MacroEmittedRelocation>,
    ) -> Result<(), MacroEmitError<Self::EncodeError>> {
        unimplemented!("ArmIsa::emit_macro — Stage C")
    }

    // instruction_source_size + macro_source_size: defaults
    // are correct for ARM mode (4 bytes per instruction).
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
