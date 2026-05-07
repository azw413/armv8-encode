//! `RewritePlan` — the editable rewrite-IR container.
//!
//! Workflow:
//!
//! 1. [`RewritePlan::lift`] converts a CFG plus its decoded instructions into
//!    a plan whose branch targets are symbolic. A fusion pass recognises
//!    common multi-instruction idioms (`adrp+add`) and represents them as
//!    [`MacroOp`] entries.
//! 2. Edit operations (`redirect_branch`, `redirect_macro_target`,
//!    `replace_terminator`, `insert_*`) mutate the plan in place. They never
//!    recompute addresses.
//! 3. The layout pass (separate module) decides where each block lands.
//! 4. The emit pass walks the laid-out plan and produces bytes.

use crate::container::{Container, RelocationKind, SymbolId};
use crate::isa::aarch64::{Aarch64Mnemonic, DecodedInstruction, DecodedOperand, Register};
use crate::mc::{BasicBlockId, ControlFlowGraph};
use crate::rewrite::ir::{
    MacroKind, MacroOp, RewriteBlock, RewriteInstruction, RewriteOp, RewriteOperand, Target,
};
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
    /// `redirect_macro_target` was called for an address that resolves to a
    /// non-macro op.
    NotAMacro(u64),
    /// `redirect_branch` / similar was called for an address that resolves
    /// to a macro op rather than a single instruction. Use the macro-
    /// specific edit instead.
    NotAnInstruction(u64),
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

        // Build a lookup from "absolute instruction address" → relocation
        // info. PC-relative AArch64 instructions in unlinked `.o` files
        // carry a placeholder displacement of zero and rely on the
        // accompanying relocation for the real target. If we lift the
        // placeholder zero verbatim, layout will resolve it as a real
        // PC-relative branch (often into the middle of the same
        // function), producing a runnable but completely broken binary.
        // Consult the container so those instructions lift to
        // `Target::Symbol` and the relocation is preserved through emit.
        let pc_relative_relocations =
            container.map(build_pc_relative_relocation_lookup).unwrap_or_default();

        let blocks = cfg
            .blocks
            .iter()
            .map(|block| {
                let lifted: Vec<RewriteInstruction> = instructions[block.instructions.clone()]
                    .iter()
                    .map(|insn| {
                        lift_instruction(
                            insn,
                            &block_at_address,
                            container,
                            &pc_relative_relocations,
                        )
                    })
                    .collect();
                let ops = fuse_macros(
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

    /// Find the (block index, op index) of the op whose source addresses
    /// include `address`, if any.
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

    /// Read access to the op at `address` (instruction or macro).
    pub fn op_at(&self, address: u64) -> Option<&RewriteOp> {
        self.locate(address).map(|(b, i)| &self.blocks[b].ops[i])
    }

    /// Mutable access to the op at `address`.
    pub fn op_at_mut(&mut self, address: u64) -> Option<&mut RewriteOp> {
        let (b, i) = self.locate(address)?;
        Some(&mut self.blocks[b].ops[i])
    }

    /// Convenience: read the singleton instruction at `address`. Returns
    /// `None` when the address resolves to a macro instead — use
    /// [`Self::op_at`] for the polymorphic view.
    pub fn instruction_at(&self, address: u64) -> Option<&RewriteInstruction> {
        match self.op_at(address)? {
            RewriteOp::Instruction(insn) => Some(insn),
            RewriteOp::Macro(_) => None,
        }
    }

    /// Mutable singleton-instruction view at `address`. Returns `None` for
    /// macro ops.
    pub fn instruction_at_mut(&mut self, address: u64) -> Option<&mut RewriteInstruction> {
        match self.op_at_mut(address)? {
            RewriteOp::Instruction(insn) => Some(insn),
            RewriteOp::Macro(_) => None,
        }
    }

    /// Change the PC-relative target of the singleton instruction at
    /// `address`. Errors with `NotAnInstruction` if the address resolves
    /// to a macro — use [`Self::redirect_macro_target`] for macros.
    pub fn redirect_branch(
        &mut self,
        address: u64,
        new_target: Target,
    ) -> Result<(), EditError> {
        let op = self
            .op_at_mut(address)
            .ok_or(EditError::AddressNotFound(address))?;
        let instr = match op {
            RewriteOp::Instruction(insn) => insn,
            RewriteOp::Macro(_) => return Err(EditError::NotAnInstruction(address)),
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

    /// Redirect the symbolic target of the macro op at `address`. Errors
    /// with `NotAMacro` for singleton instructions.
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
            RewriteOp::Instruction(_) => Err(EditError::NotAMacro(address)),
        }
    }

    /// Replace the terminator of `block` with a new instruction. The old
    /// terminator (the last op in the block) is dropped.
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
        let new = RewriteOp::Instruction(new_terminator);
        if let Some(last) = block.ops.last_mut() {
            *last = new;
        } else {
            block.ops.push(new);
        }
        Ok(())
    }

    /// Insert one or more instructions (as singleton ops) immediately after
    /// the op at `address`. New instructions get `original_address = None`.
    /// The block containing the anchor receives the new ops; no block
    /// split is performed.
    pub fn insert_after_address(
        &mut self,
        address: u64,
        new_instructions: Vec<RewriteInstruction>,
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

    /// Remove the op at `address`, returning it.
    pub fn remove_at_address(&mut self, address: u64) -> Result<RewriteOp, EditError> {
        let (block_index, op_index) = self
            .locate(address)
            .ok_or(EditError::AddressNotFound(address))?;
        Ok(self.blocks[block_index].ops.remove(op_index))
    }
}

fn lift_instruction(
    insn: &DecodedInstruction,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
    pc_relative_relocations: &HashMap<u64, InstructionRelocation>,
) -> RewriteInstruction {
    // If a relocation applies to this instruction *and* targets a
    // PC-relative operand, prefer it over whatever displacement the
    // decoded operand carries. The decoded operand's address is computed
    // from the placeholder word (typically zero in unlinked .o input),
    // so blindly trusting it produces a branch into the function's
    // prologue.
    let reloc_target = pc_relative_relocations
        .get(&insn.address)
        .filter(|r| is_pc_relative_operand_kind(r.kind))
        .and_then(|r| r.symbol.map(Target::Symbol));

    let operands = insn
        .operands
        .iter()
        .map(|operand| match operand {
            DecodedOperand::BranchTarget(addr) => {
                let target = reloc_target.unwrap_or_else(|| {
                    resolve_address(*addr, block_at_address, container)
                });
                RewriteOperand::Branch(target)
            }
            DecodedOperand::PageTarget(addr) => {
                let target = reloc_target.unwrap_or_else(|| {
                    resolve_address(*addr, block_at_address, container)
                });
                RewriteOperand::Page(target)
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

/// Per-instruction relocation summary used during lift. Carries enough
/// to substitute symbolic targets where the encoded operand would
/// otherwise carry a placeholder.
#[derive(Debug, Clone)]
struct InstructionRelocation {
    kind: RelocationKind,
    symbol: Option<SymbolId>,
}

/// Build an "instruction address → relocation" map from the container.
///
/// Unlinked AArch64 `.o` files store branch displacements as zero
/// placeholders alongside `R_AARCH64_CALL26` / `R_AARCH64_CONDBR19` /
/// `R_AARCH64_TSTBR14` relocations, and store `adrp` page references as
/// zero alongside `R_AARCH64_ADR_PREL_PG_HI21`. The companion `add`
/// /`ldr`/`str` instructions in an `adrp+...` pair store the page
/// offset as zero alongside `R_AARCH64_ADD_ABS_LO12_NC` (or one of the
/// `LDST*_ABS_LO12_NC` variants). Lift consults this map both for
/// singleton instructions (branches, lone adrps) and during macro
/// fusion (adrp + add → LoadAddress).
fn build_pc_relative_relocation_lookup(
    container: &Container,
) -> HashMap<u64, InstructionRelocation> {
    let mut map = HashMap::new();
    for relocation in &container.relocations {
        if !is_relocation_kind_we_lift(relocation.kind) {
            continue;
        }
        let section = container.section(relocation.section);
        let address = section.address + relocation.offset;
        // First reloc wins on conflicts. The map is keyed by instruction
        // address, and AArch64 has at most one relocation per
        // instruction in well-formed input.
        map.entry(address).or_insert(InstructionRelocation {
            kind: relocation.kind,
            symbol: relocation.symbol,
        });
    }
    map
}

/// Relocations whose target the rewrite layer wants to lift to a
/// symbolic [`Target`]. Branch* + AdrpPage21 substitute directly into
/// PC-relative operands; PageOffset12 has no PC-relative operand on its
/// host instruction but pairs with an adrp to form a macro target.
fn is_relocation_kind_we_lift(kind: RelocationKind) -> bool {
    matches!(
        kind,
        RelocationKind::Branch26
            | RelocationKind::Branch19
            | RelocationKind::Branch14
            | RelocationKind::AdrpPage21
            | RelocationKind::AddPageOffset12
            | RelocationKind::LoadStorePageOffset12 { .. }
    )
}

/// True when this relocation kind targets a PC-relative *operand* on
/// the instruction (so [`lift_instruction`] should swap the operand's
/// resolved address for a [`Target::Symbol`]).
fn is_pc_relative_operand_kind(kind: RelocationKind) -> bool {
    matches!(
        kind,
        RelocationKind::Branch26
            | RelocationKind::Branch19
            | RelocationKind::Branch14
            | RelocationKind::AdrpPage21
    )
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

/// Walk a flat list of singleton instructions and fuse recognized
/// multi-instruction idioms into [`MacroOp`]s. Handles two patterns:
/// - `adrp Rd, page` + `add Rd, Rd, #imm` → [`MacroKind::LoadAddress`].
/// - `adrp Rd, page` + `ldr/str Rt, [Rd, #imm]` → [`MacroKind::AccessValue`].
///
/// Both patterns require strict adjacency. Non-adjacent patterns
/// (compiler-scheduled) need lightweight data-flow tracking and are out
/// of scope here.
fn fuse_macros(
    instructions: Vec<RewriteInstruction>,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
    relocations: &HashMap<u64, InstructionRelocation>,
) -> Vec<RewriteOp> {
    let mut ops = Vec::with_capacity(instructions.len());
    let mut iter = instructions.into_iter().peekable();

    while let Some(current) = iter.next() {
        // Look for `adrp Rd, page` followed by a recognised companion. If
        // we can fuse, consume the next item and emit a Macro; otherwise
        // pass `current` through as a singleton.
        if matches!(current.mnemonic, Aarch64Mnemonic::Adrp) {
            if let Some(next) = iter.peek() {
                if let Some(macro_op) = try_fuse_adrp_add(
                    &current,
                    next,
                    block_at_address,
                    container,
                    relocations,
                ) {
                    iter.next();
                    ops.push(RewriteOp::Macro(macro_op));
                    continue;
                }
                if let Some(macro_op) = try_fuse_adrp_ldst(&current, next, relocations) {
                    iter.next();
                    ops.push(RewriteOp::Macro(macro_op));
                    continue;
                }
            }
        }
        ops.push(RewriteOp::Instruction(current));
    }

    ops
}

/// Recognise `adrp Rd, sym ; ldr/str Rt, [Rd, #:lo12:sym]`. Both
/// instructions must carry container relocations naming the same
/// symbol — without that, we have no way to tell whether the load uses
/// the adrp's result for symbol access vs. some unrelated address
/// computation. The relocation pair is the unambiguous signal.
fn try_fuse_adrp_ldst(
    adrp: &RewriteInstruction,
    companion: &RewriteInstruction,
    relocations: &HashMap<u64, InstructionRelocation>,
) -> Option<MacroOp> {
    if !matches!(
        companion.mnemonic,
        Aarch64Mnemonic::Ldr | Aarch64Mnemonic::Str
    ) {
        return None;
    }

    // adrp shape: [Register(Rd), Page(_)].
    let adrp_rd = match adrp.operands.as_slice() {
        [RewriteOperand::Decoded(DecodedOperand::Register(rd)), RewriteOperand::Page(_)] => rd,
        _ => return None,
    };

    // Companion shape: [Register(Rt), Memory{ base, offset, mode: Offset }].
    // We only care that the memory base register is the adrp's Rd —
    // that's the data-flow link. The Rt destination/source can be any
    // register and any width.
    let companion_base = match companion.operands.as_slice() {
        [
            RewriteOperand::Decoded(DecodedOperand::Register(_)),
            RewriteOperand::Decoded(DecodedOperand::Memory(memory)),
        ] => &memory.base,
        _ => return None,
    };
    if !same_register(adrp_rd, companion_base) {
        return None;
    }

    // Use the relocation pair as the source of truth, just like the
    // adrp+add fuser. Same-symbol on AdrpPage21 + LoadStorePageOffset12
    // ⇒ macro target.
    let target = macro_target_from_relocations(
        adrp,
        companion,
        relocations,
        CompanionRelocKind::LoadStore,
    )?;

    Some(MacroOp {
        kind: MacroKind::AccessValue,
        register: adrp_rd.clone(),
        target,
        original_addresses: [adrp.original_address, companion.original_address]
            .into_iter()
            .flatten()
            .collect(),
        original_instructions: vec![adrp.clone(), companion.clone()],
    })
}

fn try_fuse_adrp_add(
    adrp: &RewriteInstruction,
    add: &RewriteInstruction,
    block_at_address: &HashMap<u64, BasicBlockId>,
    container: Option<&Container>,
    relocations: &HashMap<u64, InstructionRelocation>,
) -> Option<MacroOp> {
    // adrp operands: [Register(Rd), Page(Target)].
    let adrp_rd = match adrp.operands.as_slice() {
        [RewriteOperand::Decoded(DecodedOperand::Register(rd)), RewriteOperand::Page(_)] => rd,
        _ => return None,
    };
    let adrp_target = match &adrp.operands[1] {
        RewriteOperand::Page(target) => *target,
        _ => return None,
    };

    // The pair we want is `add Rd, Rn, #imm`. Our decoder canonicalises
    // `add Rd, Rd, #0` as the `mov Rd, Rd` alias, so we accept both
    // shapes — Mov with two register operands is equivalent to
    // `add Rd, Rn, #0` here.
    let (add_rd, add_rn, add_imm) = match (add.mnemonic, add.operands.as_slice()) {
        (
            Aarch64Mnemonic::Add,
            [
                RewriteOperand::Decoded(DecodedOperand::Register(rd)),
                RewriteOperand::Decoded(DecodedOperand::Register(rn)),
                RewriteOperand::Decoded(DecodedOperand::Immediate(imm)),
            ],
        ) => (rd, rn, *imm),
        (
            Aarch64Mnemonic::Mov,
            [
                RewriteOperand::Decoded(DecodedOperand::Register(rd)),
                RewriteOperand::Decoded(DecodedOperand::Register(rn)),
            ],
        ) => (rd, rn, 0_i64),
        _ => return None,
    };

    if !same_register(adrp_rd, add_rd) || !same_register(adrp_rd, add_rn) {
        return None;
    }
    if add_imm < 0 || add_imm > 0xfff {
        // Page offsets are 12 bits; anything outside this isn't a fused
        // pair we can represent.
        return None;
    }

    // If both halves of the pair carry matching relocations naming the
    // same symbol, that *is* the macro target — the encoded operands
    // are placeholder zeros and resolving them by address would give a
    // bogus answer. This is the unlinked-.o case (clang emits
    // `R_AARCH64_ADR_PREL_PG_HI21 + R_AARCH64_ADD_ABS_LO12_NC` paired).
    let target = match macro_target_from_relocations(
        adrp,
        add,
        relocations,
        CompanionRelocKind::Add,
    ) {
        Some(t) => t,
        None => {
            // No relocations (already-linked code, or a hand-rolled
            // pair). Compose the symbolic target by looking up the
            // absolute address of (adrp_page + add_imm).
            let combined_address = match adrp_target {
                Target::Absolute(page) => page.wrapping_add(add_imm as u64),
                // If the adrp already points at a Block / Symbol, it
                // does so because the page address matched a block or
                // symbol entry — *not* an address inside it. The
                // combined adrp+add normally points to (page + offset),
                // which is rarely the page itself, so we conservatively
                // bail and let the user edit the two instructions
                // separately.
                Target::Block(_) | Target::Symbol(_) | Target::Constant(_) => {
                    return None;
                }
            };
            resolve_address(combined_address, block_at_address, container)
        }
    };

    Some(MacroOp {
        kind: MacroKind::LoadAddress,
        register: adrp_rd.clone(),
        target,
        original_addresses: [adrp.original_address, add.original_address]
            .into_iter()
            .flatten()
            .collect(),
        original_instructions: vec![adrp.clone(), add.clone()],
    })
}

/// If the adrp and add instructions both carry container relocations that
/// name the same symbol (`R_AARCH64_ADR_PREL_PG_HI21` paired with
/// `R_AARCH64_ADD_ABS_LO12_NC`), return that symbol as the macro's
/// resolved target. Otherwise `None` — caller falls back to the
/// address-synthesis path.
fn macro_target_from_relocations(
    adrp: &RewriteInstruction,
    companion: &RewriteInstruction,
    relocations: &HashMap<u64, InstructionRelocation>,
    expected_companion_kind: CompanionRelocKind,
) -> Option<Target> {
    let adrp_addr = adrp.original_address?;
    let companion_addr = companion.original_address?;

    let adrp_reloc = relocations.get(&adrp_addr)?;
    let companion_reloc = relocations.get(&companion_addr)?;

    if adrp_reloc.kind != RelocationKind::AdrpPage21 {
        return None;
    }
    if !companion_reloc_matches(companion_reloc.kind, expected_companion_kind) {
        return None;
    }
    let adrp_sym = adrp_reloc.symbol?;
    let companion_sym = companion_reloc.symbol?;
    if adrp_sym != companion_sym {
        // Mismatched symbols — not a fused pair. Could be a valid
        // construct (rare) but not one we want to collapse here.
        return None;
    }
    Some(Target::Symbol(adrp_sym))
}

/// Which lo-12 relocation shape the caller expects on the companion of
/// an `adrp`. The `add` form pairs with `R_AARCH64_ADD_ABS_LO12_NC`;
/// `ldr`/`str` pair with one of the `R_AARCH64_LDST*_ABS_LO12_NC`
/// variants. We don't pin the LDST access width here because clang
/// freely mixes it with the matching companion mnemonic, so the macro
/// matcher accepts any LDST variant against any `Ldr`/`Str` mnemonic.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum CompanionRelocKind {
    Add,
    LoadStore,
}

fn companion_reloc_matches(actual: RelocationKind, expected: CompanionRelocKind) -> bool {
    match expected {
        CompanionRelocKind::Add => matches!(actual, RelocationKind::AddPageOffset12),
        CompanionRelocKind::LoadStore => {
            matches!(actual, RelocationKind::LoadStorePageOffset12 { .. })
        }
    }
}

fn same_register(a: &Register, b: &Register) -> bool {
    use crate::isa::aarch64::RegisterClass::{W, WOrSp, X, XOrSp};
    if a.index != b.index {
        return false;
    }
    // adrp's Rd is always plain `X`, but add's RdSp / RnSp decode to
    // `XOrSp`. They refer to the same physical register; treat them as
    // equal for fusion. Same story for 32-bit W vs WOrSp, though
    // adrp+add fusion is 64-bit only in practice.
    let lhs_64 = matches!(a.class, X | XOrSp);
    let rhs_64 = matches!(b.class, X | XOrSp);
    let lhs_32 = matches!(a.class, W | WOrSp);
    let rhs_32 = matches!(b.class, W | WOrSp);
    (lhs_64 && rhs_64) || (lhs_32 && rhs_32)
}
