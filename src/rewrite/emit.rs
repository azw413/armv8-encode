//! Encoding of a laid-out [`RewritePlan`] back into AArch64 bytes.
//!
//! The emit pass turns symbolic targets back into numeric addresses.
//! Layout has already done all the placement work, so emit's job is
//! mechanical:
//!
//! - For each instruction at the [`EmitStrategy::Normal`] strategy whose
//!   target is fully resolvable, lower its branch/page operands against the
//!   block-address table from [`Layout`] and call the existing
//!   `encode_instruction`.
//! - For each instruction at [`EmitStrategy::InvertedConditional`], emit
//!   the two-instruction widened sequence: `<inverted> +8 ; b far_target`.
//! - For instructions whose target is `Target::Symbol(undefined)`, emit a
//!   placeholder word with displacement 0 and append an
//!   [`EmittedRelocation`]. The linker will fill in the real displacement
//!   when the symbol is resolved.
//!
//! `emit` returns an [`EmitOutput`] containing both the bytes and the
//! relocations. For symbol-free plans the relocations vec is empty.

use crate::container::{Container, RelocationKind, SymbolId};
use crate::isa::aarch64::{
    encode_instruction, invert_conditional_branch, Aarch64Mnemonic, DecodedOperand, EncodeError,
    InstructionTemplate, Register,
};
use crate::rewrite::ir::{MacroKind, MacroOp, RewriteInstruction, RewriteOp, RewriteOperand, Target};
use crate::rewrite::layout::{EmitStrategy, Layout, LayoutError};
use crate::rewrite::plan::RewritePlan;

/// A fix-up the rewriter wants the linker to apply.
///
/// Produced when an instruction references a `Target::Symbol(id)` whose
/// container symbol is undefined (an extern import). The instruction's
/// encoded word carries displacement 0 / page 0; the linker overwrites the
/// relocated field at link time.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EmittedRelocation {
    /// Byte offset within the emitted byte stream where the fix-up applies.
    pub offset: u64,
    pub kind: RelocationKind,
    pub symbol: SymbolId,
    pub addend: i64,
}

/// Bytes produced by the emit pass plus any relocations the linker needs.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct EmitOutput {
    pub bytes: Vec<u8>,
    pub relocations: Vec<EmittedRelocation>,
}

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
    /// An instruction targets an undefined symbol but the rewriter doesn't
    /// know which relocation kind to emit for the mnemonic. Add the
    /// mnemonic to `relocation_kind_for_mnemonic`.
    NoRelocationForMnemonic { mnemonic: Aarch64Mnemonic },
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

/// Emit `plan` according to `layout`. Returns the byte stream that should
/// land at `layout.base_address` plus any relocations the rewriter needs
/// the linker to apply.
///
/// `container` is consulted to resolve `Target::Symbol` operands — defined
/// symbols fold to their container address; undefined symbols produce a
/// placeholder + relocation. Pass `None` only for plans that don't
/// reference container symbols.
pub fn emit(
    plan: &RewritePlan,
    layout: &Layout,
    container: Option<&Container>,
) -> Result<EmitOutput, EmitError> {
    let mut output = EmitOutput {
        bytes: Vec::with_capacity(layout.total_size as usize),
        relocations: Vec::new(),
    };

    for (block_index, block) in plan.blocks.iter().enumerate() {
        for (op_index, op) in block.ops.iter().enumerate() {
            let instr_layout = layout.instruction_layouts[block_index][op_index];
            match op {
                RewriteOp::Instruction(instruction) => {
                    emit_instruction(
                        instruction,
                        instr_layout,
                        &layout.block_addresses,
                        container,
                        &mut output,
                    )?;
                }
                RewriteOp::Macro(macro_op) => {
                    emit_macro(
                        macro_op,
                        instr_layout.address,
                        &layout.block_addresses,
                        container,
                        &mut output,
                    )?;
                }
            }
        }
    }

    Ok(output)
}

fn emit_instruction(
    instruction: &RewriteInstruction,
    instr_layout: crate::rewrite::layout::InstructionLayout,
    block_addresses: &[u64],
    container: Option<&Container>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    if let Some((symbol_id, page)) = needs_relocation(instruction, container) {
        return emit_with_relocation(instruction, instr_layout.address, symbol_id, page, output);
    }

    match instr_layout.strategy {
        EmitStrategy::Normal => {
            let template = build_template(
                instruction,
                instr_layout.address,
                block_addresses,
                container,
            )?;
            let word = encode_instruction(&template)?;
            output.bytes.extend_from_slice(&word.to_le_bytes());
        }
        EmitStrategy::InvertedConditional => {
            emit_widened_conditional(
                instruction,
                instr_layout.address,
                block_addresses,
                container,
                &mut output.bytes,
            )?;
        }
    }
    Ok(())
}

/// Emit a fused macro by expanding it back into its component
/// instructions with the symbolic target rebound. For
/// [`MacroKind::LoadAddress`] this means producing an `adrp` and an
/// `add`, with the address of `target` split into a page-relative high
/// half and a 12-bit low offset. Undefined-symbol targets emit two
/// relocations (AdrpPage21 + PageOffset12).
fn emit_macro(
    macro_op: &MacroOp,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    match macro_op.kind {
        MacroKind::LoadAddress => {
            emit_load_address(macro_op, here, block_addresses, container, output)
        }
    }
}

fn emit_load_address(
    macro_op: &MacroOp,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    let target_is_undefined = match (macro_op.target, container) {
        (Target::Symbol(id), Some(container)) => {
            container.address_of_symbol(id).is_none()
        }
        _ => false,
    };

    if target_is_undefined {
        // Placeholder bytes: adrp Rd, here ; add Rd, Rd, #0. Offsets get
        // overwritten by the linker via the two emitted relocations.
        let symbol_id = match macro_op.target {
            Target::Symbol(id) => id,
            _ => unreachable!("target_is_undefined implies Symbol"),
        };

        let adrp_offset = output.bytes.len() as u64;
        let adrp_template = build_adrp_template(&macro_op.register, here & !0xfff, here);
        let adrp_word = encode_instruction(&adrp_template)?;
        output.bytes.extend_from_slice(&adrp_word.to_le_bytes());
        output.relocations.push(EmittedRelocation {
            offset: adrp_offset,
            kind: RelocationKind::AdrpPage21,
            symbol: symbol_id,
            addend: 0,
        });

        let add_offset = output.bytes.len() as u64;
        let add_template = build_add_immediate_template(&macro_op.register, 0);
        let add_word = encode_instruction(&add_template)?;
        output.bytes.extend_from_slice(&add_word.to_le_bytes());
        output.relocations.push(EmittedRelocation {
            offset: add_offset,
            kind: RelocationKind::PageOffset12,
            symbol: symbol_id,
            addend: 0,
        });

        return Ok(());
    }

    // Resolved target: split into page + offset. The page-target operand
    // of adrp expects the *page address* of the destination; the add
    // immediate is the in-page offset.
    let target_address =
        crate::rewrite::layout::resolve_target(macro_op.target, block_addresses, container)?;
    let target_page = target_address & !0xfff;
    let page_offset = (target_address & 0xfff) as i64;

    let adrp_template = build_adrp_template(&macro_op.register, target_page, here);
    let adrp_word = encode_instruction(&adrp_template)?;
    output.bytes.extend_from_slice(&adrp_word.to_le_bytes());

    let add_template = build_add_immediate_template(&macro_op.register, page_offset);
    let add_word = encode_instruction(&add_template)?;
    output.bytes.extend_from_slice(&add_word.to_le_bytes());

    Ok(())
}

fn build_adrp_template(rd: &Register, target_page: u64, address: u64) -> InstructionTemplate {
    InstructionTemplate {
        address,
        mnemonic: Aarch64Mnemonic::Adrp,
        operands: vec![
            DecodedOperand::Register(rd.clone()),
            DecodedOperand::PageTarget(target_page),
        ],
    }
}

fn build_add_immediate_template(rd: &Register, immediate: i64) -> InstructionTemplate {
    InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Add,
        operands: vec![
            DecodedOperand::Register(rd.clone()),
            DecodedOperand::Register(rd.clone()),
            DecodedOperand::Immediate(immediate),
        ],
    }
}

/// Inspect an instruction's PC-relative target. Returns `Some((symbol_id,
/// is_page))` when the target is a `Target::Symbol(id)` that the supplied
/// container considers undefined.
fn needs_relocation(
    instruction: &RewriteInstruction,
    container: Option<&Container>,
) -> Option<(SymbolId, bool)> {
    let container = container?;
    for operand in &instruction.operands {
        let (target, is_page) = match operand {
            RewriteOperand::Branch(target) => (*target, false),
            RewriteOperand::Page(target) => (*target, true),
            _ => continue,
        };
        if let Target::Symbol(id) = target {
            if container.address_of_symbol(id).is_none() {
                return Some((id, is_page));
            }
        }
    }
    None
}

fn emit_with_relocation(
    instruction: &RewriteInstruction,
    here: u64,
    symbol: SymbolId,
    is_page: bool,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    let kind = relocation_kind_for_mnemonic(instruction.mnemonic, is_page).ok_or(
        EmitError::NoRelocationForMnemonic {
            mnemonic: instruction.mnemonic,
        },
    )?;

    // Encode with the operand pointing back at `here`, which produces a
    // zero displacement / page-offset. The linker overwrites this field
    // when it applies the relocation.
    let mut operands = Vec::with_capacity(instruction.operands.len());
    for operand in &instruction.operands {
        operands.push(match operand {
            RewriteOperand::Decoded(decoded) => decoded.clone(),
            RewriteOperand::Branch(_) => DecodedOperand::BranchTarget(here),
            RewriteOperand::Page(_) => DecodedOperand::PageTarget(here & !0xfff),
        });
    }

    let template = InstructionTemplate {
        address: here,
        mnemonic: instruction.mnemonic,
        operands,
    };
    let word = encode_instruction(&template)?;
    let offset = output.bytes.len() as u64;
    output.bytes.extend_from_slice(&word.to_le_bytes());
    output.relocations.push(EmittedRelocation {
        offset,
        kind,
        symbol,
        addend: 0,
    });
    Ok(())
}

/// Map a mnemonic to the relocation kind the linker needs in order to
/// patch its PC-relative operand. Returns `None` for mnemonics whose
/// operand layout the rewriter doesn't yet know how to relocate.
pub(crate) fn relocation_kind_for_mnemonic(
    mnemonic: Aarch64Mnemonic,
    page_operand: bool,
) -> Option<RelocationKind> {
    if page_operand {
        return match mnemonic {
            Aarch64Mnemonic::Adrp => Some(RelocationKind::AdrpPage21),
            _ => None,
        };
    }
    match mnemonic {
        Aarch64Mnemonic::B | Aarch64Mnemonic::Bl => Some(RelocationKind::Branch26),
        Aarch64Mnemonic::Beq
        | Aarch64Mnemonic::Bne
        | Aarch64Mnemonic::Bcs
        | Aarch64Mnemonic::Bcc
        | Aarch64Mnemonic::Bmi
        | Aarch64Mnemonic::Bpl
        | Aarch64Mnemonic::Bvs
        | Aarch64Mnemonic::Bvc
        | Aarch64Mnemonic::Bhi
        | Aarch64Mnemonic::Bls
        | Aarch64Mnemonic::Bge
        | Aarch64Mnemonic::Blt
        | Aarch64Mnemonic::Bgt
        | Aarch64Mnemonic::Ble
        | Aarch64Mnemonic::Cbz
        | Aarch64Mnemonic::Cbnz => Some(RelocationKind::Branch19),
        Aarch64Mnemonic::Tbz | Aarch64Mnemonic::Tbnz => Some(RelocationKind::Branch14),
        _ => None,
    }
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
