//! Relocation-aware code rewriting.
//!
//! This layer will build control-flow graphs, split and lay out basic blocks,
//! and emit relocatable code modifications.

use crate::mc::BasicBlock;

/// Placeholder control-flow graph.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self::default()
    }
}
