//! The [`Isa`] trait — the rewrite layer is generic over it.
//!
//! Once the crate supports more than one ISA (AArch64 plus
//! ARMv7's two modes), the rewrite pipeline (lift → edit →
//! lay out → emit) needs to call into per-ISA logic without
//! hardcoding any specific ISA's mnemonic / operand / register
//! types. The trait carries:
//!
//! - **Type associations** — `Mnemonic`, `Operand`, `Register`,
//!   `EncodeError`, `MacroKind`. The rewrite layer's IR types
//!   are parameterised over these.
//! - **Branch geometry** — `pcrel_range_bytes` for the layout
//!   pass's range checks; `invert_conditional_branch` and
//!   `widened_conditional_size` for out-of-range widening.
//! - **Encoding** — `encode` turns `(mnemonic, operands,
//!   address)` into bytes.
//! - **Operand introspection** — `pcrel_kind` /
//!   `substitute_pcrel` let lift / emit handle PC-relative
//!   targets opaquely.
//! - **Relocation kind mapping** — which ISA-specific
//!   `RelocationKind`s correspond to PC-relative operands and
//!   the inverse.
//! - **Macro fusion** — `fuse_macros` recognises
//!   multi-instruction idioms (`adrp+add` on AArch64,
//!   `movw+movt` on ARMv7).

use crate::container::{Container, RelocationKind};
use crate::mc::BasicBlockId;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

/// Bytes produced by [`Isa::encode`].
#[derive(Debug, Clone)]
pub struct IsaEncodeOutput {
    pub bytes: Vec<u8>,
}

/// Classification of a PC-relative operand.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PcRelKind {
    /// Branch destination — direct call/jump operand subject
    /// to layout-pass widening.
    Branch,
    /// Page-relative target (AArch64 `adrp`). ISAs without
    /// this concept never produce `Page` operands.
    Page,
}

/// Per-instruction relocation summary used during lift +
/// fusion. The IR layer builds a `HashMap<u64, FusionRelocationInfo>`
/// keyed by instruction address and passes it to
/// [`Isa::fuse_macros`].
#[derive(Debug, Clone)]
pub struct FusionRelocationInfo {
    pub kind: RelocationKind,
    pub symbol: Option<crate::container::SymbolId>,
}

/// The architecture-neutral interface the rewrite layer is
/// parameterised over. See module docs for design notes.
pub trait Isa: 'static + Sized + Debug {
    /// Mnemonic enum (e.g. `Aarch64Mnemonic`).
    type Mnemonic: Copy + Eq + Hash + Debug + 'static;
    /// Register type.
    type Register: Clone + Eq + Debug + 'static;
    /// Decoded operand type.
    type Operand: Clone + Eq + Debug + 'static;
    /// Encoder error.
    type EncodeError: Debug + 'static;
    /// ISA-specific macro-fusion kinds.
    type MacroKind: Copy + Eq + Hash + Debug + 'static;
    /// Sweep output — what the per-ISA `disassemble_bytes`
    /// returns. The rewrite layer caches a `Vec<DecodedInstruction>`
    /// in `LiftedTextSection` and uses
    /// [`Isa::decoded_address`] / [`Isa::decoded_mnemonic`] /
    /// [`Isa::decoded_operands`] to feed the generic lift path
    /// without knowing the concrete shape.
    type DecodedInstruction: Clone + Debug + 'static;

    // --- DecodedInstruction accessors ----------------------

    /// Address of the decoded instruction.
    fn decoded_address(insn: &Self::DecodedInstruction) -> u64;

    /// Mnemonic of the decoded instruction.
    fn decoded_mnemonic(insn: &Self::DecodedInstruction) -> Self::Mnemonic;

    /// Operand slice of the decoded instruction. Borrowed so
    /// the rewrite layer can build `DecodedRef`s without
    /// cloning the operand vec.
    fn decoded_operands(insn: &Self::DecodedInstruction) -> &[Self::Operand];

    // --- Operand introspection -----------------------------

    /// If `operand` carries a PC-relative target, return the
    /// kind plus its absolute address.
    fn pcrel_kind(operand: &Self::Operand) -> Option<(PcRelKind, u64)>;

    /// Replace the address inside a PC-relative operand.
    /// Used by the emit pass to lower `Target::Block(id)`
    /// back to a concrete displacement.
    fn substitute_pcrel(operand: &Self::Operand, address: u64) -> Self::Operand;

    /// Construct a fresh PC-relative operand of the given
    /// kind pointing at `address`. Used by the emit pass to
    /// rebuild operands from a symbolic [`super::Target`]
    /// when no template-operand is available in the
    /// instruction's operand list.
    fn make_pcrel_operand(kind: PcRelKind, address: u64) -> Self::Operand;

    // --- Branch / widening geometry ------------------------

    /// Half-range of `mnemonic`'s PC-relative operand in
    /// bytes. `None` for non-branch / unsupported.
    fn pcrel_range_bytes(mnemonic: Self::Mnemonic) -> Option<i64>;

    /// Mnemonic with the inverted condition (`Beq → Bne`,
    /// etc.). `None` for unconditional / non-branch.
    fn invert_conditional_branch(mnemonic: Self::Mnemonic) -> Option<Self::Mnemonic>;

    /// Encoded byte size of the widened-conditional
    /// sequence.
    fn widened_conditional_size() -> u64;

    /// Effective half-range of the widened conditional's
    /// inner unconditional branch.
    fn widened_conditional_range() -> i64;

    // --- Encoding ------------------------------------------

    /// Encode an instruction at `address`.
    fn encode(
        mnemonic: Self::Mnemonic,
        operands: &[Self::Operand],
        address: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError>;

    /// Encode a widened conditional:
    ///
    /// ```text
    ///   <inverted_cond> .Lskip
    ///   <unconditional_branch> far_target
    /// .Lskip:
    /// ```
    fn encode_widened_conditional(
        mnemonic: Self::Mnemonic,
        operands_template: &[Self::Operand],
        here: u64,
        far_target: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError>;

    // --- Relocation mapping --------------------------------

    /// Container relocation kind for an instruction whose
    /// PC-relative operand targets an undefined symbol.
    fn relocation_kind_for(
        mnemonic: Self::Mnemonic,
        kind: PcRelKind,
    ) -> Option<RelocationKind>;

    /// True iff this relocation targets a PC-relative operand
    /// the lift pass should reinterpret as `Target::Symbol`.
    fn is_pc_relative_relocation(kind: RelocationKind) -> bool;

    /// True iff this relocation kind matters during lift —
    /// includes `is_pc_relative_relocation` plus macro-fusion
    /// companion kinds (e.g. AArch64 `PageOffset12`).
    fn is_lift_relevant_relocation(kind: RelocationKind) -> bool;

    // --- Macro fusion --------------------------------------

    /// Recognise multi-instruction idioms in `instructions`
    /// and emit a fused [`crate::rewrite::ir::RewriteOp`]
    /// stream. Default: no fusion (every instruction passes
    /// through as a singleton).
    fn fuse_macros(
        instructions: Vec<crate::rewrite::ir::RewriteInstruction<Self>>,
        block_at_address: &HashMap<u64, BasicBlockId>,
        container: Option<&Container>,
        relocations: &HashMap<u64, FusionRelocationInfo>,
    ) -> Vec<crate::rewrite::ir::RewriteOp<Self>> {
        let _ = (block_at_address, container, relocations);
        instructions
            .into_iter()
            .map(crate::rewrite::ir::RewriteOp::Instruction)
            .collect()
    }

    // --- Sizing --------------------------------------------

    /// Source-byte size of the macro built from
    /// `original_instructions`. Default: sum of each
    /// instruction's source size.
    fn macro_source_size(
        original_instructions: &[crate::rewrite::ir::RewriteInstruction<Self>],
    ) -> u64 {
        original_instructions
            .iter()
            .map(|i| Self::instruction_source_size(i.mnemonic))
            .sum()
    }

    /// Source-byte size of a singleton instruction. Aarch64
    /// is always 4; variable-width ISAs override based on
    /// the mnemonic.
    fn instruction_source_size(_mnemonic: Self::Mnemonic) -> u64 {
        4
    }

    // --- Macro emission ------------------------------------

    /// Emit a fused [`crate::rewrite::ir::MacroOp`] back into
    /// bytes (and any required relocations). The default
    /// implementation expands the macro back into its
    /// `original_instructions` verbatim — appropriate for
    /// macros that don't need symbolic-target rebinding.
    /// AArch64 overrides to handle `adrp+add`/`adrp+ldr/str`
    /// rebinding; ARMv7 overrides for `movw+movt`.
    fn emit_macro(
        macro_op: &crate::rewrite::ir::MacroOp<Self>,
        here: u64,
        block_addresses: &[u64],
        container: Option<&Container>,
        current_section: Option<crate::container::SectionId>,
        bytes: &mut Vec<u8>,
        relocations: &mut Vec<MacroEmittedRelocation>,
    ) -> Result<(), MacroEmitError<Self::EncodeError>> {
        let _ = (block_addresses, container, current_section, relocations);
        // Default: re-emit each original instruction verbatim.
        let mut current = here;
        for original in &macro_op.original_instructions {
            // Operand re-projection: drop symbolic Branch/Page
            // operands by substituting placeholder addresses.
            let operands: Vec<Self::Operand> = original
                .operands
                .iter()
                .map(|o| match o {
                    crate::rewrite::ir::RewriteOperand::Decoded(d) => d.clone(),
                    // For symbolic operands in fallback macro
                    // emit there's no good answer; a default
                    // ISA wouldn't have them anyway.
                    _ => return_first_decoded_or_clone::<Self>(&original.operands),
                })
                .collect();
            let out = Self::encode(original.mnemonic, &operands, current)
                .map_err(MacroEmitError::Encode)?;
            current = current.wrapping_add(out.bytes.len() as u64);
            bytes.extend(out.bytes);
        }
        Ok(())
    }
}

fn return_first_decoded_or_clone<I: Isa>(
    operands: &[crate::rewrite::ir::RewriteOperand<I>],
) -> I::Operand {
    operands
        .iter()
        .find_map(|o| match o {
            crate::rewrite::ir::RewriteOperand::Decoded(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("default emit_macro saw a fully-symbolic operand list with no Decoded")
        })
}

/// Relocation a macro wants the emit pass to record.
#[derive(Debug, Clone)]
pub struct MacroEmittedRelocation {
    pub offset: u64,
    pub kind: RelocationKind,
    pub symbol: crate::container::SymbolId,
    pub addend: i64,
}

/// Error from [`Isa::emit_macro`].
#[derive(Debug)]
pub enum MacroEmitError<E> {
    Encode(E),
    /// Macro is malformed (wrong number of original
    /// instructions, etc.).
    Malformed,
    /// Macro's target requires a relocation kind this ISA
    /// hasn't mapped.
    NoRelocationForMnemonic,
}
