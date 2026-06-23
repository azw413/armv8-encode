//! Emit a laid-out [`RewritePlan`] back into bytes.
//!
//! Generic over [`Isa`] — encoding, widening sequences, and
//! macro emission all delegate to the trait.

use crate::container::{Container, RelocationKind, SectionId, SectionKind, SymbolId};
use crate::isa::{Isa, MacroEmitError, MacroEmittedRelocation, PcRelKind};
use crate::rewrite::ir::{
    MacroOp, RewriteInstruction, RewriteOp, RewriteOperand, Target,
};
use crate::rewrite::layout::{resolve_target, EmitStrategy, Layout, LayoutError};
use crate::rewrite::plan::RewritePlan;

/// A fix-up the rewriter wants the linker to apply.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EmittedRelocation {
    pub offset: u64,
    pub kind: RelocationKind,
    pub symbol: SymbolId,
    pub addend: i64,
}

impl From<MacroEmittedRelocation> for EmittedRelocation {
    fn from(r: MacroEmittedRelocation) -> Self {
        Self {
            offset: r.offset,
            kind: r.kind,
            symbol: r.symbol,
            addend: r.addend,
        }
    }
}

/// Bytes produced by the emit pass plus any relocations the
/// linker needs.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct EmitOutput {
    pub bytes: Vec<u8>,
    pub relocations: Vec<EmittedRelocation>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EmitError {
    Encode(String),
    Layout(LayoutError),
    /// Widening was requested for a mnemonic that has no
    /// inversion — should not happen for well-formed plans.
    InvalidWidening,
    /// Widened sequence had no branch target operand.
    MissingWideningTarget,
    /// Instruction targets an undefined symbol but its
    /// mnemonic / kind has no relocation mapping.
    NoRelocationForMnemonic,
    /// Macro op malformed (wrong number of original
    /// instructions, etc.).
    MalformedMacro,
}

impl<E: std::fmt::Debug> From<MacroEmitError<E>> for EmitError {
    fn from(e: MacroEmitError<E>) -> Self {
        match e {
            MacroEmitError::Encode(inner) => EmitError::Encode(format!("{inner:?}")),
            MacroEmitError::Malformed => EmitError::MalformedMacro,
            MacroEmitError::NoRelocationForMnemonic => EmitError::NoRelocationForMnemonic,
        }
    }
}

impl From<LayoutError> for EmitError {
    fn from(error: LayoutError) -> Self {
        EmitError::Layout(error)
    }
}

/// Emit `plan` according to `layout`. Returns the byte stream
/// that should land at `layout.base_address` plus any
/// relocations the rewriter needs the linker to apply.
pub fn emit<I: Isa>(
    plan: &RewritePlan<I>,
    layout: &Layout,
    container: Option<&Container>,
) -> Result<EmitOutput, EmitError> {
    let mut output = EmitOutput {
        bytes: Vec::with_capacity(layout.total_size as usize),
        relocations: Vec::new(),
    };

    let current_section = container.and_then(|container| {
        container
            .sections
            .iter()
            .find(|section| section.size > 0 && section.address == layout.base_address)
            .map(|section| section.id)
    });

    for (block_index, block) in plan.blocks.iter().enumerate() {
        for (op_index, op) in block.ops.iter().enumerate() {
            let instr_layout = layout.instruction_layouts[block_index][op_index];
            match op {
                RewriteOp::Instruction(instruction) => {
                    emit_instruction::<I>(
                        instruction,
                        instr_layout,
                        &layout.block_addresses,
                        container,
                        current_section,
                        &mut output,
                    )?;
                }
                RewriteOp::Macro(macro_op) => {
                    emit_macro::<I>(
                        macro_op,
                        instr_layout.address,
                        &layout.block_addresses,
                        container,
                        current_section,
                        &mut output,
                    )?;
                }
                // Pre-encoded position-independent bytes: copied verbatim, no
                // relocations (see `RewriteOp::Raw`).
                RewriteOp::Raw(bytes) => output.bytes.extend_from_slice(bytes),
            }
        }
    }

    Ok(output)
}

fn emit_instruction<I: Isa>(
    instruction: &RewriteInstruction<I>,
    instr_layout: crate::rewrite::layout::InstructionLayout,
    block_addresses: &[u64],
    container: Option<&Container>,
    current_section: Option<SectionId>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    let reloc = needs_relocation::<I>(instruction, container, current_section);

    // An unmodified instruction re-emitted in its own section at its original
    // address is copied verbatim from the source bytes rather than re-encoded.
    // Re-encoding from operands is lossy (extended-register addressing, some
    // FP/SIMD forms, alias encodings), and an unchanged PC-relative reference
    // (e.g. an `adrp` into another section) must not be turned into a relocation
    // the final image has no linker to apply. In a *relocatable* object, though,
    // a reference that needs a relocation must keep it (the object gets
    // relinked), so only skip verbatim for that case.
    let is_relocatable = container.is_some_and(|c| {
        matches!(c.kind, crate::container::ContainerKind::Relocatable)
    });
    if !(is_relocatable && reloc.is_some()) {
        if let Some(raw) =
            verbatim_bytes::<I>(instruction, instr_layout.address, block_addresses, container)
        {
            output.bytes.extend_from_slice(&raw);
            return Ok(());
        }
    }

    if let Some((symbol_id, kind)) = reloc {
        return emit_with_relocation::<I>(instruction, instr_layout.address, symbol_id, kind, output);
    }

    match instr_layout.strategy {
        EmitStrategy::Normal => {
            let operands = lower_operands::<I>(instruction, block_addresses, container)?;
            let out = I::encode(instruction.mnemonic, &operands, instr_layout.address)
                .map_err(|e| EmitError::Encode(format!("{e:?}")))?;
            output.bytes.extend_from_slice(&out.bytes);
        }
        EmitStrategy::InvertedConditional => {
            // Find the original branch target and resolve it.
            let target = instruction
                .pc_relative_target()
                .ok_or(EmitError::MissingWideningTarget)?;
            let target_address =
                crate::rewrite::layout::resolve_target(target, block_addresses, container)?;
            // Build a placeholder operand list — the trait's
            // widening encoder will substitute the inner
            // branch's target.
            let operands_template = lower_operands_with_placeholder::<I>(instruction);
            let out = I::encode_widened_conditional(
                instruction.mnemonic,
                &operands_template,
                instr_layout.address,
                target_address,
            )
            .map_err(|e| EmitError::Encode(format!("{e:?}")))?;
            output.bytes.extend_from_slice(&out.bytes);
        }
    }
    Ok(())
}

/// If `instruction` is unmodified — its IR matches the original bytes at
/// `original_address`, which lie in the section currently being re-emitted —
/// return those original bytes for the emitter to copy verbatim. Returns `None`
/// when the instruction was changed, originates elsewhere, or can't be placed at
/// `emit_addr` without invalidating a PC-relative field; the caller then
/// re-encodes from operands.
fn verbatim_bytes<I: Isa>(
    instruction: &RewriteInstruction<I>,
    emit_addr: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Option<Vec<u8>> {
    let orig_addr = instruction.original_address?;
    let container = container?;
    // Source bytes must come from an executable section — guards against a
    // synthesized instruction whose (bogus, often low) original address happens
    // to resolve into some data section. Code copied from another text address
    // (e.g. a relocated function body) is fine: the decode + operand check below
    // confirms the bytes really are this instruction.
    let sid = container.section_for_address(orig_addr)?;
    let section = container.section(sid);
    if section.kind != SectionKind::Text {
        return None;
    }
    let off = usize::try_from(orig_addr.checked_sub(section.address)?).ok()?;
    let tail = section.bytes.get(off..)?;
    let decoded = I::decode(orig_addr, tail)?;

    // Unmodified: same mnemonic and every operand still matches the original.
    if I::decoded_mnemonic(&decoded) != instruction.mnemonic {
        return None;
    }
    let decoded_ops = I::decoded_operands(&decoded);
    if decoded_ops.len() != instruction.operands.len() {
        return None;
    }
    for (d, r) in decoded_ops.iter().zip(&instruction.operands) {
        let matches = match r {
            // A plain operand must be byte-for-byte the same as decoded.
            RewriteOperand::Decoded(o) => o == d,
            // A symbolic branch/page is unmodified iff it still points where the
            // original instruction did — the lift symbolized it from this very
            // PC-relative target. (A pass that retargeted it, e.g. a trampoline
            // `b stub`, resolves elsewhere and falls through to re-encode.)
            RewriteOperand::Branch(t) => matches!(I::pcrel_kind(d), Some((PcRelKind::Branch, abs))
                if resolve_target(*t, block_addresses, Some(container)).ok() == Some(abs)),
            RewriteOperand::Page(t) => matches!(I::pcrel_kind(d), Some((PcRelKind::Page, abs))
                if resolve_target(*t, block_addresses, Some(container)).ok() == Some(abs)),
        };
        if !matches {
            return None;
        }
    }

    // Same address: the original encoding (PC-relative or not) is still valid.
    // Relocated: only position-independent instructions are safe to copy as-is.
    if orig_addr != emit_addr && decoded_ops.iter().any(|o| I::pcrel_kind(o).is_some()) {
        return None;
    }

    let size = I::decoded_size(&decoded) as usize;
    tail.get(..size).map(<[u8]>::to_vec)
}

fn emit_macro<I: Isa>(
    macro_op: &MacroOp<I>,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    current_section: Option<SectionId>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    let mut macro_relocations = Vec::new();
    let bytes_offset = output.bytes.len() as u64;
    I::emit_macro(
        macro_op,
        here,
        block_addresses,
        container,
        current_section,
        &mut output.bytes,
        &mut macro_relocations,
    )?;
    // Adjust relocation offsets to be absolute within the
    // output stream (the trait impl recorded them relative to
    // its bytes view, which matches output.bytes since we
    // share the buffer).
    for rel in macro_relocations {
        output.relocations.push(EmittedRelocation::from(rel));
    }
    let _ = bytes_offset; // silence warning if unused.
    Ok(())
}

fn needs_relocation<I: Isa>(
    instruction: &RewriteInstruction<I>,
    container: Option<&Container>,
    current_section: Option<SectionId>,
) -> Option<(SymbolId, PcRelKind)> {
    let container = container?;
    for operand in &instruction.operands {
        let (target, kind) = match operand {
            RewriteOperand::Branch(target) => (*target, PcRelKind::Branch),
            RewriteOperand::Page(target) => (*target, PcRelKind::Page),
            _ => continue,
        };
        if let Target::Symbol(id) = target {
            if symbol_needs_relocation(container, id, current_section) {
                return Some((id, kind));
            }
        }
    }
    None
}

/// True when emit must produce a relocation for a
/// `Target::Symbol(id)` rather than fold the symbol's address
/// into a displacement.
pub(crate) fn symbol_needs_relocation(
    container: &Container,
    id: SymbolId,
    current_section: Option<SectionId>,
) -> bool {
    use crate::container::SymbolKind;
    let symbol = container.symbol(id);
    if symbol.is_undefined {
        let has_plt_stub = container
            .elf_image
            .as_ref()
            .map(|img| img.plt_stubs.contains_key(&id))
            .unwrap_or(false);
        return !has_plt_stub;
    }
    // Appended code (e.g. via `add_function_from_plan`) has no owning section:
    // it's being folded into a final, fully-linked image with no subsequent
    // linker, so every defined reference — including section-relative ones like
    // an `adrp` into `.rodata` — must be folded to its address, not emitted as a
    // relocation that nothing will apply.
    let Some(current) = current_section else {
        return false;
    };
    if symbol.kind == SymbolKind::Section {
        return true;
    }
    let Some(symbol_section) = symbol.section else {
        return false;
    };
    if symbol_section == current {
        return false;
    }
    let current_section_address = container
        .sections
        .iter()
        .find(|s| s.id == current)
        .map(|s| s.address)
        .unwrap_or(0);
    current_section_address == 0
}

fn emit_with_relocation<I: Isa>(
    instruction: &RewriteInstruction<I>,
    here: u64,
    symbol: SymbolId,
    kind: PcRelKind,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    let reloc_kind = I::relocation_kind_for(instruction.mnemonic, kind)
        .ok_or(EmitError::NoRelocationForMnemonic)?;

    // Encode with the operand pointing back at `here` so the
    // displacement is zero. Linker fills in the real value.
    let operands: Vec<I::Operand> = instruction
        .operands
        .iter()
        .map(|operand| match operand {
            RewriteOperand::Decoded(d) => d.clone(),
            RewriteOperand::Branch(_) => I::make_pcrel_operand(PcRelKind::Branch, here),
            RewriteOperand::Page(_) => {
                I::make_pcrel_operand(PcRelKind::Page, here & !0xfff)
            }
        })
        .collect();

    let out = I::encode(instruction.mnemonic, &operands, here)
        .map_err(|e| EmitError::Encode(format!("{e:?}")))?;
    let offset = output.bytes.len() as u64;
    output.bytes.extend_from_slice(&out.bytes);
    output.relocations.push(EmittedRelocation {
        offset,
        kind: reloc_kind,
        symbol,
        addend: 0,
    });
    Ok(())
}

fn lower_operands<I: Isa>(
    instruction: &RewriteInstruction<I>,
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Result<Vec<I::Operand>, EmitError> {
    let mut out = Vec::with_capacity(instruction.operands.len());
    for operand in &instruction.operands {
        out.push(lower_operand::<I>(operand, block_addresses, container)?);
    }
    Ok(out)
}

fn lower_operand<I: Isa>(
    operand: &RewriteOperand<I>,
    block_addresses: &[u64],
    container: Option<&Container>,
) -> Result<I::Operand, EmitError> {
    match operand {
        RewriteOperand::Decoded(d) => Ok(d.clone()),
        RewriteOperand::Branch(target) => {
            let address =
                crate::rewrite::layout::resolve_target(*target, block_addresses, container)?;
            Ok(I::make_pcrel_operand(PcRelKind::Branch, address))
        }
        RewriteOperand::Page(target) => {
            let address =
                crate::rewrite::layout::resolve_target(*target, block_addresses, container)?;
            Ok(I::make_pcrel_operand(PcRelKind::Page, address))
        }
    }
}

fn lower_operands_with_placeholder<I: Isa>(
    instruction: &RewriteInstruction<I>,
) -> Vec<I::Operand> {
    instruction
        .operands
        .iter()
        .filter_map(|o| match o {
            RewriteOperand::Decoded(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}
