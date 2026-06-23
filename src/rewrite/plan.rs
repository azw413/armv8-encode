//! `RewritePlan` — the editable rewrite-IR container.
//!
//! Generic over an [`Isa`] implementation so the same plan
//! type works for AArch64, ARMv7-Thumb, and ARMv7-ARM.
//!
//! Workflow:
//!
//! 1. [`RewritePlan::lift`] / [`RewritePlan::lift_with_container`]
//!    converts a CFG plus its decoded instructions into a plan
//!    whose branch targets are symbolic. The ISA's
//!    [`Isa::fuse_macros`] hook recognises common
//!    multi-instruction idioms and represents them as
//!    [`MacroOp`] entries.
//! 2. Edit operations (`redirect_branch`, `redirect_macro_target`,
//!    `replace_terminator`, `insert_*`) mutate the plan in place.
//! 3. The layout pass decides where each block lands.
//! 4. The emit pass walks the laid-out plan and produces bytes.

use crate::container::Container;
use crate::isa::{FusionRelocationInfo, Isa, PcRelKind};
use crate::mc::{BasicBlockId, ControlFlowGraph};
use crate::rewrite::ir::{
    RewriteBlock, RewriteInstruction, RewriteOp, RewriteOperand, Target,
};
use std::collections::HashMap;

/// An editable, symbolic representation of a code region.
pub struct RewritePlan<I: Isa> {
    pub blocks: Vec<RewriteBlock<I>>,
}

impl<I: Isa> Default for RewritePlan<I> {
    fn default() -> Self {
        Self { blocks: Vec::new() }
    }
}

impl<I: Isa> Clone for RewritePlan<I> {
    fn clone(&self) -> Self {
        Self { blocks: self.blocks.clone() }
    }
}

impl<I: Isa> PartialEq for RewritePlan<I> {
    fn eq(&self, other: &Self) -> bool {
        self.blocks == other.blocks
    }
}

impl<I: Isa> Eq for RewritePlan<I> {}

impl<I: Isa> std::fmt::Debug for RewritePlan<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RewritePlan").field("blocks", &self.blocks).finish()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EditError {
    AddressNotFound(u64),
    NoBranchOperand(u64),
    NotAMacro(u64),
    NotAnInstruction(u64),
    UnknownBlock(BasicBlockId),
}

/// Per-instruction view into a decoded slice. The plan layer
/// is generic over how decoded instructions are represented;
/// callers feed in `(address, mnemonic, operands)` triples.
pub struct DecodedRef<'a, I: Isa> {
    pub address: u64,
    pub mnemonic: I::Mnemonic,
    pub operands: &'a [I::Operand],
    /// Encoded byte length of the source instruction. Threaded into
    /// the lifted [`RewriteInstruction::source_size`] so variable-
    /// width ISAs lay out at their true footprint.
    pub size: u64,
}

impl<I: Isa> RewritePlan<I> {
    /// Convenience: lift directly from a slice of per-ISA
    /// `DecodedInstruction`s without the caller having to
    /// build a [`DecodedRef`] vec. Works for any ISA via the
    /// [`Isa::decoded_address`] / [`Isa::decoded_mnemonic`] /
    /// [`Isa::decoded_operands`] accessors.
    pub fn lift_from_decoded(
        cfg: &ControlFlowGraph,
        instructions: &[I::DecodedInstruction],
    ) -> Self {
        let refs: Vec<DecodedRef<I>> = instructions
            .iter()
            .map(|i| DecodedRef {
                address: I::decoded_address(i),
                mnemonic: I::decoded_mnemonic(i),
                operands: I::decoded_operands(i),
                size: I::decoded_size(i),
            })
            .collect();
        Self::lift_inner(cfg, &refs, None)
    }

    /// Like [`Self::lift_from_decoded`] but threads a
    /// [`Container`] for symbol resolution during lift.
    pub fn lift_from_decoded_with_container(
        cfg: &ControlFlowGraph,
        instructions: &[I::DecodedInstruction],
        container: &Container,
    ) -> Self {
        let refs: Vec<DecodedRef<I>> = instructions
            .iter()
            .map(|i| DecodedRef {
                address: I::decoded_address(i),
                mnemonic: I::decoded_mnemonic(i),
                operands: I::decoded_operands(i),
                size: I::decoded_size(i),
            })
            .collect();
        Self::lift_inner(cfg, &refs, Some(container))
    }
}

impl RewritePlan<crate::isa::aarch64::Aarch64Isa> {
    /// AArch64 convenience alias for
    /// [`Self::lift_from_decoded`]. Preserved so callers
    /// migrating from the pre-generic API don't have to
    /// rename.
    pub fn lift(
        cfg: &ControlFlowGraph,
        instructions: &[crate::isa::aarch64::DecodedInstruction],
    ) -> Self {
        Self::lift_from_decoded(cfg, instructions)
    }

    /// AArch64 convenience alias for
    /// [`Self::lift_from_decoded_with_container`].
    pub fn lift_with_container(
        cfg: &ControlFlowGraph,
        instructions: &[crate::isa::aarch64::DecodedInstruction],
        container: &Container,
    ) -> Self {
        Self::lift_from_decoded_with_container(cfg, instructions, container)
    }
}

impl<I: Isa> RewritePlan<I> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a single-block plan from a flat list of
    /// [`RewriteInstruction`]s, running the ISA's macro-fusion
    /// pass.
    pub fn from_instructions(
        instructions: Vec<RewriteInstruction<I>>,
        container: Option<&Container>,
    ) -> Self {
        let block_at_address = HashMap::new();
        let relocations = HashMap::new();
        let ops = I::fuse_macros(instructions, &block_at_address, container, &relocations);
        Self {
            blocks: vec![RewriteBlock {
                id: BasicBlockId(0),
                ops,
            }],
        }
    }

    /// Lift a `(CFG, decoded-refs)` pair into a rewrite plan.
    /// The generic form takes [`DecodedRef`] slices —
    /// per-ISA helpers (e.g. AArch64's [`Self::lift`]) provide
    /// nicer wrappers that take the ISA's native
    /// `DecodedInstruction` type directly.
    pub fn lift_refs<'a>(
        cfg: &ControlFlowGraph,
        instructions: &'a [DecodedRef<'a, I>],
    ) -> Self {
        Self::lift_inner(cfg, instructions, None)
    }

    /// Generic form of [`Self::lift_with_container`] taking
    /// [`DecodedRef`] slices directly.
    pub fn lift_refs_with_container<'a>(
        cfg: &ControlFlowGraph,
        instructions: &'a [DecodedRef<'a, I>],
        container: &Container,
    ) -> Self {
        Self::lift_inner(cfg, instructions, Some(container))
    }

    fn lift_inner<'a>(
        cfg: &ControlFlowGraph,
        instructions: &'a [DecodedRef<'a, I>],
        container: Option<&Container>,
    ) -> Self {
        let block_at_address: HashMap<u64, BasicBlockId> = cfg
            .blocks
            .iter()
            .map(|block| (block.start, block.id))
            .collect();

        let pc_relative_relocations = container
            .map(build_pc_relative_relocation_lookup::<I>)
            .unwrap_or_default();

        let blocks = cfg
            .blocks
            .iter()
            .map(|block| {
                let lifted: Vec<RewriteInstruction<I>> = instructions
                    [block.instructions.clone()]
                .iter()
                .map(|insn| {
                    lift_instruction::<I>(
                        insn,
                        &block_at_address,
                        container,
                        &pc_relative_relocations,
                    )
                })
                .collect();
                let ops = I::fuse_macros(
                    lifted,
                    &block_at_address,
                    container,
                    &pc_relative_relocations,
                );
                RewriteBlock { id: block.id, ops }
            })
            .collect();

        Self { blocks }
    }

    fn locate(&self, address: u64) -> Option<(usize, usize)> {
        for (block_index, block) in self.blocks.iter().enumerate() {
            for (op_index, op) in block.ops.iter().enumerate() {
                if op.matches_source_address(address) {
                    return Some((block_index, op_index));
                }
            }
        }
        None
    }

    pub fn op_at(&self, address: u64) -> Option<&RewriteOp<I>> {
        self.locate(address).map(|(b, i)| &self.blocks[b].ops[i])
    }

    pub fn op_at_mut(&mut self, address: u64) -> Option<&mut RewriteOp<I>> {
        let (b, i) = self.locate(address)?;
        Some(&mut self.blocks[b].ops[i])
    }

    pub fn instruction_at(&self, address: u64) -> Option<&RewriteInstruction<I>> {
        match self.op_at(address)? {
            RewriteOp::Instruction(insn) => Some(insn),
            RewriteOp::Macro(_) | RewriteOp::Raw(_) => None,
        }
    }

    pub fn instruction_at_mut(&mut self, address: u64) -> Option<&mut RewriteInstruction<I>> {
        match self.op_at_mut(address)? {
            RewriteOp::Instruction(insn) => Some(insn),
            RewriteOp::Macro(_) | RewriteOp::Raw(_) => None,
        }
    }

    pub fn redirect_branch(&mut self, address: u64, new_target: Target) -> Result<(), EditError> {
        let op = self
            .op_at_mut(address)
            .ok_or(EditError::AddressNotFound(address))?;
        let instr = match op {
            RewriteOp::Instruction(instr) => instr,
            RewriteOp::Macro(_) | RewriteOp::Raw(_) => {
                return Err(EditError::NotAnInstruction(address))
            }
        };
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

    pub fn redirect_macro_target(
        &mut self,
        address: u64,
        new_target: Target,
    ) -> Result<(), EditError> {
        let op = self
            .op_at_mut(address)
            .ok_or(EditError::AddressNotFound(address))?;
        match op {
            RewriteOp::Macro(macro_op) => {
                macro_op.target = new_target;
                Ok(())
            }
            RewriteOp::Instruction(_) | RewriteOp::Raw(_) => Err(EditError::NotAMacro(address)),
        }
    }

    pub fn replace_terminator(
        &mut self,
        block: BasicBlockId,
        new_terminator: RewriteInstruction<I>,
    ) -> Result<(), EditError> {
        let block_index = block.0;
        let block = self
            .blocks
            .get_mut(block_index)
            .ok_or(EditError::UnknownBlock(BasicBlockId(block_index)))?;
        let new = RewriteOp::Instruction(new_terminator);
        if let Some(last) = block.ops.last_mut() {
            *last = new;
        } else {
            block.ops.push(new);
        }
        Ok(())
    }

    pub fn insert_after_address(
        &mut self,
        address: u64,
        new_instructions: Vec<RewriteInstruction<I>>,
    ) -> Result<(), EditError> {
        let (block_index, op_index) = self
            .locate(address)
            .ok_or(EditError::AddressNotFound(address))?;
        let block = &mut self.blocks[block_index];
        let insert_at = op_index + 1;
        for (offset, instruction) in new_instructions.into_iter().enumerate() {
            block
                .ops
                .insert(insert_at + offset, RewriteOp::Instruction(instruction));
        }
        Ok(())
    }

    pub fn remove_at_address(&mut self, address: u64) -> Result<RewriteOp<I>, EditError> {
        let (block_index, op_index) = self
            .locate(address)
            .ok_or(EditError::AddressNotFound(address))?;
        Ok(self.blocks[block_index].ops.remove(op_index))
    }
}

fn lift_instruction<'a, I: Isa>(
    insn: &DecodedRef<'a, I>,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
    pc_relative_relocations: &HashMap<u64, FusionRelocationInfo>,
) -> RewriteInstruction<I> {
    // If a relocation applies to this instruction and targets
    // a PC-relative operand, prefer it over whatever
    // displacement the decoded operand carries.
    let reloc_target = pc_relative_relocations
        .get(&insn.address)
        .filter(|r| I::is_pc_relative_relocation(r.kind))
        .and_then(|r| r.symbol.map(Target::Symbol));

    let operands = insn
        .operands
        .iter()
        .map(|operand| match I::pcrel_kind(operand) {
            Some((PcRelKind::Branch, addr)) => {
                let target = reloc_target.unwrap_or_else(|| {
                    resolve_address(addr, block_at_address, container)
                });
                RewriteOperand::Branch(target)
            }
            Some((PcRelKind::Page, addr)) => {
                let target = reloc_target.unwrap_or_else(|| {
                    resolve_address(addr, block_at_address, container)
                });
                RewriteOperand::Page(target)
            }
            None => RewriteOperand::Decoded(operand.clone()),
        })
        .collect();
    RewriteInstruction {
        mnemonic: insn.mnemonic,
        operands,
        original_address: Some(insn.address),
        source_size: Some(insn.size),
    }
}

fn build_pc_relative_relocation_lookup<I: Isa>(
    container: &Container,
) -> HashMap<u64, FusionRelocationInfo> {
    let mut map = HashMap::new();
    for relocation in &container.relocations {
        if !I::is_lift_relevant_relocation(relocation.kind) {
            continue;
        }
        let section = container.section(relocation.section);
        let address = section.address + relocation.offset;
        let symbol = relocation.symbol;
        map.entry(address)
            .or_insert(FusionRelocationInfo {
                kind: relocation.kind,
                symbol,
            });
    }
    map
}

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
