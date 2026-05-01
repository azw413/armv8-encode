//! Encoding of a laid-out [`RewritePlan`] back into AArch64 bytes.
//!
//! The emit pass is the only stage that turns symbolic targets back into
//! numeric addresses. Layout has already done all the placement work, so
//! emit's job is mechanical:
//!
//! - For each instruction at the [`EmitStrategy::Normal`] strategy, resolve
//!   its branch/page operands against the block-address table from
//!   [`Layout`] and call the existing `encode_instruction`.
//! - For each instruction at [`EmitStrategy::InvertedConditional`], emit the
//!   two-instruction widened sequence: `<inverted> +8 ; b far_target`.

use crate::container::Container;
use crate::isa::aarch64::{
    encode_instruction, invert_conditional_branch, Aarch64Mnemonic, DecodedOperand, EncodeError,
    InstructionTemplate,
};
use crate::rewrite::ir::{RewriteInstruction, RewriteOperand, Target};
use crate::rewrite::layout::{EmitStrategy, Layout, LayoutError};
use crate::rewrite::plan::RewritePlan;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EmitError {
    Encode(EncodeError),
    Layout(LayoutError),
    /// Widening was requested but the mnemonic isn't conditional. Indicates
    /// a layout/emit invariant has been broken — should not happen for
    /// well-formed plans.
    InvalidWidening { mnemonic: Aarch64Mnemonic },
    /// Widened sequence has no branch target operand. Same invariant
    /// violation as above.
    MissingWideningTarget,
}

impl From<EncodeError> for EmitError {
    fn from(error: EncodeError) -> Self {
        EmitError::Encode(error)
    }
}

impl From<LayoutError> for EmitError {
    fn from(error: LayoutError) -> Self {
        EmitError::Layout(error)
    }
}

/// Emit `plan` according to `layout`, returning the byte stream that should
/// land at `layout.base_address`.
///
/// `container` is consulted to resolve `Target::Symbol` operands; it must
/// be the same container that was passed to `lay_out`. Pass `None` for
/// symbol-free plans.
pub fn emit(
    plan: &RewritePlan,
    layout: &Layout,
    container: Option<&Container>,
) -> Result<Vec<u8>, EmitError> {
    let mut bytes = Vec::with_capacity(layout.total_size as usize);

    for (block_index, block) in plan.blocks.iter().enumerate() {
        for (instr_index, instruction) in block.instructions.iter().enumerate() {
            let instr_layout = layout.instruction_layouts[block_index][instr_index];
            match instr_layout.strategy {
                EmitStrategy::Normal => {
                    let template = build_template(
                        instruction,
                        instr_layout.address,
                        &layout.block_addresses,
                        container,
                    )?;
                    let word = encode_instruction(&template)?;
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
                EmitStrategy::InvertedConditional => {
                    emit_widened_conditional(
                        instruction,
                        instr_layout.address,
                        &layout.block_addresses,
                        container,
                        &mut bytes,
                    )?;
                }
            }
        }
    }

    Ok(bytes)
}

fn build_template(
    instruction: &RewriteInstruction,
    address: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Result<InstructionTemplate, EmitError> {
    let mut operands = Vec::with_capacity(instruction.operands.len());
    for operand in &instruction.operands {
        operands.push(lower_operand(operand, block_addresses, container)?);
    }
    Ok(InstructionTemplate {
        address,
        mnemonic: instruction.mnemonic,
        operands,
    })
}

fn lower_operand(
    operand: &RewriteOperand,
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Result<DecodedOperand, EmitError> {
    match operand {
        RewriteOperand::Decoded(decoded) => Ok(decoded.clone()),
        RewriteOperand::Branch(target) => {
            let address =
                crate::rewrite::layout::resolve_target(*target, block_addresses, container)?;
            Ok(DecodedOperand::BranchTarget(address))
        }
        RewriteOperand::Page(target) => {
            let address =
                crate::rewrite::layout::resolve_target(*target, block_addresses, container)?;
            Ok(DecodedOperand::PageTarget(address))
        }
    }
}

/// Emit the two-instruction widened conditional:
///
/// ```text
/// <inverted_cond> .Lskip   ; address `here`
/// b               far      ; address `here + 4`
/// .Lskip:                  ; address `here + 8`
/// ```
fn emit_widened_conditional(
    instruction: &RewriteInstruction,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    bytes: &mut Vec<u8>,
) -> Result<(), EmitError> {
    let inverted_mnemonic = invert_conditional_branch(instruction.mnemonic).ok_or(
        EmitError::InvalidWidening {
            mnemonic: instruction.mnemonic,
        },
    )?;

    // Build the inverted-condition operand list: same shape as the original,
    // but with the branch target replaced by the post-widening skip address.
    let skip_address = here.wrapping_add(8);
    let mut inverted_operands = Vec::with_capacity(instruction.operands.len());
    let mut original_target: Option<Target> = None;

    for operand in &instruction.operands {
        match operand {
            RewriteOperand::Branch(target) => {
                original_target = Some(*target);
                inverted_operands.push(DecodedOperand::BranchTarget(skip_address));
            }
            RewriteOperand::Page(_) => {
                // Page operands don't appear on conditional branches; if we
                // get here, layout decided to widen something it shouldn't
                // have.
                return Err(EmitError::InvalidWidening {
                    mnemonic: instruction.mnemonic,
                });
            }
            RewriteOperand::Decoded(decoded) => {
                inverted_operands.push(decoded.clone());
            }
        }
    }

    let original_target = original_target.ok_or(EmitError::MissingWideningTarget)?;

    let inverted_template = InstructionTemplate {
        address: here,
        mnemonic: inverted_mnemonic,
        operands: inverted_operands,
    };
    let inverted_word = encode_instruction(&inverted_template)?;
    bytes.extend_from_slice(&inverted_word.to_le_bytes());

    let far_address =
        crate::rewrite::layout::resolve_target(original_target, block_addresses, container)?;
    let b_template = InstructionTemplate {
        address: here.wrapping_add(4),
        mnemonic: Aarch64Mnemonic::B,
        operands: vec![DecodedOperand::BranchTarget(far_address)],
    };
    let b_word = encode_instruction(&b_template)?;
    bytes.extend_from_slice(&b_word.to_le_bytes());

    Ok(())
}
