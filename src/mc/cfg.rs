//! Basic-block discovery and control-flow graph construction.
//!
//! Architecture-neutral: takes any slice of `T: InstructionInfo` (in source
//! order) and returns a [`ControlFlowGraph`]. ISA-specific knowledge is
//! confined to each instruction's `control_flow()` value, which the trait
//! implementor produced.
//!
//! ## Algorithm
//!
//! Standard textbook two-pass:
//!
//! 1. Identify *leaders*: the first instruction; the target of any direct
//!    branch; the instruction immediately after any terminator. Targets that
//!    fall outside the input slice don't become leaders here, but the edge
//!    that referenced them is recorded as [`EdgeTarget::External`].
//! 2. Walk leaders in address order. Each block runs from one leader up to
//!    and including the next terminator (or to the next leader, whichever
//!    comes first), then we look at that terminator's [`ControlFlow`] and
//!    emit successor edges.
//!
//! ## Limitations of this pass
//!
//! - Indirect branches (`br xN`, `blr xN`) emit a single [`EdgeTarget::Indirect`]
//!   successor. Resolving them needs jump-table or symbol context that lives
//!   above this layer.
//! - Traps (`svc`, `brk`, `hlt`, …) are conservatively given a fallthrough
//!   successor on the assumption that most syscalls return. Real handling is
//!   OS-specific and can be tightened once we have ABI metadata.
//! - The input is assumed to be a single contiguous instruction stream. A
//!   future recursive-descent variant will accept multiple roots and skip
//!   non-instruction holes (literal pools).

use crate::mc::{ControlFlow, InstructionInfo};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

/// Stable identifier for a basic block within a [`ControlFlowGraph`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BasicBlockId(pub usize);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BasicBlock {
    pub id: BasicBlockId,
    /// Address of the first instruction (inclusive).
    pub start: u64,
    /// Address one past the last instruction (exclusive).
    pub end: u64,
    /// Range of indices into the source instruction slice. `instructions.end`
    /// is one past the last index, matching `Range` conventions.
    pub instructions: Range<usize>,
    /// The terminator's control-flow effect, or `None` if the block runs off
    /// the end of the analyzed region without one.
    pub terminator: Option<ControlFlow>,
    /// Outgoing edges. Empty for returns and for blocks that fall off the
    /// end of the input.
    pub successors: Vec<Edge>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Edge {
    pub kind: EdgeKind,
    pub target: EdgeTarget,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum EdgeKind {
    /// Linear continuation past a non-jumping terminator (conditional
    /// not-taken, return-from-call, post-trap).
    Fallthrough,
    /// Unconditional jump taken.
    Jump,
    /// Conditional branch taken.
    BranchTaken,
    /// Direct or indirect call to a function entry. The call site's
    /// fallthrough is recorded as a separate `Fallthrough` edge.
    Call,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum EdgeTarget {
    /// Address resolves to a block in this CFG.
    Block(BasicBlockId),
    /// Address points outside the analyzed instruction stream — typically an
    /// extern function, another section, or beyond the slice end.
    External(u64),
    /// Computed at runtime (`br xN`, `blr xN`); destination unknown without
    /// further context.
    Indirect,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn block(&self, id: BasicBlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }

    /// First block by address order, if any. The builder always emits blocks
    /// in address order, so this is `BasicBlockId(0)` when non-empty.
    pub fn entry(&self) -> Option<BasicBlockId> {
        if self.blocks.is_empty() {
            None
        } else {
            Some(BasicBlockId(0))
        }
    }

    /// Find the block that starts at `address`, if any.
    pub fn block_at(&self, address: u64) -> Option<BasicBlockId> {
        self.blocks
            .iter()
            .find(|block| block.start == address)
            .map(|block| block.id)
    }

    /// Predecessors of `block`, derived by scanning every block's
    /// successors. O(|blocks| * |edges|); fine for small graphs, worth
    /// caching for larger ones.
    pub fn predecessors_of(&self, block: BasicBlockId) -> Vec<BasicBlockId> {
        let mut out = Vec::new();
        for predecessor in &self.blocks {
            for edge in &predecessor.successors {
                if edge.target == EdgeTarget::Block(block) {
                    out.push(predecessor.id);
                    break;
                }
            }
        }
        out
    }
}

/// Build a [`ControlFlowGraph`] from an in-order slice of decoded
/// instructions. The slice must be sorted by address with no gaps and no
/// overlap — the natural output of a linear sweep.
pub fn build<T: InstructionInfo>(instructions: &[T]) -> ControlFlowGraph {
    if instructions.is_empty() {
        return ControlFlowGraph::default();
    }

    let address_to_index: HashMap<u64, usize> = instructions
        .iter()
        .enumerate()
        .map(|(index, instruction)| (instruction.address(), index))
        .collect();

    let leader_indices = collect_leaders(instructions, &address_to_index);
    let mut blocks = form_blocks(instructions, &leader_indices);
    let block_at_address: HashMap<u64, BasicBlockId> = blocks
        .iter()
        .map(|block| (block.start, block.id))
        .collect();
    attach_successors(&mut blocks, &block_at_address);

    ControlFlowGraph { blocks }
}

fn collect_leaders<T: InstructionInfo>(
    instructions: &[T],
    address_to_index: &HashMap<u64, usize>,
) -> BTreeSet<usize> {
    let mut leaders = BTreeSet::new();
    leaders.insert(0);

    for (index, instruction) in instructions.iter().enumerate() {
        let cf = instruction.control_flow();
        if !cf.is_terminator() {
            continue;
        }

        // The instruction immediately after a terminator starts a new block.
        if index + 1 < instructions.len() {
            leaders.insert(index + 1);
        }

        // A direct branch target inside the slice is a leader.
        if let Some(target) = cf.direct_target() {
            if let Some(&target_index) = address_to_index.get(&target) {
                leaders.insert(target_index);
            }
        }
    }

    leaders
}

fn form_blocks<T: InstructionInfo>(
    instructions: &[T],
    leader_indices: &BTreeSet<usize>,
) -> Vec<BasicBlock> {
    let mut leaders = leader_indices.iter().copied().peekable();
    let mut blocks = Vec::new();

    while let Some(start_index) = leaders.next() {
        let next_leader = leaders.peek().copied().unwrap_or(instructions.len());

        // Walk from start_index toward next_leader. The block ends either at
        // the first terminator we encounter (inclusive) or at next_leader.
        let mut end_index = next_leader;
        let mut terminator = None;
        for index in start_index..next_leader {
            let cf = instructions[index].control_flow();
            if cf.is_terminator() {
                end_index = index + 1;
                terminator = Some(cf);
                break;
            }
        }

        let start_address = instructions[start_index].address();
        let last = &instructions[end_index - 1];
        let end_address = last.address() + last.size();

        blocks.push(BasicBlock {
            id: BasicBlockId(blocks.len()),
            start: start_address,
            end: end_address,
            instructions: start_index..end_index,
            terminator,
            successors: Vec::new(),
        });
    }

    blocks
}

fn attach_successors(
    blocks: &mut [BasicBlock],
    block_at_address: &HashMap<u64, BasicBlockId>,
) {
    for block in blocks.iter_mut() {
        let Some(terminator) = block.terminator else {
            continue;
        };

        match terminator {
            ControlFlow::Fall => {
                // `Fall` is not a terminator; `is_terminator` excludes it.
                unreachable!("Fall reached terminator branch");
            }
            ControlFlow::Jump { target } => {
                block.successors.push(Edge {
                    kind: EdgeKind::Jump,
                    target: resolve(target, block_at_address),
                });
            }
            ControlFlow::ConditionalJump {
                target,
                fallthrough,
            } => {
                block.successors.push(Edge {
                    kind: EdgeKind::BranchTaken,
                    target: resolve(target, block_at_address),
                });
                block.successors.push(Edge {
                    kind: EdgeKind::Fallthrough,
                    target: resolve(fallthrough, block_at_address),
                });
            }
            ControlFlow::Call {
                target,
                fallthrough,
            } => {
                block.successors.push(Edge {
                    kind: EdgeKind::Call,
                    target: resolve(target, block_at_address),
                });
                block.successors.push(Edge {
                    kind: EdgeKind::Fallthrough,
                    target: resolve(fallthrough, block_at_address),
                });
            }
            ControlFlow::Return => {
                // No successors — destination is in the link register.
            }
            ControlFlow::IndirectJump => {
                block.successors.push(Edge {
                    kind: EdgeKind::Jump,
                    target: EdgeTarget::Indirect,
                });
            }
            ControlFlow::IndirectCall { fallthrough } => {
                block.successors.push(Edge {
                    kind: EdgeKind::Call,
                    target: EdgeTarget::Indirect,
                });
                block.successors.push(Edge {
                    kind: EdgeKind::Fallthrough,
                    target: resolve(fallthrough, block_at_address),
                });
            }
            ControlFlow::Trap => {
                // Conservative: assume the handler returns. OS-specific
                // analysis can later prune this edge for `_exit`-shaped
                // syscalls.
                block.successors.push(Edge {
                    kind: EdgeKind::Fallthrough,
                    target: resolve(block.end, block_at_address),
                });
            }
        }
    }
}

fn resolve(address: u64, block_at_address: &HashMap<u64, BasicBlockId>) -> EdgeTarget {
    match block_at_address.get(&address) {
        Some(&id) => EdgeTarget::Block(id),
        None => EdgeTarget::External(address),
    }
}
