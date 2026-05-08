//! Address assignment and branch-range fix-up.
//!
//! Given a [`RewritePlan`], `lay_out` walks the blocks in order, assigns
//! each instruction a final address starting from a caller-supplied base,
//! checks every PC-relative branch for displacement-range overflow, widens
//! out-of-range conditionals into a `<inverted> .Lskip ; b far ; .Lskip:`
//! sequence, and iterates to a fixed point.
//!
//! Long unconditional jumps (>128 MiB) currently error rather than emitting
//! a branch island; islands need a literal-pool model that the constant /
//! symbol layer hasn't grown yet.

use crate::container::Container;
use crate::isa::aarch64::{
    invert_conditional_branch, pcrel_range_bytes, Aarch64Mnemonic,
};
use crate::rewrite::ir::{RewriteInstruction, RewriteOp, Target};
use crate::rewrite::plan::RewritePlan;

const MAX_LAYOUT_ITERATIONS: usize = 16;

/// Layout decision for a single instruction at emit time.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum EmitStrategy {
    /// Encode as a single 4-byte instruction.
    Normal,
    /// Widen a conditional branch into:
    ///
    /// ```text
    /// <inverted_cond> .Lskip   ; 4 bytes
    /// b               far_target  ; 4 bytes
    /// .Lskip:
    /// ```
    ///
    /// Total: 8 bytes.
    InvertedConditional,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InstructionLayout {
    /// Final address.
    pub address: u64,
    /// Encoded byte size — 4 for `Normal`, 8 for `InvertedConditional`.
    pub size: u64,
    pub strategy: EmitStrategy,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Layout {
    pub base_address: u64,
    pub total_size: u64,
    /// Final start address per block, indexed by `BasicBlockId.0`.
    pub block_addresses: Vec<u64>,
    /// Per-block per-instruction layout: `[block][instruction]`.
    pub instruction_layouts: Vec<Vec<InstructionLayout>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LayoutError {
    /// PC-relative displacement is out of range and the instruction can't
    /// be widened (or has already been widened and still doesn't fit).
    DisplacementTooLarge {
        instruction_address: u64,
        target_address: u64,
        displacement: i64,
    },
    /// Branch target is a `Constant` — no resolver yet. Lands with the
    /// literal-pool layer.
    UnresolvableTarget { kind: &'static str },
    /// Branch target is a `Symbol` that the supplied container considers
    /// undefined (extern). Resolving these will require emitting a
    /// relocation record; until then, callers must rewrite the target to
    /// `Absolute` or supply a container in which the symbol is defined.
    UndefinedSymbol { symbol_id: usize },
    /// `Target::Symbol` was used but `lay_out` was called without a
    /// container. Either pass one, or rewrite the target to `Absolute`.
    SymbolWithoutContainer,
    /// Widening hasn't reached a stable layout in
    /// `MAX_LAYOUT_ITERATIONS` passes. Almost certainly a bug in the
    /// widening logic; users should never see this.
    DidNotConverge,
}

/// Lay out `plan` starting at `base_address`. Iterates widening to a fixed
/// point and returns the final layout.
///
/// `container` is consulted to resolve `Target::Symbol` operands. Pass
/// `None` for plans that don't reference container symbols.
pub fn lay_out(
    plan: &RewritePlan,
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

        let grew = widen_out_of_range(
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

/// Assign each block's start address and each instruction's address based on
/// the current `size` field of `instruction_layouts`. Returns block start
/// addresses and the total emitted size.
fn assign_addresses(
    plan: &RewritePlan,
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

/// Walk every PC-relative branch and widen any whose displacement no longer
/// fits its operand's range. Returns `true` if anything was widened (caller
/// must reassign addresses and check again).
fn widen_out_of_range(
    plan: &RewritePlan,
    instruction_layouts: &mut [Vec<InstructionLayout>],
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Result<bool, LayoutError> {
    let mut grew = false;

    for (block_index, block) in plan.blocks.iter().enumerate() {
        for (instr_index, op) in block.ops.iter().enumerate() {
            // Macros (like adrp+add LoadAddress) don't widen — their
            // ranges are huge (±4 GiB) and we don't have a widening
            // strategy for them. Skip range checks here.
            let RewriteOp::Instruction(instr) = op else {
                continue;
            };
            let Some(target) = pc_relative_branch_target(instr) else {
                continue;
            };

            // Undefined-symbol targets carry no displacement constraint at
            // layout time — the linker fills them in later via a
            // relocation that emit will produce. Skip the range check.
            let target_address = match resolve_target(target, block_addresses, container) {
                Ok(addr) => addr,
                Err(LayoutError::UndefinedSymbol { .. }) => continue,
                Err(other) => return Err(other),
            };
            let here = instruction_layouts[block_index][instr_index].address;
            let strategy = instruction_layouts[block_index][instr_index].strategy;
            let displacement = (target_address as i64).wrapping_sub(here as i64);

            let range = effective_range(instr.mnemonic, strategy);
            let Some(range) = range else { continue };

            if displacement >= -range && displacement < range {
                continue;
            }

            // Out of range. Decide what to do.
            match strategy {
                EmitStrategy::Normal => {
                    if invert_conditional_branch(instr.mnemonic).is_some() {
                        // Widen it.
                        instruction_layouts[block_index][instr_index].strategy =
                            EmitStrategy::InvertedConditional;
                        instruction_layouts[block_index][instr_index].size = 8;
                        grew = true;
                    } else {
                        // Unconditional `b` / `bl` already at pcrel26.
                        // No widening implemented yet.
                        return Err(LayoutError::DisplacementTooLarge {
                            instruction_address: here,
                            target_address,
                            displacement,
                        });
                    }
                }
                EmitStrategy::InvertedConditional => {
                    // Even after widening, the embedded `b` is too far.
                    // Would need a branch island / literal pool.
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

/// Effective displacement range for `mnemonic` under `strategy`, or `None`
/// if the operand isn't subject to range checking by the layout pass
/// (e.g. `adrp` Page targets — handled at emit time).
pub(crate) fn effective_range(
    mnemonic: Aarch64Mnemonic,
    strategy: EmitStrategy,
) -> Option<i64> {
    match strategy {
        EmitStrategy::Normal => pcrel_range_bytes(mnemonic),
        // After widening, the inner `b` covers ±128 MiB.
        EmitStrategy::InvertedConditional => Some(128 * 1024 * 1024),
    }
}

/// First PC-relative *branch* target on the instruction. We deliberately
/// skip `Page` operands here: they have their own (much wider) range and
/// don't currently participate in widening.
fn pc_relative_branch_target(instr: &RewriteInstruction) -> Option<Target> {
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
            // `callable_address_of_symbol` returns the defined
            // address when the symbol has one, else the PLT stub
            // address when the input has a stub for this extern.
            // The latter lets new (appended) code call externs
            // like `puts` via the existing `.plt` slot.
            container
                .callable_address_of_symbol(id)
                .ok_or(LayoutError::UndefinedSymbol { symbol_id: id.0 })
        }
        Target::Constant(_) => Err(LayoutError::UnresolvableTarget { kind: "Constant" }),
    }
}
