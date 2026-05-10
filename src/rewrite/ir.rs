//! Rewrite-IR types — generic over an [`Isa`] implementation.
//!
//! At the rewrite layer, branch operands are *symbolic*: they
//! reference a block, a symbol, or a constant rather than a
//! hard-coded address. This is what lets the layout pass move
//! things around freely — addresses aren't recomputed until
//! lowering, when each label gets resolved against the chosen
//! layout.
//!
//! The IR types are parameterised over `I: Isa` so the same
//! pipeline accepts both AArch64 and ARMv7 (Thumb / ARM mode).
//! The `MacroOp` type carries the per-ISA `MacroKind` because
//! fusion patterns differ.

use crate::isa::Isa;
use crate::mc::BasicBlockId;
use std::fmt::{self, Debug};

pub use crate::container::SymbolId;

/// Identifier for a constant — literal-pool entry, jump-table base, …
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ConstantId(pub usize);

/// Where an address-shaped operand points after lifting.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Target {
    Block(BasicBlockId),
    Symbol(SymbolId),
    Constant(ConstantId),
    Absolute(u64),
}

/// An operand at the rewrite-IR level.
pub enum RewriteOperand<I: Isa> {
    Decoded(I::Operand),
    Branch(Target),
    Page(Target),
}

impl<I: Isa> Clone for RewriteOperand<I> {
    fn clone(&self) -> Self {
        match self {
            Self::Decoded(o) => Self::Decoded(o.clone()),
            Self::Branch(t) => Self::Branch(*t),
            Self::Page(t) => Self::Page(*t),
        }
    }
}

impl<I: Isa> PartialEq for RewriteOperand<I> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Decoded(a), Self::Decoded(b)) => a == b,
            (Self::Branch(a), Self::Branch(b)) => a == b,
            (Self::Page(a), Self::Page(b)) => a == b,
            _ => false,
        }
    }
}

impl<I: Isa> Eq for RewriteOperand<I> {}

impl<I: Isa> Debug for RewriteOperand<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decoded(o) => f.debug_tuple("Decoded").field(o).finish(),
            Self::Branch(t) => f.debug_tuple("Branch").field(t).finish(),
            Self::Page(t) => f.debug_tuple("Page").field(t).finish(),
        }
    }
}

/// A single instruction at the rewrite-IR level.
pub struct RewriteInstruction<I: Isa> {
    pub mnemonic: I::Mnemonic,
    pub operands: Vec<RewriteOperand<I>>,
    pub original_address: Option<u64>,
}

impl<I: Isa> Clone for RewriteInstruction<I> {
    fn clone(&self) -> Self {
        Self {
            mnemonic: self.mnemonic,
            operands: self.operands.clone(),
            original_address: self.original_address,
        }
    }
}

impl<I: Isa> PartialEq for RewriteInstruction<I> {
    fn eq(&self, other: &Self) -> bool {
        self.mnemonic == other.mnemonic
            && self.operands == other.operands
            && self.original_address == other.original_address
    }
}

impl<I: Isa> Eq for RewriteInstruction<I> {}

impl<I: Isa> Debug for RewriteInstruction<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RewriteInstruction")
            .field("mnemonic", &self.mnemonic)
            .field("operands", &self.operands)
            .field("original_address", &self.original_address)
            .finish()
    }
}

/// A basic block at the rewrite-IR level.
pub struct RewriteBlock<I: Isa> {
    pub id: BasicBlockId,
    pub ops: Vec<RewriteOp<I>>,
}

impl<I: Isa> Clone for RewriteBlock<I> {
    fn clone(&self) -> Self {
        Self { id: self.id, ops: self.ops.clone() }
    }
}

impl<I: Isa> PartialEq for RewriteBlock<I> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.ops == other.ops
    }
}

impl<I: Isa> Eq for RewriteBlock<I> {}

impl<I: Isa> Debug for RewriteBlock<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RewriteBlock")
            .field("id", &self.id)
            .field("ops", &self.ops)
            .finish()
    }
}

/// One unit of code within a [`RewriteBlock`].
pub enum RewriteOp<I: Isa> {
    Instruction(RewriteInstruction<I>),
    Macro(MacroOp<I>),
}

impl<I: Isa> Clone for RewriteOp<I> {
    fn clone(&self) -> Self {
        match self {
            Self::Instruction(i) => Self::Instruction(i.clone()),
            Self::Macro(m) => Self::Macro(m.clone()),
        }
    }
}

impl<I: Isa> PartialEq for RewriteOp<I> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Instruction(a), Self::Instruction(b)) => a == b,
            (Self::Macro(a), Self::Macro(b)) => a == b,
            _ => false,
        }
    }
}

impl<I: Isa> Eq for RewriteOp<I> {}

impl<I: Isa> Debug for RewriteOp<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instruction(i) => f.debug_tuple("Instruction").field(i).finish(),
            Self::Macro(m) => f.debug_tuple("Macro").field(m).finish(),
        }
    }
}

/// A multi-instruction idiom recognised at lift time.
pub struct MacroOp<I: Isa> {
    pub kind: I::MacroKind,
    pub register: I::Register,
    pub target: Target,
    pub original_instructions: Vec<RewriteInstruction<I>>,
    pub original_addresses: Vec<u64>,
}

impl<I: Isa> Clone for MacroOp<I> {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            register: self.register.clone(),
            target: self.target,
            original_instructions: self.original_instructions.clone(),
            original_addresses: self.original_addresses.clone(),
        }
    }
}

impl<I: Isa> PartialEq for MacroOp<I> {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.register == other.register
            && self.target == other.target
            && self.original_instructions == other.original_instructions
            && self.original_addresses == other.original_addresses
    }
}

impl<I: Isa> Eq for MacroOp<I> {}

impl<I: Isa> Debug for MacroOp<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroOp")
            .field("kind", &self.kind)
            .field("register", &self.register)
            .field("target", &self.target)
            .field("original_instructions", &self.original_instructions)
            .field("original_addresses", &self.original_addresses)
            .finish()
    }
}

impl<I: Isa> RewriteOp<I> {
    pub fn source_byte_size(&self) -> u64 {
        match self {
            Self::Instruction(insn) => I::instruction_source_size(insn.mnemonic),
            Self::Macro(macro_op) => I::macro_source_size(&macro_op.original_instructions),
        }
    }

    pub fn first_original_address(&self) -> Option<u64> {
        match self {
            Self::Instruction(insn) => insn.original_address,
            Self::Macro(macro_op) => macro_op.original_addresses.first().copied(),
        }
    }

    pub fn matches_source_address(&self, address: u64) -> bool {
        match self {
            Self::Instruction(insn) => insn.original_address == Some(address),
            Self::Macro(macro_op) => macro_op.original_addresses.contains(&address),
        }
    }
}

impl<I: Isa> RewriteInstruction<I> {
    pub fn has_pc_relative_operand(&self) -> bool {
        self.operands.iter().any(|operand| {
            matches!(operand, RewriteOperand::Branch(_) | RewriteOperand::Page(_))
        })
    }

    pub fn pc_relative_target(&self) -> Option<Target> {
        self.operands.iter().find_map(|operand| match operand {
            RewriteOperand::Branch(t) | RewriteOperand::Page(t) => Some(*t),
            _ => None,
        })
    }
}
