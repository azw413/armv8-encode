//! `RewritePlan` — the editable rewrite-IR container.
//!
//! Workflow:
//!
//! 1. [`RewritePlan::lift`] converts a CFG plus its decoded instructions into
//!    a plan whose branch targets are symbolic.
//! 2. Edit operations (`redirect_branch`, `replace_terminator`, `insert_*`)
//!    mutate the plan in place. They never recompute addresses.
//! 3. The layout pass (separate module) decides where each block lands.
//! 4. The emit pass walks the laid-out plan and produces bytes.

use crate::container::Container;
use crate::isa::aarch64::DecodedInstruction;
use crate::isa::aarch64::DecodedOperand;
use crate::mc::{BasicBlockId, ControlFlowGraph};
use crate::rewrite::ir::{RewriteBlock, RewriteInstruction, RewriteOperand, Target};
use std::collections::HashMap;

/// An editable, symbolic representation of a code region.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct RewritePlan {
    pub blocks: Vec<RewriteBlock>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EditError {
    /// No instruction in the plan has the requested original address.
    AddressNotFound(u64),
    /// The instruction at this address has no PC-relative operand to redirect.
    NoBranchOperand(u64),
    /// The block id refers to a block that is not in this plan.
    UnknownBlock(BasicBlockId),
}

impl RewritePlan {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lift a `(CFG, instructions)` pair into a rewrite plan.
    ///
    /// Each `BranchTarget(addr)` / `PageTarget(addr)` operand is matched
    /// against the CFG's block start addresses. Targets that hit a block
    /// become `Target::Block(id)`; targets outside the CFG fall back to
    /// `Target::Absolute(addr)`.
    ///
    /// For container-aware lifting that resolves cross-function targets to
    /// `Target::Symbol`, use [`Self::lift_with_container`].
    pub fn lift(cfg: &ControlFlowGraph, instructions: &[DecodedInstruction]) -> Self {
        Self::lift_inner(cfg, instructions, None)
    }

    /// Like [`Self::lift`], but also consults a [`Container`] to resolve
    /// cross-function targets.
    ///
    /// Resolution order for each PC-relative operand:
    /// 1. Address starts a block in `cfg` → `Target::Block(id)`.
    /// 2. Address matches a defined symbol in `container` → `Target::Symbol(id)`.
    /// 3. Otherwise → `Target::Absolute(addr)`.
    ///
    /// This is the lift to use when editing a function within a known
    /// object file — calls to other functions in the same file (or to
    /// extern imports) survive layout intact.
    pub fn lift_with_container(
        cfg: &ControlFlowGraph,
        instructions: &[DecodedInstruction],
        container: &Container,
    ) -> Self {
        Self::lift_inner(cfg, instructions, Some(container))
    }

    fn lift_inner(
        cfg: &ControlFlowGraph,
        instructions: &[DecodedInstruction],
        container: Option<&Container>,
    ) -> Self {
        let block_at_address: HashMap<u64, BasicBlockId> = cfg
            .blocks
            .iter()
            .map(|block| (block.start, block.id))
            .collect();

        let blocks = cfg
            .blocks
            .iter()
            .map(|block| RewriteBlock {
                id: block.id,
                instructions: instructions[block.instructions.clone()]
                    .iter()
                    .map(|insn| lift_instruction(insn, &block_at_address, container))
                    .collect(),
            })
            .collect();

        Self { blocks }
    }

    /// Find the (block index, instruction index) of the instruction whose
    /// `original_address` matches `address`, if any.
    fn locate(&self, address: u64) -> Option<(usize, usize)> {
        for (block_index, block) in self.blocks.iter().enumerate() {
            for (instr_index, instr) in block.instructions.iter().enumerate() {
                if instr.original_address == Some(address) {
                    return Some((block_index, instr_index));
                }
            }
        }
        None
    }

    pub fn instruction_at(&self, address: u64) -> Option<&RewriteInstruction> {
        self.locate(address)
            .map(|(b, i)| &self.blocks[b].instructions[i])
    }

    pub fn instruction_at_mut(&mut self, address: u64) -> Option<&mut RewriteInstruction> {
        let (b, i) = self.locate(address)?;
        Some(&mut self.blocks[b].instructions[i])
    }

    /// Change the PC-relative target of the instruction at `address`.
    ///
    /// Errors if no instruction has that source address, or if the
    /// instruction has no PC-relative operand to redirect.
    pub fn redirect_branch(
        &mut self,
        address: u64,
        new_target: Target,
    ) -> Result<(), EditError> {
        let instr = self
            .instruction_at_mut(address)
            .ok_or(EditError::AddressNotFound(address))?;
        for operand in &mut instr.operands {
            match operand {
                RewriteOperand::Branch(target) | RewriteOperand::Page(target) => {
                    *target = new_target;
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(EditError::NoBranchOperand(address))
    }

    /// Replace the terminator of `block` with a new instruction. The old
    /// terminator (the last instruction in the block) is dropped.
    pub fn replace_terminator(
        &mut self,
        block: BasicBlockId,
        new_terminator: RewriteInstruction,
    ) -> Result<(), EditError> {
        let block_index = block.0;
        let block = self
            .blocks
            .get_mut(block_index)
            .ok_or(EditError::UnknownBlock(BasicBlockId(block_index)))?;
        if let Some(last) = block.instructions.last_mut() {
            *last = new_terminator;
        } else {
            block.instructions.push(new_terminator);
        }
        Ok(())
    }

    /// Insert one or more instructions immediately after the instruction at
    /// `address`. New instructions get `original_address = None`. The block
    /// containing the anchor receives the new instructions; no block split
    /// is performed.
    pub fn insert_after_address(
        &mut self,
        address: u64,
        new_instructions: Vec<RewriteInstruction>,
    ) -> Result<(), EditError> {
        let (block_index, instr_index) = self
            .locate(address)
            .ok_or(EditError::AddressNotFound(address))?;
        let block = &mut self.blocks[block_index];
        let insert_at = instr_index + 1;
        // Splice the new instructions in. `splice` is the natural call here
        // but `splice(insert_at..insert_at, new_instructions)` is awkward; a
        // direct loop is clearer.
        for (offset, instruction) in new_instructions.into_iter().enumerate() {
            block.instructions.insert(insert_at + offset, instruction);
        }
        Ok(())
    }

    /// Remove the instruction at `address`, returning it.
    pub fn remove_at_address(
        &mut self,
        address: u64,
    ) -> Result<RewriteInstruction, EditError> {
        let (block_index, instr_index) = self
            .locate(address)
            .ok_or(EditError::AddressNotFound(address))?;
        Ok(self.blocks[block_index].instructions.remove(instr_index))
    }
}

fn lift_instruction(
    insn: &DecodedInstruction,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
) -> RewriteInstruction {
    let operands = insn
        .operands
        .iter()
        .map(|operand| match operand {
            DecodedOperand::BranchTarget(addr) => {
                RewriteOperand::Branch(resolve_address(*addr, block_at_address, container))
            }
            DecodedOperand::PageTarget(addr) => {
                RewriteOperand::Page(resolve_address(*addr, block_at_address, container))
            }
            other => RewriteOperand::Decoded(other.clone()),
        })
        .collect();
    RewriteInstruction {
        mnemonic: insn.mnemonic,
        operands,
        original_address: Some(insn.address),
    }
}

/// Resolve a numeric address to a symbolic target. Block matches in the
/// CFG win over symbol matches in the container — local control flow stays
/// local even if the linker happened to put a symbol there.
fn resolve_address(
    address: u64,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
) -> Target {
    if let Some(&id) = block_at_address.get(&address) {
        return Target::Block(id);
    }
    if let Some(container) = container {
        if let Some(symbol) = container.symbol_at_address(address) {
            return Target::Symbol(symbol.id);
        }
    }
    Target::Absolute(address)
}
