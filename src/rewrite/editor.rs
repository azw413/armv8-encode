//! High-level "edit a text section, then write" wrapper.
//!
//! The rewrite layer's primitives ([`RewritePlan`], [`lay_out`],
//! [`emit`], [`commit_to_container`]) compose into a deterministic
//! pipeline, but stitching them together by hand for routine edits
//! is a lot of ceremony for callers. [`TextEditor`] collapses the
//! six-step ceremony to three:
//!
//! ```ignore
//! use armv8_encode::container::Container;
//! use armv8_encode::rewrite::{Target, TextEditor};
//!
//! let bytes = std::fs::read("libgreet.so")?;
//! let container = Container::from_bytes(&bytes)?;
//!
//! let mut editor = TextEditor::for_section(&container, ".text")?;
//! let printf = editor.symbol_by_name("printf")?;
//! editor.redirect_branch_at(0x1234, Target::Symbol(printf))?;
//! let edited = editor.commit()?;
//!
//! std::fs::write("libgreet_rewritten.so", edited.to_bytes()?)?;
//! ```
//!
//! Under the hood the editor lifts the named text section into a
//! [`RewritePlan`], records edit operations as the caller invokes
//! them, then on [`Self::commit`] runs `lay_out` → `emit` →
//! `commit_to_container` and returns the resulting [`Container`].
//!
//! ## When to drop down to the lower-level API
//!
//! The editor is a convenience over the rewrite layer; it doesn't
//! hide anything. Use the underlying primitives directly when:
//!
//! - You need to edit multiple sections in the same container in
//!   one pass — `TextEditor` works on one section at a time.
//! - You need to inspect or mutate the [`RewritePlan`] in ways
//!   the editor doesn't expose. Get the plan via
//!   [`Self::plan_mut`] for direct access.
//! - You need to commit *without* a full
//!   `lay_out` + `emit` cycle (e.g., raw byte poking via
//!   [`Container::with_section_bytes`]).

use crate::container::{
    Architecture, Container, ContainerWriteError, SectionId, Symbol, SymbolId, SymbolKind,
};
use crate::isa::aarch64::{self, Aarch64Isa, DisassembleError, EncodeError};
use crate::isa::armv7::arm::ArmIsa;
use crate::isa::armv7::ThumbIsa;
use crate::isa::x86;
use crate::isa::Isa;
use crate::mc::{build_cfg, ControlFlowGraph};
use crate::rewrite::commit::commit_to_container;
use crate::rewrite::emit::{emit, EmitError};
use crate::rewrite::ir::{
    RewriteInstruction as RewriteInstructionGeneric, Target,
};
use crate::rewrite::layout::{lay_out, lay_out_for_size, lay_out_scattered, LayoutError};
use crate::rewrite::plan::{EditError, RewritePlan as RewritePlanGeneric};
use crate::rewrite::EmitOutput;

// AArch64-typed alias for the editor methods that bake in
// aarch64-specific instruction templates (add_function_exported,
// add_initialiser, etc.). The generic
// `LiftedTextSection<I>` works for any ISA; this alias lives
// here so the aarch64-only code reads the same as before
// generification. `add_function` itself is generic
// (see [`BinaryState::add_function_generic`]) and the
// aarch64-typed `add_function` is a thin wrapper.
type RewriteInstruction = RewriteInstructionGeneric<Aarch64Isa>;

/// Errors surfaced by the [`TextEditor`] convenience layer.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TextEditorError {
    /// No section with the given name exists in the container.
    SectionNotFound(String),
    /// The named section isn't a text-kind section (its bytes can't
    /// be disassembled).
    SectionNotText { name: String },
    /// No symbol with the given name. Returned by
    /// [`TextEditor::symbol_by_name`].
    SymbolNotFound(String),
    /// Disassembling the section's bytes failed. The section may
    /// be malformed, or it may contain non-instruction data the
    /// linear-sweep disassembler doesn't recognise.
    Disassemble(DisassembleError),
    /// ARMv7 sweep error (Thumb or ARM mode). Held as a string
    /// because the per-ISA error types aren't currently `Eq`
    /// and have shapes that don't make sense to embed in the
    /// editor's error enum verbatim.
    DisassembleArmv7(String),
    /// x86 sweep/disassembly error. Held as a string (the iced-backed
    /// error type isn't embedded in the editor enum verbatim).
    DisassembleX86(String),
    /// x86 encode/assemble error (from `BlockEncoder` / `Encoder`).
    X86Encode(String),
    /// No instruction at the requested address in the lifted section.
    AddressNotFound(u64),
    /// An edit operation failed — see [`EditError`] for cases.
    Edit(EditError),
    /// Layout failed — see [`LayoutError`] for cases.
    Layout(LayoutError),
    /// Emit failed — see [`EmitError`] for cases.
    Emit(EmitError),
    /// An instruction encoder error surfaced at edit time (e.g.,
    /// when [`Self::replace_instruction_at`] re-encodes a template
    /// before installing it).
    Encode(EncodeError),
    /// Final ELF/Mach-O serialization failed. Reported via
    /// [`Self::commit_to_bytes`] only — [`Self::commit`] returns
    /// before serialization.
    ContainerWrite(ContainerWriteError),
    /// `add_function` was called with an empty instruction list.
    EmptyFunction(String),
    /// `add_function` was called against an input whose
    /// [`ContainerKind`](crate::container::ContainerKind) doesn't
    /// support appending segments (i.e. anything other than
    /// `SharedObject` / `Executable`).
    AppendUnsupportedKind(crate::container::ContainerKind),
    /// `add_function` was called against an ELF input whose
    /// [`ElfImage`](crate::container::ElfImage) hasn't been
    /// populated. This indicates a reader bug; production paths
    /// always populate it for ET_DYN/ET_EXEC inputs.
    AppendMissingElfImage,
    /// An appended function's body emitted one or more relocations
    /// that the appended-segment writer can't yet apply. Defined and
    /// PLT-stubbed references are *folded* to a resolved displacement at
    /// emit time (see `emit::symbol_needs_relocation`), so a leftover
    /// relocation means a reference to an undefined symbol with no
    /// existing PLT stub — e.g. a newly synthesised call to an external
    /// not already imported. Emitting the matching dynamic relocation
    /// (new PLT/GOT/`.rela` entries in the appended segment) is future
    /// work; surfacing this beats silently dropping the relocation and
    /// shipping a binary that jumps to the wrong address.
    AppendNeedsUnsupportedRelocation { name: String, count: usize },
    /// `add_initialiser` was called against a library that has
    /// no `.init_array` section, or whose `.init_array` is
    /// empty. Stage-A `add_initialiser` only supports the
    /// "hijack an existing entry" case; building `.init_array`
    /// from scratch (the synthesise case) is future work.
    NoExistingInitArray,
    /// `add_initialiser` couldn't find the matching
    /// `R_AARCH64_RELATIVE` entry in `.rela.dyn` for the
    /// `.init_array` slot it wants to hijack. Indicates either
    /// a malformed input (the slot lacks a relocation, so
    /// `load_bias` wouldn't be applied) or an unusual relocation
    /// scheme this implementation doesn't yet handle.
    NoMatchingRelaDynEntry { init_array_vaddr: u64 },
    /// `add_initialiser(Append)` needs to insert new
    /// `.dynamic` tags (DT_INIT_ARRAY / DT_INIT_ARRAYSZ etc.)
    /// but the input's `.dynamic` doesn't have enough trailing
    /// DT_NULL slots to absorb them in place. Growing
    /// `.dynamic` itself is future work.
    DynamicTooFull { needed: usize },
    /// `add_initialiser(Append)` requires the input to have a
    /// `.rela.dyn` section to extend with the new relative
    /// reloc. Synthesising `.rela.dyn` from scratch is future
    /// work.
    NoExistingRelaDyn,
    /// The caller set [`BinaryState::prohibit_new_segments`]
    /// (typically because they're targeting iOS / App Store,
    /// which rejects dylibs / executables with multiple R-X
    /// segments) and the requested operation would require
    /// adding an `__APPENDED`-style segment. The `reason`
    /// field names the specific call that triggered the
    /// rejection so callers can identify which add_*
    /// invocation crossed the line.
    WouldCreateNewSegment { reason: String },
    /// `commit_chained_fixups` was called against an input
    /// that doesn't have an `LC_DYLD_CHAINED_FIXUPS` load
    /// command, so there's nowhere to land the blob.
    NoChainedFixupsCommand,
    /// `commit_chained_fixups` was called with a blob that
    /// doesn't fit in the existing `LC_DYLD_CHAINED_FIXUPS`
    /// data range. Growing the range requires moving every
    /// later `__LINKEDIT` chunk plus a resign; deferred to a
    /// future grow path.
    ChainedFixupsBlobTooLarge { new_size: u64, capacity: u64 },
    /// `remove_library_dependency` was called with a name that
    /// isn't present in the input's dependency list (no
    /// `DT_NEEDED` tag on ELF / no `LC_LOAD_DYLIB` on Mach-O
    /// resolves to it). The caller should check the existing
    /// dependency list first if the name is dynamic.
    LibraryDependencyNotFound(String),
    /// An input `.dynsym` / `.dynamic` section had a byte length
    /// that isn't a whole number of entries — the binary is
    /// malformed. Surfaced instead of panicking so a hostile input
    /// is a caught error.
    MalformedDynamicData(crate::container::dynsym_extension::DynParseError),
    /// [`BinaryState::reserve_text_region`] was asked for space on a
    /// format that doesn't support the reserve/carve API yet (only
    /// Mach-O is wired up in this increment).
    ReserveUnsupportedFormat(crate::container::BinaryFormat),
    /// A text-space reservation was requested but one already exists.
    /// The current increment supports a single reserved region at a
    /// time; release or commit before reserving again.
    TextRegionAlreadyReserved,
    /// A `RegionId` was passed that doesn't match the active
    /// reservation (stale handle, or none reserved).
    NoSuchRegion,
    /// The free-space policy ([`Exhaustion::Fail`]) couldn't satisfy
    /// the reservation: `needed` bytes requested, `available` was the
    /// largest contiguous free run found.
    InsufficientTextSpace { needed: u64, available: u64 },
    /// The reservation needs more room than free space offers and the
    /// policy is [`Exhaustion::Grow`] — extending `__TEXT` by `deficit`
    /// bytes (shifting `__DATA*`/`__LINKEDIT`) is required. The growth
    /// path is not implemented in this increment.
    WouldRequireTextGrowth { deficit: u64 },
    /// A placement/carve exceeded its region's capacity: `requested`
    /// bytes, `available` remained.
    RegionFull { requested: u64, available: u64 },
    /// `reserve_text_region` requires load-command / front edits
    /// (`add_library_dependency`, exported-symbol appends) to be
    /// resolved first, because they can move `__text` and invalidate a
    /// handed-out region base. Reserve before those, or after they've
    /// been committed.
    ReserveNeedsFrontEditsFirst,
    /// A caller-supplied alignment was zero.
    InvalidAlignment,
}

impl From<crate::rewrite::space::ReserveError> for TextEditorError {
    fn from(e: crate::rewrite::space::ReserveError) -> Self {
        use crate::rewrite::space::ReserveError;
        match e {
            ReserveError::ZeroAlign => TextEditorError::InvalidAlignment,
            ReserveError::InsufficientFreeSpace {
                needed,
                largest_available,
            } => TextEditorError::InsufficientTextSpace {
                needed,
                available: largest_available,
            },
            ReserveError::GrowthRequired { deficit } => {
                TextEditorError::WouldRequireTextGrowth { deficit }
            }
        }
    }
}

impl From<crate::rewrite::space::CarveError> for TextEditorError {
    fn from(e: crate::rewrite::space::CarveError) -> Self {
        use crate::rewrite::space::CarveError;
        match e {
            CarveError::ZeroAlign => TextEditorError::InvalidAlignment,
            CarveError::RegionFull { requested, available } => {
                TextEditorError::RegionFull { requested, available }
            }
            CarveError::Overflow => TextEditorError::RegionFull {
                requested: u64::MAX,
                available: 0,
            },
        }
    }
}

impl From<crate::container::dynsym_extension::DynParseError> for TextEditorError {
    fn from(e: crate::container::dynsym_extension::DynParseError) -> Self {
        TextEditorError::MalformedDynamicData(e)
    }
}

impl std::fmt::Display for TextEditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SectionNotFound(name) => write!(f, "no section named {name:?}"),
            Self::SectionNotText { name } => {
                write!(f, "section {name:?} is not text-kind; cannot disassemble")
            }
            Self::SymbolNotFound(name) => write!(f, "no symbol named {name:?}"),
            Self::Disassemble(err) => write!(f, "disassembly failed: {err:?}"),
            Self::DisassembleArmv7(msg) => write!(f, "armv7 disassembly failed: {msg}"),
            Self::DisassembleX86(msg) => write!(f, "x86 disassembly failed: {msg}"),
            Self::X86Encode(msg) => write!(f, "x86 encode failed: {msg}"),
            Self::AddressNotFound(addr) => {
                write!(f, "no instruction at address {addr:#x} in lifted section")
            }
            Self::Edit(err) => write!(f, "edit failed: {err:?}"),
            Self::Layout(err) => write!(f, "layout failed: {err:?}"),
            Self::Emit(err) => write!(f, "emit failed: {err:?}"),
            Self::Encode(err) => write!(f, "encode failed: {err:?}"),
            Self::ContainerWrite(err) => write!(f, "container serialization failed: {err}"),
            Self::EmptyFunction(name) => write!(
                f,
                "add_function({name:?}) requires at least one instruction",
            ),
            Self::AppendUnsupportedKind(kind) => write!(
                f,
                "add_function requires ET_DYN/ET_EXEC; container kind is {kind:?}",
            ),
            Self::AppendMissingElfImage => write!(
                f,
                "add_function requires container.elf_image to be populated; \
                 the reader should have populated it for this input",
            ),
            Self::AppendNeedsUnsupportedRelocation { name, count } => write!(
                f,
                "appended function {name:?} emitted {count} relocation(s) the \
                 appended-segment writer can't apply (a reference to an \
                 undefined symbol with no PLT stub); synthesising dynamic \
                 relocations for the appended segment is not yet supported",
            ),
            Self::NoExistingInitArray => write!(
                f,
                "add_initialiser requires the input to have a non-empty \
                 .init_array; synthesising .init_array from scratch is \
                 not yet supported",
            ),
            Self::NoMatchingRelaDynEntry { init_array_vaddr } => write!(
                f,
                "add_initialiser: no R_AARCH64_RELATIVE entry in .rela.dyn \
                 targets the .init_array slot at vaddr 0x{init_array_vaddr:x}",
            ),
            Self::DynamicTooFull { needed } => write!(
                f,
                "add_initialiser(Append) needs {needed} unused DT_NULL slots \
                 in .dynamic but the input doesn't have enough; growing \
                 .dynamic is not yet supported",
            ),
            Self::NoExistingRelaDyn => write!(
                f,
                "add_initialiser(Append) requires the input to have a \
                 .rela.dyn section; synthesising .rela.dyn from scratch \
                 is not yet supported",
            ),
            Self::WouldCreateNewSegment { reason } => write!(
                f,
                "operation would create a new segment but \
                 prohibit_new_segments() is set: {reason}",
            ),
            Self::LibraryDependencyNotFound(name) => write!(
                f,
                "remove_library_dependency: no DT_NEEDED / LC_LOAD_DYLIB \
                 entry matches {name:?}",
            ),
            Self::NoChainedFixupsCommand => write!(
                f,
                "commit_chained_fixups: input has no LC_DYLD_CHAINED_FIXUPS \
                 load command — nothing to overwrite",
            ),
            Self::MalformedDynamicData(err) => {
                write!(f, "malformed dynamic linking data in input: {err}")
            }
            Self::ReserveUnsupportedFormat(fmt) => write!(
                f,
                "reserve_text_region is only implemented for Mach-O; \
                 container format is {fmt:?}",
            ),
            Self::TextRegionAlreadyReserved => write!(
                f,
                "a text-space region is already reserved; this increment \
                 supports one region at a time",
            ),
            Self::NoSuchRegion => write!(f, "no such reserved text region (stale handle?)"),
            Self::InsufficientTextSpace { needed, available } => write!(
                f,
                "reserve_text_region: need {needed} contiguous free bytes in \
                 __TEXT but the largest free run is {available}; allow a grow \
                 or reduce the request",
            ),
            Self::WouldRequireTextGrowth { deficit } => write!(
                f,
                "reserve_text_region: satisfying this needs __TEXT grown by \
                 {deficit} more bytes (shifting __DATA*/__LINKEDIT); the grow \
                 path is not yet implemented",
            ),
            Self::RegionFull { requested, available } => write!(
                f,
                "carve/placement of {requested} bytes exceeds the region's \
                 remaining {available} bytes",
            ),
            Self::ReserveNeedsFrontEditsFirst => write!(
                f,
                "reserve_text_region must run before load-command/front edits \
                 (add_library_dependency, exported appends) that can move __text",
            ),
            Self::InvalidAlignment => write!(f, "alignment must be non-zero"),
            Self::ChainedFixupsBlobTooLarge { new_size, capacity } => write!(
                f,
                "commit_chained_fixups: new blob is {new_size} bytes but the \
                 existing LC_DYLD_CHAINED_FIXUPS reserves only {capacity}; \
                 growing the range requires a __LINKEDIT shift which this \
                 path doesn't yet support",
            ),
        }
    }
}

impl From<ContainerWriteError> for TextEditorError {
    fn from(err: ContainerWriteError) -> Self {
        Self::ContainerWrite(err)
    }
}

impl std::error::Error for TextEditorError {}

impl From<DisassembleError> for TextEditorError {
    fn from(err: DisassembleError) -> Self {
        Self::Disassemble(err)
    }
}

impl From<EditError> for TextEditorError {
    fn from(err: EditError) -> Self {
        Self::Edit(err)
    }
}

impl From<LayoutError> for TextEditorError {
    fn from(err: LayoutError) -> Self {
        Self::Layout(err)
    }
}

impl From<EmitError> for TextEditorError {
    fn from(err: EmitError) -> Self {
        Self::Emit(err)
    }
}

impl From<EncodeError> for TextEditorError {
    fn from(err: EncodeError) -> Self {
        Self::Encode(err)
    }
}

fn thumb_disassemble_error_to_text_editor(
    err: crate::isa::armv7::sweep::ThumbDisassembleError,
) -> TextEditorError {
    TextEditorError::DisassembleArmv7(format!("{err}"))
}

fn arm_disassemble_error_to_text_editor(
    err: crate::isa::armv7::arm::sweep::ArmDisassembleError,
) -> TextEditorError {
    TextEditorError::DisassembleArmv7(format!("{err}"))
}

/// Run the lay_out / emit / commit_to_container pipeline for
/// a single lifted-text section against `container`. Generic
/// over the ISA so all three variants of
/// [`LiftedTextSectionAny`] share one code path.
fn commit_lifted_section<I: Isa>(
    section: &LiftedTextSection<I>,
    container: &Container,
) -> Result<(Container, EmitOutput, SectionId), TextEditorError> {
    let layout = lay_out(&section.plan, section.base_address, Some(container))?;
    let mut output: EmitOutput = emit(&section.plan, &layout, Some(container))?;
    // Drop raw overlays (hole-placed code/data) onto the emitted bytes.
    for (va, bytes) in &section.overlays {
        let off = va.wrapping_sub(section.base_address) as usize;
        if let Some(slot) = output.bytes.get_mut(off..off + bytes.len()) {
            slot.copy_from_slice(bytes);
        }
    }
    let updated = commit_to_container(container, section.section_id, output.clone());
    Ok((updated, output, section.section_id))
}

/// Run the layout + emit + container-update pipeline for
/// whichever ISA's section is lifted, returning the updated
/// container and the EmitOutput for the appended-segment
/// override path. Used by `commit_to_bytes`'s Some(appended)
/// branch.
fn commit_lifted_any(
    text: &LiftedTextSectionAny,
    container: &Container,
) -> Result<(Container, EmitOutput, SectionId), TextEditorError> {
    match text {
        LiftedTextSectionAny::Aarch64(s) => commit_lifted_section(s, container),
        LiftedTextSectionAny::Thumb(s) => commit_lifted_section(s, container),
        LiftedTextSectionAny::Arm(s) => commit_lifted_section(s, container),
        LiftedTextSectionAny::X86(s) => commit_lifted_x86(s, container),
    }
}

/// x86 commit: assemble the (possibly edited) instruction list via
/// iced's `BlockEncoder` and splice the result back into the section.
/// Unlike the fixed-width ISAs, x86 doesn't go through the generic
/// `lay_out`/`emit` pipeline — `BlockEncoder` owns branch sizing and
/// RIP-relative fix-up. In-place text edits produce no new relocations.
fn commit_lifted_x86(
    section: &X86LiftedTextSection,
    container: &Container,
) -> Result<(Container, EmitOutput, SectionId), TextEditorError> {
    // Patch mode (no lifted instruction list): write `.text` VERBATIM and apply
    // the recorded byte-patches (e.g. trampolines) in place. This is the only
    // sound mode for a real binary — re-assembling the whole section relayouts
    // every instruction and breaks RIP-relative/data refs. The in-place
    // re-assemble path below is kept for the bypass redirect API on tiny inputs.
    let output = if section.instructions.is_empty() {
        let mut bytes = container.section(section.section_id).bytes.clone();
        for (addr, patch) in &section.patches {
            let off = addr
                .checked_sub(section.base_address)
                .and_then(|o| usize::try_from(o).ok())
                .ok_or_else(|| {
                    TextEditorError::X86Encode(format!("patch addr {addr:#x} before .text base"))
                })?;
            if off + patch.len() > bytes.len() {
                return Err(TextEditorError::X86Encode(format!(
                    "patch at {addr:#x} ({} bytes) overruns .text",
                    patch.len()
                )));
            }
            bytes[off..off + patch.len()].copy_from_slice(patch);
        }
        // No relocations: the bytes are the original `.text` verbatim, so its
        // existing `.rela.*` entries still apply unchanged (and the appended-
        // segment writer preserves them from the original image — it never reads
        // this field). Each patch is a self-contained `jmp rel32` trampoline,
        // which is PC-relative and needs no relocation. The relocated copy's own
        // references are folded/checked in `add_function_from_plan`.
        EmitOutput { bytes, relocations: Vec::new() }
    } else {
        let assembled = x86::assemble(&section.instructions, section.base_address, section.bitness)
            .map_err(|e| TextEditorError::X86Encode(e.to_string()))?;
        EmitOutput { bytes: assembled.bytes, relocations: Vec::new() }
    };
    let updated = commit_to_container(container, section.section_id, output.clone());
    Ok((updated, output, section.section_id))
}

/// A lifted x86 / x86_64 text section.
///
/// Distinct from [`LiftedTextSection<I>`]: x86 isn't driven through the
/// `Isa`-generic rewrite plan (its branch layout is handled by
/// `BlockEncoder` at emit time, not the fixed-width widening pass), so
/// it keeps a plain editable list of `iced_x86::Instruction`s. Edit
/// primitives mutate that list; [`commit_lifted_x86`] re-assembles it.
#[derive(Debug, Clone)]
pub struct X86LiftedTextSection {
    section_id: SectionId,
    base_address: u64,
    bitness: x86::Bitness,
    /// Editable instruction list. Each instruction's `ip()` is its
    /// current source address, which `BlockEncoder` uses to resolve
    /// intra-section branch targets. Empty in *patch mode* (see
    /// [`BinaryEditor::prepare_text_patch`]).
    instructions: Vec<iced_x86::Instruction>,
    /// Byte-patches `(address, bytes)` applied over the verbatim original
    /// `.text` at commit (patch mode) — e.g. a `jmp rel32` trampoline at a
    /// relocated function's entry. Unlike the re-assemble path, this leaves the
    /// rest of `.text` byte-identical, so RIP-relative/data refs survive.
    patches: Vec<(u64, Vec<u8>)>,
}

impl X86LiftedTextSection {
    /// Index of the instruction whose address is `address`.
    fn index_at(&self, address: u64) -> Result<usize, TextEditorError> {
        self.instructions
            .iter()
            .position(|i| i.ip() == address)
            .ok_or(TextEditorError::AddressNotFound(address))
    }

    /// Retarget the direct branch/call at `address` to point at the
    /// absolute address `target`. If `target` is inside this section,
    /// `BlockEncoder` fixes up the displacement (and resizes the branch
    /// if needed); otherwise it's encoded as the reaching form to the
    /// absolute target (e.g. a PLT stub).
    pub fn redirect_branch_at(&mut self, address: u64, target: u64) -> Result<(), TextEditorError> {
        let index = self.index_at(address)?;
        self.instructions[index].set_near_branch64(target);
        Ok(())
    }

    /// Replace the instruction at `address` with `new`. The replacement
    /// keeps the slot's source IP so surrounding branches still resolve.
    pub fn replace_instruction_at(
        &mut self,
        address: u64,
        mut new: iced_x86::Instruction,
    ) -> Result<(), TextEditorError> {
        let index = self.index_at(address)?;
        new.set_ip(address);
        self.instructions[index] = new;
        Ok(())
    }

    /// The decoded instruction list (read-only).
    pub fn instructions(&self) -> &[iced_x86::Instruction] {
        &self.instructions
    }

    /// Record a `jmp rel32` trampoline at `entry` targeting absolute `target`
    /// (e.g. a relocated copy in the appended segment). At commit, the 5-byte
    /// `E9 rel32` overwrites the original bytes at `entry`; the rest of `.text`
    /// is written verbatim. `rel32` reaches ±2 GiB, so the appended segment is
    /// always in range. Use with [`BinaryEditor::prepare_text_patch`] (patch
    /// mode); the function's now-dead body past the 5 bytes is never executed.
    pub fn add_trampoline(&mut self, entry: u64, target: u64) -> Result<(), TextEditorError> {
        let instr = iced_x86::Instruction::with_branch(iced_x86::Code::Jmp_rel32_64, target)
            .map_err(|e| TextEditorError::X86Encode(e.to_string()))?;
        let bytes = x86::encode_instruction(&instr, entry, self.bitness)
            .map_err(|e| TextEditorError::X86Encode(e.to_string()))?;
        self.patches.push((entry, bytes));
        Ok(())
    }
}

/// High-level editor for an ELF/Mach-O binary.
///
/// Composes two scopes:
///
/// - [`BinaryEditor::binary`] — whole-binary operations
///   (appending functions, registering dynamic exports,
///   declaring library dependencies, etc.). Available
///   without lifting any text section.
/// - [`BinaryEditor::text`] — section-scoped operations
///   (redirecting branches, replacing instructions, etc.).
///   Populated by calling [`Self::lift_text_section`].
///
/// The two scopes live in separate fields so callers can hold
/// `&mut` references to both simultaneously via destructuring:
///
/// ```ignore
/// let mut editor = BinaryEditor::new(&container)?;
/// editor.lift_text_section(".text")?;
///
/// let BinaryEditor { binary, text, .. } = &mut editor;
/// let text = text.as_mut().unwrap();
///
/// let new_fn = binary.add_function("foo", body)?;
/// let addr = text.function_address("greet_double").unwrap();
/// text.replace_instruction_at(addr, /* b new_fn */)?;
/// ```
///
/// `commit_to_bytes` consumes both scopes and runs the writer
/// pipeline, producing a runnable byte stream.
#[derive(Debug, Clone)]
pub struct BinaryEditor {
    /// Whole-binary editing state. Always present.
    pub binary: BinaryState,
    /// The currently-lifted text section, if any. Populated
    /// by [`Self::lift_text_section`]. Section-scoped methods
    /// (e.g. `replace_instruction_at`) live on
    /// [`LiftedTextSection`] (now generic over `Isa`); the
    /// variant enum [`LiftedTextSectionAny`] wraps whichever
    /// ISA was lifted. Use the helpers
    /// `text.as_ref()?.aarch64()` etc. to reach the per-ISA
    /// view.
    pub text: Option<LiftedTextSectionAny>,
}

/// Backwards-compatible alias for the old name. New code
/// should use [`BinaryEditor`].
#[deprecated(
    since = "0.1.0",
    note = "renamed to BinaryEditor; use BinaryEditor::new(&container) or \
            BinaryEditor::for_section(&container, name) (the old constructor) \
            instead",
)]
pub type TextEditor = BinaryEditor;

/// Whole-binary editing state.
///
/// Holds the working copy of the [`Container`] plus the
/// queues / overrides accumulated by editor methods (appended
/// functions, dynamic-tag updates, etc.). Every method here
/// operates without needing a lifted text section.
#[derive(Debug, Clone)]
pub struct BinaryState {
    /// The container being edited. Mutated as functions are added
    /// (their symbols land in `container.symbols`); the existing
    /// section table and bytes stay otherwise unchanged until
    /// [`BinaryEditor::commit`] runs the layout pipeline.
    container: Container,
    /// Functions added via [`Self::add_function`]. Each entry is
    /// already laid out and emitted at its assigned virtual
    /// address; commit only needs to concatenate and append.
    /// `None` until the first add_function call so containers that
    /// only do in-place edits keep going through the cheaper
    /// in-place writer path.
    appended: Option<AppendedFunctionsState>,
    /// Runtime-WRITABLE appended data ([`Self::add_writable_data`]),
    /// emitted at commit as a separate `PF_R | PF_W` PT_LOAD at its own
    /// vaddr (picked BELOW the code append region so buffer addresses
    /// are stable before any referencing function is appended). `None`
    /// until the first `add_writable_data` call.
    appended_writable: Option<WritableState>,
    /// Whole-section byte overrides queued by editor methods
    /// (e.g. [`Self::add_initialiser`] queues a rewritten
    /// `.rela.dyn`). Applied at commit time on the
    /// appended-segment writer path. Keyed by section index;
    /// later entries supersede earlier ones for the same
    /// section.
    section_overrides: Vec<(usize, Vec<u8>)>,
    /// New `.init_array` slots that should land in the appended
    /// segment with matching new `R_AARCH64_RELATIVE` entries
    /// in a rebuilt `.rela.dyn`. Populated by
    /// [`Self::add_initialiser`] with
    /// [`InitialiserPosition::Append`]. Resolved at commit
    /// time alongside (or instead of) the dynsym-export rebuild.
    pending_appended_init_slots: Vec<u64>,
    /// Library names queued by [`Self::add_library_dependency`].
    /// At commit time each name is appended to `.dynstr` and a
    /// new `DT_NEEDED` tag is inserted into `.dynamic` pointing
    /// at the new dynstr offset. The new `.dynstr` lives in the
    /// appended segment (via the same machinery the export path
    /// uses).
    pending_library_deps: Vec<String>,
    /// When true, the Mach-O writer's intra-`__TEXT`
    /// placement strategy is bypassed and the
    /// appended-segment fallback is used instead. Set by
    /// methods that require an `__APPENDED` segment for
    /// their format-specific bookkeeping
    /// (`add_function_exported` needs `__LINKEDIT` shifting
    /// for the rebuilt symtab/trie; `add_library_dependency`
    /// just needs the segment writer's path active).
    force_segment_placement: bool,
    /// When true, any operation that would require creating
    /// a new segment fails immediately with
    /// [`TextEditorError::WouldCreateNewSegment`]. Set by
    /// callers via [`Self::prohibit_new_segments`] to enforce
    /// iOS / App Store compatibility (which rejects dylibs
    /// with multiple R-X segments). Affects Mach-O only —
    /// for ELF, where appended PT_LOAD is the standard
    /// pattern, the flag is silently ignored.
    no_new_segments: bool,
    /// Caller-staged absolute-file-offset byte overrides
    /// passed to the Mach-O writer verbatim. Used by
    /// [`Self::commit_chained_fixups`] (and similar future
    /// methods) to splice a freshly-serialised blob into the
    /// output at a known file offset without going through the
    /// neutral container model — there's no Container concept
    /// for "the LC_DYLD_CHAINED_FIXUPS blob," so this is the
    /// least-invasive way to land it.
    macho_raw_byte_overrides: Vec<(u64, Vec<u8>)>,
    /// The active text-space reservation, if any. Established by
    /// [`Self::reserve_text_region`] and consumed by the region-scoped
    /// placement methods (`add_function_in` / `add_data_in` /
    /// `add_raw_code_in`). The [`space::Region`] is the bump allocator
    /// whose cursor stays in lockstep with the shared `appended`
    /// buffer's length, so the two never disagree about where the next
    /// item lands. `None` until the first reservation; a single region
    /// is supported per edit in this increment.
    reserved_text_region: Option<(RegionId, crate::rewrite::space::Region)>,
    /// When a reservation needed more room than free space offered and
    /// the policy was [`crate::rewrite::space::Exhaustion::Grow`], the
    /// uniform-shift growth geometry `(growth_point, delta)`. Commit
    /// routes to [`Self::build_grown_macho`] when this is set.
    pending_text_grow: Option<(u64, u64)>,
}

/// Opaque handle to a reserved text-space region. Passed back to the
/// region-scoped placement methods. `Copy` so it composes with the
/// `binary`/`text` borrow split the way `SymbolId`/`SectionId` do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionId(pub u32);

/// The result of a successful [`BinaryState::reserve_text_region`]: a
/// contiguous R-X span you can carve functions/data out of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    /// Handle to pass to `add_function_in` / `add_data_in` /
    /// `add_raw_code_in` / `next_address_in`.
    pub region: RegionId,
    /// Virtual address of the span start. Stable for the life of the
    /// reservation (provided front/load-command edits were settled
    /// first — see [`TextEditorError::ReserveNeedsFrontEditsFirst`]).
    pub base_address: u64,
    /// File offset of the span start.
    pub base_file_offset: u64,
    /// Total bytes reserved (`min_bytes + headroom`).
    pub capacity: u64,
    /// What free space fulfilled the reservation, and how much of each.
    /// A single contiguous run today, so one entry — but shaped as a
    /// list so scattered fulfilment can report multiple sources later.
    pub sources: Vec<(crate::rewrite::space::SpaceSource, u64)>,
}

/// A text section lifted into editable IR form.
///
/// Created by [`BinaryEditor::lift_text_section`]; modified by
/// section-scoped methods (`redirect_branch_at`,
/// `replace_instruction_at`, etc.). Lives in
/// [`BinaryEditor::text`] so the parent editor can hold `&mut`
/// references to both scopes via destructuring.
#[derive(Debug, Clone)]
pub struct LiftedTextSection<I: Isa> {
    /// Section id we're rewriting.
    section_id: SectionId,
    /// Base address the section loads at.
    base_address: u64,
    /// Decoded instructions for the section (cached so subsequent
    /// edit operations don't re-disassemble).
    instructions: Vec<I::DecodedInstruction>,
    /// CFG built from the instructions; lift consumes both.
    cfg: ControlFlowGraph,
    /// The mutable plan. Edit primitives delegate to this.
    plan: RewritePlanGeneric<I>,
    /// Raw byte overlays applied to the emitted section after layout/emit, keyed
    /// by virtual address. Used to drop pre-assembled, position-independent code
    /// or data into freed regions ("holes") of the section without routing it
    /// through the per-instruction plan.
    overlays: Vec<(u64, Vec<u8>)>,
}

/// Variant wrapper holding a lifted section for any supported
/// ISA. `BinaryEditor::text` holds an `Option` of this, so the
/// editor can dispatch on the container's architecture without
/// the rest of `BinaryEditor` becoming generic.
///
/// Callers that know which ISA they're rewriting downcast via
/// [`Self::aarch64_mut`] / [`Self::thumb_mut`] / [`Self::arm_mut`]
/// to reach the per-ISA section-scoped methods (the rewriter's
/// aarch64-specific helpers like `add_function` /
/// `add_initialiser` live on `BinaryState` and operate on the
/// AArch64 path regardless).
#[derive(Debug, Clone)]
pub enum LiftedTextSectionAny {
    Aarch64(LiftedTextSection<Aarch64Isa>),
    Thumb(LiftedTextSection<ThumbIsa>),
    Arm(LiftedTextSection<ArmIsa>),
    X86(X86LiftedTextSection),
}

impl LiftedTextSectionAny {
    /// AArch64-typed view. `None` if the lifted section is a
    /// different ISA.
    pub fn aarch64(&self) -> Option<&LiftedTextSection<Aarch64Isa>> {
        match self {
            Self::Aarch64(s) => Some(s),
            _ => None,
        }
    }

    pub fn aarch64_mut(&mut self) -> Option<&mut LiftedTextSection<Aarch64Isa>> {
        match self {
            Self::Aarch64(s) => Some(s),
            _ => None,
        }
    }

    pub fn thumb(&self) -> Option<&LiftedTextSection<ThumbIsa>> {
        match self {
            Self::Thumb(s) => Some(s),
            _ => None,
        }
    }

    pub fn thumb_mut(&mut self) -> Option<&mut LiftedTextSection<ThumbIsa>> {
        match self {
            Self::Thumb(s) => Some(s),
            _ => None,
        }
    }

    pub fn arm(&self) -> Option<&LiftedTextSection<ArmIsa>> {
        match self {
            Self::Arm(s) => Some(s),
            _ => None,
        }
    }

    pub fn arm_mut(&mut self) -> Option<&mut LiftedTextSection<ArmIsa>> {
        match self {
            Self::Arm(s) => Some(s),
            _ => None,
        }
    }

    /// x86 / x86_64-typed view. `None` if the lifted section is a
    /// different ISA.
    pub fn x86(&self) -> Option<&X86LiftedTextSection> {
        match self {
            Self::X86(s) => Some(s),
            _ => None,
        }
    }

    pub fn x86_mut(&mut self) -> Option<&mut X86LiftedTextSection> {
        match self {
            Self::X86(s) => Some(s),
            _ => None,
        }
    }

    /// ISA-agnostic accessors. Useful for read-only consumers
    /// that just need the section id or base address (e.g. the
    /// commit pipeline) without caring which ISA the section
    /// was lifted as.
    pub fn section_id(&self) -> SectionId {
        match self {
            Self::Aarch64(s) => s.section_id,
            Self::Thumb(s) => s.section_id,
            Self::Arm(s) => s.section_id,
            Self::X86(s) => s.section_id,
        }
    }

    pub fn base_address(&self) -> u64 {
        match self {
            Self::Aarch64(s) => s.base_address,
            Self::Thumb(s) => s.base_address,
            Self::Arm(s) => s.base_address,
            Self::X86(s) => s.base_address,
        }
    }

    /// ISA-agnostic raw byte overlay at virtual address `va`, applied to the
    /// committed section. For the fixed-width ISAs this is an *overlay* on top of
    /// the re-laid-out section; for x86 it's a *byte-patch* on top of the verbatim
    /// section (same `(va, bytes)` model under both, applied at commit). The
    /// hole-placement helpers use this to drop position-independent code/data into
    /// freed `.text` regions without caring which ISA the section is.
    pub fn overlay_bytes(&mut self, va: u64, bytes: Vec<u8>) {
        match self {
            Self::Aarch64(s) => s.overlay_bytes(va, bytes),
            Self::Thumb(s) => s.overlay_bytes(va, bytes),
            Self::Arm(s) => s.overlay_bytes(va, bytes),
            Self::X86(s) => s.patches.push((va, bytes)),
        }
    }
}

// Back-compat aliases. Methods on the aarch64 variant — which
// most existing code calls — now live on
// `LiftedTextSection<Aarch64Isa>`. Callers using the old
// `editor.text.as_mut().unwrap().replace_instruction_at(...)`
// shape get a deref-style helper via
// [`LiftedTextSectionAny::aarch64_mut`].
//
// `BinaryEditor::text` stores [`LiftedTextSectionAny`]; the
// aarch64 short-form is at [`BinaryEditor::aarch64_text_mut`].

/// An appended [`RewritePlan`](crate::rewrite::RewritePlan) with its source
/// [`Isa`] erased, so the heterogeneous `add_function*` paths can all retain
/// their plan in one `Vec`. Mirrors the "generic `I` → erased enum" mechanism
/// [`LiftedTextSectionAny`] uses for lifted sections: a variant per concrete
/// ISA, each holding the ISA-typed value. The commit-time re-emit pass
/// ([`AppendedFunctionsState::rebase`]) matches on the variant to recover the
/// concrete `I` for `lay_out::<I>` / `emit::<I>`.
#[derive(Debug, Clone)]
enum RetainedPlanAny {
    Aarch64(RewritePlanGeneric<Aarch64Isa>),
    Thumb(RewritePlanGeneric<ThumbIsa>),
    Arm(RewritePlanGeneric<ArmIsa>),
    X86(RewritePlanGeneric<crate::isa::x86::X86Isa>),
}

/// Erases a generic `RewritePlan<I>` into [`RetainedPlanAny`] from a context
/// that only knows `I: Isa` (e.g. `add_function_generic::<I>`), so the retained
/// plan can live in the heterogeneous `RetainedAppend` vec. Solves the same
/// "generic `I` → erased enum" problem `LiftedTextSectionAny` solves at its
/// construction sites — but here `I` is a type parameter, not a matched
/// concrete type, so we recover the concrete ISA by `Any`-downcasting (every
/// `Isa` is `'static`, so this is a `TypeId` check). Panics only if a new ISA
/// is added without a matching arm here, which is caught immediately by the
/// `add_function*` tests.
fn wrap_retained_plan<I: Isa>(plan: RewritePlanGeneric<I>) -> RetainedPlanAny {
    use std::any::Any;
    // `Option<RewritePlan<I>>` lets us `take` the concrete value out of the
    // `&mut dyn Any` once a downcast matches, without cloning or transmuting.
    let mut held: Option<RewritePlanGeneric<I>> = Some(plan);
    let any: &mut dyn Any = &mut held;
    if let Some(p) = any.downcast_mut::<Option<RewritePlanGeneric<Aarch64Isa>>>() {
        return RetainedPlanAny::Aarch64(p.take().expect("plan present"));
    }
    if let Some(p) = any.downcast_mut::<Option<RewritePlanGeneric<ThumbIsa>>>() {
        return RetainedPlanAny::Thumb(p.take().expect("plan present"));
    }
    if let Some(p) = any.downcast_mut::<Option<RewritePlanGeneric<ArmIsa>>>() {
        return RetainedPlanAny::Arm(p.take().expect("plan present"));
    }
    if let Some(p) =
        any.downcast_mut::<Option<RewritePlanGeneric<crate::isa::x86::X86Isa>>>()
    {
        return RetainedPlanAny::X86(p.take().expect("plan present"));
    }
    unreachable!("RetainedPlanAny has no variant for this Isa — add one");
}

/// One appended item retained so the Mach-O commit fallback can re-emit it at a
/// fresh segment base when the optimistic intra-`__TEXT` base doesn't fit. The
/// emitted bytes can't simply be shifted — `adrp`/page-relative and
/// cross-symbol references are absolute — so functions are re-laid-out and
/// re-emitted at the new base. Raw data is base-independent and copied verbatim.
#[derive(Debug, Clone)]
enum RetainedAppend {
    /// A function appended via `add_function*`: re-emittable at any base by
    /// `lay_out` + `emit` of `plan` at `base + offset`.
    Function {
        name: String,
        offset: u64,
        plan: RetainedPlanAny,
    },
    /// Raw appended data ([`BinaryState::add_data`]): base-independent bytes
    /// copied verbatim at `offset`. The bytes don't move relative to the base,
    /// but the blob's *symbol* address does — appended functions reference it by
    /// `adrp`/`add` against `Target::Symbol(name)`, which resolves to the symbol
    /// address — so `name` is retained to rebase that symbol with the segment.
    Data {
        name: String,
        offset: u64,
        bytes: Vec<u8>,
    },
}

/// Cumulative state for functions appended via
/// [`TextEditor::add_function`]. Lives behind an `Option` on the
/// editor so callers who never add functions stay on the cheaper
/// in-place commit path.
#[derive(Debug, Clone)]
struct AppendedFunctionsState {
    /// Virtual address of the new segment's first byte. Picked at
    /// the first add_function call by walking the input's PT_LOAD
    /// extents.
    segment_vaddr: u64,
    /// Concatenated bytes of every appended function in addition
    /// order. The function at `segment_vaddr + offset` lives at
    /// `bytes[offset..offset+len]`.
    bytes: Vec<u8>,
    /// Functions registered with the dynamic linker via
    /// [`TextEditor::add_function_exported`]. At commit time the
    /// writer extends `.dynsym` / `.dynstr` / `.gnu.version` and
    /// regenerates `.gnu.hash` to expose these for dlopen/dlsym.
    /// Empty when callers only use the unexported `add_function`.
    exports: Vec<ExportedSymbol>,
    /// Per-item re-emit information, in addition order, used by the
    /// Mach-O commit fallback ([`Self::rebase`]) when the optimistic
    /// intra-`__TEXT` base doesn't fit and the payload must be
    /// re-emitted at a fresh segment base. Each entry records its
    /// 0-based `offset` into `bytes` so the rebased buffer can be
    /// reconstructed (including alignment padding) from offsets
    /// alone. Empty until the first `add_function` / `add_data`.
    items: Vec<RetainedAppend>,
}

impl AppendedFunctionsState {
    /// Re-emit every appended function at `new_base` (the Mach-O fallback when
    /// the optimistic intra-`__TEXT` base baked into the emitted bytes doesn't
    /// fit a `__TEXT` free region). The bytes can't simply be shifted —
    /// `adrp`/page-relative and cross-symbol references are absolute — so each
    /// function is re-laid-out + re-emitted at `new_base + offset`.
    ///
    /// Operates on `container`: it updates each appended symbol's address there
    /// (so the *caller* must pass the very container whose symbols / `macho_image`
    /// the writer consumes) and uses it as the emit context so cross-function
    /// references resolve to the rebased addresses. Updates
    /// `self.segment_vaddr` / `self.bytes` and returns the rebuilt concatenated
    /// bytes (which equal the new `self.bytes`).
    fn rebase(
        &mut self,
        new_base: u64,
        container: &mut Container,
    ) -> Result<Vec<u8>, TextEditorError> {
        let old_base = self.segment_vaddr;

        // Phase 1: move every appended symbol — functions AND data blobs — to its
        // rebased address FIRST, so references emitted in phase 2 resolve to the
        // new addresses (not the stale intra-`__TEXT` ones). This must include
        // data: appended functions reach an `add_data` blob (e.g. a VM's bytecode)
        // by `adrp`/`add` against its `Target::Symbol`, which folds to the symbol
        // address at emit — a stale data symbol sends that load to the old page.
        // Match by name *and* old address (`old_base + offset`) so we never
        // disturb an unrelated homonym elsewhere in the symbol table.
        for item in &self.items {
            let (name, offset) = match item {
                RetainedAppend::Function { name, offset, .. } => (name, *offset),
                RetainedAppend::Data { name, offset, .. } => (name, *offset),
            };
            let old_addr = old_base + offset;
            let new_addr = new_base + offset;
            if let Some(sym) = container
                .symbols
                .iter_mut()
                .find(|s| s.name == *name && s.address == old_addr)
            {
                sym.address = new_addr;
            }
        }

        // Phase 2: rebuild the concatenated buffer at `new_base`. Functions are
        // re-laid-out + re-emitted at their rebased vaddr; data is copied
        // verbatim. Inter-item alignment padding is reconstructed implicitly
        // from each item's recorded offset (the buffer is zero-filled to the
        // offset before each item is written).
        let mut bytes = vec![0u8; self.bytes.len()];
        for item in &self.items {
            match item {
                RetainedAppend::Function { offset, plan, .. } => {
                    let vaddr = new_base + offset;
                    let emitted = match plan {
                        RetainedPlanAny::Aarch64(p) => {
                            let layout = lay_out::<Aarch64Isa>(p, vaddr, Some(container))?;
                            emit::<Aarch64Isa>(p, &layout, Some(container))?
                        }
                        RetainedPlanAny::Thumb(p) => {
                            let layout = lay_out::<ThumbIsa>(p, vaddr, Some(container))?;
                            emit::<ThumbIsa>(p, &layout, Some(container))?
                        }
                        RetainedPlanAny::Arm(p) => {
                            let layout = lay_out::<ArmIsa>(p, vaddr, Some(container))?;
                            emit::<ArmIsa>(p, &layout, Some(container))?
                        }
                        RetainedPlanAny::X86(p) => {
                            let layout =
                                lay_out::<crate::isa::x86::X86Isa>(p, vaddr, Some(container))?;
                            emit::<crate::isa::x86::X86Isa>(p, &layout, Some(container))?
                        }
                    };
                    // Same invariant as the append path: a re-emitted body that
                    // still carries relocations references something we cannot
                    // place — fail loudly rather than ship a wrong jump.
                    if !emitted.relocations.is_empty() {
                        return Err(TextEditorError::AppendNeedsUnsupportedRelocation {
                            name: match item {
                                RetainedAppend::Function { name, .. } => name.clone(),
                                RetainedAppend::Data { .. } => String::new(),
                            },
                            count: emitted.relocations.len(),
                        });
                    }
                    let start = *offset as usize;
                    let end = start + emitted.bytes.len();
                    if end > bytes.len() {
                        bytes.resize(end, 0);
                    }
                    bytes[start..end].copy_from_slice(&emitted.bytes);
                }
                RetainedAppend::Data { offset, bytes: data, .. } => {
                    let start = *offset as usize;
                    let end = start + data.len();
                    if end > bytes.len() {
                        bytes.resize(end, 0);
                    }
                    bytes[start..end].copy_from_slice(data);
                }
            }
        }

        self.segment_vaddr = new_base;
        self.bytes = bytes.clone();
        Ok(bytes)
    }
}

/// Accumulated runtime-writable appended data (see
/// [`BinaryState::add_writable_data`]). Concatenated in addition order
/// at `segment_vaddr`; emitted as a separate R+W PT_LOAD at commit.
#[derive(Debug, Clone)]
struct WritableState {
    segment_vaddr: u64,
    bytes: Vec<u8>,
}

/// Where in the existing `.init_array` chain a freshly-appended
/// initialiser should be inserted, controlling whether it runs
/// before or after the library's own constructors.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum InitialiserPosition {
    /// Hijack the *first* slot. The appended code runs before any
    /// other ctor in the library — including CRT helpers like
    /// `frame_dummy`. The original first-slot ctor still runs (the
    /// wrapper chain-tails to it), so the full set of original
    /// ctors fires; the appended code is just prepended at the
    /// front. Use this when "must run before everything" matters.
    First,
    /// Hijack the *last* slot. The appended code runs after every
    /// ctor preceding the last one (CRT helpers and any earlier
    /// user ctors), but before the very last user ctor — which the
    /// wrapper chain-tails to so it still runs. Slightly less
    /// invasive: most CRT setup has already happened. Default for
    /// callers who don't care about strict ordering.
    Last,
    /// Add a *brand-new* slot to the end of `.init_array` rather
    /// than hijacking an existing one. Use this when the input
    /// has no `.init_array`, or when you want to add an
    /// initialiser without disturbing any existing ctor's
    /// behaviour.
    ///
    /// Stage-B implementation: the new `.init_array` (originals
    /// plus one new slot) and the extended `.rela.dyn` (originals
    /// plus one new `R_AARCH64_RELATIVE`) are emitted in the
    /// appended segment, and `.dynamic` is patched to point at
    /// them via DT_INIT_ARRAY / DT_INIT_ARRAYSZ / DT_RELA /
    /// DT_RELASZ / DT_RELACOUNT. New tags are inserted into
    /// trailing DT_NULL slots — the input's `.dynamic` therefore
    /// needs at least two unused DT_NULL entries when no
    /// DT_INIT_ARRAY tag is already present (one if the tags
    /// already exist).
    ///
    /// No chain-back: the user `body` is registered as a ctor in
    /// its own right and runs after all original ctors complete.
    Append,
}

/// One symbol slated for promotion to a dynsym export at commit
/// time. Created by [`TextEditor::add_function_exported`].
#[derive(Debug, Clone)]
struct ExportedSymbol {
    /// Symbol id in the container's static `.symtab`. Reserved
    /// for future cross-checks (e.g., refusing duplicate exports
    /// of the same id); current commit path resolves by name and
    /// vaddr only.
    #[allow(dead_code)]
    symbol_id: SymbolId,
    /// Public name to write into `.dynstr`. Same string as the
    /// symbol's static name today.
    name: String,
    /// Virtual address (assigned when add_function laid the
    /// body out).
    vaddr: u64,
    /// Function size in bytes.
    size: u64,
}

/// Best-fit allocate `need` (rounded up to 4) bytes from `holes` (each
/// `(vaddr, size)`): pick the smallest hole that fits, return its start, shrink
/// it, and drop a remainder smaller than one instruction. `None` if none fit.
fn take_hole(holes: &mut Vec<(u64, u64)>, need: u64) -> Option<u64> {
    let need = (need + 3) & !3;
    if need == 0 {
        return None;
    }
    let idx = holes
        .iter()
        .enumerate()
        .filter(|(_, &(_, size))| size >= need)
        .min_by_key(|(_, &(_, size))| size)
        .map(|(i, _)| i)?;
    let va = holes[idx].0;
    holes[idx].0 += need;
    holes[idx].1 -= need;
    if holes[idx].1 < 4 {
        holes.remove(idx);
    }
    Some(va)
}

/// Enough state to undo one [`reserve_hole`] without cloning the whole pool.
/// `removed` is `Some((idx, entry))` when the reservation consumed the hole
/// entirely (so it was removed); otherwise the hole was shrunk in place.
struct HoleReservation {
    va: u64,
    need: u64,
    removed: Option<(usize, (u64, u64))>,
}

/// Best-fit-reserve `need` bytes from `holes`, returning the placement `va` and a
/// [`HoleReservation`] that [`release_hole`] can roll back — O(holes) once, not a
/// full clone per call. On success the pool is mutated exactly as [`take_hole`]
/// would; callers that succeed simply drop the reservation.
fn reserve_hole(holes: &mut Vec<(u64, u64)>, need: u64) -> Option<(u64, HoleReservation)> {
    let need = (need + 3) & !3;
    if need == 0 {
        return None;
    }
    let idx = holes
        .iter()
        .enumerate()
        .filter(|(_, &(_, size))| size >= need)
        .min_by_key(|(_, &(_, size))| size)
        .map(|(i, _)| i)?;
    let va = holes[idx].0;
    let original = holes[idx];
    holes[idx].0 += need;
    holes[idx].1 -= need;
    let removed = if holes[idx].1 < 4 {
        let entry = original;
        holes.remove(idx);
        Some((idx, entry))
    } else {
        None
    };
    Some((va, HoleReservation { va, need, removed }))
}

/// Undo a [`reserve_hole`]: restore the consumed bytes so a failed placement
/// leaves the pool exactly as it was.
fn release_hole(holes: &mut Vec<(u64, u64)>, r: HoleReservation) {
    if let Some((idx, entry)) = r.removed {
        // The hole was fully consumed and removed — reinsert it verbatim.
        holes.insert(idx.min(holes.len()), entry);
    } else {
        // The hole was shrunk in place — find it by its post-reserve start
        // (`va + need`) and grow it back.
        let start = r.va + r.need;
        if let Some(h) = holes.iter_mut().find(|h| h.0 == start) {
            h.0 = r.va;
            h.1 += r.need;
        }
    }
}

/// Where each block of a plan landed after [`BinaryEditor::place_plan_scattered`].
///
/// `block_vas[i]` is the scattered virtual address of block `i`; `block_offsets[i]`
/// is that block's offset in the original contiguous blob (a prefix sum of block
/// sizes). Together they map any byte offset `o` in the original blob to its
/// scattered address: find the block `b` with the greatest `block_offsets[b] <= o`,
/// then the address is `block_vas[b] + (o - block_offsets[b])`.
#[derive(Debug, Clone)]
pub struct ScatterPlacement {
    pub symbol: SymbolId,
    pub block_vas: Vec<u64>,
    pub block_offsets: Vec<u64>,
}

impl BinaryEditor {
    /// Construct a new editor over `container` without lifting any
    /// text section. Use this when the only edits are
    /// whole-binary (e.g. [`BinaryState::add_library_dependency`],
    /// [`BinaryState::add_function`]).
    pub fn new(container: &Container) -> Result<Self, TextEditorError> {
        Ok(Self {
            binary: BinaryState {
                container: container.clone(),
                appended: None,
                appended_writable: None,
                section_overrides: Vec::new(),
                pending_appended_init_slots: Vec::new(),
                pending_library_deps: Vec::new(),
                force_segment_placement: false,
                no_new_segments: false,
                macho_raw_byte_overrides: Vec::new(),
                reserved_text_region: None,
                pending_text_grow: None,
            },
            text: None,
        })
    }

    /// Lift the named text section into the [`Self::text`] field
    /// so its instructions are editable. Replaces any
    /// previously-lifted section.
    ///
    /// `name` is matched literally against
    /// `container.sections[i].name`. The section must be a
    /// [`SectionKind::Text`](crate::container::SectionKind::Text);
    /// non-text sections return [`TextEditorError::SectionNotText`].
    pub fn lift_text_section(&mut self, name: &str) -> Result<(), TextEditorError> {
        let section = self
            .binary
            .container
            .sections
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| TextEditorError::SectionNotFound(name.to_string()))?;

        let (base, code) = section
            .for_disassembly()
            .ok_or_else(|| TextEditorError::SectionNotText {
                name: name.to_string(),
            })?;
        let section_id = section.id;

        // Dispatch on container architecture. The IR is generic
        // over `Isa`; per-ISA disassembly + CFG construction
        // happens here, then the wrapped section is stored in
        // the enum variant matching the architecture.
        let lifted = match self.binary.container.architecture {
            Architecture::Aarch64 => {
                let instructions = aarch64::disassemble_bytes(base, code)?;
                let cfg = build_cfg(&instructions);
                let plan =
                    RewritePlanGeneric::<Aarch64Isa>::lift_from_decoded_with_container(
                        &cfg,
                        &instructions,
                        &self.binary.container,
                    );
                LiftedTextSectionAny::Aarch64(LiftedTextSection {
                    section_id,
                    base_address: base,
                    instructions,
                    cfg,
                    plan,
                    overlays: Vec::new(),
                })
            }
            Architecture::Arm => {
                // ARMv7 container — Thumb-2 is the more common
                // case for production code; default to Thumb
                // sweep. Callers needing ARM-mode (A32) text
                // lift should use [`Self::lift_text_section_arm`].
                let instructions =
                    crate::isa::armv7::sweep::disassemble_bytes(base, code)
                        .map_err(thumb_disassemble_error_to_text_editor)?;
                let cfg = build_cfg(&instructions);
                let plan = RewritePlanGeneric::<ThumbIsa>::lift_from_decoded_with_container(
                    &cfg,
                    &instructions,
                    &self.binary.container,
                );
                LiftedTextSectionAny::Thumb(LiftedTextSection {
                    section_id,
                    base_address: base,
                    instructions,
                    cfg,
                    plan,
                    overlays: Vec::new(),
                })
            }
            Architecture::X86_64 | Architecture::X86 => {
                let bitness = x86::bitness_for_architecture(self.binary.container.architecture)
                    .expect("x86 architecture maps to a bitness");
                let decoded = x86::disassemble_bytes(base, code, bitness)
                    .map_err(|e| TextEditorError::DisassembleX86(e.to_string()))?;
                let instructions = decoded.into_iter().map(|d| d.instr).collect();
                LiftedTextSectionAny::X86(X86LiftedTextSection {
                    section_id,
                    base_address: base,
                    bitness,
                    instructions,
                    patches: Vec::new(),
                })
            }
            Architecture::Other => {
                return Err(TextEditorError::SectionNotText {
                    name: format!(
                        "{name} (container architecture is unsupported)"
                    ),
                });
            }
        };
        self.text = Some(lifted);
        Ok(())
    }

    /// AArch64 tolerant lift: like [`Self::lift_text_section`], but words the
    /// decoder doesn't recognise become placeholder `NOP`s (the lift never fails
    /// on an unsupported encoding), and their addresses are returned. The caller
    /// MUST NOT rewrite any function overlapping a returned address — the
    /// placeholders don't round-trip, so those functions must be left verbatim.
    /// Non-AArch64 containers fall back to the strict [`Self::lift_text_section`]
    /// and return an empty list (other ISAs have no tolerant sweep yet).
    pub fn lift_text_section_tolerant(
        &mut self,
        name: &str,
    ) -> Result<Vec<(u64, u32)>, TextEditorError> {
        if !matches!(self.binary.container.architecture, Architecture::Aarch64) {
            self.lift_text_section(name)?;
            return Ok(Vec::new());
        }
        let section = self
            .binary
            .container
            .sections
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| TextEditorError::SectionNotFound(name.to_string()))?;
        let (base, code) = section
            .for_disassembly()
            .ok_or_else(|| TextEditorError::SectionNotText { name: name.to_string() })?;
        let section_id = section.id;

        let (instructions, undecodable) = aarch64::disassemble_bytes_tolerant(base, code)?;
        let cfg = build_cfg(&instructions);
        let plan = RewritePlanGeneric::<Aarch64Isa>::lift_from_decoded_with_container(
            &cfg,
            &instructions,
            &self.binary.container,
        );
        self.text = Some(LiftedTextSectionAny::Aarch64(LiftedTextSection {
            section_id,
            base_address: base,
            instructions,
            cfg,
            plan,
            overlays: Vec::new(),
        }));
        Ok(undecodable)
    }

    /// Prepare an x86/x86-64 text section for **byte-patch mode**: record its
    /// id/base/bitness WITHOUT disassembling it (so a real `.text` with embedded
    /// data/jump-tables doesn't trip the fail-fast sweep). The section is written
    /// VERBATIM at commit, with only the recorded patches
    /// ([`X86LiftedTextSection::add_trampoline`]) applied in place — the model for
    /// relocating a function to the appended segment + trampolining its entry,
    /// leaving the rest of `.text` (and its RIP-relative refs) untouched.
    pub fn prepare_text_patch(&mut self, name: &str) -> Result<(), TextEditorError> {
        if !matches!(
            self.binary.container.architecture,
            Architecture::X86_64 | Architecture::X86
        ) {
            return Err(TextEditorError::SectionNotText {
                name: format!("{name} (prepare_text_patch requires an x86 container)"),
            });
        }
        let section = self
            .binary
            .container
            .sections
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| TextEditorError::SectionNotFound(name.to_string()))?;
        let (base, _code) = section
            .for_disassembly()
            .ok_or_else(|| TextEditorError::SectionNotText { name: name.to_string() })?;
        let bitness = x86::bitness_for_architecture(self.binary.container.architecture)
            .expect("x86 architecture maps to a bitness");
        self.text = Some(LiftedTextSectionAny::X86(X86LiftedTextSection {
            section_id: section.id,
            base_address: base,
            bitness,
            instructions: Vec::new(),
            patches: Vec::new(),
        }));
        Ok(())
    }

    /// Lift the named text section as ARM-mode (A32) rather
    /// than Thumb-2. Use this for sections (most commonly the
    /// PLT) that the linker emits as A32 even in an
    /// otherwise-Thumb binary. Errors if the container's
    /// architecture isn't ARMv7.
    pub fn lift_text_section_arm(&mut self, name: &str) -> Result<(), TextEditorError> {
        if !matches!(self.binary.container.architecture, Architecture::Arm) {
            return Err(TextEditorError::SectionNotText {
                name: format!("{name} (lift_text_section_arm requires ARMv7 container)"),
            });
        }
        let section = self
            .binary
            .container
            .sections
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| TextEditorError::SectionNotFound(name.to_string()))?;
        let (base, code) = section
            .for_disassembly()
            .ok_or_else(|| TextEditorError::SectionNotText {
                name: name.to_string(),
            })?;
        let section_id = section.id;
        let instructions =
            crate::isa::armv7::arm::sweep::disassemble_bytes(base, code)
                .map_err(arm_disassemble_error_to_text_editor)?;
        let cfg = build_cfg(&instructions);
        let plan = RewritePlanGeneric::<ArmIsa>::lift_from_decoded_with_container(
            &cfg,
            &instructions,
            &self.binary.container,
        );
        self.text = Some(LiftedTextSectionAny::Arm(LiftedTextSection {
            section_id,
            base_address: base,
            instructions,
            cfg,
            plan,
            overlays: Vec::new(),
        }));
        Ok(())
    }

    /// Convenience constructor: build a new [`BinaryEditor`] and
    /// lift the named text section in one step. Equivalent to
    /// `BinaryEditor::new(container)?.lift_text_section(name)?`.
    /// Preserved for callers migrating from the old `TextEditor::for_section`.
    pub fn for_section(container: &Container, name: &str) -> Result<Self, TextEditorError> {
        let mut editor = Self::new(container)?;
        editor.lift_text_section(name)?;
        Ok(editor)
    }

    /// Like [`Self::for_section`], but tolerant of undecodable instructions
    /// (AArch64 only): words the decoder doesn't recognise are lifted as
    /// placeholder `NOP`s and their `(address, word)` returned, instead of failing
    /// the whole lift. The caller MUST avoid rewriting any function that overlaps a
    /// returned address — those placeholders don't round-trip, so the function's
    /// original bytes must be left verbatim. For non-AArch64 containers this is
    /// identical to [`Self::for_section`] and always returns an empty list.
    pub fn for_section_tolerant(
        container: &Container,
        name: &str,
    ) -> Result<(Self, Vec<(u64, u32)>), TextEditorError> {
        let mut editor = Self::new(container)?;
        let undecodable = editor.lift_text_section_tolerant(name)?;
        Ok((editor, undecodable))
    }

    /// Place raw `data` (e.g. a constant pool or encrypted blob) into a freed
    /// `.text` hole instead of appending: best-fit `data.len()` bytes from
    /// `holes` (each `(vaddr, size)`), overlay the bytes there, and define an
    /// Object symbol at that address. Returns the symbol, or `None` (the hole
    /// list unchanged) when nothing fits — the caller should append instead.
    /// ISA-agnostic (works on any lifted text section, including x86 patch mode).
    pub fn place_data_in_holes(
        &mut self,
        name: &str,
        data: &[u8],
        holes: &mut Vec<(u64, u64)>,
    ) -> Option<SymbolId> {
        let Some(va) = take_hole(holes, data.len() as u64) else {
            return None;
        };
        let text = self.text.as_mut()?;
        let section = text.section_id();
        text.overlay_bytes(va, data.to_vec());
        Some(self.binary.define_text_symbol(name, va, data.len() as u64, false, section))
    }

    /// Place a position-independent code `plan` into a freed `.text` hole:
    /// best-fit it, emit at the hole's address, overlay the bytes, and define a
    /// Function symbol there. Returns `None` (holes unchanged) if it doesn't fit
    /// or would require a relocation (which an overlay can't carry) — the caller
    /// appends instead. Generic over the plan's [`Isa`]; the overlay routes
    /// through the lifted section regardless of ISA (x86 lands it as a byte-patch).
    pub fn place_plan_in_holes<I: Isa>(
        &mut self,
        name: &str,
        plan: &RewritePlanGeneric<I>,
        holes: &mut Vec<(u64, u64)>,
    ) -> Result<Option<SymbolId>, TextEditorError> {
        // Size the blob from a layout alone (no base-0 emit): `lay_out_for_size`
        // tolerates out-of-range unconditional branches, so a `bl`/`b` to a distant
        // callee doesn't spuriously fail the sizing (that branch might well be in
        // range once the blob is placed in a hole near its target). `total_size` is
        // the byte count including any conditional-branch widening.
        let size = lay_out_for_size::<I>(plan, 0, Some(&self.binary.container))?.total_size;
        // Reserve a hole without disturbing the pool on failure. `take_hole` mutates
        // in place; to keep this O(1) per call (a clone would be O(holes) → O(n²)
        // across a whole-binary run), we record the reservation and only roll it
        // back on the rare fallback path.
        let Some((va, reservation)) = reserve_hole(holes, size) else {
            return Ok(None);
        };
        // Authoritative placement at the chosen hole VA: layout (range check) then
        // emit (encode + relocation check). If a branch is genuinely out of range
        // from here, or a reference can't be folded, fall back (`Ok(None)`) so the
        // caller tries scattered / appended placement rather than aborting.
        let layout = match lay_out::<I>(plan, va, Some(&self.binary.container)) {
            Ok(l) => l,
            Err(LayoutError::DisplacementTooLarge { .. }) | Err(LayoutError::DidNotConverge) => {
                release_hole(holes, reservation);
                return Ok(None);
            }
            Err(other) => {
                release_hole(holes, reservation);
                return Err(other.into());
            }
        };
        let output = match emit::<I>(plan, &layout, Some(&self.binary.container)) {
            Ok(o) if o.relocations.is_empty() => o,
            // Blob emits a relocation we can't place, OR an out-of-range branch/adrp
            // the layout's coarser check let through — fall back either way.
            Ok(_) | Err(EmitError::Encode(_)) => {
                release_hole(holes, reservation);
                return Ok(None);
            }
            Err(other) => {
                release_hole(holes, reservation);
                return Err(TextEditorError::Emit(other));
            }
        };
        let text = self
            .text
            .as_mut()
            .ok_or_else(|| TextEditorError::SectionNotFound("<no lifted .text>".into()))?;
        let section = text.section_id();
        text.overlay_bytes(va, output.bytes);
        Ok(Some(self.binary.define_text_symbol(name, va, size, true, section)))
    }

    /// Place a position-independent AArch64 code `plan` SCATTERED across several
    /// freed `.text` holes: assign each basic block to its own best-fit hole,
    /// lay out at the fixed per-block addresses, emit, and overlay each block's
    /// bytes at its hole. The blocks stay correct because they are connected by
    /// the plan's own inter-block branches; this only works when every such
    /// branch lands in range (PC-relative `b`/`b.cond`) and the plan emits no
    /// relocations. Returns `None` (holes unchanged) if it cannot be placed this
    /// way — the caller should try a whole-hole placement or append instead.
    ///
    /// Requires every block to be non-empty (a branch to an empty block has no
    /// well-defined scattered address); plans with an empty block fall back.
    ///
    /// On success returns a [`ScatterPlacement`] describing where each block
    /// landed (its scattered VA plus its contiguous 0-based offset), so callers
    /// that need per-instruction addresses (e.g. a computed-goto dispatch table)
    /// can map a byte offset in the original blob to its scattered address.
    pub fn place_plan_scattered<I: Isa>(
        &mut self,
        name: &str,
        plan: &RewritePlanGeneric<I>,
        holes: &mut Vec<(u64, u64)>,
    ) -> Result<Option<ScatterPlacement>, TextEditorError> {
        if plan.blocks.is_empty() {
            return Ok(None);
        }
        // Per-block byte size = sum of each op's source byte size. lay_out_scattered
        // forbids widening, so emitted block sizes match these exactly.
        let block_sizes: Vec<u64> = plan
            .blocks
            .iter()
            .map(|b| b.ops.iter().map(|op| op.source_byte_size()).sum())
            .collect();
        if block_sizes.iter().any(|&s| s == 0) {
            return Ok(None);
        }
        // Tentatively allocate each block into a hole using a clone of the pool,
        // so a partial failure leaves the caller's holes untouched.
        let mut tentative = holes.clone();
        let mut addrs: Vec<u64> = Vec::with_capacity(block_sizes.len());
        for &sz in &block_sizes {
            match take_hole(&mut tentative, sz) {
                Some(va) => addrs.push(va),
                None => return Ok(None),
            }
        }
        let layout = match lay_out_scattered::<I>(plan, &addrs, Some(&self.binary.container)) {
            Ok(l) => l,
            Err(LayoutError::DisplacementTooLarge { .. }) | Err(LayoutError::DidNotConverge) => {
                return Ok(None);
            }
            Err(other) => return Err(other.into()),
        };
        let output = emit::<I>(plan, &layout, Some(&self.binary.container))?;
        if !output.relocations.is_empty() {
            return Ok(None);
        }
        // Commit the hole allocation now that placement is guaranteed.
        *holes = tentative;
        let total: u64 = block_sizes.iter().sum();
        let text = self
            .text
            .as_mut()
            .ok_or_else(|| TextEditorError::SectionNotFound("<no lifted .text>".into()))?;
        let section = text.section_id();
        // emit() produces block bytes in block order, contiguous per block.
        let mut off = 0usize;
        let mut block_offsets: Vec<u64> = Vec::with_capacity(block_sizes.len());
        for (bi, &sz) in block_sizes.iter().enumerate() {
            block_offsets.push(off as u64);
            let end = off + sz as usize;
            text.overlay_bytes(addrs[bi], output.bytes[off..end].to_vec());
            off = end;
        }
        let symbol = self.binary.define_text_symbol(name, addrs[0], total, true, section);
        Ok(Some(ScatterPlacement { symbol, block_vas: addrs, block_offsets }))
    }

    /// Iterate over symbols defined in the lifted text section.
    /// Useful for "edit every function in `.text`" workflows.
    /// Panics if no text section has been lifted.
    pub fn symbols_in_section(&self) -> impl Iterator<Item = &Symbol> + '_ {
        let id = self
            .text
            .as_ref()
            .expect("lift_text_section before symbols_in_section")
            .section_id();
        self.binary
            .container
            .symbols
            .iter()
            .filter(move |s| s.section == Some(id) && !s.is_undefined)
    }
}

impl BinaryState {
    /// Define a local symbol at `addr` in `section` (Function if `func`, else
    /// Object) and return its id. Used to label artifacts placed into `.text`
    /// holes so other edits can reference them by symbol.
    fn define_text_symbol(
        &mut self,
        name: &str,
        addr: u64,
        size: u64,
        func: bool,
        section: SectionId,
    ) -> SymbolId {
        let id = SymbolId(self.container.symbols.len());
        self.container.symbols.push(crate::container::Symbol {
            id,
            name: name.to_string(),
            address: addr,
            size,
            kind: if func {
                SymbolKind::Function
            } else {
                SymbolKind::Object
            },
            binding: crate::container::SymbolBinding::Local,
            section: Some(section),
            is_undefined: false,
            flags: None,
        });
        id
    }

    /// Look up a symbol by name. Returns the first defined or
    /// undefined symbol whose name matches exactly.
    ///
    /// Useful to translate "function name in source" into the
    /// [`Target::Symbol`] form the rewriter wants. Many platforms
    /// prefix C symbols (e.g., Mach-O underscoring); the lookup
    /// is exact-match — pass the symbol name as it appears in the
    /// container's symbol table.
    pub fn symbol_by_name(&self, name: &str) -> Result<SymbolId, TextEditorError> {
        self.container
            .symbols
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.id)
            .ok_or_else(|| TextEditorError::SymbolNotFound(name.to_string()))
    }

    /// Address of `id`'s symbol after any layout the editor
    /// has performed up to this point. Used by callers that
    /// `add_function` / `add_initialiser` and need to know
    /// where the new code landed (e.g. to record a runtime
    /// trap address derived from the body's vaddr).
    pub fn symbol_address(&self, id: SymbolId) -> u64 {
        self.container.symbol(id).address
    }

    /// Look up a function symbol by name. Stricter than
    /// [`Self::symbol_by_name`]: only matches `STT_FUNC`-kind
    /// symbols. Useful when redirecting calls to disambiguate
    /// from same-named data symbols (rare but possible on some
    /// platforms).
    pub fn function_by_name(&self, name: &str) -> Result<SymbolId, TextEditorError> {
        self.container
            .symbols
            .iter()
            .find(|s| s.name == name && s.kind == SymbolKind::Function)
            .map(|s| s.id)
            .ok_or_else(|| TextEditorError::SymbolNotFound(name.to_string()))
    }

    /// Address of the *defined* function symbol with this name,
    /// if any. Skips undefined / extern symbols (which have no
    /// address in this binary). For binaries with at most one
    /// defined `STT_FUNC` per name (the common case) this is the
    /// natural "where does foo live?" lookup.
    pub fn function_address(&self, name: &str) -> Option<u64> {
        self.container
            .symbols
            .iter()
            .find(|s| {
                s.name == name && s.kind == SymbolKind::Function && !s.is_undefined
            })
            .map(|s| s.address)
    }
}

impl<I: Isa> LiftedTextSection<I> {
    /// Direct access to the underlying [`RewritePlan`]. Use this
    /// when you need to inspect or mutate the plan beyond what
    /// the editor's proxy methods expose.
    pub fn plan_mut(&mut self) -> &mut RewritePlanGeneric<I> {
        &mut self.plan
    }

    /// Read-only view of the [`RewritePlan`].
    pub fn plan(&self) -> &RewritePlanGeneric<I> {
        &self.plan
    }

    /// Decoded instructions for the section, in source order.
    /// Useful when you need to know what's at a given address
    /// before deciding how to rewrite it.
    pub fn instructions(&self) -> &[I::DecodedInstruction] {
        &self.instructions
    }

    /// Control-flow graph for the section. Useful when an edit
    /// needs to reason about basic-block boundaries (e.g.,
    /// inserting at the end of a particular block).
    pub fn cfg(&self) -> &ControlFlowGraph {
        &self.cfg
    }

    /// Base virtual address of the edited section.
    pub fn base_address(&self) -> u64 {
        self.base_address
    }

    /// Overlay `bytes` onto the section at virtual address `va`, applied after
    /// layout/emit at commit time (so it overwrites whatever the plan emitted
    /// there). Use it to place pre-assembled, position-independent code or data
    /// into a freed region of this section — e.g. reusing the hole left by a
    /// destroyed function body instead of appending a new segment. The caller is
    /// responsible for keeping `[va, va+len)` inside the section and not
    /// overlapping live instructions or other overlays.
    pub fn overlay_bytes(&mut self, va: u64, bytes: Vec<u8>) {
        self.overlays.push((va, bytes));
    }

    /// The section id this view edits. Lets callers correlate
    /// with [`BinaryState::container`] for cross-cutting work.
    pub fn section_id(&self) -> SectionId {
        self.section_id
    }

    /// Redirect the branch instruction at `address` to a new
    /// target. Convenience proxy for [`RewritePlan::redirect_branch`].
    ///
    /// `address` is the absolute virtual address of the branch (as
    /// it appears in the disassembly); for ET_DYN that's the
    /// section's source `sh_addr` plus the in-section offset.
    pub fn redirect_branch_at(
        &mut self,
        address: u64,
        new_target: Target,
    ) -> Result<(), TextEditorError> {
        self.plan.redirect_branch(address, new_target)?;
        Ok(())
    }

    /// Redirect the symbolic target of the macro op at `address`.
    /// Convenience proxy for [`RewritePlan::redirect_macro_target`].
    pub fn redirect_macro_target_at(
        &mut self,
        address: u64,
        new_target: Target,
    ) -> Result<(), TextEditorError> {
        self.plan.redirect_macro_target(address, new_target)?;
        Ok(())
    }

    /// Replace the instruction at `address` with a new one. The
    /// new instruction is provided as a [`RewriteInstruction`] so
    /// callers can express symbolic operands directly.
    pub fn replace_instruction_at(
        &mut self,
        address: u64,
        new_instruction: RewriteInstructionGeneric<I>,
    ) -> Result<(), TextEditorError> {
        let target = self
            .plan
            .instruction_at_mut(address)
            .ok_or(EditError::AddressNotFound(address))?;
        *target = new_instruction;
        Ok(())
    }

    /// Insert one or more instructions immediately after the op at
    /// `address`. Proxy for [`RewritePlan::insert_after_address`].
    pub fn insert_after_address(
        &mut self,
        address: u64,
        new_instructions: Vec<RewriteInstructionGeneric<I>>,
    ) -> Result<(), TextEditorError> {
        self.plan.insert_after_address(address, new_instructions)?;
        Ok(())
    }

    /// Remove the op at `address`. Proxy for
    /// [`RewritePlan::remove_at_address`]. The removed op is
    /// dropped; callers who need it should hold a clone first.
    pub fn remove_at_address(&mut self, address: u64) -> Result<(), TextEditorError> {
        self.plan.remove_at_address(address)?;
        Ok(())
    }
}

impl BinaryState {
    /// Promise that no operation in this commit will create
    /// a new segment. Once set, any call that would require
    /// adding (or expanding into) a fresh `__APPENDED`-style
    /// segment fails immediately with
    /// [`TextEditorError::WouldCreateNewSegment`].
    ///
    /// Required for iOS / App Store submissions: dylibs with
    /// more than one R-X segment are rejected during review.
    /// On Mach-O, calling this enforces:
    ///
    /// - `add_function` / `add_data` / `add_initialiser`
    ///   payloads must fit in `__TEXT` free regions (gaps
    ///   between sections + segment-tail padding).
    /// - `add_function_exported` errors at queue time —
    ///   rebuilding the export trie / symtab / strtab needs
    ///   `__LINKEDIT`-shifting which only the
    ///   `__APPENDED`-segment writer provides.
    /// - `add_library_dependency` errors at queue time —
    ///   LC_LOAD_DYLIB injection uses the same writer path.
    ///
    /// On ELF the flag is silently ignored. ELF's
    /// append-PT_LOAD is the standard pattern for runtime
    /// rewriting and there's no analogue of App Store gating.
    ///
    /// The flag is one-way: once set, it stays set for the
    /// lifetime of the editor. Callers who want a permissive
    /// editor should construct a fresh one.
    pub fn prohibit_new_segments(&mut self) {
        self.no_new_segments = true;
    }

    /// Stage a freshly-serialised `LC_DYLD_CHAINED_FIXUPS`
    /// blob to be written into the output at commit time.
    ///
    /// The new blob is placed at the existing load command's
    /// `dataoff`, overwriting the original. The blob must fit
    /// in the existing data range — i.e. `blob.len() <=
    /// LC_DYLD_CHAINED_FIXUPS.datasize`. Smaller blobs are
    /// fine: the tail is zero-padded by the serialiser.
    /// Larger blobs return
    /// [`TextEditorError::ChainedFixupsBlobTooLarge`].
    ///
    /// The companion in-segment chain entries (the on-disk
    /// pointer slots themselves) must be rewritten separately
    /// by the caller via
    /// [`crate::container::ChainedFixups::encode_into_segments`],
    /// applied as section-byte overrides on the segments whose
    /// fixup slots changed. The blob and the slots have to
    /// match for dyld to accept the binary.
    ///
    /// Mach-O only — ELF doesn't have chained fixups.
    pub fn commit_chained_fixups(
        &mut self,
        blob: &[u8],
    ) -> Result<(), TextEditorError> {
        let image = self
            .container
            .macho_image
            .as_ref()
            .ok_or(TextEditorError::NoChainedFixupsCommand)?;
        let cf = image
            .layout
            .chained_fixups
            .ok_or(TextEditorError::NoChainedFixupsCommand)?;
        if blob.len() as u64 > cf.datasize {
            return Err(TextEditorError::ChainedFixupsBlobTooLarge {
                new_size: blob.len() as u64,
                capacity: cf.datasize,
            });
        }
        // Pad with zeros up to the original datasize so the
        // unused tail bytes don't carry stale chain data.
        let mut padded = blob.to_vec();
        padded.resize(cf.datasize as usize, 0);
        self.macho_raw_byte_overrides.push((cf.dataoff, padded));
        Ok(())
    }

    /// Stage a verbatim byte override at an absolute file
    /// offset in the Mach-O output. Lower-level than
    /// [`Self::commit_chained_fixups`]; the caller is
    /// responsible for ensuring the bytes are well-formed and
    /// fit at the offset. Used to splice rewritten on-disk
    /// pointer slots (the chain entries themselves) into the
    /// segment bytes after editing a `ChainedFixups`.
    pub fn stage_macho_raw_bytes(&mut self, file_offset: u64, bytes: Vec<u8>) {
        self.macho_raw_byte_overrides.push((file_offset, bytes));
    }

    /// Commit a modified [`crate::container::ChainedFixups`]
    /// in one call: serialises the blob, stages it via
    /// [`Self::commit_chained_fixups`], and *also* stages
    /// every rewritten on-disk pointer slot at its file
    /// offset via [`Self::stage_macho_raw_bytes`]. The two
    /// pieces (chain starts in the blob, chain entries in the
    /// segment bytes) must match for dyld to accept the
    /// image; this method keeps them coherent.
    ///
    /// Use this in preference to manually calling
    /// `cf.serialize()` + `commit_chained_fixups()` +
    /// `cf.encode_into_segments()` — the manual sequence is
    /// what this method runs internally.
    ///
    /// Mach-O only.
    pub fn commit_chained_fixups_full(
        &mut self,
        cf: &crate::container::ChainedFixups,
    ) -> Result<(), TextEditorError> {
        let image = self
            .container
            .macho_image
            .as_ref()
            .ok_or(TextEditorError::NoChainedFixupsCommand)?;
        let segments = image.layout.segments.clone();
        // 1. Re-encode chain slots.
        let slot_bytes = cf
            .slot_bytes(&segments)
            .map_err(|e| TextEditorError::ContainerWrite(e.into()))?;
        // 2. Serialise the blob.
        let ser = cf
            .serialize(&segments)
            .map_err(|e| TextEditorError::ContainerWrite(e.into()))?;
        // 3. Stage the blob (size-check happens inside).
        self.commit_chained_fixups(&ser.bytes)?;
        // 4. Stage every slot byte range. Each slot is 8
        //    bytes; the writer's raw-byte overrides happily
        //    handle thousands of small entries.
        for (file_off, raw) in slot_bytes {
            self.stage_macho_raw_bytes(file_off, raw.to_vec());
        }
        Ok(())
    }

    /// Append `payload` to the trailing padding of a Mach-O
    /// segment, returning the vaddr where the bytes will land.
    /// The bytes are staged via [`Self::stage_macho_raw_bytes`]
    /// and committed on the next `commit_to_bytes*` call.
    ///
    /// Useful for adding small payloads (e.g. new entries to
    /// `__objc_classname` / `__objc_methname` when renaming to
    /// a longer string) without growing the segment or adding
    /// a fresh `LC_SEGMENT_64`. The caller is responsible for
    /// updating any references to point at the returned vaddr
    /// — this method only reserves the bytes.
    ///
    /// Returns
    /// [`TextEditorError::ChainedFixupsBlobTooLarge`]-style
    /// error if no padding region in `segname` can hold
    /// `payload.len()` bytes; the caller should fall back to
    /// the appended-segment writer path (via existing
    /// machinery like `add_function`) for larger payloads.
    ///
    /// Mach-O only.
    pub fn append_macho_segment_padding(
        &mut self,
        segname: &str,
        payload: &[u8],
    ) -> Result<u64, TextEditorError> {
        let image = self
            .container
            .macho_image
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        let regions = image.layout.segment_free_regions(segname);
        // Best-fit: smallest region that holds the payload.
        let region = regions
            .iter()
            .filter(|r| r.size >= payload.len() as u64)
            .min_by_key(|r| r.size)
            .ok_or_else(|| TextEditorError::ChainedFixupsBlobTooLarge {
                new_size: payload.len() as u64,
                capacity: regions.iter().map(|r| r.size).max().unwrap_or(0),
            })?;
        let vaddr = region.vaddr;
        let file_offset = region.file_offset;
        self.macho_raw_byte_overrides
            .push((file_offset, payload.to_vec()));
        Ok(vaddr)
    }

    /// Check whether placing `additional_bytes` in the
    /// existing intra-`__TEXT` free region (the one already
    /// chosen via [`Self::pick_append_vaddr`]) would still
    /// fit. Used by `add_function` / `add_data` /
    /// `add_initialiser` in strict mode to fail fast when a
    /// payload outgrows the available padding instead of
    /// silently overflowing into adjacent sections.
    ///
    /// No-op when not in strict mode, when format isn't
    /// Mach-O, or when the appended segment isn't intra-text
    /// (i.e. a segment fallback is already in play).
    fn check_intra_text_capacity(
        &self,
        additional_bytes: u64,
        op_name: &str,
    ) -> Result<(), TextEditorError> {
        if !self.no_new_segments {
            return Ok(());
        }
        if !matches!(self.container.format, crate::container::BinaryFormat::Macho) {
            return Ok(());
        }
        let Some(appended) = &self.appended else {
            return Ok(());
        };
        let Some(image) = self.container.macho_image.as_ref() else {
            return Ok(());
        };
        let regions = image.layout.text_free_regions();
        // Find the region the segment_vaddr lives in.
        let Some(region) = regions
            .iter()
            .find(|r| {
                appended.segment_vaddr >= r.vaddr
                    && appended.segment_vaddr < r.vaddr + r.size
            })
        else {
            // Not intra-text — segment fallback already
            // active; that's an internal inconsistency in
            // strict mode but we don't error here (caller
            // will hit it elsewhere).
            return Ok(());
        };
        let used = appended.segment_vaddr - region.vaddr + appended.bytes.len() as u64;
        if used + additional_bytes > region.size {
            return Err(TextEditorError::WouldCreateNewSegment {
                reason: format!(
                    "{op_name} ({additional_bytes} bytes) exceeds the \
                     available __TEXT free region (used {used} of {} \
                     bytes); growing into a new segment is forbidden \
                     by prohibit_new_segments()",
                    region.size,
                ),
            });
        }
        Ok(())
    }

    /// Pick a virtual address for a fresh appended segment.
    /// Page-aligned past the highest existing mapped vaddr.
    /// Format-aware: ELF reads PT_LOAD entries from the
    /// captured `elf_image`; Mach-O reads `LC_SEGMENT_64`
    /// entries from `macho_image.layout`.
    fn pick_append_vaddr(&self) -> Result<u64, TextEditorError> {
        match self.container.format {
            crate::container::BinaryFormat::Elf => {
                let image = self
                    .container
                    .elf_image
                    .as_ref()
                    .ok_or(TextEditorError::AppendMissingElfImage)?;
                let mut floor = image
                    .program_headers
                    .iter()
                    .filter(|p| p.p_type == object::elf::PT_LOAD)
                    .map(|p| p.p_vaddr.saturating_add(p.p_memsz))
                    .max()
                    .unwrap_or(0);
                // Sit above any already-placed appended region so the
                // code and writable segments never overlap (whichever
                // was allocated first keeps the lower vaddr).
                if let Some(a) = &self.appended {
                    floor = floor.max(a.segment_vaddr + a.bytes.len() as u64);
                }
                if let Some(w) = &self.appended_writable {
                    floor = floor.max(w.segment_vaddr + w.bytes.len() as u64);
                }
                const PAGE: u64 = 0x10000;
                let aligned = (floor + PAGE - 1) & !(PAGE - 1);
                Ok(aligned.max(PAGE))
            }
            crate::container::BinaryFormat::Macho => {
                let image = self
                    .container
                    .macho_image
                    .as_ref()
                    .ok_or(TextEditorError::AppendMissingElfImage)?;
                // Prefer placing into a free region inside the
                // existing `__TEXT` segment so the output
                // doesn't grow a new R-X segment (App Store
                // submissions reject dylibs / executables with
                // more than one R-X segment).
                //
                // Bypass when `force_segment_placement` is
                // set: callers that need `__LINKEDIT`-shifting
                // metadata writes (`add_function_exported`,
                // `add_library_dependency`) flag this before
                // any byte placement so the layout matches
                // what the segment writer expects.
                //
                // We don't know the eventual payload size at
                // `pick_append_vaddr` time — `add_function` /
                // `add_data` are called incrementally. Pick
                // the start of the largest free region as a
                // hint; the commit-time path validates the
                // accumulated size still fits.
                if !self.force_segment_placement {
                    if let Some((vaddr, _file_offset)) =
                        image.layout.allocate_in_text(4, 4)
                    {
                        return Ok(vaddr);
                    }
                }
                // Strict mode: bail rather than fall through
                // to the segment fallback, which would create
                // an `__APPENDED` segment.
                if self.no_new_segments {
                    return Err(TextEditorError::WouldCreateNewSegment {
                        reason: "Mach-O `__TEXT` has no free region big \
                                 enough for the requested append, and \
                                 prohibit_new_segments() forbids creating \
                                 a new `__APPENDED` segment"
                            .into(),
                    });
                }
                // No __TEXT free region — fall back to the
                // appended-segment path. Output will have an
                // extra R-X `__APPENDED` segment, which works
                // for macOS but won't pass App Store review.
                let max_input_vaddr = image.layout.max_vaddr_end();
                const PAGE: u64 = 0x4000; // arm64 macOS page size
                const GAP: u64 = 16 * PAGE;
                let aligned = (max_input_vaddr + GAP + PAGE - 1) & !(PAGE - 1);
                Ok(aligned.max(PAGE))
            }
            // PE append-segment placement is not implemented yet.
            crate::container::BinaryFormat::Pe => {
                Err(TextEditorError::AppendUnsupportedKind(self.container.kind))
            }
        }
    }

    /// Append a new function to the binary in a fresh executable
    /// segment.
    ///
    /// The new function lives in a freshly-allocated PT_LOAD R-X
    /// segment placed beyond the input's mapped range. It gets a
    /// container [`Symbol`] with `kind = SymbolKind::Function`,
    /// returned as a `SymbolId` so callers can immediately retarget
    /// existing branches at it via
    /// [`Self::redirect_branch_at`].
    ///
    /// Use this to add new functionality to a shared library
    /// without having to grow the original `.text`. Typical
    /// pattern: `add_function` to introduce a "decorator" body,
    /// then `redirect_branch_at` to make existing call sites
    /// invoke it instead of (or in addition to) the original
    /// target.
    ///
    /// `instructions` are laid out and emitted at the new
    /// function's assigned virtual address using the existing
    /// rewriter pipeline, so PC-relative operands inside the new
    /// function resolve correctly. Symbolic targets
    /// ([`Target::Symbol`], [`Target::Block`]) referencing other
    /// items in the container resolve at emit time the same way
    /// as for an in-section edit.
    ///
    /// ## Limits (first iteration)
    ///
    /// - The new function can call existing intra-library
    ///   functions (folded as PC-relative branches at emit) and
    ///   reference defined symbols within ±128 MiB.
    /// - The new function should not branch into the new segment
    ///   to a target *before* its own start; targets later in the
    ///   segment work as long as the layout has been computed
    ///   (i.e. for inter-function calls, register the callee via
    ///   `add_function` first, then build the caller).
    /// - Calls to undefined externs (e.g. `printf` via the PLT)
    ///   are not yet supported — emit can't yet produce the
    ///   PLT-aware relocation an ET_DYN runtime linker expects.
    // ---- Reserve / carve text-space API -------------------------------
    //
    // A controlled front-end over the same intra-`__TEXT` placement the
    // region-less `add_function` uses. `reserve_text_region` pins a
    // contiguous R-X span (harvested from `__TEXT` free space via the
    // pure `space` allocator) and bounds total placement to its
    // capacity; the `*_in` methods carve functions/data out of it. This
    // increment fulfils reservations from free space only — the
    // `Exhaustion::Grow` path (extend `__TEXT`, shift `__DATA*`) reports
    // `WouldRequireTextGrowth` rather than growing.

    /// Reserve a contiguous R-X region inside `__TEXT`'s free space.
    ///
    /// Harvests the segment's inter-section holes and tail pad, picks a
    /// span via [`space::plan_reservation`], and returns a
    /// [`Reservation`] you can carve out of. Fails (rather than moving
    /// anything) unless `req.on_exhaustion` is [`Exhaustion::Grow`], in
    /// which case it reports [`TextEditorError::WouldRequireTextGrowth`]
    /// (the grow path is a later increment).
    ///
    /// Mach-O only for now. Must run before front/load-command edits
    /// (`add_library_dependency`, exported appends) so the region base
    /// it hands out can't be invalidated by `__text` moving.
    pub fn reserve_text_region(
        &mut self,
        req: crate::rewrite::space::ReserveRequest,
    ) -> Result<Reservation, TextEditorError> {
        if !matches!(self.container.format, crate::container::BinaryFormat::Macho) {
            return Err(TextEditorError::ReserveUnsupportedFormat(self.container.format));
        }
        if !self.pending_library_deps.is_empty() || self.force_segment_placement {
            return Err(TextEditorError::ReserveNeedsFrontEditsFirst);
        }
        if self.reserved_text_region.is_some() || self.appended.is_some() {
            return Err(TextEditorError::TextRegionAlreadyReserved);
        }
        let extents = self.macho_text_free_extents()?;
        let plan = match crate::rewrite::space::plan_reservation(&extents, &req) {
            Ok(plan) => plan,
            // Free space is insufficient and the caller allowed a grow:
            // switch to the uniform-shift grow geometry, which commit
            // routes through build_grown_macho.
            Err(crate::rewrite::space::ReserveError::GrowthRequired { .. }) => {
                let needed = req.min_bytes.saturating_add(req.headroom);
                return self.setup_grow_reservation(needed);
            }
            Err(e) => return Err(e.into()),
        };
        // Seed the appended state at the reserved base so the existing
        // intra-`__TEXT` commit path places the carved content.
        self.appended = Some(AppendedFunctionsState {
            segment_vaddr: plan.base_address,
            bytes: Vec::new(),
            exports: Vec::new(),
            items: Vec::new(),
        });
        let id = RegionId(0);
        self.reserved_text_region = Some((id, plan.region()));
        Ok(Reservation {
            region: id,
            base_address: plan.base_address,
            base_file_offset: plan.base_file_offset,
            capacity: plan.capacity,
            sources: vec![(plan.source, plan.capacity)],
        })
    }

    /// Set up a reservation fulfilled by growing `__TEXT` (uniform-shift)
    /// rather than from free space. The reserved region is the inserted
    /// gap just before `__text`; the pending grow geometry is recorded so
    /// commit routes through [`Self::build_grown_macho`].
    fn setup_grow_reservation(&mut self, needed: u64) -> Result<Reservation, TextEditorError> {
        use crate::rewrite::space::{Region, SpaceSource};
        const PAGE: u64 = 0x4000;
        let (growth_point, base_address) = {
            let image = self
                .container
                .macho_image
                .as_ref()
                .ok_or(TextEditorError::AppendMissingElfImage)?;
            let text = image
                .layout
                .segments
                .iter()
                .find(|s| s.name == "__TEXT")
                .ok_or(TextEditorError::AppendMissingElfImage)?;
            let growth_point = image
                .layout
                .sections
                .iter()
                .filter(|s| s.segname == "__TEXT" && s.file_offset > 0)
                .map(|s| s.file_offset)
                .min()
                .ok_or(TextEditorError::AppendMissingElfImage)?;
            (growth_point, text.vmaddr + (growth_point - text.fileoff))
        };
        let pages = ((needed + PAGE - 1) / PAGE).max(1);
        let delta = pages * PAGE;
        self.pending_text_grow = Some((growth_point, delta));
        self.appended = Some(AppendedFunctionsState {
            segment_vaddr: base_address,
            bytes: Vec::new(),
            exports: Vec::new(),
            items: Vec::new(),
        });
        let id = RegionId(0);
        self.reserved_text_region = Some((
            id,
            Region::new(base_address, growth_point, delta, SpaceSource::GrewText { pages }),
        ));
        Ok(Reservation {
            region: id,
            base_address,
            base_file_offset: growth_point,
            capacity: delta,
            sources: vec![(SpaceSource::GrewText { pages }, delta)],
        })
    }

    /// The address the next `align`-aligned carve in `region` would
    /// land at, without consuming anything. Lets a caller that encodes
    /// its own machine code target a stable address before placing it
    /// with [`Self::add_raw_code_in`].
    pub fn next_address_in(&self, region: RegionId, align: u64) -> Result<u64, TextEditorError> {
        if align == 0 {
            return Err(TextEditorError::InvalidAlignment);
        }
        let (id, r) = self
            .reserved_text_region
            .as_ref()
            .ok_or(TextEditorError::NoSuchRegion)?;
        if *id != region {
            return Err(TextEditorError::NoSuchRegion);
        }
        let cur = r
            .base_address()
            .checked_add(r.used())
            .ok_or(TextEditorError::RegionFull { requested: 0, available: r.remaining() })?;
        let rem = cur % align;
        if rem == 0 {
            Ok(cur)
        } else {
            cur.checked_add(align - rem)
                .ok_or(TextEditorError::RegionFull { requested: 0, available: r.remaining() })
        }
    }

    /// Remaining free bytes in `region`, or `None` if the handle is
    /// stale.
    pub fn region_remaining(&self, region: RegionId) -> Option<u64> {
        self.reserved_text_region
            .as_ref()
            .filter(|(id, _)| *id == region)
            .map(|(_, r)| r.remaining())
    }

    /// Append a new function's (typed) body into `region`. Lays out and
    /// emits at the region's next 4-aligned address, so its internal
    /// branches and PC-relative references resolve against where it
    /// actually lands. Returns the new function's symbol.
    ///
    /// The body may reference *other* already-registered symbols but not
    /// itself (its own symbol is minted after emit) in this increment.
    pub fn add_function_in(
        &mut self,
        region: RegionId,
        name: &str,
        instructions: Vec<RewriteInstruction>,
    ) -> Result<SymbolId, TextEditorError> {
        if instructions.is_empty() {
            return Err(TextEditorError::EmptyFunction(name.to_string()));
        }
        let target_addr = self.next_address_in(region, 4)?;
        // Lay out + emit at the target address (pure — no state change,
        // so a RegionFull carve below rolls nothing back).
        let plan =
            RewritePlanGeneric::<Aarch64Isa>::from_instructions(instructions, Some(&self.container));
        let layout = lay_out(&plan, target_addr, Some(&self.container))?;
        let output = emit(&plan, &layout, Some(&self.container))?;
        let carve = {
            let r = self.active_region_mut(region)?;
            r.carve(output.bytes.len() as u64, 4)?
        };
        debug_assert_eq!(carve.address, target_addr);
        {
            let state = self.appended.as_mut().ok_or(TextEditorError::NoSuchRegion)?;
            state.bytes.resize(carve.offset_in_region as usize, 0);
            state.bytes.extend_from_slice(&output.bytes);
            state.items.push(RetainedAppend::Function {
                name: name.to_string(),
                offset: carve.offset_in_region,
                plan: wrap_retained_plan(plan),
            });
        }
        let symbol_id = SymbolId(self.container.symbols.len());
        self.container.symbols.push(crate::container::Symbol {
            id: symbol_id,
            name: name.to_string(),
            address: carve.address,
            size: output.bytes.len() as u64,
            kind: SymbolKind::Function,
            binding: crate::container::SymbolBinding::Global,
            section: None,
            is_undefined: false,
            flags: None,
        });
        Ok(symbol_id)
    }

    /// Append read-only data into `region`, aligned to `align`. Returns
    /// its symbol (kind `Object`).
    pub fn add_data_in(
        &mut self,
        region: RegionId,
        name: &str,
        bytes: &[u8],
        align: u64,
    ) -> Result<SymbolId, TextEditorError> {
        self.place_raw_in_region(region, name, bytes, align, SymbolKind::Object)
    }

    /// Append pre-encoded raw machine code into `region`, aligned to
    /// `align`. The caller is responsible for having encoded the bytes
    /// against the address [`Self::next_address_in`] reports for the
    /// same `align` — the region places them verbatim and never rebases
    /// a free-space reservation, so a stable target address is
    /// guaranteed. Returns its symbol (kind `Function`).
    pub fn add_raw_code_in(
        &mut self,
        region: RegionId,
        name: &str,
        bytes: &[u8],
        align: u64,
    ) -> Result<SymbolId, TextEditorError> {
        self.place_raw_in_region(region, name, bytes, align, SymbolKind::Function)
    }

    /// Shared raw-bytes placement for [`Self::add_data_in`] /
    /// [`Self::add_raw_code_in`]. Keeps the region cursor and the shared
    /// `appended` buffer in lockstep.
    fn place_raw_in_region(
        &mut self,
        region: RegionId,
        name: &str,
        bytes: &[u8],
        align: u64,
        kind: SymbolKind,
    ) -> Result<SymbolId, TextEditorError> {
        if align == 0 {
            return Err(TextEditorError::InvalidAlignment);
        }
        let carve = {
            let r = self.active_region_mut(region)?;
            r.carve(bytes.len() as u64, align)?
        };
        {
            let state = self.appended.as_mut().ok_or(TextEditorError::NoSuchRegion)?;
            state.bytes.resize(carve.offset_in_region as usize, 0);
            state.bytes.extend_from_slice(bytes);
            state.items.push(RetainedAppend::Data {
                name: name.to_string(),
                offset: carve.offset_in_region,
                bytes: bytes.to_vec(),
            });
        }
        let symbol_id = SymbolId(self.container.symbols.len());
        self.container.symbols.push(crate::container::Symbol {
            id: symbol_id,
            name: name.to_string(),
            address: carve.address,
            size: bytes.len() as u64,
            kind,
            binding: crate::container::SymbolBinding::Global,
            section: None,
            is_undefined: false,
            flags: None,
        });
        Ok(symbol_id)
    }

    /// Look up the active region by handle for a mutable carve.
    fn active_region_mut(
        &mut self,
        region: RegionId,
    ) -> Result<&mut crate::rewrite::space::Region, TextEditorError> {
        match &mut self.reserved_text_region {
            Some((id, r)) if *id == region => Ok(r),
            _ => Err(TextEditorError::NoSuchRegion),
        }
    }

    /// Build the free-space extents the allocator draws on from the
    /// Mach-O `__TEXT` free regions. The highest-address run is the tail
    /// pad; earlier ones are inter-section holes. (`text_free_regions`
    /// skips the header-pad region, so no `HeaderPad` source is offered
    /// yet — that's a later increment.)
    fn macho_text_free_extents(
        &self,
    ) -> Result<Vec<crate::rewrite::space::FreeExtent>, TextEditorError> {
        use crate::rewrite::space::{FreeExtent, SpaceSource};
        let image = self
            .container
            .macho_image
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        let mut regions: Vec<_> = image
            .layout
            .text_free_regions()
            .into_iter()
            .filter(|r| r.segment_name == "__TEXT")
            .collect();
        regions.sort_by_key(|r| r.vaddr);
        let last = regions.len().saturating_sub(1);
        Ok(regions
            .iter()
            .enumerate()
            .map(|(i, r)| FreeExtent {
                address: r.vaddr,
                file_offset: r.file_offset,
                len: r.size,
                source: if i == last {
                    SpaceSource::TailPad
                } else {
                    SpaceSource::InterSectionHole
                },
            })
            .collect())
    }

    /// Build a grown Mach-O under the uniform-shift model *with* the
    /// load-critical chained-fixup `+delta` fixup applied.
    ///
    /// Inserts `delta` pages at `growth_point` (the first `__TEXT`
    /// section offset), places `appended_bytes` at
    /// `appended_file_offset` in the new gap, and — because the whole
    /// image past the insert moves by `delta` — reads the chained
    /// fixups, shifts every slot location and rebase target by `delta`,
    /// and splices the re-encoded slots (into `__DATA*`) and blob (into
    /// `__LINKEDIT`) at their shifted offsets. All PC-relative code is
    /// preserved by the geometry, so no instruction re-encode is needed.
    ///
    /// The result is structurally coherent *and* fixup-correct. Its
    /// runtime proof is the `macho_runtime` harness. Export-trie /
    /// symbol-value `+delta` (tooling correctness, not load-blocking)
    /// are follow-ups.
    pub(crate) fn build_grown_macho(
        &self,
        growth_point: u64,
        delta: u64,
        appended_file_offset: u64,
        appended_bytes: &[u8],
        sign: bool,
    ) -> Result<Vec<u8>, TextEditorError> {
        use crate::container::macho_writer::{write_with_text_growth_opts, MachOWriteOptions};
        let image = self
            .container
            .macho_image
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;

        // Segment list as it will look post-grow: segments at/after the
        // insert move by `delta`; the straddling __TEXT keeps its base
        // and grows. Used to re-encode the fixups at their new offsets.
        let mut shifted = image.layout.segments.clone();
        for s in &mut shifted {
            if s.fileoff >= growth_point {
                s.vmaddr += delta;
                s.fileoff += delta;
            } else if s.fileoff + s.filesize > growth_point {
                s.vmsize += delta;
                s.filesize += delta;
            }
        }

        let mut raw_overrides: Vec<(u64, Vec<u8>)> = Vec::new();

        // The load-critical fixup: shift every chained-fixup slot + rebase
        // target by `delta` and re-emit blob + slots at shifted offsets.
        // A minimal dylib can declare LC_DYLD_CHAINED_FIXUPS yet carry no
        // chains (no rebases/binds) — `read` reports that as `Truncated`;
        // there's nothing to shift, so skip it. Other read errors are real.
        if let Some(cfhdr) = image.layout.chained_fixups {
            match crate::container::ChainedFixups::read(image) {
                Ok(mut cf) => {
                    if cf.segments.iter().any(|s| !s.fixups.is_empty()) {
                        cf.shift_by(delta);
                        let slots = cf
                            .slot_bytes(&shifted)
                            .map_err(|e| TextEditorError::ContainerWrite(e.into()))?;
                        for (file_off, word) in slots {
                            raw_overrides.push((file_off, word.to_vec()));
                        }
                        let ser = cf
                            .serialize(&shifted)
                            .map_err(|e| TextEditorError::ContainerWrite(e.into()))?;
                        let mut blob = ser.bytes;
                        if blob.len() as u64 > cfhdr.datasize {
                            return Err(TextEditorError::ChainedFixupsBlobTooLarge {
                                new_size: blob.len() as u64,
                                capacity: cfhdr.datasize,
                            });
                        }
                        blob.resize(cfhdr.datasize as usize, 0);
                        raw_overrides.push((cfhdr.dataoff + delta, blob));
                    }
                }
                Err(crate::container::chained_fixups::ChainedFixupsError::Truncated(_)) => {
                    // No parseable chains — nothing to shift.
                }
                Err(e) => return Err(TextEditorError::ContainerWrite(e.into())),
            }
        }

        // Exports-critical fixup: the export trie stores each symbol's
        // address as an offset from the (fixed) image base, but the
        // symbols moved +delta — so every regular export's offset shifts.
        // Without this, dlsym / linked callers resolve moved exports at
        // their old addresses.
        if let Some(trie) = image.layout.exports_trie {
            let start = trie.dataoff as usize;
            let end = start + trie.datasize as usize;
            let blob = image
                .raw_bytes
                .get(start..end)
                .ok_or_else(|| {
                    TextEditorError::ContainerWrite(ContainerWriteError::ObjectWrite(
                        "grow: export trie range out of bounds".into(),
                    ))
                })?;
            let mut exports = crate::container::macho_export_trie::parse(blob)
                .map_err(TextEditorError::ContainerWrite)?;
            const EXPORT_SYMBOL_FLAGS_REEXPORT: u64 = 0x08;
            for e in &mut exports {
                // Reexports reference another dylib (no local address);
                // regular / stub-and-resolver exports moved by delta.
                if e.flags & EXPORT_SYMBOL_FLAGS_REEXPORT == 0 {
                    e.address_offset = e.address_offset.saturating_add(delta);
                }
            }
            let mut rebuilt = crate::container::macho_export_trie::build(&exports);
            if rebuilt.len() as u64 > trie.datasize {
                // The +delta ULEB expansion outgrew the original slot;
                // relocating the trie into a grown __LINKEDIT is a
                // follow-up. Surface it rather than truncate.
                return Err(TextEditorError::ContainerWrite(
                    ContainerWriteError::ObjectWrite(format!(
                        "grow: rebuilt export trie ({} bytes) outgrew its \
                         {}-byte slot; trie relocation is not yet implemented",
                        rebuilt.len(),
                        trie.datasize,
                    )),
                ));
            }
            rebuilt.resize(trie.datasize as usize, 0);
            raw_overrides.push((trie.dataoff + delta, rebuilt));
        }

        let opts = MachOWriteOptions {
            sign,
            raw_byte_overrides: raw_overrides,
        };
        write_with_text_growth_opts(
            &self.container,
            growth_point,
            delta,
            appended_file_offset,
            appended_bytes,
            &[],
            &opts,
        )
        .map_err(TextEditorError::ContainerWrite)
    }

    pub fn add_function(
        &mut self,
        name: &str,
        instructions: Vec<RewriteInstruction>,
    ) -> Result<SymbolId, TextEditorError> {
        self.add_function_generic::<Aarch64Isa>(name, instructions)
    }

    /// Generic over the source ISA. Used directly when the
    /// caller is rewriting ARMv7 (Thumb or ARM-mode) code; the
    /// aarch64-typed [`Self::add_function`] is a thin wrapper.
    ///
    /// Notes for ARMv7 callers:
    /// - Body sizing uses [`Isa::instruction_source_size`]
    ///   (which is mnemonic-aware for Thumb: 2 bytes for
    ///   halfword-only mnemonics, else 4). Mixed-width Thumb
    ///   bodies pessimistically reserve 4 bytes per instruction
    ///   at layout time but the encoded output uses the actual
    ///   row's width.
    /// - Function entry alignment stays at 4 bytes — Thumb
    ///   entry points still need 4-byte alignment for the
    ///   PT_LOAD to be aligned, and BL/BLX targets to Thumb
    ///   functions encode with the low bit *set* via the
    ///   symbol address (not the entry vaddr) — that's a
    ///   caller responsibility.
    pub fn add_function_generic<I: Isa>(
        &mut self,
        name: &str,
        instructions: Vec<RewriteInstructionGeneric<I>>,
    ) -> Result<SymbolId, TextEditorError> {
        if instructions.is_empty() {
            return Err(TextEditorError::EmptyFunction(name.to_string()));
        }

        // Make sure the input is an ET_DYN/ET_EXEC ELF — the
        // appended-segment writer is the only thing that knows how
        // to emit a fresh PT_LOAD, and that's gated to those
        // formats.
        if !matches!(
            self.container.kind,
            crate::container::ContainerKind::SharedObject
                | crate::container::ContainerKind::Executable,
        ) {
            return Err(TextEditorError::AppendUnsupportedKind(self.container.kind));
        }

        // Estimate the body's byte size for the intra-`__TEXT`
        // capacity check. Sum per-mnemonic source sizes via the
        // Isa trait; on aarch64 / ARM-mode this is uniformly 4,
        // on Thumb it varies between 2 and 4.
        let body_bytes: u64 = instructions
            .iter()
            .map(|insn| I::instruction_source_size(insn.mnemonic))
            .sum();
        let new_size = body_bytes + 3; // 3-byte alignment slop
        self.check_intra_text_capacity(new_size, &format!("add_function({name:?})"))?;

        // Allocate vaddr for this function. First call picks the
        // segment vaddr from the input's PT_LOAD extents; later
        // calls pack into the same segment in order.
        let segment_vaddr = match &self.appended {
            Some(state) => state.segment_vaddr,
            None => self.pick_append_vaddr()?,
        };
        // Function bodies must be 4-byte aligned so PC-relative
        // branches inside them encode without a non-multiple-of-4
        // displacement. If a previous `add_data` left an
        // unaligned cumulative offset, pad up to the next 4
        // boundary before placing this function.
        let raw_cumulative = self
            .appended
            .as_ref()
            .map(|s| s.bytes.len() as u64)
            .unwrap_or(0);
        let cumulative_offset = (raw_cumulative + 3) & !3;
        if cumulative_offset > raw_cumulative {
            let state = self.appended.get_or_insert(AppendedFunctionsState {
                segment_vaddr,
                bytes: Vec::new(),
                exports: Vec::new(),
                items: Vec::new(),
            });
            state.bytes.resize(cumulative_offset as usize, 0);
        }
        let function_vaddr = segment_vaddr + cumulative_offset;

        // Register the symbol in the container *before* laying out
        // the function so the function's body can refer to itself
        // by SymbolId (e.g. for tail calls). Symbol section is set
        // to None — the new function isn't in any pre-existing
        // section — and `address` is the assigned vaddr.
        let symbol_id = SymbolId(self.container.symbols.len());
        self.container.symbols.push(crate::container::Symbol {
            id: symbol_id,
            name: name.to_string(),
            address: function_vaddr,
            size: body_bytes,
            kind: SymbolKind::Function,
            binding: crate::container::SymbolBinding::Global,
            section: None,
            is_undefined: false,
            flags: None,
        });

        // Lay out + emit the new function at its assigned vaddr.
        // `from_instructions` runs the same fusion pass `lift`
        // does, so caller-supplied macro-shaped pairs against a
        // [`Target::Symbol`] page operand fuse into a
        // [`I::MacroKind`] macro that emit resolves at the
        // function's final vaddr.
        let plan = RewritePlanGeneric::<I>::from_instructions(instructions, Some(&self.container));

        let layout = lay_out(&plan, function_vaddr, Some(&self.container))?;
        let output = emit(&plan, &layout, Some(&self.container))?;

        // Append to the cumulative segment buffer.
        let state = self.appended.get_or_insert(AppendedFunctionsState {
            segment_vaddr,
            bytes: Vec::new(),
            exports: Vec::new(),
            items: Vec::new(),
        });
        // Retain the plan (ISA-erased) so the Mach-O commit fallback can
        // re-emit this function at a fresh base if the intra-`__TEXT` base
        // doesn't fit. `cumulative_offset` is this body's 0-based start.
        state.items.push(RetainedAppend::Function {
            name: name.to_string(),
            offset: cumulative_offset,
            plan: wrap_retained_plan(plan),
        });
        state.bytes.extend_from_slice(&output.bytes);

        Ok(symbol_id)
    }

    /// Append a function from a pre-built rewrite plan into a fresh `PT_LOAD`
    /// segment, returning its symbol.
    ///
    /// Like [`Self::add_function`], but the caller supplies a (possibly
    /// multi-block) [`RewritePlan`](crate::rewrite::RewritePlan) instead of a
    /// flat instruction list. Intra-function `Target::Block` branches in the
    /// plan are resolved against the function's own layout, so this can relocate
    /// a branching function (e.g. one lifted via
    /// [`RewritePlan::lift_from_decoded`](crate::rewrite::RewritePlan::lift_from_decoded))
    /// with its control flow intact. Same ET_DYN/ET_EXEC requirement as
    /// `add_function`.
    pub fn add_function_from_plan<I: Isa>(
        &mut self,
        name: &str,
        plan: RewritePlanGeneric<I>,
    ) -> Result<SymbolId, TextEditorError> {
        if plan.blocks.iter().all(|b| b.ops.is_empty()) {
            return Err(TextEditorError::EmptyFunction(name.to_string()));
        }
        if !matches!(
            self.container.kind,
            crate::container::ContainerKind::SharedObject
                | crate::container::ContainerKind::Executable,
        ) {
            return Err(TextEditorError::AppendUnsupportedKind(self.container.kind));
        }

        // Conservative capacity estimate (a fused macro op can be >1
        // instruction); the real size is taken from the emitted bytes.
        let op_count: u64 = plan.blocks.iter().map(|b| b.ops.len() as u64).sum();
        self.check_intra_text_capacity(
            op_count * 8 + 3,
            &format!("add_function_from_plan({name:?})"),
        )?;

        let segment_vaddr = match &self.appended {
            Some(state) => state.segment_vaddr,
            None => self.pick_append_vaddr()?,
        };
        let raw_cumulative = self
            .appended
            .as_ref()
            .map(|s| s.bytes.len() as u64)
            .unwrap_or(0);
        let cumulative_offset = (raw_cumulative + 3) & !3;
        if cumulative_offset > raw_cumulative {
            let state = self.appended.get_or_insert(AppendedFunctionsState {
                segment_vaddr,
                bytes: Vec::new(),
                exports: Vec::new(),
                items: Vec::new(),
            });
            state.bytes.resize(cumulative_offset as usize, 0);
        }
        let function_vaddr = segment_vaddr + cumulative_offset;

        // Register the symbol before layout so the body can self-reference.
        let symbol_id = SymbolId(self.container.symbols.len());
        self.container.symbols.push(crate::container::Symbol {
            id: symbol_id,
            name: name.to_string(),
            address: function_vaddr,
            size: 0,
            kind: SymbolKind::Function,
            binding: crate::container::SymbolBinding::Global,
            section: None,
            is_undefined: false,
            flags: None,
        });

        let layout = lay_out(&plan, function_vaddr, Some(&self.container))?;
        let output = emit(&plan, &layout, Some(&self.container))?;
        // The appended-segment writer copies `output.bytes` verbatim and emits
        // no dynamic relocations for them. Emit folds every *resolvable*
        // reference (intra-binary branches/calls and PLT-stubbed externals) to a
        // concrete displacement, so a non-empty relocation set here is a
        // reference we cannot place — fail loudly rather than drop it and ship a
        // binary that jumps to the wrong address. Mirrors the hole-placement
        // paths (`place_plan_in_holes`/`place_plan_scattered`), which bail for
        // the same reason.
        if !output.relocations.is_empty() {
            // Roll back the symbol we provisionally registered above.
            self.container.symbols.pop();
            return Err(TextEditorError::AppendNeedsUnsupportedRelocation {
                name: name.to_string(),
                count: output.relocations.len(),
            });
        }
        self.container.symbols[symbol_id.0].size = output.bytes.len() as u64;

        let state = self.appended.get_or_insert(AppendedFunctionsState {
            segment_vaddr,
            bytes: Vec::new(),
            exports: Vec::new(),
            items: Vec::new(),
        });
        // Retain the plan (ISA-erased) so the Mach-O commit fallback can
        // re-emit this function at a fresh base if the intra-`__TEXT` base
        // doesn't fit. `cumulative_offset` is this body's 0-based start.
        state.items.push(RetainedAppend::Function {
            name: name.to_string(),
            offset: cumulative_offset,
            plan: wrap_retained_plan(plan),
        });
        state.bytes.extend_from_slice(&output.bytes);

        Ok(symbol_id)
    }

    /// Append a new function to the binary *and* expose it as a
    /// dynamic export.
    ///
    /// Same shape as [`Self::add_function`], plus: the symbol is
    /// promoted to the dynamic symbol table at commit time. After
    /// linking, callers using `dlopen` + `dlsym(handle, name)`
    /// (or another library linking against the rewritten one)
    /// will resolve the new function by name.
    ///
    /// Promotion involves rebuilding `.dynsym`, `.dynstr`,
    /// `.gnu.version`, and `.gnu.hash` and pointing the
    /// `.dynamic` `DT_SYMTAB`/`DT_STRTAB`/`DT_GNU_HASH`/`DT_VERSYM`
    /// tags at fresh copies placed in the appended segment. The
    /// original copies stay in the file but become orphaned (the
    /// loader follows the `.dynamic` tags, so they're inert).
    ///
    /// ## Limits
    ///
    /// - The new export is unversioned (versym = 1, the
    ///   generic-base version). Versioned exports
    ///   (`.gnu.version_d`-tracked) aren't yet supported.
    /// - The export's binding is `STB_GLOBAL` and visibility is
    ///   `STV_DEFAULT`. No way yet to mark it `STV_HIDDEN` or
    ///   `STV_PROTECTED`.
    /// - The regenerated `.gnu.hash` uses `nbuckets = 1` (every
    ///   hashable dynsym entry chains in one bucket). Lookup
    ///   stays fast for small export sets; large export counts
    ///   may want a higher bucket count, which would require
    ///   sorting dynsym by hash bucket and remapping indices in
    ///   `.rela.plt` / `.rela.dyn` / `.gnu.version` — out of
    ///   scope today.
    pub fn add_function_exported(
        &mut self,
        name: &str,
        instructions: Vec<RewriteInstruction>,
    ) -> Result<SymbolId, TextEditorError> {
        // Strict mode: `add_function_exported` always needs
        // the `__APPENDED`-segment writer (rebuild export
        // trie + symtab + shift __LINKEDIT). Reject up front
        // when the caller has promised no new segments.
        if self.no_new_segments
            && matches!(self.container.format, crate::container::BinaryFormat::Macho)
        {
            return Err(TextEditorError::WouldCreateNewSegment {
                reason: format!(
                    "add_function_exported({name:?}) requires the \
                     __APPENDED-segment writer to rebuild the export \
                     trie and symtab — incompatible with \
                     prohibit_new_segments()"
                ),
            });
        }
        // Mach-O export plumbing rebuilds the symtab/trie
        // and shifts __LINKEDIT — that flow only fits the
        // `__APPENDED`-segment writer path, not the
        // intra-`__TEXT` placement strategy. Mark
        // `force_segment_placement` before laying out the
        // function so its bytes land at a vaddr the segment
        // writer can accept.
        self.force_segment_placement = true;
        let symbol_id = self.add_function(name, instructions)?;
        // The function symbol's vaddr/size were populated by
        // add_function. Capture them for the dynsym promotion.
        let symbol = self.container.symbol(symbol_id);
        let export = ExportedSymbol {
            symbol_id,
            name: name.to_string(),
            vaddr: symbol.address,
            size: symbol.size,
        };
        // appended is guaranteed Some at this point — add_function
        // initialises it on first call.
        self.appended
            .as_mut()
            .expect("add_function leaves appended state populated")
            .exports
            .push(export);
        Ok(symbol_id)
    }

    /// Append a function that runs at library load time, before
    /// any other code in the library is reachable from the host.
    ///
    /// **How it works.** ELF shared objects expose a list of
    /// constructor function pointers via the `.init_array`
    /// section, which the dynamic linker walks during `dlopen` /
    /// initial process startup. Each slot is an 8-byte pointer
    /// fixed up at load time by an `R_AARCH64_RELATIVE` entry in
    /// `.rela.dyn`. `add_initialiser` redirects one such slot
    /// (the first or last, per `position`) to a
    /// freshly-appended wrapper that:
    ///
    /// 1. saves the loader-supplied `(argc, argv, envp)`
    ///    arguments (`x0`/`x1`/`x2`) on the stack;
    /// 2. calls the user-supplied `body` as a regular function;
    /// 3. restores `(argc, argv, envp)`;
    /// 4. tail-calls the *original* constructor that the slot
    ///    pointed to (so existing init code still runs);
    /// 5. returns to the loader.
    ///
    /// The chain order within the hijacked slot is therefore
    /// "user code first, original ctor second" — RunBefore
    /// semantics. `position` controls which slot is hijacked:
    /// [`InitialiserPosition::First`] makes the appended code
    /// run before *every* other ctor in the library;
    /// [`InitialiserPosition::Last`] inserts it ahead of the
    /// final ctor only (CRT helpers and earlier user ctors run
    /// untouched, before the wrapper).
    ///
    /// `body` is a complete AArch64 function: it must include
    /// its own prologue, epilogue, and `ret`. The wrapper calls
    /// it with `bl`, so the body is treated as a leaf-style
    /// callee that may freely use any caller-saved registers.
    /// `x0`/`x1`/`x2` reaching the body carry the loader's
    /// `(argc, argv, envp)`; the body may ignore them.
    ///
    /// ## Limits (Stage A)
    ///
    /// - The input must already have a non-empty `.init_array`.
    ///   Synthesising `.init_array` from scratch (and growing
    ///   `.rela.dyn` / extending `.dynamic`) is Stage B.
    ///   Returns [`TextEditorError::NoExistingInitArray`]
    ///   otherwise.
    /// - We hijack a single slot per call (first or last). Other
    ///   slots run unchanged. Calling `add_initialiser` more
    ///   than once with the same position will hijack the same
    ///   slot twice, which works but produces a chain (each
    ///   call's wrapper tail-calls the previous wrapper) — for
    ///   most use cases, prefer one call per position.
    /// - The slot must be relocated by an `R_AARCH64_RELATIVE`
    ///   entry in `.rela.dyn`. Other relocation kinds (e.g.
    ///   IRELATIVE for ifuncs) return
    ///   [`TextEditorError::NoMatchingRelaDynEntry`].
    ///
    /// ## Returns
    ///
    /// The [`SymbolId`] of the user `body` function (not the
    /// wrapper). Callers can pass it to other editor methods if
    /// they want to call the same code from elsewhere.
    ///
    /// Multiple `add_initialiser` calls compose: a later hijack of the same (or
    /// another) `.init_array` slot reads the *effective* `.rela.dyn` (honouring
    /// earlier staged overrides), so its wrapper chain-tails to the previously
    /// installed wrapper instead of orphaning it.
    fn effective_section_bytes(&self, idx: usize) -> &[u8] {
        self.section_overrides
            .iter()
            .rev()
            .find(|(i, _)| *i == idx)
            .map(|(_, b)| b.as_slice())
            .unwrap_or(&self.container.sections[idx].bytes)
    }

    pub fn add_initialiser(
        &mut self,
        name: &str,
        body: Vec<RewriteInstruction>,
        position: InitialiserPosition,
    ) -> Result<SymbolId, TextEditorError> {
        if body.is_empty() {
            return Err(TextEditorError::EmptyFunction(name.to_string()));
        }
        if !matches!(
            self.container.kind,
            crate::container::ContainerKind::SharedObject
                | crate::container::ContainerKind::Executable,
        ) {
            return Err(TextEditorError::AppendUnsupportedKind(self.container.kind));
        }

        // Mach-O dispatch: handled separately below since the
        // mechanics differ (no `.init_array` / `.rela.dyn`;
        // hijack works by patching `__init_offsets[0]` to
        // point at our wrapper, and the wrapper chain-tails to
        // the original ctor).
        if matches!(self.container.format, crate::container::BinaryFormat::Macho) {
            return self.add_initialiser_macho(name, body, position);
        }

        // Append branch: brand-new slot. No hijack, no
        // chain-back. Defer the heavy lifting to commit time —
        // we just register the user body and stash its vaddr.
        if matches!(position, InitialiserPosition::Append) {
            // Cheap up-front validation so failures surface
            // before we commit to mutating state. .rela.dyn
            // synthesis isn't yet supported, so the input must
            // already have one.
            if !self
                .container
                .sections
                .iter()
                .any(|s| s.name == ".rela.dyn")
            {
                return Err(TextEditorError::NoExistingRelaDyn);
            }
            // We need an ELF image to apply the eventual
            // `.dynamic` edit at commit time. DT_INIT_ARRAY /
            // DT_INIT_ARRAYSZ tag insertion (when absent) or
            // existing-tag updates (when present) happen then —
            // if `.dynamic` doesn't have trailing DT_NULL
            // room for any new tags, the writer relocates
            // `.dynamic` into the appended segment (rewriting
            // PT_DYNAMIC) instead of failing here. See
            // [`Self::extend_segment_for_grown_dynamic`].
            if self.container.elf_image.is_none() {
                return Err(TextEditorError::AppendMissingElfImage);
            }

            let body_name = format!("{name}__body");
            let user_body_id = self.add_function(&body_name, body)?;
            let user_body_vaddr = self.container.symbol(user_body_id).address;
            self.pending_appended_init_slots.push(user_body_vaddr);
            return Ok(user_body_id);
        }

        // First/Last branch: hijack an existing slot.
        // Locate the last .init_array slot's vaddr and the
        // matching R_AARCH64_RELATIVE entry's current addend
        // (= original constructor's runtime offset). All before
        // we touch anything mutable, so we can fail cleanly.
        let init_array_idx = self
            .container
            .sections
            .iter()
            .position(|s| s.name == ".init_array")
            .ok_or(TextEditorError::NoExistingInitArray)?;
        let init_array_section = &self.container.sections[init_array_idx];
        if init_array_section.bytes.len() < 8 {
            return Err(TextEditorError::NoExistingInitArray);
        }
        // .init_array entries are 8-byte function pointers.
        // Pick the slot to hijack based on `position`: First is
        // index 0, Last is the final 8-byte chunk.
        let slot_count = init_array_section.bytes.len() / 8;
        let slot_index = match position {
            InitialiserPosition::First => 0usize,
            InitialiserPosition::Last => slot_count - 1,
            InitialiserPosition::Append => unreachable!("handled above"),
        };
        let slot_offset_in_section = slot_index * 8;
        let slot_vaddr =
            init_array_section.address + slot_offset_in_section as u64;

        let rela_dyn_idx = self
            .container
            .sections
            .iter()
            .position(|s| s.name == ".rela.dyn")
            .ok_or(TextEditorError::NoMatchingRelaDynEntry {
                init_array_vaddr: slot_vaddr,
            })?;
        let rela_dyn_bytes = self.effective_section_bytes(rela_dyn_idx);
        // Each Elf64_Rela is 24 bytes: r_offset(8) + r_info(8) + r_addend(8).
        // We want the entry whose r_offset == slot_vaddr and
        // whose relocation type is R_AARCH64_RELATIVE (1027 / 0x403).
        const RELA_ENTRY_SIZE: usize = 24;
        const R_AARCH64_RELATIVE: u32 = 1027;
        let mut found_entry_offset: Option<usize> = None;
        let mut original_ctor_offset_addend: i64 = 0;
        for entry_off in (0..rela_dyn_bytes.len()).step_by(RELA_ENTRY_SIZE) {
            if entry_off + RELA_ENTRY_SIZE > rela_dyn_bytes.len() {
                break;
            }
            let r_offset = u64::from_le_bytes(
                rela_dyn_bytes[entry_off..entry_off + 8].try_into().unwrap(),
            );
            let r_info = u64::from_le_bytes(
                rela_dyn_bytes[entry_off + 8..entry_off + 16].try_into().unwrap(),
            );
            let r_addend = i64::from_le_bytes(
                rela_dyn_bytes[entry_off + 16..entry_off + 24].try_into().unwrap(),
            );
            // Lower 32 bits of r_info are the relocation type for
            // ELF64.
            let r_type = (r_info & 0xffff_ffff) as u32;
            if r_offset == slot_vaddr && r_type == R_AARCH64_RELATIVE {
                found_entry_offset = Some(entry_off);
                original_ctor_offset_addend = r_addend;
                break;
            }
        }
        let rela_entry_off = found_entry_offset.ok_or(
            TextEditorError::NoMatchingRelaDynEntry {
                init_array_vaddr: slot_vaddr,
            },
        )?;

        // The original ctor's runtime offset (from the addend)
        // equals its vaddr in the (un-relocated) .so — the
        // dynamic linker will compute `load_bias + addend` at
        // load time and `load_bias + Symbol.address` at runtime
        // for the original ctor, so they reconcile. Translate
        // the addend into a Symbol so the rewriter can fold
        // `bl Target::Symbol(...)` into a PC-relative branch
        // against the real function. The matching symbol must
        // already exist in the symbol table.
        let original_ctor_vaddr = original_ctor_offset_addend as u64;
        let original_ctor_id = self
            .container
            .symbols
            .iter()
            .find(|s| {
                s.kind == SymbolKind::Function
                    && !s.is_undefined
                    && s.address == original_ctor_vaddr
            })
            .map(|s| s.id)
            .ok_or_else(|| TextEditorError::SymbolNotFound(format!(
                "<original ctor at vaddr 0x{original_ctor_vaddr:x}>"
            )))?;

        // Step 1: register the user body via add_function. It's
        // an ordinary appended function with its own
        // prologue/epilogue.
        let body_name = format!("{name}__body");
        let user_body_id = self.add_function(&body_name, body)?;

        // Step 2: synthesise the wrapper. Builds a small fixed
        // sequence:
        //
        //   stp x29, x30, [sp, #-48]!
        //   stp x0,  x1,  [sp, #16]      ; preserve argc, argv
        //   str x2,       [sp, #32]      ; preserve envp
        //   mov x29, sp
        //   bl  user_body
        //   ldp x0,  x1,  [sp, #16]
        //   ldr x2,       [sp, #32]
        //   bl  original_ctor
        //   ldp x29, x30, [sp], #48
        //   ret
        //
        // bl user_body and bl original_ctor are symbolic so the
        // rewriter resolves their offsets against the wrapper's
        // final vaddr.
        use crate::isa::aarch64::DecodedOperand;
        use crate::rewrite::ir::RewriteOperand;
        // Templates here are fixed bit patterns we author
        // ourselves and validated up front (see verify-encodings
        // notes alongside this method); decoding can't fail in
        // practice. `expect` makes the assumption explicit
        // rather than burying it in a `From` impl that would
        // suggest decode failures are recoverable here.
        let template = |word: u32| -> RewriteInstruction {
            let decoded = aarch64::decode_instruction(0, word)
                .expect("static add_initialiser wrapper template must decode");
            RewriteInstruction {
                mnemonic: decoded.mnemonic,
                operands: decoded
                    .operands
                    .into_iter()
                    .map(RewriteOperand::Decoded)
                    .collect(),
                original_address: None,
                source_size: None,
            }
        };
        let symbolic_bl = |word: u32, target: Target| -> Result<RewriteInstruction, TextEditorError> {
            let mut t = template(word);
            for op in t.operands.iter_mut() {
                if matches!(op, RewriteOperand::Decoded(DecodedOperand::BranchTarget(_))) {
                    *op = RewriteOperand::Branch(target);
                    return Ok(t);
                }
            }
            // BL template is supposed to have a BranchTarget; if
            // the table evolves and removes it, surface that
            // clearly rather than silently failing.
            Err(TextEditorError::Encode(EncodeError::Unimplemented {
                kind: "bl template has no BranchTarget operand to substitute",
            }))
        };

        let wrapper_body: Vec<RewriteInstruction> = vec![
            template(0xa9bd7bfd),                              // stp x29, x30, [sp, #-48]!
            template(0xa90107e0),                              // stp x0,  x1,  [sp, #16]
            template(0xf90013e2),                              // str x2,       [sp, #32]
            template(0x910003fd),                              // mov x29, sp
            symbolic_bl(0x94000000, Target::Symbol(user_body_id))?, // bl user_body
            template(0xa94107e0),                              // ldp x0,  x1,  [sp, #16]
            template(0xf94013e2),                              // ldr x2,       [sp, #32]
            symbolic_bl(0x94000000, Target::Symbol(original_ctor_id))?, // bl original_ctor
            template(0xa8c37bfd),                              // ldp x29, x30, [sp], #48
            template(0xd65f03c0),                              // ret
        ];
        let wrapper_name = format!("{name}__wrapper");
        let wrapper_id = self.add_function(&wrapper_name, wrapper_body)?;
        let wrapper_vaddr = self.container.symbol(wrapper_id).address;

        // Step 3: rewrite the matching .rela.dyn entry's addend
        // to point at our wrapper. The slot bytes themselves
        // don't need changing — R_AARCH64_RELATIVE writes
        // `load_bias + addend` into the slot at load time,
        // ignoring the slot's static contents.
        let mut new_rela_dyn = self.effective_section_bytes(rela_dyn_idx).to_vec();
        new_rela_dyn[rela_entry_off + 16..rela_entry_off + 24]
            .copy_from_slice(&(wrapper_vaddr as i64).to_le_bytes());
        self.section_overrides.push((rela_dyn_idx, new_rela_dyn));

        Ok(user_body_id)
    }

    /// Mach-O implementation of [`Self::add_initialiser`].
    /// Hijacks the existing `__init_offsets` slot (which
    /// modern macOS toolchains emit by default in lieu of
    /// `__mod_init_func`) to point at a freshly-appended
    /// wrapper that chain-tail-calls back to the original
    /// constructor.
    ///
    /// Limitations:
    ///
    /// - Only `InitialiserPosition::First` and `::Last` are
    ///   supported. They behave identically when the input
    ///   has only one ctor (the typical case for hand-written
    ///   Mach-O dylibs). `::Append` (brand-new slot)
    ///   isn't yet implemented — would need to grow
    ///   `__init_offsets` in place, which shifts subsequent
    ///   `__TEXT` content.
    /// - The input must already have a non-empty
    ///   `__init_offsets` section. Inputs without one
    ///   (e.g. legacy toolchains that emit `__mod_init_func`
    ///   instead) return [`TextEditorError::NoExistingInitArray`].
    fn add_initialiser_macho(
        &mut self,
        name: &str,
        body: Vec<RewriteInstruction>,
        position: InitialiserPosition,
    ) -> Result<SymbolId, TextEditorError> {
        if matches!(position, InitialiserPosition::Append) {
            return Err(TextEditorError::Encode(EncodeError::Unimplemented {
                kind: "Mach-O add_initialiser(Append) not yet implemented",
            }));
        }
        let macho_image = self
            .container
            .macho_image
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;

        // Find the `__init_offsets` section.
        let init_offsets_section = macho_image
            .layout
            .section("__TEXT", "__init_offsets")
            .ok_or(TextEditorError::NoExistingInitArray)?;
        if init_offsets_section.size < 4 {
            return Err(TextEditorError::NoExistingInitArray);
        }

        // Each entry is a u32 image-base-relative offset.
        let entry_count = (init_offsets_section.size / 4) as usize;
        let slot_index = match position {
            InitialiserPosition::First => 0,
            InitialiserPosition::Last => entry_count - 1,
            InitialiserPosition::Append => unreachable!(),
        };
        let slot_byte_offset = slot_index * 4;

        // Read the original ctor offset from the section bytes.
        // Find the matching neutral container section by
        // address — `__init_offsets` is the section at
        // init_offsets_section.vaddr in the container's
        // sections list.
        let container_section_idx = self
            .container
            .sections
            .iter()
            .position(|s| s.address == init_offsets_section.vaddr)
            .ok_or(TextEditorError::NoExistingInitArray)?;
        let original_offsets_bytes =
            self.container.sections[container_section_idx].bytes.clone();
        let mut original_offset_bytes = [0u8; 4];
        original_offset_bytes
            .copy_from_slice(&original_offsets_bytes[slot_byte_offset..slot_byte_offset + 4]);
        let original_ctor_offset = u32::from_le_bytes(original_offset_bytes) as u64;

        // The original ctor's image-base-relative offset.
        // Locate it in the container's symbol table by
        // address (Mach-O dylibs typically load at vaddr 0,
        // so image-base offset == vaddr).
        let original_ctor_id = self
            .container
            .symbols
            .iter()
            .find(|s| {
                s.kind == SymbolKind::Function
                    && !s.is_undefined
                    && s.address == original_ctor_offset
            })
            .map(|s| s.id)
            .ok_or_else(|| {
                TextEditorError::SymbolNotFound(format!(
                    "<original ctor at offset 0x{original_ctor_offset:x}>"
                ))
            })?;

        // Step 1: register the user body via add_function.
        let body_name = format!("{name}__body");
        let user_body_id = self.add_function(&body_name, body)?;

        // Step 2: synthesise the wrapper (same shape as ELF).
        // Wrapper preserves x0/x1/x2 across user_body since
        // dyld calls ctors with `(argc, argv, envp)` in those
        // registers.
        use crate::isa::aarch64::DecodedOperand;
        use crate::rewrite::ir::RewriteOperand;
        let template = |word: u32| -> RewriteInstruction {
            let decoded = aarch64::decode_instruction(0, word)
                .expect("static add_initialiser wrapper template must decode");
            RewriteInstruction {
                mnemonic: decoded.mnemonic,
                operands: decoded
                    .operands
                    .into_iter()
                    .map(RewriteOperand::Decoded)
                    .collect(),
                original_address: None,
                source_size: None,
            }
        };
        let symbolic_bl = |word: u32, target: Target| -> Result<RewriteInstruction, TextEditorError> {
            let mut t = template(word);
            for op in t.operands.iter_mut() {
                if matches!(op, RewriteOperand::Decoded(DecodedOperand::BranchTarget(_))) {
                    *op = RewriteOperand::Branch(target);
                    return Ok(t);
                }
            }
            Err(TextEditorError::Encode(EncodeError::Unimplemented {
                kind: "bl template has no BranchTarget operand to substitute",
            }))
        };
        let wrapper_body: Vec<RewriteInstruction> = vec![
            template(0xa9bd7bfd),                                       // stp x29, x30, [sp, #-48]!
            template(0xa90107e0),                                       // stp x0,  x1,  [sp, #16]
            template(0xf90013e2),                                       // str x2,       [sp, #32]
            template(0x910003fd),                                       // mov x29, sp
            symbolic_bl(0x94000000, Target::Symbol(user_body_id))?,     // bl user_body
            template(0xa94107e0),                                       // ldp x0,  x1,  [sp, #16]
            template(0xf94013e2),                                       // ldr x2,       [sp, #32]
            symbolic_bl(0x94000000, Target::Symbol(original_ctor_id))?, // bl original_ctor
            template(0xa8c37bfd),                                       // ldp x29, x30, [sp], #48
            template(0xd65f03c0),                                       // ret
        ];
        let wrapper_name = format!("{name}__wrapper");
        let wrapper_id = self.add_function(&wrapper_name, wrapper_body)?;
        let wrapper_vaddr = self.container.symbol(wrapper_id).address;

        // Step 3: override the __init_offsets slot bytes to
        // point at our wrapper. The section override is
        // same-length (4 bytes), so it lands in place via the
        // Mach-O writer's existing section_overrides path.
        let mut new_init_offsets = original_offsets_bytes;
        new_init_offsets[slot_byte_offset..slot_byte_offset + 4]
            .copy_from_slice(&(wrapper_vaddr as u32).to_le_bytes());
        self.section_overrides
            .push((container_section_idx, new_init_offsets));

        Ok(user_body_id)
    }

    /// Append a read-only byte blob to the binary in the same
    /// segment that [`Self::add_function`] populates.
    ///
    /// Returns a `SymbolId` whose address points at the first
    /// byte of the blob in the appended segment. Use this to
    /// place strings, lookup tables, and other constant data the
    /// new functions reference. The new function can compute the
    /// blob's address via the standard `adrp + add` pair against
    /// `Target::Symbol(blob_id)`, which `add_function`'s emit
    /// pass folds to the right page-relative addressing.
    ///
    /// The new segment is R-X (readable + executable, no write
    /// flag), which fits read-only data and code. For runtime-WRITABLE
    /// data (a separate R+W segment), use [`Self::add_writable_data`].
    ///
    /// `align` is the byte alignment the blob requires (1 for
    /// strings, 4 for u32 tables, etc.). The cumulative segment
    /// is padded to satisfy it.
    /// The vaddr the next [`Self::add_data`] with this `align` will assign — without
    /// appending anything. Lets a caller build position-dependent appended content
    /// (e.g. a blob that must reference other sections by a fixed delta) before
    /// committing it. Pure: mirrors `add_data`'s placement exactly.
    pub fn pending_append_vaddr(&self, align: u64) -> Result<u64, TextEditorError> {
        let segment_vaddr = match &self.appended {
            Some(state) => state.segment_vaddr,
            None => self.pick_append_vaddr()?,
        };
        let cumulative = self.appended.as_ref().map(|s| s.bytes.len() as u64).unwrap_or(0);
        let aligned_cumulative =
            if align <= 1 { cumulative } else { (cumulative + align - 1) & !(align - 1) };
        Ok(segment_vaddr + aligned_cumulative)
    }

    pub fn add_data(
        &mut self,
        name: &str,
        bytes: &[u8],
        align: u64,
    ) -> Result<SymbolId, TextEditorError> {
        if !matches!(
            self.container.kind,
            crate::container::ContainerKind::SharedObject
                | crate::container::ContainerKind::Executable,
        ) {
            return Err(TextEditorError::AppendUnsupportedKind(self.container.kind));
        }

        // Strict mode: bound the cumulative payload to the
        // chosen intra-`__TEXT` free region.
        let new_size = bytes.len() as u64 + align.saturating_sub(1);
        self.check_intra_text_capacity(new_size, &format!("add_data({name:?})"))?;

        // Initialise the appended state lazily — same pattern as
        // add_function.
        let segment_vaddr = match &self.appended {
            Some(state) => state.segment_vaddr,
            None => self.pick_append_vaddr()?,
        };

        // Pad cumulative bytes up to the requested alignment.
        let state = self.appended.get_or_insert(AppendedFunctionsState {
            segment_vaddr,
            bytes: Vec::new(),
            exports: Vec::new(),
            items: Vec::new(),
        });
        let cumulative = state.bytes.len() as u64;
        let aligned_cumulative = if align <= 1 {
            cumulative
        } else {
            (cumulative + align - 1) & !(align - 1)
        };
        if aligned_cumulative > cumulative {
            state.bytes.resize(aligned_cumulative as usize, 0);
        }

        let blob_vaddr = state.segment_vaddr + aligned_cumulative;
        // Retain the raw bytes (base-independent) at their aligned offset so
        // the Mach-O commit fallback can copy them verbatim when re-emitting
        // the payload at a fresh segment base.
        state.items.push(RetainedAppend::Data {
            name: name.to_string(),
            offset: aligned_cumulative,
            bytes: bytes.to_vec(),
        });
        state.bytes.extend_from_slice(bytes);

        // Register the symbol after we know the blob's vaddr.
        let symbol_id = SymbolId(self.container.symbols.len());
        self.container.symbols.push(crate::container::Symbol {
            id: symbol_id,
            name: name.to_string(),
            address: blob_vaddr,
            size: bytes.len() as u64,
            kind: SymbolKind::Object,
            binding: crate::container::SymbolBinding::Global,
            section: None,
            is_undefined: false,
            flags: None,
        });

        Ok(symbol_id)
    }

    /// Append runtime-WRITABLE data, returning a `SymbolId` whose
    /// address points at the first byte of the blob.
    ///
    /// Unlike [`Self::add_data`] (whose bytes live in the R+X appended
    /// segment and are read-only at runtime), these bytes land in a
    /// separate `PF_R | PF_W` PT_LOAD — use it for buffers that
    /// appended code writes to at run time (e.g. decrypting data into
    /// scratch). The blob's address is final on return (the writable
    /// segment is placed at its own vaddr, below the code append
    /// region), so an appended function may reference it via
    /// `Target::Symbol` even though more code is appended afterwards.
    ///
    /// To keep addresses stable, call this BEFORE appending any
    /// function that references the returned symbol — and, ideally,
    /// before any `add_function`/`add_data` at all, so the writable
    /// region claims the low append slot and the (growing) code region
    /// sits above it. ELF only.
    pub fn add_writable_data(
        &mut self,
        name: &str,
        bytes: &[u8],
        align: u64,
    ) -> Result<SymbolId, TextEditorError> {
        if !matches!(
            self.container.kind,
            crate::container::ContainerKind::SharedObject
                | crate::container::ContainerKind::Executable,
        ) {
            return Err(TextEditorError::AppendUnsupportedKind(self.container.kind));
        }
        if self.container.format != crate::container::BinaryFormat::Elf {
            return Err(TextEditorError::AppendUnsupportedKind(self.container.kind));
        }

        let new_size = bytes.len() as u64 + align.saturating_sub(1);
        self.check_intra_text_capacity(new_size, &format!("add_writable_data({name:?})"))?;

        // Pick the writable region's base lazily, above anything already
        // placed (mirrors `add_data`'s use of the code region's vaddr).
        let segment_vaddr = match &self.appended_writable {
            Some(state) => state.segment_vaddr,
            None => self.pick_append_vaddr()?,
        };
        let state = self.appended_writable.get_or_insert(WritableState {
            segment_vaddr,
            bytes: Vec::new(),
        });
        let cumulative = state.bytes.len() as u64;
        let aligned_cumulative = if align <= 1 {
            cumulative
        } else {
            (cumulative + align - 1) & !(align - 1)
        };
        if aligned_cumulative > cumulative {
            state.bytes.resize(aligned_cumulative as usize, 0);
        }
        let blob_vaddr = state.segment_vaddr + aligned_cumulative;
        state.bytes.extend_from_slice(bytes);

        let symbol_id = SymbolId(self.container.symbols.len());
        self.container.symbols.push(crate::container::Symbol {
            id: symbol_id,
            name: name.to_string(),
            address: blob_vaddr,
            size: bytes.len() as u64,
            kind: SymbolKind::Object,
            binding: crate::container::SymbolBinding::Global,
            section: None,
            is_undefined: false,
            flags: None,
        });

        Ok(symbol_id)
    }

    /// Force the dynamic linker to load another shared library
    /// when this one is loaded.
    ///
    /// Internally this appends `library_name` to a rebuilt
    /// `.dynstr` (placed in the appended segment) and inserts
    /// a new `DT_NEEDED` tag in `.dynamic` pointing at the new
    /// string offset. The loader walks `DT_NEEDED` tags during
    /// dependency resolution and `dlopen`s each one before
    /// firing this library's constructors, so the named
    /// library is guaranteed to be loaded — and its symbols
    /// reachable through `dlsym(RTLD_DEFAULT, ...)` — by the
    /// time any code in this library runs.
    ///
    /// `library_name` is the SONAME the loader should resolve
    /// (e.g. `"libcurl.so.4"`, `"libfoo.so"`). If the host
    /// also depends on the same library, the loader
    /// deduplicates — adding a redundant dependency is a
    /// no-op at runtime, just a few wasted bytes.
    ///
    /// ## Limits
    ///
    /// - Each new dependency consumes one trailing `DT_NULL`
    ///   slot in `.dynamic`. If there aren't enough,
    ///   [`TextEditorError::DynamicTooFull`] is returned.
    ///   Growing `.dynamic` itself is future work.
    /// - The library is added unconditionally to
    ///   `DT_NEEDED`; there's no way to express weak deps,
    ///   `RUNPATH`/`RPATH` overrides, or version requirements
    ///   yet.
    ///
    /// ## API note
    ///
    /// This method lives on `TextEditor` for continuity with
    /// the other `add_*` editor methods, even though it
    /// doesn't touch the text section. The editor has
    /// outgrown its name; treat it as a binary-level editor
    /// that's reached via a text-section entry point.
    pub fn add_library_dependency(
        &mut self,
        library_name: &str,
    ) -> Result<(), TextEditorError> {
        if !matches!(
            self.container.kind,
            crate::container::ContainerKind::SharedObject
                | crate::container::ContainerKind::Executable,
        ) {
            return Err(TextEditorError::AppendUnsupportedKind(self.container.kind));
        }
        // Strict mode: LC_LOAD_DYLIB injection on Mach-O
        // routes through the `__APPENDED`-segment writer
        // (load-command splice plus __LINKEDIT shift). Reject
        // when the caller has promised no new segments.
        if self.no_new_segments
            && matches!(self.container.format, crate::container::BinaryFormat::Macho)
        {
            return Err(TextEditorError::WouldCreateNewSegment {
                reason: format!(
                    "add_library_dependency({library_name:?}) requires the \
                     __APPENDED-segment writer for LC_LOAD_DYLIB injection — \
                     incompatible with prohibit_new_segments()"
                ),
            });
        }
        // We need format-specific image data at commit time:
        // ELF requires elf_image (for `.dynamic` editing /
        // .dynstr extension); Mach-O requires macho_image
        // (for LC_LOAD_DYLIB splicing). Either being absent
        // means the reader couldn't parse the input enough
        // for the commit-time work to succeed.
        match self.container.format {
            crate::container::BinaryFormat::Elf
                if self.container.elf_image.is_none() =>
            {
                return Err(TextEditorError::AppendMissingElfImage);
            }
            crate::container::BinaryFormat::Macho
                if self.container.macho_image.is_none() =>
            {
                return Err(TextEditorError::AppendMissingElfImage);
            }
            _ => {}
        }
        self.pending_library_deps.push(library_name.to_string());
        // Mach-O LC_LOAD_DYLIB injection uses the
        // appended-segment writer's load-command splice
        // pattern; force segment placement so any
        // accompanying `add_function` / `add_data` calls
        // also land in `__APPENDED` (the intra-`__TEXT`
        // path doesn't grow load commands).
        self.force_segment_placement = true;
        Ok(())
    }

    /// Drop a previously-recorded library dependency so the
    /// dynamic linker stops loading it alongside this
    /// binary.
    ///
    /// Symmetric to [`Self::add_library_dependency`]:
    ///
    /// - **ELF**: walks `.dynamic`, finds the `DT_NEEDED` tag
    ///   whose value names `library_name` (resolved via
    ///   `.dynstr`), drops the entry, and pads the trailing
    ///   slot with `DT_NULL` so the section's byte length is
    ///   unchanged. The orphan string in `.dynstr` is left in
    ///   place — harmless, since nothing references it any more.
    /// - **Mach-O**: walks the load-command list, finds the
    ///   `LC_LOAD_DYLIB` (or `LC_LOAD_WEAK_DYLIB` /
    ///   `LC_REEXPORT_DYLIB` / `LC_LOAD_UPWARD_DYLIB`) whose
    ///   inline path equals `library_name`, removes it, zero-
    ///   pads the freed tail bytes (which become extra
    ///   headerpad), and patches `mach_header_64.ncmds` /
    ///   `sizeofcmds`. The binary is re-signed ad-hoc on
    ///   commit.
    ///
    /// Both formats keep the file's overall byte length and
    /// section/segment offsets unchanged, so the change
    /// commits through the in-place writer path — no appended
    /// segment is created.
    ///
    /// Returns
    /// [`TextEditorError::LibraryDependencyNotFound`] if no
    /// matching entry is present.
    ///
    /// ## Caveat: bind-info references
    ///
    /// On Mach-O, the bind info (LC_DYLD_INFO /
    /// LC_DYLD_CHAINED_FIXUPS) encodes each imported symbol's
    /// dylib by *ordinal* (1-based index into the dylib LC
    /// list). Removing an entry shifts later ordinals down,
    /// which corrupts bind info for any symbol still imported
    /// from a shifted-down library. This method intentionally
    /// does *not* fix that up — it's safe only when the
    /// removed library has no remaining bound imports (the
    /// common case: callers either remove a force-loaded dep
    /// they added themselves, or remove an unused dep an
    /// audit flagged). Callers who need to remove a depended-
    /// upon library must rebind its imports first.
    ///
    /// On ELF, undefined symbols carry no per-library
    /// reference, so this caveat doesn't apply.
    pub fn remove_library_dependency(
        &mut self,
        library_name: &str,
    ) -> Result<(), TextEditorError> {
        match self.container.format {
            crate::container::BinaryFormat::Elf => {
                self.remove_library_dependency_elf(library_name)
            }
            crate::container::BinaryFormat::Macho => {
                self.remove_library_dependency_macho(library_name)
            }
            // PE import-table editing is not implemented yet.
            crate::container::BinaryFormat::Pe => {
                Err(TextEditorError::AppendUnsupportedKind(self.container.kind))
            }
        }
    }

    fn remove_library_dependency_elf(
        &mut self,
        library_name: &str,
    ) -> Result<(), TextEditorError> {
        use crate::container::dynsym_extension as dx;
        use object::elf::DT_NEEDED;

        let image = self
            .container
            .elf_image
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        let dynstr = image
            .dynstr
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?
            .bytes
            .clone();

        let (new_entries, removed) = dx::remove_dynamic_entries(
            &image.dynamic,
            DT_NEEDED as u64,
            |value| {
                matches!(
                    dx::read_dynstr_at(&dynstr, value as u32),
                    Some(s) if s == library_name,
                )
            },
        );
        if removed == 0 {
            return Err(TextEditorError::LibraryDependencyNotFound(
                library_name.to_string(),
            ));
        }

        let new_bytes = dx::encode_dynamic(&new_entries, self.container.is_64());

        // Mutate the container in place so the writer's
        // section-bytes path emits the new contents.
        let dynamic_idx = self
            .container
            .sections
            .iter()
            .position(|s| s.name == ".dynamic")
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        // Preserve the section's existing byte length — the
        // writer's reserve phase assumes section offsets are
        // stable. `remove_dynamic_entries` pads with DT_NULL to
        // keep the entry count, so `new_bytes.len()` already
        // matches the source byte length from the input
        // (entry count × {16 for ELF64, 8 for ELF32}).
        let orig_len = self.container.sections[dynamic_idx].bytes.len();
        debug_assert_eq!(new_bytes.len(), orig_len);
        self.container.sections[dynamic_idx].bytes = new_bytes;
        self.container.sections[dynamic_idx].size = orig_len as u64;

        // Update the parsed image so any subsequent edits see
        // a consistent view.
        if let Some(image) = self.container.elf_image.as_mut() {
            image.dynamic = new_entries;
        }
        Ok(())
    }

    fn remove_library_dependency_macho(
        &mut self,
        library_name: &str,
    ) -> Result<(), TextEditorError> {
        let image = self
            .container
            .macho_image
            .as_mut()
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        let removed = crate::container::macho_writer::strip_lc_load_dylib(
            &mut image.raw_bytes,
            library_name,
        )?;
        if removed.is_none() {
            return Err(TextEditorError::LibraryDependencyNotFound(
                library_name.to_string(),
            ));
        }
        // Re-parse the layout so subsequent edits see the
        // updated ncmds / sizeofcmds. `MachOImage::parse`
        // consumes its input, so move the mutated raw_bytes
        // through it.
        let raw = std::mem::take(&mut image.raw_bytes);
        let reparsed = crate::container::MachOImage::parse(raw)?;
        *image = reparsed;
        Ok(())
    }

    /// Run the layout + emit + commit pipeline and return the
    /// rewritten container.
    ///
    /// The returned container can be serialized via
    /// [`Container::to_bytes`] to obtain a runnable byte stream
    /// — when no functions were appended via [`Self::add_function`].
    /// In the appended-function case, [`Self::commit`] returns a
    /// container whose `to_bytes` would *not* include the
    /// appended segment (the neutral container model has no slot
    /// for it). Callers who appended functions should use
    /// [`Self::commit_to_bytes`] instead, which drives the writer
    /// path that emits the new segment.
    ///
    /// On any failure (layout, emit, encoding) the editor's state
    /// is consumed and the error is returned; recovering and
    /// retrying requires constructing a fresh editor.
    /// Pack extended `.dynsym` / `.dynstr` / `.gnu.version` /
    /// `.gnu.hash` blobs into the appended segment after the
    /// existing bytes, and stage a section-bytes override for
    /// `.dynamic` so its DT_SYMTAB / DT_STRTAB / DT_STRSZ /
    /// DT_GNU_HASH / DT_VERSYM tags follow the new copies.
    /// Also handles library dependencies (`add_library_dependency`):
    /// each dep name is appended to `.dynstr` and a new
    /// `DT_NEEDED` tag is inserted into `.dynamic`.
    ///
    /// Either `exports` or `library_deps` may be empty; at
    /// least one must be non-empty when this is called (the
    /// caller gates on that).
    ///
    /// Returns the extended segment bytes. The original `.dynsym`
    /// etc. sections in the file stay in place but become
    /// orphaned — the dynamic linker follows the `.dynamic` tags
    /// (by virtual address) and never reads them again.
    fn extend_segment_for_exports(
        segment_vaddr: u64,
        existing_segment_bytes: &[u8],
        exports: &[ExportedSymbol],
        library_deps: &[String],
        image: &crate::container::ElfImage,
        container: &Container,
        overrides: &mut std::collections::HashMap<usize, Vec<u8>>,
    ) -> Result<Vec<u8>, TextEditorError> {
        use crate::container::dynsym_extension as dx;
        use crate::container::gnu_hash;

        // Source blobs we need to extend. They're populated for
        // any ET_DYN input the reader handled.
        let source_dynsym = image
            .dynsym
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        let source_dynstr = image
            .dynstr
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        let source_versym = image
            .gnu_versym
            .as_ref()
            .ok_or(TextEditorError::AppendMissingElfImage)?;

        // Walk source dynsym; we'll append one entry per export.
        let mut dynsym_entries = dx::parse_dynsym(&source_dynsym.bytes)?;
        let mut dynstr_bytes = source_dynstr.bytes.clone();
        let mut versym_bytes = source_versym.bytes.clone();

        // The shndx for the appended segment's section. We don't
        // know its final section index until the writer emits;
        // for now use 0 (SHN_UNDEF). The dynamic linker resolves
        // exports by name + value, not by section index, so this
        // is fine. Tools that walk by shndx may show "?" but
        // dlopen/dlsym work.
        let appended_shndx: u16 = 0;

        for export in exports {
            // Append the name to .dynstr.
            let (name_offset, new_dynstr) = dx::append_dynstr(&dynstr_bytes, &export.name);
            dynstr_bytes = new_dynstr;

            // STB_GLOBAL << 4 | STT_FUNC = 0x12.
            let st_info = (object::elf::STB_GLOBAL << 4) | object::elf::STT_FUNC;
            // STV_DEFAULT visibility.
            let st_other = object::elf::STV_DEFAULT;

            dynsym_entries.push(dx::DynsymEntry {
                st_name: name_offset,
                st_info,
                st_other,
                st_shndx: appended_shndx,
                st_value: export.vaddr,
                st_size: export.size,
            });

            // versym: 1 = base version (unversioned but defined).
            versym_bytes = dx::append_gnu_versym(&versym_bytes, 1);
        }

        // Library dependencies: append each name to .dynstr;
        // collect the resulting offsets so we can insert one
        // DT_NEEDED tag per dep below.
        let mut dt_needed_offsets: Vec<u32> = Vec::with_capacity(library_deps.len());
        for dep in library_deps {
            let (name_offset, new_dynstr) = dx::append_dynstr(&dynstr_bytes, dep);
            dynstr_bytes = new_dynstr;
            dt_needed_offsets.push(name_offset);
        }

        // Pack the segment. The dynsym/gnu_hash/versym rebuild
        // is only needed when there are exports; for a deps-
        // only call we only repack .dynstr and update the
        // dynamic tags.
        let mut segment_bytes = existing_segment_bytes.to_vec();
        use object::elf::*;
        let mut tag_updates: Vec<(u64, u64)> = Vec::new();

        if !exports.is_empty() {
            // Rebuild .gnu.hash from the full extended dynsym.
            // symbol_base is the index of the first hashable
            // entry in the source dynsym (read it from the
            // existing .gnu.hash header).
            let source_gnu_hash = image
                .gnu_hash
                .as_ref()
                .ok_or(TextEditorError::AppendMissingElfImage)?;
            if source_gnu_hash.bytes.len() < 16 {
                return Err(TextEditorError::AppendMissingElfImage);
            }
            let symbol_base = u32::from_le_bytes(
                source_gnu_hash.bytes[4..8].try_into().unwrap(),
            );

            let hashable: Vec<gnu_hash::HashableSymbol<'_>> = dynsym_entries
                .iter()
                .enumerate()
                .skip(symbol_base as usize)
                .map(|(i, entry)| {
                    let name = name_at_offset(&dynstr_bytes, entry.st_name);
                    gnu_hash::HashableSymbol {
                        dynsym_index: i as u32,
                        name,
                    }
                })
                .collect();

            // nbuckets=1 keeps the layout invariant simple
            // regardless of where new exports land. bloom_size=1
            // (one 64-bit word) is fine for small export sets;
            // bloom_shift=6 matches what GNU emits for ELF64.
            let new_gnu_hash_bytes =
                gnu_hash::build_gnu_hash(&hashable, symbol_base, 1, 1, 6);
            let new_dynsym_bytes = dx::encode_dynsym(&dynsym_entries);

            // Order: existing segment content → dynsym →
            // dynstr → versym → gnu_hash.
            let dynsym_vaddr = pack(&mut segment_bytes, segment_vaddr, &new_dynsym_bytes, 8);
            let dynstr_vaddr = pack(&mut segment_bytes, segment_vaddr, &dynstr_bytes, 1);
            let versym_vaddr = pack(&mut segment_bytes, segment_vaddr, &versym_bytes, 2);
            let gnu_hash_vaddr =
                pack(&mut segment_bytes, segment_vaddr, &new_gnu_hash_bytes, 8);

            tag_updates.extend_from_slice(&[
                (DT_SYMTAB as u64, dynsym_vaddr),
                (DT_STRTAB as u64, dynstr_vaddr),
                (DT_STRSZ as u64, dynstr_bytes.len() as u64),
                (DT_GNU_HASH as u64, gnu_hash_vaddr),
                (DT_VERSYM as u64, versym_vaddr),
            ]);
        } else {
            // Deps-only path: only `.dynstr` got new bytes.
            let dynstr_vaddr = pack(&mut segment_bytes, segment_vaddr, &dynstr_bytes, 1);
            tag_updates.extend_from_slice(&[
                (DT_STRTAB as u64, dynstr_vaddr),
                (DT_STRSZ as u64, dynstr_bytes.len() as u64),
            ]);
        }

        let mut new_dynamic_entries = dx::update_dynamic_tags(&image.dynamic, &tag_updates);
        if !dt_needed_offsets.is_empty() {
            let additions: Vec<(u64, u64)> = dt_needed_offsets
                .iter()
                .map(|&off| (DT_NEEDED as u64, off as u64))
                .collect();
            // Use the growing variant: if the result doesn't
            // fit in the original `.dynamic` section's byte
            // length, the finalizer in `commit_to_bytes` will
            // relocate the section to the appended segment and
            // patch PT_DYNAMIC. Either way, the in-place
            // override stays valid for the relocation finalizer
            // to find.
            new_dynamic_entries = dx::insert_dynamic_tags_growing(
                &new_dynamic_entries,
                &additions,
            );
        }
        let new_dynamic_bytes = dx::encode_dynamic(&new_dynamic_entries, container.is_64());

        // Stage the .dynamic override. Find its section index in
        // the container.
        let dynamic_index = container
            .sections
            .iter()
            .position(|s| s.name == ".dynamic")
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        overrides.insert(dynamic_index, new_dynamic_bytes);

        Ok(segment_bytes)
    }

    /// Build a fresh `.init_array` and matching `.rela.dyn` in
    /// the appended segment for callers that asked for new
    /// initialiser slots via [`InitialiserPosition::Append`].
    ///
    /// Steps:
    /// 1. Read the existing `.init_array` (may be empty/absent)
    ///    and the existing `.rela.dyn` (must be present).
    /// 2. Concatenate existing init_array entries with one new
    ///    8-byte zero slot per pending entry, place the result
    ///    in the segment. The slot's static contents don't
    ///    matter — `R_AARCH64_RELATIVE` writes
    ///    `load_bias + addend` at load time.
    /// 3. Build a new `.rela.dyn` by:
    ///    - copying every entry from the source `.rela.dyn`,
    ///      translating any `r_offset` that pointed at the
    ///      original `.init_array` to the equivalent offset in
    ///      the new `.init_array` (slots that previously sat
    ///      inside the original section are now at the new
    ///      vaddr);
    ///    - appending one new `R_AARCH64_RELATIVE` entry for
    ///      each pending slot, addend = the body's vaddr.
    ///    Place the result in the segment.
    /// 4. Override `.dynamic` to:
    ///    - add `DT_INIT_ARRAY` / `DT_INIT_ARRAYSZ` if absent,
    ///      or update them if present;
    ///    - update `DT_RELA` / `DT_RELASZ` / `DT_RELACOUNT`
    ///      to point at the new `.rela.dyn`.
    fn extend_segment_for_appended_init_slots(
        segment_vaddr: u64,
        existing_segment_bytes: Vec<u8>,
        pending_slots: &[u64],
        image: &crate::container::ElfImage,
        container: &Container,
        overrides: &mut std::collections::HashMap<usize, Vec<u8>>,
    ) -> Result<Vec<u8>, TextEditorError> {
        use crate::container::dynsym_extension as dx;
        use object::elf::*;

        // .rela.dyn must exist (Stage-B limit).
        let rela_dyn_idx = container
            .sections
            .iter()
            .position(|s| s.name == ".rela.dyn")
            .ok_or(TextEditorError::NoExistingRelaDyn)?;

        // The original .rela.dyn bytes — use the override if a
        // hijack-path call already staged one, otherwise the
        // section's source bytes.
        let original_rela_dyn = overrides
            .get(&rela_dyn_idx)
            .cloned()
            .unwrap_or_else(|| container.sections[rela_dyn_idx].bytes.clone());

        // Original .init_array section, if present. Empty Vec
        // when absent — we can still synthesise a fresh
        // .init_array entirely.
        let original_init_array_section = container
            .sections
            .iter()
            .find(|s| s.name == ".init_array");
        let original_init_array_bytes: Vec<u8> = original_init_array_section
            .map(|s| s.bytes.clone())
            .unwrap_or_default();
        let original_init_array_vaddr = original_init_array_section
            .map(|s| s.address)
            .unwrap_or(0);
        let original_init_array_len = original_init_array_bytes.len() as u64;

        // Build new .init_array bytes: original + one 8-byte
        // zero slot per pending entry. The result will be
        // placed in the segment; its eventual vaddr is computed
        // after we pack.
        let mut new_init_array_bytes = original_init_array_bytes.clone();
        let added_slot_count = pending_slots.len();
        for _ in 0..added_slot_count {
            new_init_array_bytes.extend_from_slice(&0u64.to_le_bytes());
        }

        // Pack .init_array into the segment first so we know
        // its vaddr before we build .rela.dyn.
        let mut segment_bytes = existing_segment_bytes;
        let new_init_array_vaddr =
            pack(&mut segment_bytes, segment_vaddr, &new_init_array_bytes, 8);
        let new_init_array_size = new_init_array_bytes.len() as u64;

        // Build new .rela.dyn:
        //   - decode each source entry, translating r_offset if
        //     it falls inside the original .init_array;
        //   - INSERT new R_AARCH64_RELATIVE entries within the
        //     leading-RELATIVE block (where DT_RELACOUNT
        //     terminates) so the contiguous-RELATIVE invariant
        //     is preserved. Loaders rely on this: with
        //     `DT_RELACOUNT = N`, the first N entries must all
        //     be RELATIVE. Inserting at the back would push
        //     non-RELATIVE entries inside the RELATIVE prefix.
        const RELA_ENTRY_SIZE: usize = 24;
        const R_AARCH64_RELATIVE: u32 = 1027;
        // Decode source entries first.
        let mut decoded: Vec<(u64, u64, i64)> = Vec::new();
        for entry_off in (0..original_rela_dyn.len()).step_by(RELA_ENTRY_SIZE) {
            if entry_off + RELA_ENTRY_SIZE > original_rela_dyn.len() {
                break;
            }
            let mut r_offset = u64::from_le_bytes(
                original_rela_dyn[entry_off..entry_off + 8]
                    .try_into()
                    .unwrap(),
            );
            let r_info = u64::from_le_bytes(
                original_rela_dyn[entry_off + 8..entry_off + 16]
                    .try_into()
                    .unwrap(),
            );
            let r_addend = i64::from_le_bytes(
                original_rela_dyn[entry_off + 16..entry_off + 24]
                    .try_into()
                    .unwrap(),
            );
            if original_init_array_len > 0
                && r_offset >= original_init_array_vaddr
                && r_offset < original_init_array_vaddr + original_init_array_len
            {
                let delta = r_offset - original_init_array_vaddr;
                r_offset = new_init_array_vaddr + delta;
            }
            decoded.push((r_offset, r_info, r_addend));
        }
        // Build the new entries to insert (one per pending
        // slot). Slot vaddrs land at the end of the new
        // .init_array (after the originals), in pending_slots
        // order.
        let new_slots_first_vaddr = new_init_array_vaddr + original_init_array_len;
        let mut new_relative_entries: Vec<(u64, u64, i64)> = Vec::with_capacity(added_slot_count);
        for (i, &body_vaddr) in pending_slots.iter().enumerate() {
            let slot_vaddr = new_slots_first_vaddr + (i as u64) * 8;
            new_relative_entries.push((slot_vaddr, R_AARCH64_RELATIVE as u64, body_vaddr as i64));
        }
        // Insert position: just after the last RELATIVE entry
        // in the source. The source's leading RELATIVE block
        // ends at index `relacount`; we insert the new
        // RELATIVE entries there. Anything after stays put.
        let original_relacount_usize = image
            .dynamic
            .iter()
            .find(|e| e.tag == DT_RELACOUNT as u64)
            .map(|e| e.value as usize)
            .unwrap_or(decoded.len()); // if no RELACOUNT, treat all as RELATIVE
        let mut combined: Vec<(u64, u64, i64)> =
            Vec::with_capacity(decoded.len() + new_relative_entries.len());
        combined.extend(decoded.iter().take(original_relacount_usize).copied());
        combined.extend(new_relative_entries.into_iter());
        combined.extend(decoded.iter().skip(original_relacount_usize).copied());

        let mut new_rela_dyn = Vec::with_capacity(combined.len() * RELA_ENTRY_SIZE);
        for (r_offset, r_info, r_addend) in &combined {
            new_rela_dyn.extend_from_slice(&r_offset.to_le_bytes());
            new_rela_dyn.extend_from_slice(&r_info.to_le_bytes());
            new_rela_dyn.extend_from_slice(&r_addend.to_le_bytes());
        }

        // Pack new .rela.dyn into the segment.
        let new_rela_dyn_vaddr = pack(&mut segment_bytes, segment_vaddr, &new_rela_dyn, 8);
        let new_rela_dyn_size = new_rela_dyn.len() as u64;
        // DT_RELACOUNT is "number of leading R_AARCH64_RELATIVE
        // entries". The source RELACOUNT counts all relative
        // entries from the source's leading run; new entries
        // we append are also RELATIVE so they extend that run
        // (the runtime linker just iterates the count).
        let original_relacount = image
            .dynamic
            .iter()
            .find(|e| e.tag == DT_RELACOUNT as u64)
            .map(|e| e.value)
            .unwrap_or(0);
        let new_relacount = original_relacount + added_slot_count as u64;

        // Update or insert .dynamic tags. If the dynsym/deps
        // path already staged a .dynamic override, chain off
        // that — otherwise we'd lose its DT_NEEDED additions
        // when we re-encode from `image.dynamic`.
        let dynamic_index = container
            .sections
            .iter()
            .position(|s| s.name == ".dynamic")
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        let starting_dynamic: Vec<crate::container::DynamicEntry> = match overrides.get(&dynamic_index) {
            Some(bytes) => dx::parse_dynamic(bytes, container.is_64())?,
            None => image.dynamic.clone(),
        };
        let has_init_array_tag = starting_dynamic
            .iter()
            .any(|e| e.tag == DT_INIT_ARRAY as u64);
        let updates: &[(u64, u64)] = &[
            (DT_RELA as u64, new_rela_dyn_vaddr),
            (DT_RELASZ as u64, new_rela_dyn_size),
            (DT_RELACOUNT as u64, new_relacount),
            // If the tags exist, update them; if not, the
            // update_dynamic_tags helper passes through, and the
            // insert step below adds them.
            (DT_INIT_ARRAY as u64, new_init_array_vaddr),
            (DT_INIT_ARRAYSZ as u64, new_init_array_size),
        ];
        let mut new_dynamic_entries = dx::update_dynamic_tags(&starting_dynamic, updates);
        if !has_init_array_tag {
            // Use the growing variant: if the original
            // `.dynamic` doesn't have trailing-DT_NULL room for
            // these additions, the finalizer in
            // `commit_to_bytes` relocates `.dynamic` into the
            // appended segment and patches PT_DYNAMIC.
            let additions = &[
                (DT_INIT_ARRAY as u64, new_init_array_vaddr),
                (DT_INIT_ARRAYSZ as u64, new_init_array_size),
            ];
            new_dynamic_entries =
                dx::insert_dynamic_tags_growing(&new_dynamic_entries, additions);
        }
        let new_dynamic_bytes = dx::encode_dynamic(&new_dynamic_entries, container.is_64());
        overrides.insert(dynamic_index, new_dynamic_bytes);

        // We've placed a new .rela.dyn in the appended segment
        // and pointed DT_RELA at it; the original section bytes
        // become orphaned (the loader walks via DT_RELA, by
        // virtual address). Drop any prior in-place override
        // for .rela.dyn — keeping it would write its bytes at
        // the original file offset which the loader no longer
        // reads, but it's also confusing for any tools that
        // read the section header.
        //
        // Actually, we want to leave the section bytes alone
        // *unless* a Stage-A hijack also staged .rela.dyn
        // changes. The hijack-path edit modified an addend at
        // the original .init_array slot vaddr, but we've now
        // moved .init_array to a new vaddr. Keeping the old
        // override would point a relative reloc at an obsolete
        // vaddr — harmless (loader doesn't read that .rela.dyn
        // any more) but messy.
        //
        // For simplicity: if both Append and a hijack path ran
        // in the same commit, the hijack already wrote into
        // the *original* rela.dyn bytes which we copied into
        // `original_rela_dyn` above and translated through to
        // the new section. So the override has already been
        // honoured. We can safely drop the standalone override
        // for .rela.dyn since we're providing a more
        // up-to-date one inside the segment.
        overrides.remove(&rela_dyn_idx);

        Ok(segment_bytes)
    }

    /// If the staged `.dynamic` override is larger than the
    /// original section's byte length, relocate `.dynamic` into
    /// the appended segment and patch PT_DYNAMIC's
    /// p_vaddr/p_memsz/p_filesz to point at the new copy. The
    /// original section's bytes are left untouched (orphaned)
    /// so static tooling continues to see a structurally-valid
    /// (if stale) `.dynamic`.
    ///
    /// We add headroom DT_NULL slots so subsequent calls to
    /// `add_library_dependency` etc. can re-grow without
    /// triggering another relocation. The finalizer also
    /// produces a valid result when no relocation is needed:
    /// it pads the override (if any) with DT_NULLs back to the
    /// original section size so the writer's in-place section-
    /// override path doesn't shrink the section.
    fn finalize_dynamic_size_or_relocate(
        segment_vaddr: u64,
        existing_segment_bytes: Vec<u8>,
        image: &mut crate::container::ElfImage,
        container: &Container,
        overrides: &mut std::collections::HashMap<usize, Vec<u8>>,
    ) -> Result<Vec<u8>, TextEditorError> {
        use crate::container::dynsym_extension as dx;
        use object::elf;

        // A statically-linked executable has no `.dynamic` section at all. With
        // no `.dynamic` to grow there is nothing to finalize — the appended
        // segment stands on its own (pure code/data, no dynamic tags).
        let Some(dynamic_index) = container
            .sections
            .iter()
            .position(|s| s.name == ".dynamic")
        else {
            return Ok(existing_segment_bytes);
        };
        let original_size = container.sections[dynamic_index].bytes.len();

        // No `.dynamic` override staged → nothing to do.
        let Some(override_bytes) = overrides.get(&dynamic_index).cloned() else {
            return Ok(existing_segment_bytes);
        };

        if override_bytes.len() <= original_size {
            // Fits in place. Pad with DT_NULL entries (16-byte
            // each) up to the original section size so the
            // writer's in-place override path emits the right
            // number of bytes.
            if override_bytes.len() < original_size {
                let mut padded = override_bytes.clone();
                let needed_padding = original_size - padded.len();
                // Each DT_NULL is 16 zero bytes; partial padding
                // would be malformed, but original_size and
                // override length are both multiples of 16 by
                // construction.
                debug_assert!(needed_padding % 16 == 0);
                padded.extend(std::iter::repeat(0u8).take(needed_padding));
                overrides.insert(dynamic_index, padded);
            }
            return Ok(existing_segment_bytes);
        }

        // Override grew past original size → relocate. Add
        // headroom DT_NULL slots so subsequent calls can re-grow
        // without another relocation. 32 spare slots × 16 bytes
        // = 512 extra bytes; small relative to page alignment.
        const HEADROOM_NULL_SLOTS: usize = 32;
        let entries = dx::parse_dynamic(&override_bytes, container.is_64())?;
        // Strip trailing nulls and re-append exactly one
        // terminator plus the headroom slots.
        let mut grown: Vec<crate::container::DynamicEntry> = entries
            .iter()
            .take_while(|e| e.tag != elf::DT_NULL as u64)
            .copied()
            .collect();
        for _ in 0..(HEADROOM_NULL_SLOTS + 1) {
            grown.push(crate::container::DynamicEntry {
                tag: elf::DT_NULL as u64,
                value: 0,
            });
        }
        let relocated_bytes = dx::encode_dynamic(&grown, container.is_64());

        let mut segment_bytes = existing_segment_bytes;
        // .dynamic entries are 8-byte aligned (Elf64_Dyn = 8B
        // tag + 8B value); align to 8 in the segment.
        let new_dynamic_vaddr =
            pack(&mut segment_bytes, segment_vaddr, &relocated_bytes, 8);
        let new_dynamic_filesz = relocated_bytes.len() as u64;

        // Patch PT_DYNAMIC's p_vaddr/p_memsz/p_filesz/p_offset.
        // p_offset is the file offset; it should also point at
        // the new copy's location inside the appended segment's
        // file region. Since we don't know the appended
        // segment's eventual file_offset from inside this
        // helper, leave p_offset alone — the writer fixes it up
        // by computing it from p_vaddr + (segment file_offset
        // - segment vaddr) at emit time.
        //
        // Actually, that's not how the existing writer works:
        // it emits p_offset verbatim from image.program_headers
        // (line 680 of elf_writer.rs). So an unmodified
        // p_offset would point at the *original* `.dynamic`'s
        // file location, which is wrong.
        //
        // Fix: compute the appended segment's file_offset based
        // on the writer's deterministic placement. The writer
        // places the appended segment at a page-aligned offset
        // past all section content; getting that exact value
        // here would duplicate the writer's reservation logic.
        //
        // Pragmatic alternative: dyld doesn't strictly need
        // p_offset for PT_DYNAMIC — it loads PT_DYNAMIC's
        // contents from the PT_LOAD that maps the same vaddr
        // range. So setting p_offset to 0 (or leaving it stale)
        // works if the loader is lenient. But the SysV gABI
        // says PT_DYNAMIC's p_offset/p_filesz should point at
        // the on-disk image. Some debuggers / tools rely on it.
        //
        // For correctness: patch elf_writer's program-header
        // emit to recompute PT_DYNAMIC's p_offset from
        // (appended_file_offset + (p_vaddr - appended_vaddr))
        // when the vaddr lies inside the appended segment.
        // That's a small writer-side addition; for now mark
        // p_offset as the in-segment offset added to a
        // sentinel and rely on the writer to fix it up.
        let appended_offset = new_dynamic_vaddr - segment_vaddr;
        let pt_dynamic = image
            .program_headers
            .iter_mut()
            .find(|p| p.p_type == elf::PT_DYNAMIC)
            .ok_or(TextEditorError::AppendMissingElfImage)?;
        pt_dynamic.p_vaddr = new_dynamic_vaddr;
        pt_dynamic.p_paddr = new_dynamic_vaddr;
        pt_dynamic.p_memsz = new_dynamic_filesz;
        pt_dynamic.p_filesz = new_dynamic_filesz;
        // Sentinel marker: writer recognises p_offset values
        // tagged with the high bit of the appended offset
        // (caller-cooperative protocol). Simpler: set p_offset
        // to a sentinel and have the writer detect "this
        // PT_DYNAMIC's vaddr falls inside the appended
        // segment" and compute the file offset from segment
        // file_offset. We embed the appended-segment-relative
        // offset here; the writer recognises any PT_DYNAMIC
        // whose vaddr lies in the appended segment and
        // computes p_offset from the appended segment's
        // file_offset.
        let _ = appended_offset; // kept for clarity; writer recomputes.
        pt_dynamic.p_offset = 0; // placeholder; writer overwrites.

        // Drop the in-place override since we've moved
        // `.dynamic` to a new location. The original section's
        // bytes stay verbatim in the file.
        overrides.remove(&dynamic_index);

        Ok(segment_bytes)
    }

}

impl BinaryEditor {
    /// Run the layout + emit + commit pipeline and return the
    /// rewritten container.
    ///
    /// The returned container can be serialized via
    /// [`Container::to_bytes`] to obtain a runnable byte stream
    /// — when no functions were appended via
    /// [`BinaryState::add_function`]. In the appended-function
    /// case, the returned container's `to_bytes` would *not*
    /// include the appended segment (the neutral container
    /// model has no slot for it). Callers who appended functions
    /// should use [`Self::commit_to_bytes`] instead, which drives
    /// the writer path that emits the new segment.
    ///
    /// On any failure (layout, emit, encoding) the editor's state
    /// is consumed and the error is returned; recovering and
    /// retrying requires constructing a fresh editor.
    pub fn commit(self) -> Result<Container, TextEditorError> {
        let text = self
            .text
            .ok_or_else(|| TextEditorError::SectionNotFound(
                "<no lifted text section; call lift_text_section before commit>".into(),
            ))?;
        let (edited, _output, _section_id) =
            commit_lifted_any(&text, &self.binary.container)?;
        Ok(edited)
    }

    /// Like [`Self::commit`] but also serializes the resulting
    /// container to bytes. Convenience for the common case where
    /// the caller wants a runnable `.so`/`.o` blob immediately.
    ///
    /// When functions were registered via
    /// [`BinaryState::add_function`], this drives the elf_writer's
    /// append-segment path so the new functions land in a fresh
    /// PT_LOAD segment beyond the input's mapped range.
    ///
    /// Callers who only used whole-binary methods (e.g. just
    /// [`BinaryState::add_library_dependency`]) without lifting a
    /// text section can call this directly — the in-section
    /// rewrite phase is skipped when no text section is present.
    pub fn commit_to_bytes(self) -> Result<Vec<u8>, TextEditorError> {
        self.commit_to_bytes_with_sign(true)
    }

    /// Like [`Self::commit_to_bytes`] but emits an unsigned
    /// Mach-O. Useful when the caller wants to apply a real
    /// codesign identity out-of-band (the default `codesign
    /// -s - --force` produces an ad-hoc signature that macOS
    /// loads but Gatekeeper rejects). For ELF this is
    /// equivalent to [`Self::commit_to_bytes`] — there's no
    /// signing step.
    ///
    /// The output won't load on macOS 10.15+ without
    /// post-processing signing.
    pub fn commit_to_bytes_unsigned(self) -> Result<Vec<u8>, TextEditorError> {
        self.commit_to_bytes_with_sign(false)
    }

    fn commit_to_bytes_with_sign(mut self, sign: bool) -> Result<Vec<u8>, TextEditorError> {
        // Dispatch on container format up-front for the
        // appended-segment case. ELF and Mach-O have separate
        // append-segment writers; only ELF uses the
        // dynsym/init/dynamic finalizers below.
        let is_macho = matches!(
            self.binary.container.format,
            crate::container::BinaryFormat::Macho,
        );

        // Uniform-shift grow path: the whole operation (byte insertion,
        // load-command patch, chained-fixup + export-trie +delta) lives
        // in build_grown_macho. The reserved region's placed content is
        // the appended buffer, dropped into the inserted gap.
        if let Some((growth_point, delta)) = self.binary.pending_text_grow {
            let payload = self
                .binary
                .appended
                .as_ref()
                .map(|a| a.bytes.clone())
                .unwrap_or_default();
            return self
                .binary
                .build_grown_macho(growth_point, delta, growth_point, &payload, sign);
        }

        // Library-dep additions need to land in the appended
        // segment (ELF: they grow `.dynstr`, which can't grow
        // in place; Mach-O: the LC_LOAD_DYLIB splice just
        // needs the appended-segment writer path active so
        // the commit goes through `write_with_appended_segment`
        // rather than the round-trip writer). If a caller used
        // only `add_library_dependency` and no `add_function` /
        // `add_data`, lazily initialise an empty appended
        // segment so the override-supporting writer path runs.
        if self.binary.appended.is_none()
            && (!self.binary.pending_library_deps.is_empty()
                || self.binary.appended_writable.is_some())
        {
            // Writable data (or a library dep) but no appended code: the
            // appended-segment writer path carries both, so lazily make
            // an empty code segment. `pick_append_vaddr` already sits
            // above the writable region, so they stay disjoint.
            let segment_vaddr = self.binary.pick_append_vaddr()?;
            self.binary.appended = Some(AppendedFunctionsState {
                segment_vaddr,
                bytes: Vec::new(),
                exports: Vec::new(),
                items: Vec::new(),
            });
        }
        // Take the lifted section and the binary state out so
        // we can consume both. `binary` is bound mutably because
        // the Mach-O append fallback may need to rebase the
        // appended payload (and its symbols in `binary.container`)
        // BEFORE the lifted-text commit reads that symbol table.
        let BinaryEditor { mut binary, text } = self;
        match binary.appended {
            None => {
                // No appended functions — straightforward path.
                // If no text section was lifted either, the
                // only edits are whole-binary mutations the
                // caller applied directly to `container` (e.g.
                // `remove_library_dependency`); fall through
                // to the in-place writer with the mutated
                // container as-is.
                let edited = match text {
                    Some(text) => {
                        let (edited, _output, _section_id) =
                            commit_lifted_any(&text, &binary.container)?;
                        edited
                    }
                    None => binary.container.clone(),
                };
                // On Mach-O, route through the writer's
                // options-aware entry point when the caller
                // wants an unsigned build or staged raw
                // overrides (e.g. a rewritten chained-fixups
                // blob). For ELF, `sign` is meaningless and
                // raw overrides aren't supported through this
                // path — fall through to `Container::to_bytes`.
                if is_macho && (!sign || !binary.macho_raw_byte_overrides.is_empty()) {
                    let opts = crate::container::macho_writer::MachOWriteOptions {
                        sign,
                        raw_byte_overrides: binary.macho_raw_byte_overrides.clone(),
                    };
                    crate::container::macho_writer::write_with_options(&edited, &opts)
                        .map_err(TextEditorError::from)
                } else {
                    edited.to_bytes().map_err(TextEditorError::from)
                }
            }
            Some(mut appended) => {
                // Mach-O append-fallback rebase, performed BEFORE the
                // lifted-text commit below. `pick_append_vaddr`
                // optimistically picks an intra-`__TEXT` base at the
                // *first* append (before the total payload size is known)
                // and bakes it into every emitted body / symbol address.
                // If the finished payload no longer fits a `__TEXT` free
                // region, an `__APPENDED` segment at that stale base would
                // overlap `__TEXT`, so we must rebase it above the mapped
                // range. This MUST happen before `commit_lifted_any`
                // resolves the lifted `__text`'s symbolic branch targets
                // (e.g. a trampoline `b <stub>`) against
                // `binary.container`'s symbol table — otherwise the
                // trampoline bakes in the OLD (now-unmapped) stub address
                // and the rebased binary SIGBUSes. The placement decision
                // reads only the *layout* (`__TEXT` extents, free regions,
                // max_vaddr_end), which the text commit does not change, so
                // it is safe to evaluate against the pre-commit container.
                //
                // Scalars are read out (and the immutable borrow dropped)
                // before taking `&mut binary.container` for the rebase, to
                // avoid overlapping borrows.
                if is_macho {
                    let (do_rebase, new_base) = match binary.container.macho_image.as_ref() {
                        Some(img) => {
                            // Intra-`__TEXT` placement is eligible iff no
                            // exports / library deps are queued and some
                            // `__TEXT` free region holds the whole payload —
                            // the same test the post-commit branch uses.
                            let intra_fits = appended.exports.is_empty()
                                && binary.pending_library_deps.is_empty()
                                && img
                                    .layout
                                    .text_free_regions()
                                    .iter()
                                    .any(|r| {
                                        appended.segment_vaddr >= r.vaddr
                                            && appended.segment_vaddr
                                                + appended.bytes.len() as u64
                                                <= r.vaddr + r.size
                                    });
                            let base_in_text = img
                                .layout
                                .segments
                                .iter()
                                .find(|s| s.name == "__TEXT")
                                .map(|t| {
                                    appended.segment_vaddr >= t.vmaddr
                                        && appended.segment_vaddr < t.vmaddr + t.vmsize
                                })
                                .unwrap_or(false);
                            if base_in_text && !intra_fits {
                                // Same safe-base computation as
                                // `pick_append_vaddr`'s Mach-O segment
                                // fallback: sit a fixed gap above the highest
                                // mapped vaddr, page-aligned.
                                const PAGE: u64 = 0x4000; // arm64 macOS page size
                                const GAP: u64 = 16 * PAGE;
                                let max_input_vaddr = img.layout.max_vaddr_end();
                                let new_base = ((max_input_vaddr + GAP + PAGE - 1)
                                    & !(PAGE - 1))
                                    .max(PAGE);
                                (true, new_base)
                            } else {
                                (false, 0)
                            }
                        }
                        None => (false, 0),
                    };
                    if do_rebase {
                        // Rebase against the very container whose symbol
                        // table `commit_lifted_any` reads below, so the
                        // lifted-text trampolines resolve to the rebased
                        // (new-base) stub addresses. After this,
                        // `appended.segment_vaddr` / `appended.bytes` are
                        // already at the new base for both the text commit
                        // and the later `__APPENDED` segment construction.
                        appended.rebase(new_base, &mut binary.container)?;
                    }
                }

                // Run the in-place layout/emit for the lifted
                // section (if any) so branch redirects to new
                // functions get folded against the
                // (already-updated) symbol table. If no section
                // was lifted, skip in-section rewriting — only
                // whole-binary edits land.
                let mut overrides = std::collections::HashMap::new();
                let updated = match text {
                    Some(text) => {
                        let (updated, output, section_id_for_override) =
                            commit_lifted_any(&text, &binary.container)?;
                        let _ = output; // emit output captured for section override below
                        let section_index = section_id_for_override.0;
                        overrides.insert(
                            section_index,
                            updated.sections[section_index].bytes.clone(),
                        );
                        updated
                    }
                    None => binary.container.clone(),
                };

                if is_macho {
                    // Caller-staged section_overrides (e.g. from
                    // text-edit commits above) are merged with
                    // the editor's queue so the writer applies
                    // them at original file offsets before
                    // splicing in the new segment.
                    let mut all_overrides: Vec<(usize, Vec<u8>)> = overrides
                        .into_iter()
                        .collect();
                    for (idx, bytes) in &binary.section_overrides {
                        all_overrides.push((*idx, bytes.clone()));
                    }

                    // Intra-`__TEXT` placement (Phase 6.5):
                    // when the appended bytes fit in a
                    // `__TEXT` free region, place them there
                    // without growing the load-command list.
                    // Required for App Store submissions
                    // (which reject dylibs with multiple R-X
                    // segments). Only available when no
                    // exports / library deps are queued —
                    // those need `__LINKEDIT`-shifting flows
                    // that the appended-segment path
                    // provides.
                    let macho_image = updated
                        .macho_image
                        .as_ref()
                        .ok_or(TextEditorError::AppendMissingElfImage)?;
                    let intra_text_eligible = appended.exports.is_empty()
                        && binary.pending_library_deps.is_empty()
                        && macho_image
                            .layout
                            .text_free_regions()
                            .iter()
                            .any(|r| {
                                appended.segment_vaddr >= r.vaddr
                                    && appended.segment_vaddr + appended.bytes.len() as u64
                                        <= r.vaddr + r.size
                            });
                    if intra_text_eligible {
                        let region = macho_image
                            .layout
                            .text_free_regions()
                            .into_iter()
                            .find(|r| {
                                appended.segment_vaddr >= r.vaddr
                                    && appended.segment_vaddr + appended.bytes.len() as u64
                                        <= r.vaddr + r.size
                            })
                            .unwrap();
                        let file_offset = region.file_offset
                            + (appended.segment_vaddr - region.vaddr);
                        let intra = crate::container::macho_writer::IntraTextAppend {
                            vaddr: appended.segment_vaddr,
                            file_offset,
                            bytes: appended.bytes,
                        };
                        let opts = crate::container::macho_writer::MachOWriteOptions {
                            sign,
                            raw_byte_overrides: binary.macho_raw_byte_overrides.clone(),
                        };
                        return crate::container::macho_writer::write_with_intra_text_append_opts(
                            &updated, intra, &all_overrides, &opts,
                        )
                        .map_err(TextEditorError::from);
                    }

                    // The intra-`__TEXT` placement above didn't fire, so the
                    // payload doesn't fit a `__TEXT` free region. When the
                    // optimistic `pick_append_vaddr` base sits inside `__TEXT`,
                    // the payload was already rebased to a safe segment base
                    // above the mapped range at the START of this branch —
                    // BEFORE the lifted-text commit — so its trampolines
                    // resolve to the rebased stub addresses. By here,
                    // `appended.segment_vaddr` / `appended.bytes` (and the
                    // appended symbols in `updated`, derived from the rebased
                    // `binary.container`) already carry the new base; the
                    // `__APPENDED` segment is built directly from them with no
                    // further rebase.

                    // Phase 3+ append-segment fallback. Output
                    // will have an extra `__APPENDED` R-X
                    // segment, which works on macOS but won't
                    // pass App Store review.
                    //
                    // Phase 5: registered exports become
                    // MachOExportRequest entries the writer
                    // splices into LC_DYLD_EXPORTS_TRIE +
                    // LC_SYMTAB.
                    let macho_exports: Vec<crate::container::macho_writer::MachOExportRequest> =
                        appended
                            .exports
                            .iter()
                            .map(|exp| {
                                crate::container::macho_writer::MachOExportRequest {
                                    name: exp.name.clone(),
                                    vaddr: exp.vaddr,
                                }
                            })
                            .collect();
                    let segment = crate::container::macho_writer::AppendedSegment::new(
                        appended.segment_vaddr,
                        appended.bytes,
                    );
                    let opts = crate::container::macho_writer::MachOWriteOptions {
                        sign,
                        raw_byte_overrides: binary.macho_raw_byte_overrides.clone(),
                    };
                    return crate::container::macho_writer::write_with_appended_segment_opts(
                        &updated,
                        segment,
                        &all_overrides,
                        &macho_exports,
                        &binary.pending_library_deps,
                        &opts,
                    )
                    .map_err(TextEditorError::from);
                }

                let mut image = updated
                    .elf_image
                    .as_ref()
                    .ok_or(TextEditorError::AppendMissingElfImage)?
                    .clone();

                // Apply any queued whole-section overrides
                // (e.g. .rela.dyn rewritten by add_initialiser).
                // Insert in order so later queues win on the same
                // section index.
                for (idx, bytes) in &binary.section_overrides {
                    overrides.insert(*idx, bytes.clone());
                }

                // If the segment will mix code (function bodies
                // already in `appended.bytes`) with data (rebuilt
                // dynsym/dynstr or relocated `.dynamic`), pad the
                // code prefix up to APPEND_PAGE_ALIGN here, BEFORE
                // any extension routine assigns vaddrs to data.
                // Reason: the writer emits two PT_LOADs for split
                // segments (Android refuses W+X), so code and
                // data must occupy disjoint pages. If we don't
                // pad here, the data extensions get vaddrs
                // computed from `appended.bytes.len()` (unpadded),
                // but the writer would then have to shift the
                // bytes in-file to reach a page boundary —
                // breaking the vaddr → file_offset mapping the
                // editor already baked into `.dynamic` tags.
                // Padding upfront keeps vaddrs and file offsets
                // in sync.
                let needs_split = !appended.bytes.is_empty()
                    && (!appended.exports.is_empty()
                        || !binary.pending_library_deps.is_empty()
                        || !binary.pending_appended_init_slots.is_empty());
                if needs_split {
                    let cur = appended.bytes.len() as u64;
                    let aligned = (cur
                        + crate::container::elf_writer::APPEND_PAGE_ALIGN
                        - 1)
                        & !(crate::container::elf_writer::APPEND_PAGE_ALIGN - 1);
                    let pad = (aligned - cur) as usize;
                    if pad > 0 {
                        // NOP-pad — same instruction (`nop`)
                        // armv8-encode emits elsewhere when it
                        // needs filler in executable regions.
                        let nop = [0x1fu8, 0x20, 0x03, 0xd5];
                        let mut padded = Vec::with_capacity(appended.bytes.len() + pad);
                        padded.extend_from_slice(&appended.bytes);
                        for _ in 0..(pad / 4) {
                            padded.extend_from_slice(&nop);
                        }
                        for _ in 0..(pad % 4) {
                            padded.push(0);
                        }
                        appended.bytes = padded;
                    }
                }

                // If exports or library dependencies were
                // registered, extend the dynsym family / dynstr
                // and pack the new copies into the appended
                // segment at the end.
                let needs_dynsym_dynstr_rebuild = !appended.exports.is_empty()
                    || !binary.pending_library_deps.is_empty();
                let mut segment_bytes = if !needs_dynsym_dynstr_rebuild {
                    appended.bytes.clone()
                } else {
                    BinaryState::extend_segment_for_exports(
                        appended.segment_vaddr,
                        &appended.bytes,
                        &appended.exports,
                        &binary.pending_library_deps,
                        &image,
                        &updated,
                        &mut overrides,
                    )?
                };
                // If add_initialiser(Append) was called, extend
                // the segment with a rebuilt .init_array (and
                // matching .rela.dyn) and stage the .dynamic
                // tag updates that point the loader at them.
                if !binary.pending_appended_init_slots.is_empty() {
                    segment_bytes = BinaryState::extend_segment_for_appended_init_slots(
                        appended.segment_vaddr,
                        segment_bytes,
                        &binary.pending_appended_init_slots,
                        &image,
                        &updated,
                        &mut overrides,
                    )?;
                }
                // Finalizer: if the staged `.dynamic` override
                // grew past the original section's byte length,
                // relocate `.dynamic` into the appended segment
                // and patch PT_DYNAMIC to point at it.
                segment_bytes = BinaryState::finalize_dynamic_size_or_relocate(
                    appended.segment_vaddr,
                    segment_bytes,
                    &mut image,
                    &updated,
                    &mut overrides,
                )?;
                // The first `appended.bytes.len()` bytes of the
                // segment are executable code (function bodies +
                // any page-padding the split branch above
                // inserted). `extend_segment_for_exports` and
                // friends append data after that. The writer uses
                // `code_len` to choose between a single PT_LOAD
                // (all-code or all-data) and a split (R+X over
                // the code prefix, R+W over the data suffix).
                // Android refuses W+X PT_LOADs, so any append
                // that mixes both must split.
                let original_code_len = appended.bytes.len() as u64;
                let no_executable_append = appended.exports.is_empty()
                    && appended.bytes.is_empty()
                    && binary.pending_appended_init_slots.is_empty();
                let mut segment = crate::container::elf_writer::AppendedSegment::new(
                    appended.segment_vaddr,
                    segment_bytes,
                );
                if no_executable_append {
                    use object::elf;
                    segment.p_flags = elf::PF_R | elf::PF_W;
                } else if original_code_len > 0
                    && (original_code_len as usize) < segment.bytes.len()
                {
                    // Mixed code + data — request a writer split.
                    segment.code_len = original_code_len;
                } else {
                    // Pure-code append (no exports/dyn rebuild).
                    use object::elf;
                    segment.p_flags = elf::PF_R | elf::PF_X;
                }
                let writable_seg = binary.appended_writable.as_ref().map(|w| {
                    crate::container::elf_writer::WritableSegment {
                        vaddr: w.segment_vaddr,
                        bytes: w.bytes.clone(),
                        section_name: ".data.armv8_encode_writable".to_string(),
                    }
                });
                crate::container::elf_writer::write_with_appended_segment_inner(
                    &updated, &image, segment, overrides, writable_seg,
                )
                .map_err(TextEditorError::from)
            }
        }
    }
}

/// Append `payload` to `segment_bytes` at the next `align`-aligned
/// offset and return the virtual address it lands at. Used by
/// [`TextEditor::extend_segment_for_exports`] to pack the four
/// rebuilt sections after the function bodies.
fn pack(segment_bytes: &mut Vec<u8>, segment_vaddr: u64, payload: &[u8], align: u64) -> u64 {
    if align > 1 {
        let current = segment_bytes.len() as u64;
        let aligned = (current + align - 1) & !(align - 1);
        if aligned > current {
            segment_bytes.resize(aligned as usize, 0);
        }
    }
    let offset = segment_bytes.len() as u64;
    segment_bytes.extend_from_slice(payload);
    segment_vaddr + offset
}

/// Read a NUL-terminated name at `offset` in a string-table-style
/// byte slice. Returns an empty slice if the offset is out of
/// range or no NUL is found before the end (defensive — callers
/// should pass valid offsets).
fn name_at_offset(strtab: &[u8], offset: u32) -> &[u8] {
    let offset = offset as usize;
    if offset >= strtab.len() {
        return &[];
    }
    match strtab[offset..].iter().position(|&b| b == 0) {
        Some(end) => &strtab[offset..offset + end],
        None => &[],
    }
}
