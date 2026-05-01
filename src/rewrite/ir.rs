//! Rewrite-IR types.
//!
//! At the rewrite layer, branch operands are *symbolic*: they reference a
//! block, a symbol, or a constant rather than a hard-coded address. This is
//! what lets the layout pass move things around freely — addresses aren't
//! recomputed until lowering, when each label gets resolved against the
//! chosen layout.
//!
//! See the module-level docs in `rewrite::mod` for the full lift → edit →
//! lay-out → emit flow.

use crate::isa::aarch64::{Aarch64Mnemonic, DecodedOperand};
use crate::mc::BasicBlockId;

/// Identifier for an extern symbol — function entry, global, GOT slot.
///
/// Re-exported from the container layer so a single ID space spans
/// "what the binary file says" and "what the rewriter is editing."
pub use crate::container::SymbolId;

/// Identifier for a constant — literal-pool entry, jump-table base, …
///
/// Like `SymbolId`, defined now so the surface is stable; resolution
/// (literal-pool layout, etc.) lands later.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ConstantId(pub usize);

/// Where an address-shaped operand points after lifting.
///
/// `Block` is the common case for code already inside the rewrite plan.
/// `Absolute` is the "the source binary had a literal address here" case
/// — rare in well-formed code, but real for stripped binaries and hand-rolled
/// boot stubs.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Target {
    Block(BasicBlockId),
    Symbol(SymbolId),
    Constant(ConstantId),
    Absolute(u64),
}

/// An operand at the rewrite-IR level.
///
/// Most operands pass through unchanged via `Decoded`. Only PC-relative ones
/// get the symbolic treatment, since they're the ones whose meaning depends
/// on layout.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RewriteOperand {
    /// Pass-through. Must not contain `BranchTarget` or `PageTarget` —
    /// those are lifted to `Branch` / `Page`.
    Decoded(DecodedOperand),
    /// PC-relative branch target.
    Branch(Target),
    /// Page-relative target (e.g. `adrp`).
    Page(Target),
}

/// A single instruction at the rewrite-IR level.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RewriteInstruction {
    pub mnemonic: Aarch64Mnemonic,
    pub operands: Vec<RewriteOperand>,
    /// Address this instruction had in the source binary, if any. Diagnostic
    /// only; useful for round-tripping unmodified regions and for error
    /// messages that point at the original source.
    pub original_address: Option<u64>,
}

/// A basic block at the rewrite-IR level.
///
/// Blocks here track the `BasicBlockId` from the analysis CFG so callers can
/// cross-reference between the two. Internal control flow within a block
/// stays implicit: instruction order is the source-order from the CFG, and
/// the terminator is the last instruction.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RewriteBlock {
    pub id: BasicBlockId,
    pub instructions: Vec<RewriteInstruction>,
}

impl RewriteInstruction {
    /// True when this instruction has at least one symbolic branch / page
    /// target. Used by the layout pass to decide whether displacement
    /// checks are needed.
    pub fn has_pc_relative_operand(&self) -> bool {
        self.operands.iter().any(|operand| {
            matches!(
                operand,
                RewriteOperand::Branch(_) | RewriteOperand::Page(_)
            )
        })
    }

    /// First branch / page target on this instruction, if any. AArch64
    /// branches have at most one PC-relative operand, so this is enough for
    /// range checks and target resolution.
    pub fn pc_relative_target(&self) -> Option<Target> {
        self.operands.iter().find_map(|operand| match operand {
            RewriteOperand::Branch(t) | RewriteOperand::Page(t) => Some(*t),
            _ => None,
        })
    }
}
