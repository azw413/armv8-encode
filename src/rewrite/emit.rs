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
    /// A macro op's `original_instructions` doesn't have the expected
    /// shape (e.g. an [`MacroKind::AccessValue`] without exactly two
    /// originals). Indicates a lift-side bug that produced an
    /// inconsistent macro.
    MalformedMacro,
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

    // Identify the section currently being rewritten so emit can
    // decide which `Target::Symbol` references are "intra-section"
    // (safe to fold to a displacement) and which are cross-section
    // (must emit a relocation). We use the layout's base address as
    // the section signature: the rewrite output lands at the same
    // address its source section did, so the section whose
    // `address` matches is the one we're rebuilding.
    let current_section = container.and_then(|container| {
        container
            .sections
            .iter()
            .find(|section| {
                section.size > 0 && section.address == layout.base_address
            })
            .map(|section| section.id)
    });

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
                        current_section,
                        &mut output,
                    )?;
                }
                RewriteOp::Macro(macro_op) => {
                    emit_macro(
                        macro_op,
                        instr_layout.address,
                        &layout.block_addresses,
                        container,
                        current_section,
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
    current_section: Option<crate::container::SectionId>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    if let Some((symbol_id, page)) = needs_relocation(instruction, container, current_section) {
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
    current_section: Option<crate::container::SectionId>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    match macro_op.kind {
        MacroKind::LoadAddress => emit_load_address(
            macro_op,
            here,
            block_addresses,
            container,
            current_section,
            output,
        ),
        MacroKind::AccessValue => {
            emit_access_value(macro_op, here, container, current_section, output)
        }
    }
}

/// Emit an `adrp + ldr/str` pair. The structure mirrors
/// [`emit_load_address`] but the companion instruction's operand layout
/// (memory operand vs. add immediate) differs, and we pass the
/// companion through verbatim from `original_instructions` rather than
/// rebuilding it.
fn emit_access_value(
    macro_op: &MacroOp,
    here: u64,
    container: Option<&Container>,
    current_section: Option<crate::container::SectionId>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    if macro_op.original_instructions.len() != 2 {
        return Err(EmitError::MalformedMacro);
    }
    let original_adrp = &macro_op.original_instructions[0];
    let original_companion = &macro_op.original_instructions[1];

    let target_needs_relocation = match (macro_op.target, container) {
        (Target::Symbol(id), Some(container)) => {
            symbol_needs_relocation(container, id, current_section)
        }
        _ => false,
    };

    if target_needs_relocation {
        let symbol_id = match macro_op.target {
            Target::Symbol(id) => id,
            _ => unreachable!("target_needs_relocation implies Symbol"),
        };

        // adrp Rd, here (placeholder zero page).
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

        // Companion: re-encode the original verbatim. Its memory-operand
        // immediate is whatever the source had (typically zero for a
        // PageOffset12 reloc-bearing instruction); the linker will patch
        // the encoded immediate when it applies the relocation.
        let companion_template = InstructionTemplate {
            address: here + 4,
            mnemonic: original_companion.mnemonic,
            operands: original_companion
                .operands
                .iter()
                .map(decoded_from_rewrite_operand)
                .collect(),
        };
        let companion_offset = output.bytes.len() as u64;
        let companion_word = encode_instruction(&companion_template)?;
        output.bytes.extend_from_slice(&companion_word.to_le_bytes());
        let access_width_bytes = ldst_access_width_bytes(original_companion).ok_or(
            EmitError::NoRelocationForMnemonic {
                mnemonic: original_companion.mnemonic,
            },
        )?;
        output.relocations.push(EmittedRelocation {
            offset: companion_offset,
            kind: RelocationKind::LoadStorePageOffset12 { access_width_bytes },
            symbol: symbol_id,
            addend: 0,
        });

        return Ok(());
    }

    // Resolved target: compute page+offset and emit verbatim adrp +
    // companion. We can't know how to splice the offset into an
    // arbitrary load/store memory operand here, so the foldable case is
    // limited to "target unchanged from source" — re-emit the original
    // word for both halves.
    //
    // This matches the no-op rewrite case for already-linked code that
    // somehow reached the macro path. In practice, an `adrp+ldr/str`
    // macro fused via relocations always wants the relocation path
    // above; the foldable case here is defensive.
    let adrp_template = InstructionTemplate {
        address: here,
        mnemonic: original_adrp.mnemonic,
        operands: original_adrp
            .operands
            .iter()
            .map(decoded_from_rewrite_operand)
            .collect(),
    };
    let adrp_word = encode_instruction(&adrp_template)?;
    output.bytes.extend_from_slice(&adrp_word.to_le_bytes());

    let companion_template = InstructionTemplate {
        address: here + 4,
        mnemonic: original_companion.mnemonic,
        operands: original_companion
            .operands
            .iter()
            .map(decoded_from_rewrite_operand)
            .collect(),
    };
    let companion_word = encode_instruction(&companion_template)?;
    output.bytes.extend_from_slice(&companion_word.to_le_bytes());

    Ok(())
}

/// Infer the access width in bytes for an `ldr`/`str` instruction.
///
/// AArch64's ELF relocation set distinguishes `LDST8/16/32/64/128` and
/// the linker right-shifts the 12-bit page offset by log2(width) before
/// patching. We need that width here so we can emit the right
/// `LoadStorePageOffset12 { access_width_bytes: ... }` variant.
///
/// For plain `Ldr`/`Str` the destination/source register's class is
/// the unambiguous signal: `W` ⇒ 4 bytes, `X` ⇒ 8 bytes. Vector
/// (`B`/`H`/`S`/`D`/`Q`) loads aren't yet covered by macro fusion;
/// we'll widen this when they appear. Returns `None` for shapes the
/// helper can't classify (caller surfaces a clean `EmitError`).
fn ldst_access_width_bytes(instruction: &RewriteInstruction) -> Option<u8> {
    use crate::isa::aarch64::RegisterClass;
    let register = match instruction.operands.first()? {
        RewriteOperand::Decoded(DecodedOperand::Register(reg)) => reg,
        _ => return None,
    };
    match register.class {
        RegisterClass::W => Some(4),
        RegisterClass::X => Some(8),
        _ => None,
    }
}

/// Convert a [`RewriteOperand`] back into a [`DecodedOperand`] for the
/// encoder. `Branch`/`Page` operands carrying symbolic targets fall back
/// to their original placeholder addresses, since this path is only
/// taken for foldable / no-relocation cases where the placeholder is
/// what the encoder needs.
fn decoded_from_rewrite_operand(operand: &RewriteOperand) -> DecodedOperand {
    match operand {
        RewriteOperand::Decoded(d) => d.clone(),
        RewriteOperand::Branch(_) => DecodedOperand::BranchTarget(0),
        RewriteOperand::Page(_) => DecodedOperand::PageTarget(0),
    }
}

fn emit_load_address(
    macro_op: &MacroOp,
    here: u64,
    block_addresses: &[u64],
    container: Option<&Container>,
    current_section: Option<crate::container::SectionId>,
    output: &mut EmitOutput,
) -> Result<(), EmitError> {
    // Macro target needs a relocation under the same rules as for
    // instruction operands (see [`symbol_needs_relocation`]): undefined
    // externs, section symbols, and cross-section references all fail
    // to fold to a stable address.
    let target_needs_relocation = match (macro_op.target, container) {
        (Target::Symbol(id), Some(container)) => {
            symbol_needs_relocation(container, id, current_section)
        }
        _ => false,
    };

    if target_needs_relocation {
        // Placeholder bytes: adrp Rd, here ; add Rd, Rd, #0. Offsets get
        // overwritten by the linker via the two emitted relocations.
        let symbol_id = match macro_op.target {
            Target::Symbol(id) => id,
            _ => unreachable!("target_needs_relocation implies Symbol"),
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
            kind: RelocationKind::AddPageOffset12,
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
/// is_page))` when the target is a `Target::Symbol(id)` whose final
/// address can't be folded into a displacement at rewrite time —
/// undefined externs, and section symbols whose final placement is up
/// to the linker.
fn needs_relocation(
    instruction: &RewriteInstruction,
    container: Option<&Container>,
    current_section: Option<crate::container::SectionId>,
) -> Option<(SymbolId, bool)> {
    let container = container?;
    for operand in &instruction.operands {
        let (target, is_page) = match operand {
            RewriteOperand::Branch(target) => (*target, false),
            RewriteOperand::Page(target) => (*target, true),
            _ => continue,
        };
        if let Target::Symbol(id) = target {
            if symbol_needs_relocation(container, id, current_section) {
                return Some((id, is_page));
            }
        }
    }
    None
}

/// True when emit must produce a relocation for a `Target::Symbol(id)`
/// rather than fold the symbol's address into a displacement.
///
/// In an unlinked `.o`, "the symbol's address" is just its
/// section-internal offset; the linker decides each section's final
/// placement. Folding such an offset into a displacement only stays
/// correct under one condition: both ends of the displacement live in
/// the *same* input section. The linker preserves intra-section
/// distances; cross-section distances are the linker's call.
///
/// Cases this catches:
///   - undefined externs (`is_undefined`): no address yet at all.
///   - section symbols (`SymbolKind::Section`): nominally address 0,
///     real address is the linker's choice.
///   - any defined symbol whose section differs from the section
///     currently being rewritten — this is the "cross-section
///     reference" case caught by the runtime harness on `funcs[]`
///     (an `.data` array referenced from `.text` via an adrp+add
///     pair). Folding the section-internal offset of `funcs` into
///     the rewritten `.text` produces a binary that loads the
///     wrong address at runtime.
pub(crate) fn symbol_needs_relocation(
    container: &Container,
    id: SymbolId,
    current_section: Option<crate::container::SectionId>,
) -> bool {
    use crate::container::SymbolKind;
    let symbol = container.symbol(id);
    if symbol.is_undefined {
        // Externs with a recorded `.plt` stub don't need a
        // relocation: emit folds the call into a `bl <stub>` and
        // the existing stub does the real linker work at first
        // call. Without a stub we surface a relocation as before.
        let has_plt_stub = container
            .elf_image
            .as_ref()
            .map(|img| img.plt_stubs.contains_key(&id))
            .unwrap_or(false);
        return !has_plt_stub;
    }
    if symbol.kind == SymbolKind::Section {
        return true;
    }

    // Cross-section reference handling. Two regimes:
    //
    // 1. Unlinked input (real `.o` from a compiler): every section's
    //    `address` is 0; symbol "addresses" are section-internal
    //    offsets. The linker assigns final placement. Folding a
    //    cross-section displacement at this stage would freeze in the
    //    wrong number — the harness caught this with a `funcs` array
    //    in `.data` referenced from `.text`.
    //
    // 2. Already-linked input or synthesised tests with concrete
    //    addresses: section addresses are non-zero and represent the
    //    final layout. Cross-section displacements are real distances
    //    the rewriter can fold safely.
    //
    // Distinguish by looking at the *current* section's address: 0 ⇒
    // unlinked. We don't apply the rule in regime 2 to preserve
    // synthetic-test behaviour and to allow folding edits against
    // already-linked binaries.
    let Some(current) = current_section else {
        // No section context — caller is operating on raw addresses
        // (synthetic tests, or already-linked input where the layout
        // base doesn't correspond to any section's start). Trust the
        // resolved address: don't insert a spurious relocation.
        return false;
    };
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
