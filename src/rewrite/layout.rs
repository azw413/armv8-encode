//! Address assignment and branch-range fix-up.
//!
//! Generic over [`Isa`].

use crate::container::Container;
use crate::isa::Isa;
use crate::rewrite::ir::{RewriteInstruction, RewriteOp, Target};
use crate::rewrite::plan::RewritePlan;

const MAX_LAYOUT_ITERATIONS: usize = 16;

/// Layout decision for a single instruction at emit time.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum EmitStrategy {
    /// Encode normally.
    Normal,
    /// Widen a conditional branch into:
    ///
    /// ```text
    /// <inverted_cond> .Lskip
    /// <unconditional>  far_target
    /// .Lskip:
    /// ```
    InvertedConditional,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InstructionLayout {
    pub address: u64,
    pub size: u64,
    pub strategy: EmitStrategy,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Layout {
    pub base_address: u64,
    pub total_size: u64,
    pub block_addresses: Vec<u64>,
    pub instruction_layouts: Vec<Vec<InstructionLayout>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LayoutError {
    DisplacementTooLarge {
        instruction_address: u64,
        target_address: u64,
        displacement: i64,
    },
    UnresolvableTarget { kind: &'static str },
    UndefinedSymbol { symbol_id: usize },
    SymbolWithoutContainer,
    DidNotConverge,
}

/// Lay out `plan` starting at `base_address`.
pub fn lay_out<I: Isa>(
    plan: &RewritePlan<I>,
    base_address: u64,
    container: Option<&Container>,
) -> Result<Layout, LayoutError> {
    let mut instruction_layouts: Vec<Vec<InstructionLayout>> = plan
        .blocks
        .iter()
        .map(|block| {
            block
                .ops
                .iter()
                .map(|op| InstructionLayout {
                    address: 0,
                    size: op.source_byte_size(),
                    strategy: EmitStrategy::Normal,
                })
                .collect()
        })
        .collect();

    for _iteration in 0..MAX_LAYOUT_ITERATIONS {
        let (block_addresses, total_size) =
            assign_addresses(plan, &mut instruction_layouts, base_address);

        let grew = widen_out_of_range::<I>(
            plan,
            &mut instruction_layouts,
            &block_addresses,
            container,
        )?;

        if !grew {
            return Ok(Layout {
                base_address,
                total_size,
                block_addresses,
                instruction_layouts,
            });
        }
    }

    Err(LayoutError::DidNotConverge)
}

fn assign_addresses<I: Isa>(
    plan: &RewritePlan<I>,
    instruction_layouts: &mut [Vec<InstructionLayout>],
    base_address: u64,
) -> (Vec<u64>, u64) {
    let mut current = base_address;
    let mut block_addresses = Vec::with_capacity(plan.blocks.len());

    for (block_index, block) in plan.blocks.iter().enumerate() {
        block_addresses.push(current);
        for op_index in 0..block.ops.len() {
            instruction_layouts[block_index][op_index].address = current;
            current = current.wrapping_add(instruction_layouts[block_index][op_index].size);
        }
    }

    (block_addresses, current.wrapping_sub(base_address))
}

fn widen_out_of_range<I: Isa>(
    plan: &RewritePlan<I>,
    instruction_layouts: &mut [Vec<InstructionLayout>],
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Result<bool, LayoutError> {
    let mut grew = false;

    for (block_index, block) in plan.blocks.iter().enumerate() {
        for (instr_index, op) in block.ops.iter().enumerate() {
            let RewriteOp::Instruction(instr) = op else {
                continue;
            };
            let Some(target) = pc_relative_branch_target(instr) else {
                continue;
            };

            let target_address = match resolve_target(target, block_addresses, container) {
                Ok(addr) => addr,
                Err(LayoutError::UndefinedSymbol { .. }) => continue,
                Err(other) => return Err(other),
            };
            let here = instruction_layouts[block_index][instr_index].address;
            let strategy = instruction_layouts[block_index][instr_index].strategy;
            let displacement = (target_address as i64).wrapping_sub(here as i64);

            let range = effective_range::<I>(instr.mnemonic, strategy);
            let Some(range) = range else { continue };

            if displacement >= -range && displacement < range {
                continue;
            }

            match strategy {
                EmitStrategy::Normal => {
                    if I::invert_conditional_branch(instr.mnemonic).is_some() {
                        instruction_layouts[block_index][instr_index].strategy =
                            EmitStrategy::InvertedConditional;
                        instruction_layouts[block_index][instr_index].size =
                            I::widened_conditional_size();
                        grew = true;
                    } else {
                        return Err(LayoutError::DisplacementTooLarge {
                            instruction_address: here,
                            target_address,
                            displacement,
                        });
                    }
                }
                EmitStrategy::InvertedConditional => {
                    return Err(LayoutError::DisplacementTooLarge {
                        instruction_address: here,
                        target_address,
                        displacement,
                    });
                }
            }
        }
    }

    Ok(grew)
}

pub(crate) fn effective_range<I: Isa>(
    mnemonic: I::Mnemonic,
    strategy: EmitStrategy,
) -> Option<i64> {
    match strategy {
        EmitStrategy::Normal => I::pcrel_range_bytes(mnemonic),
        EmitStrategy::InvertedConditional => Some(I::widened_conditional_range()),
    }
}

fn pc_relative_branch_target<I: Isa>(instr: &RewriteInstruction<I>) -> Option<Target> {
    use crate::rewrite::ir::RewriteOperand;
    instr.operands.iter().find_map(|operand| match operand {
        RewriteOperand::Branch(target) => Some(*target),
        _ => None,
    })
}

pub(crate) fn resolve_target(
    target: Target,
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Result<u64, LayoutError> {
    match target {
        Target::Block(id) => Ok(block_addresses[id.0]),
        Target::Absolute(addr) => Ok(addr),
        Target::Symbol(id) => {
            let container = container.ok_or(LayoutError::SymbolWithoutContainer)?;
            container
                .callable_address_of_symbol(id)
                .ok_or(LayoutError::UndefinedSymbol { symbol_id: id.0 })
        }
        Target::Constant(_) => Err(LayoutError::UnresolvableTarget { kind: "Constant" }),
    }
}
