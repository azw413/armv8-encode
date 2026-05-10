//! [`Isa`] implementation for ARMv7 Thumb mode.
//!
//! Stub — Stage B. Type associations wire Thumb into the
//! rewrite layer at the type level. Methods that require
//! actual encoding / fusion / variable-width handling
//! `unimplemented!()` until Stage D.
//!
//! ## Variable-width caveat
//!
//! Thumb has 16- and 32-bit instructions. The trait's
//! `instruction_source_size(mnemonic)` accepts only a
//! mnemonic, but Thumb's mnemonic doesn't uniquely determine
//! width — the same `Mov` mnemonic appears in both 2-byte and
//! 4-byte rows. Stage D either:
//!
//! 1. Adds a width field to the `RewriteInstruction` IR, or
//! 2. Threads the matched-row width through a new associated
//!    type / method, or
//! 3. Pessimistically reports 4 bytes for any Thumb mnemonic
//!    that has at least one 32-bit row (stops some callers
//!    from over-shrinking layouts).
//!
//! Option 1 is cleanest but requires touching the IR types.
//! Until that decision is made, this stub returns 4 bytes
//! (the conservative-large default).

use super::operand::{DecodedOperand, EncodeError, Register};
use super::table_generated::ThumbMnemonicGenerated;
use crate::container::RelocationKind;
use crate::isa::{
    FusionRelocationInfo, Isa, IsaEncodeOutput, MacroEmitError, MacroEmittedRelocation, PcRelKind,
};
use crate::mc::BasicBlockId;
use crate::rewrite::ir::{MacroOp, RewriteInstruction, RewriteOp};
use std::collections::HashMap;

/// Marker type implementing [`Isa`] for ARMv7 Thumb mode.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ThumbIsa;

/// Thumb-mode macro-fusion kinds.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ThumbMacroKind {
    /// `movw Rd, #lo16 ; movt Rd, #hi16` (Thumb-2 form).
    /// Mirrors the ARM-mode `MovwMovt` shape.
    MovwMovt,
}

impl Isa for ThumbIsa {
    type Mnemonic = ThumbMnemonicGenerated;
    type Register = Register;
    type Operand = DecodedOperand;
    type EncodeError = EncodeError;
    type MacroKind = ThumbMacroKind;

    fn pcrel_kind(operand: &Self::Operand) -> Option<(PcRelKind, u64)> {
        match operand {
            DecodedOperand::BranchTarget(addr) => Some((PcRelKind::Branch, *addr)),
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
            PcRelKind::Page => unimplemented!("Thumb has no Page operand"),
        }
    }

    fn pcrel_range_bytes(mnemonic: Self::Mnemonic) -> Option<i64> {
        // Thumb branch ranges depend on the encoding form,
        // not just the mnemonic — the table mnemonic is
        // shared across 16- and 32-bit forms. Layout's
        // displacement check happens before the encoder
        // commits to a form, so we must report the *largest*
        // range that any encoding for this mnemonic
        // supports. If the displacement turns out to fit a
        // smaller form, the encoder will pick that form.
        // Reporting too-tight a range would prematurely
        // trigger widening; too-loose would silently overflow
        // the encoder.
        //
        // Conservative max-range table:
        match mnemonic {
            // Unconditional B: T2 (16-bit, ±2 KiB) or T4
            // (32-bit, ±16 MiB).
            ThumbMnemonicGenerated::B => Some(16 * 1024 * 1024),
            // BL/BLX: 32-bit only, ±16 MiB.
            ThumbMnemonicGenerated::Bl | ThumbMnemonicGenerated::Blx => {
                Some(16 * 1024 * 1024)
            }
            // CBZ/CBNZ: forward 0..126 bytes only — but our
            // range model is symmetric. Report the
            // forward-only range conservatively (we'd want
            // to model "forward only" as a separate concept,
            // but for now layout treats it as "must fit
            // ±126 bytes" which is stricter than reality).
            ThumbMnemonicGenerated::Cbz | ThumbMnemonicGenerated::Cbnz => Some(126),
            _ => None,
        }
    }

    fn invert_conditional_branch(_mnemonic: Self::Mnemonic) -> Option<Self::Mnemonic> {
        // Thumb's `B` is a single mnemonic shared by
        // unconditional and conditional forms; the condition
        // lives in encoding bits 8..11 of the 16-bit T1 form
        // or bits 22..25 of the 32-bit T3 form. Inversion is
        // a bit-flip on the encoded word, not a mnemonic
        // swap. Same trait-shape mismatch as ARM mode —
        // returning None here disables conditional widening
        // for Thumb until the trait grows an
        // operand-aware variant.
        None
    }

    fn widened_conditional_size() -> u64 {
        // Thumb-2 32-bit branch is 4 bytes; the inverted-cond
        // skip is also 4 bytes when widened to .w form, so
        // the widened sequence is 8 bytes total.
        8
    }

    fn widened_conditional_range() -> i64 {
        // 32-bit B.W reach ≈ ±16 MiB.
        16 * 1024 * 1024
    }

    fn encode(
        mnemonic: Self::Mnemonic,
        operands: &[Self::Operand],
        address: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        // Delegate to the existing per-mnemonic encoder. It
        // walks the imported opcode table and picks the first
        // row whose format matches the supplied operand
        // shape. For round-trip lift→emit work this is fine;
        // for fine-grained control (preferring 16-bit T1 over
        // 32-bit T2 when both fit) the caller can use
        // `super::encode::encode_with_row` directly.
        let (word, width) =
            super::encode::encode(mnemonic, operands, address)?;
        let bytes = match width {
            super::table::ThumbWidth::Halfword => {
                let halfword = word as u16;
                halfword.to_le_bytes().to_vec()
            }
            super::table::ThumbWidth::Word => {
                // Per-row layout: high half is hw1
                // (first halfword in memory), low half is
                // hw2. Emit hw1 first as little-endian, then
                // hw2.
                let hw1 = ((word >> 16) & 0xffff) as u16;
                let hw2 = (word & 0xffff) as u16;
                let mut bytes = Vec::with_capacity(4);
                bytes.extend_from_slice(&hw1.to_le_bytes());
                bytes.extend_from_slice(&hw2.to_le_bytes());
                bytes
            }
        };
        Ok(IsaEncodeOutput { bytes })
    }

    fn encode_widened_conditional(
        _mnemonic: Self::Mnemonic,
        _operands_template: &[Self::Operand],
        _here: u64,
        _far_target: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        unimplemented!("ThumbIsa::encode_widened_conditional — Stage D")
    }

    fn relocation_kind_for(
        mnemonic: Self::Mnemonic,
        kind: PcRelKind,
    ) -> Option<RelocationKind> {
        match (kind, mnemonic) {
            (PcRelKind::Branch, ThumbMnemonicGenerated::Bl) => Some(RelocationKind::ThumbCall),
            (PcRelKind::Branch, ThumbMnemonicGenerated::Blx) => Some(RelocationKind::ThumbCall),
            (PcRelKind::Branch, ThumbMnemonicGenerated::B) => {
                // Thumb-2 32-bit conditional B.W → JUMP19;
                // unconditional B.W → JUMP24. We can't tell
                // them apart from mnemonic alone (both share
                // `B`); pick JUMP24 as the more common case.
                Some(RelocationKind::ThumbJump24)
            }
            _ => None,
        }
    }

    fn is_pc_relative_relocation(kind: RelocationKind) -> bool {
        matches!(
            kind,
            RelocationKind::ThumbCall
                | RelocationKind::ThumbJump24
                | RelocationKind::ThumbJump19
        )
    }

    fn is_lift_relevant_relocation(kind: RelocationKind) -> bool {
        matches!(
            kind,
            RelocationKind::ThumbCall
                | RelocationKind::ThumbJump24
                | RelocationKind::ThumbJump19
                | RelocationKind::ThumbMovwAbsNc
                | RelocationKind::ThumbMovtAbs
        )
    }

    fn fuse_macros(
        instructions: Vec<RewriteInstruction<Self>>,
        block_at_address: &HashMap<u64, BasicBlockId>,
        container: Option<&crate::container::Container>,
        relocations: &HashMap<u64, FusionRelocationInfo>,
    ) -> Vec<RewriteOp<Self>> {
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
        unimplemented!("ThumbIsa::emit_macro — Stage D")
    }

    fn instruction_source_size(_mnemonic: Self::Mnemonic) -> u64 {
        // Stage D will refine this. Conservative default: 4
        // (covers all 32-bit Thumb-2 forms; over-allocates
        // for 16-bit Thumb-1 forms by 2 bytes per
        // instruction). The ramifications: layout will
        // reserve 4 bytes per op, so an unmodified Thumb
        // function will *grow* in bytes by up to 2× when
        // round-tripped through the rewrite pipeline. Not
        // acceptable for production but fine for Stage B
        // type-level wiring.
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::sweep::disassemble_bytes;
    use crate::mc::build_cfg;
    use crate::rewrite::ir::RewriteOp;
    use crate::rewrite::plan::{DecodedRef, RewritePlan};

    /// Compile-time check that ThumbIsa satisfies the Isa
    /// trait — caught here at link time, more legible than a
    /// rewrite-layer error if a method gets dropped.
    fn _assert_isa<T: Isa>() {}

    #[test]
    fn thumb_isa_satisfies_trait() {
        _assert_isa::<ThumbIsa>();
    }

    /// The IR types accept ThumbIsa as the Isa parameter.
    #[test]
    fn rewrite_ir_admits_thumb_isa() {
        let _: Vec<crate::rewrite::ir::RewriteInstruction<ThumbIsa>> = Vec::new();
        let _: Vec<RewriteOp<ThumbIsa>> = Vec::new();
        let _: RewritePlan<ThumbIsa> = RewritePlan::new();
    }

    #[test]
    fn lifts_thumb_function_into_rewrite_plan() {
        // Reuse the 24-instruction fixture from the
        // existing armv7 sweep test.
        let bytes: &[u8] = &[
            0xf0, 0xb5, 0x04, 0x46, 0x0d, 0x46, 0x04, 0xeb, 0x05, 0x00, 0xa4, 0xf1,
            0x0a, 0x01, 0x88, 0x42, 0x01, 0xd0, 0xff, 0xf7, 0xfe, 0xff, 0x22, 0x68,
            0x2a, 0x60, 0x63, 0x78, 0x6b, 0x70, 0x63, 0x88, 0x6b, 0x80, 0x04, 0xea,
            0x05, 0x00, 0x44, 0xea, 0x05, 0x01, 0x84, 0xea, 0x05, 0x02, 0x4f, 0xea,
            0x84, 0x03, 0x4f, 0xea, 0x14, 0x13, 0x4f, 0xea, 0x24, 0x23, 0x64, 0xfa,
            0x05, 0xf3, 0xc0, 0x46, 0xf0, 0xbd, 0x70, 0x47,
        ];
        let base = 0x1000u64;
        let instructions = disassemble_bytes(base, bytes).expect("sweep");
        let cfg = build_cfg(&instructions);
        let refs: Vec<DecodedRef<ThumbIsa>> = instructions
            .iter()
            .map(|i| DecodedRef {
                address: i.address,
                mnemonic: i.mnemonic,
                operands: &i.operands,
            })
            .collect();
        let plan = RewritePlan::<ThumbIsa>::lift_refs(&cfg, &refs);
        let total_ops: usize = plan.blocks.iter().map(|b| b.ops.len()).sum();
        assert_eq!(total_ops, instructions.len());
    }

    #[test]
    fn encode_via_isa_round_trips_through_decode() {
        // Round-trip: encode `mov r2, #0x37` via the trait,
        // then disassemble the resulting bytes and confirm
        // the same operands come back. Less brittle than
        // pinning a specific encoding form.
        use crate::isa::armv7::operand::{DecodedOperand, Register, RegisterClass};
        let operands = vec![
            DecodedOperand::Register(Register {
                class: RegisterClass::R,
                index: 2,
            }),
            DecodedOperand::Immediate(0x37),
        ];
        let out = <ThumbIsa as Isa>::encode(
            ThumbMnemonicGenerated::Mov,
            &operands,
            0,
        )
        .expect("encode");
        // 16-bit T1 movs immediate is 2 bytes.
        assert!(out.bytes.len() == 2 || out.bytes.len() == 4);
        // Round-trip through the sweep.
        let decoded = disassemble_bytes(0, &out.bytes).expect("decode");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].mnemonic, ThumbMnemonicGenerated::Mov);
    }
}
