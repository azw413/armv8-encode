//! Architecture-neutral machine-code model.
//!
//! This layer holds the abstractions analysis and rewriting are written
//! against — control-flow classification, basic blocks, the control-flow
//! graph — independently of any particular ISA. ISA crates implement
//! [`InstructionInfo`] on their decoded-instruction types and everything
//! above this layer stays generic.

pub mod cfg;
pub mod control_flow;

pub use cfg::{
    build as build_cfg, BasicBlock, BasicBlockId, ControlFlowGraph, Edge, EdgeKind, EdgeTarget,
};
pub use control_flow::{ControlFlow, InstructionInfo};

/// Stable identifier for a decoded instruction within a function.
///
/// Currently a thin index over the source instruction slice; will gain more
/// structure once instructions, symbols, and relocations get a richer model.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct InstructionId(pub usize);

/// A relocatable address expression.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Address {
    Absolute(u64),
    Symbol { name: String, addend: i64 },
}
