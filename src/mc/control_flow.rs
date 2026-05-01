//! Architecture-neutral classification of an instruction's effect on the
//! program counter.
//!
//! This is the first piece of the LLVM-style abstraction over per-ISA
//! instruction info: a single `ControlFlow` value collapses what LLVM splits
//! across `isCall`, `isReturn`, `isBranch`, `isUnconditional`, `isIndirect`,
//! and `getBranchTargetOpValue`. Higher layers (basic-block discovery,
//! control-flow graph construction) consume this directly and never need to
//! know which architecture produced it.

/// What an instruction does to the program counter.
///
/// All addresses are absolute; the classifier resolves PC-relative encodings
/// against the instruction's own address.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ControlFlow {
    /// PC advances to the next instruction. The default for arithmetic,
    /// data-movement, and any instruction with no observable effect on
    /// control flow.
    Fall,

    /// PC unconditionally moves to a known absolute target.
    Jump { target: u64 },

    /// PC moves to `target` or to `fallthrough`, depending on a runtime
    /// condition (flag test, register compare, bit test).
    ConditionalJump { target: u64, fallthrough: u64 },

    /// Direct call to a known target. After the callee returns, execution
    /// resumes at `fallthrough`.
    Call { target: u64, fallthrough: u64 },

    /// Function return. The destination is derived at runtime (link register
    /// or stack), so it is not statically known.
    Return,

    /// Unconditional branch through a register. The destination is not
    /// statically known.
    IndirectJump,

    /// Call through a register. The destination is not statically known, but
    /// the return address is `fallthrough`.
    IndirectCall { fallthrough: u64 },

    /// Software interrupt or exception generator (`svc`, `brk`, `hlt`, …).
    /// Whether and where execution resumes is up to the handler — for
    /// analysis purposes this is treated as a barrier.
    Trap,
}

impl ControlFlow {
    /// True when this instruction ends a basic block. A block boundary is
    /// anywhere control may leave the linear instruction stream.
    pub fn is_terminator(self) -> bool {
        !matches!(self, ControlFlow::Fall)
    }

    /// True when execution may continue past this instruction in linear
    /// order. Calls fall through (after the callee returns); plain jumps and
    /// returns do not.
    pub fn has_fallthrough(self) -> bool {
        match self {
            ControlFlow::Fall
            | ControlFlow::ConditionalJump { .. }
            | ControlFlow::Call { .. }
            | ControlFlow::IndirectCall { .. }
            | ControlFlow::Trap => true,
            ControlFlow::Jump { .. }
            | ControlFlow::Return
            | ControlFlow::IndirectJump => false,
        }
    }

    /// Statically known direct branch / call target, if any.
    pub fn direct_target(self) -> Option<u64> {
        match self {
            ControlFlow::Jump { target }
            | ControlFlow::ConditionalJump { target, .. }
            | ControlFlow::Call { target, .. } => Some(target),
            _ => None,
        }
    }
}

/// Architecture-neutral instruction queries. ISA crates implement this on
/// their decoded-instruction type so analysis passes stay generic.
pub trait InstructionInfo {
    /// Absolute address of this instruction.
    fn address(&self) -> u64;

    /// Encoded size in bytes. Fixed at 4 for AArch64; variable for x86.
    fn size(&self) -> u64;

    /// Effect on the program counter.
    fn control_flow(&self) -> ControlFlow;
}
