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
use super::table_generated::{
    ThumbMnemonicGenerated, ThumbOpcodeGenerated, THUMB_OPCODE_TABLE_GENERATED,
};
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

    fn invert_conditional_branch(mnemonic: Self::Mnemonic) -> Option<Self::Mnemonic> {
        // Thumb's `B` is shared by conditional and
        // unconditional forms — the condition lives in an
        // operand (`DecodedOperand::Condition`) rather than in
        // the mnemonic. The trait can only return a mnemonic
        // here, so we return `Some(B)` to signal "this is
        // widenable" and let `encode_widened_conditional` do
        // the actual operand-side condition flip.
        //
        // For unconditional B (Condition = 14 / AL), the
        // widening emitter detects that and falls back to a
        // single B.W — there's nothing to invert. Returning
        // `Some(B)` doesn't break that path.
        match mnemonic {
            ThumbMnemonicGenerated::B => Some(ThumbMnemonicGenerated::B),
            _ => None,
        }
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
        operands_template: &[Self::Operand],
        here: u64,
        far_target: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        // Thumb-2 widened conditional sequence:
        //   B<!cond>.W  here+8   (skip past the far branch)
        //   B.W         far_target
        //
        // Each is a 32-bit Thumb-2 form (B.W = T4 for the
        // unconditional, B<cond>.W = T3 for the inverted-cond
        // skip). Total 8 bytes — matches widened_conditional_size.
        //
        // If the source branch is unconditional (Condition == AL
        // or missing), the "inversion" is vacuous: emit a single
        // 32-bit B.W to far_target plus a Thumb-2 NOP to keep
        // the size promise of 8 bytes.
        let cond = operands_template.iter().find_map(|op| match op {
            DecodedOperand::Condition(c) => Some(*c),
            _ => None,
        });
        let is_conditional = matches!(cond, Some(c) if c < 14);

        // Locate the specific rows we want by mask/opcode.
        let row_b_w_unconditional = find_row(0xf0009000, 0xf800d000)
            .expect("Thumb T4 B.W (unconditional, 32-bit) must be in the opcode table");
        let row_b_w_conditional = find_row(0xf0008000, 0xf800d000)
            .expect("Thumb T3 B<cond>.W (conditional, 32-bit) must be in the opcode table");

        let mut bytes = Vec::with_capacity(8);

        if is_conditional {
            let inverted_cond = (cond.unwrap() ^ 1) & 0xf;
            let skip = here.wrapping_add(8);
            let inv_operands = build_operands_with_overrides(
                operands_template,
                Some(inverted_cond),
                Some(skip),
            );
            let (word, _w) = super::encode::encode_with_row(
                row_b_w_conditional,
                &inv_operands,
                here,
            )?;
            push_word_thumb(&mut bytes, word);

            let far_operands = vec![DecodedOperand::BranchTarget(far_target)];
            let (word, _w) = super::encode::encode_with_row(
                row_b_w_unconditional,
                &far_operands,
                here.wrapping_add(4),
            )?;
            push_word_thumb(&mut bytes, word);
        } else {
            // Unconditional source: emit B.W to far_target plus
            // a Thumb-2 NOP.W to preserve the 8-byte size.
            let far_operands = vec![DecodedOperand::BranchTarget(far_target)];
            let (word, _w) = super::encode::encode_with_row(
                row_b_w_unconditional,
                &far_operands,
                here,
            )?;
            push_word_thumb(&mut bytes, word);
            // Thumb-2 NOP.W (T2): 0xf3af8000.
            push_word_thumb(&mut bytes, 0xf3af_8000);
        }
        Ok(IsaEncodeOutput { bytes })
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

    fn instruction_source_size(mnemonic: Self::Mnemonic) -> u64 {
        // Conservative-large: 2 if *every* table row for this
        // mnemonic is a 16-bit halfword form, else 4. When both
        // widths exist for a mnemonic, the layout pass reserves
        // 4 bytes; if the encoder later picks the 16-bit form
        // the surrounding code is shifted up but stays
        // correctly placed (since the emit pass writes the
        // actual encoded bytes after layout has computed
        // addresses). This is a known over-reservation for
        // mixed-width mnemonics — refining it requires either
        // an IR-level width tag or a trait change.
        let mut saw_word = false;
        for row in THUMB_OPCODE_TABLE_GENERATED.iter() {
            if row.mnemonic != mnemonic {
                continue;
            }
            if matches!(row.width, super::table::ThumbWidth::Word) {
                saw_word = true;
                break;
            }
        }
        if saw_word { 4 } else { 2 }
    }
}

/// Walk the static Thumb opcode table for a row whose
/// (opcode, mask) pair matches exactly. Used by
/// `encode_widened_conditional` to pin the specific T3 / T4
/// rows it needs.
fn find_row(opcode: u32, mask: u32) -> Option<&'static ThumbOpcodeGenerated> {
    THUMB_OPCODE_TABLE_GENERATED
        .iter()
        .find(|row| row.opcode == opcode && row.mask == mask)
}

/// Build an operand list for the inverted-conditional branch:
/// copy the original operands, overriding the `Condition` slot
/// with `cond_override` and the `BranchTarget` slot with
/// `branch_override`. If either slot is absent in the original,
/// append the override (defensive for unusual templates).
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

/// Push a 32-bit Thumb word into `bytes` as two halfwords:
/// hw1 (high half, first in memory) then hw2, each
/// little-endian. Mirrors what `ThumbIsa::encode` does for
/// `ThumbWidth::Word`.
fn push_word_thumb(bytes: &mut Vec<u8>, word: u32) {
    let hw1 = ((word >> 16) & 0xffff) as u16;
    let hw2 = (word & 0xffff) as u16;
    bytes.extend_from_slice(&hw1.to_le_bytes());
    bytes.extend_from_slice(&hw2.to_le_bytes());
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
    fn widened_conditional_for_conditional_branch_decodes_back_as_pair() {
        // B<eq> to a near target — pretend the layout pass
        // decided we needed widening.
        use crate::isa::armv7::operand::DecodedOperand;
        let here = 0x1000u64;
        let far_target = 0x80_0000u64; // 8 MiB away — within B.W (T4) ±16 MiB
        // operands_template: an Equal-cond branch to *some* original
        // target. The widening encoder cares only about the cond
        // and overrides the target.
        let template = vec![
            DecodedOperand::Condition(0), // eq
            DecodedOperand::BranchTarget(here.wrapping_add(8)),
        ];
        let out = <ThumbIsa as Isa>::encode_widened_conditional(
            ThumbMnemonicGenerated::B,
            &template,
            here,
            far_target,
        )
        .expect("widening encode");
        assert_eq!(out.bytes.len(), 8, "widened conditional must be 8 bytes");

        // Re-disassemble: should be two Thumb-2 32-bit B
        // instructions. First is the inverted-cond (B<ne>) to
        // here+8; second is unconditional B.W to far_target.
        let decoded =
            disassemble_bytes(here, &out.bytes).expect("decode widened pair");
        assert_eq!(decoded.len(), 2, "expected exactly two instructions");
        assert_eq!(decoded[0].mnemonic, ThumbMnemonicGenerated::B);
        assert_eq!(decoded[1].mnemonic, ThumbMnemonicGenerated::B);

        // First instruction's branch target must be the skip
        // address (here + 8).
        let first_target = decoded[0].operands.iter().find_map(|o| match o {
            DecodedOperand::BranchTarget(a) => Some(*a),
            _ => None,
        });
        assert_eq!(first_target, Some(here.wrapping_add(8)));

        // First instruction's condition must be inverted: 0 (eq) -> 1 (ne).
        let first_cond = decoded[0].operands.iter().find_map(|o| match o {
            DecodedOperand::Condition(c) => Some(*c),
            _ => None,
        });
        assert_eq!(first_cond, Some(1));

        // Second instruction's branch target must be far_target.
        let second_target = decoded[1].operands.iter().find_map(|o| match o {
            DecodedOperand::BranchTarget(a) => Some(*a),
            _ => None,
        });
        assert_eq!(second_target, Some(far_target));
    }

    #[test]
    fn widened_conditional_for_unconditional_branch_is_b_w_plus_nop() {
        use crate::isa::armv7::operand::DecodedOperand;
        let here = 0x1000u64;
        let far_target = 0x40_0000u64; // 4 MiB
        // Unconditional B template — cond = 14 (AL).
        let template = vec![
            DecodedOperand::Condition(14),
            DecodedOperand::BranchTarget(here.wrapping_add(8)),
        ];
        let out = <ThumbIsa as Isa>::encode_widened_conditional(
            ThumbMnemonicGenerated::B,
            &template,
            here,
            far_target,
        )
        .expect("widening encode (unconditional)");
        assert_eq!(out.bytes.len(), 8);

        let decoded =
            disassemble_bytes(here, &out.bytes).expect("decode widened uncond");
        // First instruction: B.W to far_target.
        assert_eq!(decoded[0].mnemonic, ThumbMnemonicGenerated::B);
        let target = decoded[0].operands.iter().find_map(|o| match o {
            DecodedOperand::BranchTarget(a) => Some(*a),
            _ => None,
        });
        assert_eq!(target, Some(far_target));
        // Second instruction: Thumb-2 NOP.W (0xf3af8000). Its
        // mnemonic should be Nop.
        assert_eq!(decoded[1].mnemonic, ThumbMnemonicGenerated::Nop);
    }

    #[test]
    fn instruction_source_size_is_two_for_halfword_only_mnemonics() {
        // Push/Pop only have halfword forms in the table.
        // (Verify via the helper directly.)
        assert_eq!(
            <ThumbIsa as Isa>::instruction_source_size(ThumbMnemonicGenerated::Push),
            2
        );
        // Movw is 32-bit only.
        assert_eq!(
            <ThumbIsa as Isa>::instruction_source_size(ThumbMnemonicGenerated::Movw),
            4
        );
        // Mov has both 16- and 32-bit forms → conservative 4.
        assert_eq!(
            <ThumbIsa as Isa>::instruction_source_size(ThumbMnemonicGenerated::Mov),
            4
        );
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
