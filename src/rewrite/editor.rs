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

use crate::container::{Container, ContainerWriteError, SectionId, Symbol, SymbolId, SymbolKind};
use crate::isa::aarch64::{self, DecodedInstruction, DisassembleError, EncodeError};
use crate::mc::{build_cfg, ControlFlowGraph};
use crate::rewrite::commit::commit_to_container;
use crate::rewrite::emit::{emit, EmitError};
use crate::rewrite::ir::{RewriteInstruction, Target};
use crate::rewrite::layout::{lay_out, LayoutError};
use crate::rewrite::plan::{EditError, RewritePlan};
use crate::rewrite::EmitOutput;

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

/// Editable view over one named text section of a [`Container`].
///
/// Created by [`Self::for_section`]; modified by the proxy methods
/// (`redirect_branch_at`, `replace_instruction_at`, etc.); resolved
/// by [`Self::commit`] which produces the rewritten container.
///
/// The editor borrows the source container during construction
/// (cheap — the container's contents are cloned in
/// [`commit_to_container`] anyway); after construction it owns its
/// own state.
#[derive(Debug, Clone)]
pub struct TextEditor {
    /// The container being edited. Mutated as functions are added
    /// (their symbols land in `container.symbols`); the existing
    /// section table and bytes stay otherwise unchanged until
    /// [`Self::commit`] runs the layout pipeline.
    container: Container,
    /// Section id we're rewriting.
    section_id: SectionId,
    /// Base address the section loads at.
    base_address: u64,
    /// Decoded instructions for the section (cached so subsequent
    /// edit operations don't re-disassemble).
    instructions: Vec<DecodedInstruction>,
    /// CFG built from the instructions; lift consumes both.
    cfg: ControlFlowGraph,
    /// The mutable plan. Edit primitives delegate to this.
    plan: RewritePlan,
    /// Functions added via [`Self::add_function`]. Each entry is
    /// already laid out and emitted at its assigned virtual
    /// address; commit only needs to concatenate and append.
    /// `None` until the first add_function call so containers that
    /// only do in-place edits keep going through the cheaper
    /// in-place writer path.
    appended: Option<AppendedFunctionsState>,
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

impl TextEditor {
    /// Lift the named text section of `container` into an editor.
    ///
    /// `name` is matched literally against `container.sections[i].name`.
    /// The section must be a [`SectionKind::Text`](crate::container::SectionKind::Text);
    /// non-text sections return [`TextEditorError::SectionNotText`].
    pub fn for_section(container: &Container, name: &str) -> Result<Self, TextEditorError> {
        let section = container
            .sections
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| TextEditorError::SectionNotFound(name.to_string()))?;

        let (base, code) = section
            .for_disassembly()
            .ok_or_else(|| TextEditorError::SectionNotText {
                name: name.to_string(),
            })?;

        let instructions = aarch64::disassemble_bytes(base, code)?;
        let cfg = build_cfg(&instructions);
        let plan = RewritePlan::lift_with_container(&cfg, &instructions, container);

        Ok(Self {
            container: container.clone(),
            section_id: section.id,
            base_address: base,
            instructions,
            cfg,
            plan,
            appended: None,
            section_overrides: Vec::new(),
            pending_appended_init_slots: Vec::new(),
        })
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

    /// Iterate over symbols defined in the section being edited.
    /// Useful for "edit every function in `.text`" workflows.
    pub fn symbols_in_section(&self) -> impl Iterator<Item = &Symbol> + '_ {
        let id = self.section_id;
        self.container
            .symbols
            .iter()
            .filter(move |s| s.section == Some(id) && !s.is_undefined)
    }

    /// Address of the function symbol with this name, if it lives
    /// in the section being edited. Convenience for "I want to
    /// redirect the first instruction of `foo`."
    pub fn function_address(&self, name: &str) -> Option<u64> {
        self.container
            .symbols
            .iter()
            .find(|s| {
                s.name == name && s.kind == SymbolKind::Function && s.section == Some(self.section_id)
            })
            .map(|s| s.address)
    }

    /// Direct access to the underlying [`RewritePlan`]. Use this
    /// when you need to inspect or mutate the plan beyond what
    /// the editor's proxy methods expose.
    pub fn plan_mut(&mut self) -> &mut RewritePlan {
        &mut self.plan
    }

    /// Read-only view of the [`RewritePlan`].
    pub fn plan(&self) -> &RewritePlan {
        &self.plan
    }

    /// Decoded instructions for the section, in source order.
    /// Useful when you need to know what's at a given address
    /// before deciding how to rewrite it.
    pub fn instructions(&self) -> &[DecodedInstruction] {
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
    ///
    /// This wraps the lower-level pattern of locating the op,
    /// confirming it's a singleton instruction, and overwriting
    /// it. For macro ops use [`Self::redirect_macro_target_at`].
    pub fn replace_instruction_at(
        &mut self,
        address: u64,
        new_instruction: RewriteInstruction,
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
        new_instructions: Vec<RewriteInstruction>,
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
    pub fn add_function(
        &mut self,
        name: &str,
        instructions: Vec<RewriteInstruction>,
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

        // Allocate vaddr for this function. First call picks the
        // segment vaddr from the input's PT_LOAD extents; later
        // calls pack into the same segment in order.
        let segment_vaddr = match &self.appended {
            Some(state) => state.segment_vaddr,
            None => {
                // Re-implement `pick_append_vaddr` locally so we
                // don't take a private dependency on the writer
                // module's helpers. This is just "max p_vaddr +
                // p_memsz, page-aligned up."
                let image = self
                    .container
                    .elf_image
                    .as_ref()
                    .ok_or(TextEditorError::AppendMissingElfImage)?;
                let max_input_vaddr = image
                    .program_headers
                    .iter()
                    .filter(|p| p.p_type == object::elf::PT_LOAD)
                    .map(|p| p.p_vaddr.saturating_add(p.p_memsz))
                    .max()
                    .unwrap_or(0);
                const PAGE: u64 = 0x10000;
                let aligned = (max_input_vaddr + PAGE - 1) & !(PAGE - 1);
                aligned.max(PAGE)
            }
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
            size: (instructions.len() * 4) as u64,
            kind: SymbolKind::Function,
            binding: crate::container::SymbolBinding::Global,
            section: None,
            is_undefined: false,
            flags: None,
        });

        // Lay out + emit the new function at its assigned vaddr.
        // `from_instructions` runs the same fusion pass `lift`
        // does, so caller-supplied `adrp + add` pairs against a
        // [`Target::Symbol`] page operand fuse into a
        // [`MacroKind::LoadAddress`] macro that emit resolves at
        // the function's final vaddr.
        let plan = RewritePlan::from_instructions(instructions, Some(&self.container));

        let layout = lay_out(&plan, function_vaddr, Some(&self.container))?;
        let output = emit(&plan, &layout, Some(&self.container))?;

        // Append to the cumulative segment buffer.
        let state = self.appended.get_or_insert(AppendedFunctionsState {
            segment_vaddr,
            bytes: Vec::new(),
            exports: Vec::new(),
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
            // The .dynamic must have room for the tags we'll
            // add. DT_INIT_ARRAY + DT_INIT_ARRAYSZ if absent;
            // otherwise zero (we update existing). DT_RELASZ /
            // DT_RELACOUNT etc. are always present so we never
            // need to add them.
            let image = self
                .container
                .elf_image
                .as_ref()
                .ok_or(TextEditorError::AppendMissingElfImage)?;
            let has_init_array_tag = image
                .dynamic
                .iter()
                .any(|e| e.tag == object::elf::DT_INIT_ARRAY as u64);
            let needed_new_tags = if has_init_array_tag { 0 } else { 2 };
            if needed_new_tags > 0 {
                use object::elf::DT_NULL;
                let null_count = image
                    .dynamic
                    .iter()
                    .filter(|e| e.tag == DT_NULL as u64)
                    .count();
                // Need a terminator DT_NULL to remain after we
                // overwrite some, so total nulls must exceed
                // needed_new_tags.
                if null_count <= needed_new_tags {
                    return Err(TextEditorError::DynamicTooFull {
                        needed: needed_new_tags,
                    });
                }
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
        let rela_dyn_bytes = &self.container.sections[rela_dyn_idx].bytes;
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
        let mut new_rela_dyn = self.container.sections[rela_dyn_idx].bytes.clone();
        new_rela_dyn[rela_entry_off + 16..rela_entry_off + 24]
            .copy_from_slice(&(wrapper_vaddr as i64).to_le_bytes());
        self.section_overrides.push((rela_dyn_idx, new_rela_dyn));

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
    /// flag), which fits read-only data and code. Writable data
    /// would need a separate RW segment — not yet supported.
    ///
    /// `align` is the byte alignment the blob requires (1 for
    /// strings, 4 for u32 tables, etc.). The cumulative segment
    /// is padded to satisfy it.
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

        // Initialise the appended state lazily — same pattern as
        // add_function.
        let segment_vaddr = match &self.appended {
            Some(state) => state.segment_vaddr,
            None => {
                let image = self
                    .container
                    .elf_image
                    .as_ref()
                    .ok_or(TextEditorError::AppendMissingElfImage)?;
                let max_input_vaddr = image
                    .program_headers
                    .iter()
                    .filter(|p| p.p_type == object::elf::PT_LOAD)
                    .map(|p| p.p_vaddr.saturating_add(p.p_memsz))
                    .max()
                    .unwrap_or(0);
                const PAGE: u64 = 0x10000;
                let aligned = (max_input_vaddr + PAGE - 1) & !(PAGE - 1);
                aligned.max(PAGE)
            }
        };

        // Pad cumulative bytes up to the requested alignment.
        let state = self.appended.get_or_insert(AppendedFunctionsState {
            segment_vaddr,
            bytes: Vec::new(),
            exports: Vec::new(),
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
    pub fn commit(self) -> Result<Container, TextEditorError> {
        let layout = lay_out(&self.plan, self.base_address, Some(&self.container))?;
        let output: EmitOutput = emit(&self.plan, &layout, Some(&self.container))?;
        let edited = commit_to_container(&self.container, self.section_id, output);
        Ok(edited)
    }

    /// Like [`Self::commit`] but also serializes the resulting
    /// container to bytes. Convenience for the common case where
    /// the caller wants a runnable `.so`/`.o` blob immediately.
    ///
    /// When functions were registered via [`Self::add_function`],
    /// this drives the elf_writer's append-segment path so the
    /// new functions land in a fresh PT_LOAD R-X segment beyond
    /// the input's mapped range.
    pub fn commit_to_bytes(self) -> Result<Vec<u8>, TextEditorError> {
        match self.appended {
            None => {
                // No appended functions — straightforward path.
                let edited = self.commit_in_place()?;
                edited.to_bytes().map_err(TextEditorError::from)
            }
            Some(appended) => {
                // Run the in-place layout/emit for the existing
                // section so any branch redirects to new functions
                // get folded against the (already-updated)
                // symbol table.
                let layout = lay_out(&self.plan, self.base_address, Some(&self.container))?;
                let output = emit(&self.plan, &layout, Some(&self.container))?;
                let updated = commit_to_container(&self.container, self.section_id, output);

                let image = updated
                    .elf_image
                    .as_ref()
                    .ok_or(TextEditorError::AppendMissingElfImage)?
                    .clone();
                let mut overrides = std::collections::HashMap::new();
                let section_index = self.section_id.0;
                overrides.insert(
                    section_index,
                    updated.sections[section_index].bytes.clone(),
                );
                // Apply any queued whole-section overrides
                // (e.g. .rela.dyn rewritten by add_initialiser).
                // Insert in order so later queues win on the same
                // section index.
                for (idx, bytes) in &self.section_overrides {
                    overrides.insert(*idx, bytes.clone());
                }

                // If exports were registered, extend the dynsym
                // family and pack the new copies into the
                // appended segment at the end. This produces
                // additional bytes that follow the function/data
                // bodies.
                let mut segment_bytes = if appended.exports.is_empty() {
                    appended.bytes.clone()
                } else {
                    Self::extend_segment_for_exports(
                        appended.segment_vaddr,
                        &appended.bytes,
                        &appended.exports,
                        &image,
                        &updated,
                        &mut overrides,
                    )?
                };
                // If add_initialiser(Append) was called, extend
                // the segment with a rebuilt .init_array (and
                // matching .rela.dyn) and stage the .dynamic
                // tag updates that point the loader at them.
                if !self.pending_appended_init_slots.is_empty() {
                    segment_bytes = Self::extend_segment_for_appended_init_slots(
                        appended.segment_vaddr,
                        segment_bytes,
                        &self.pending_appended_init_slots,
                        &image,
                        &updated,
                        &mut overrides,
                    )?;
                }
                let segment = crate::container::elf_writer::AppendedSegment::new(
                    appended.segment_vaddr,
                    segment_bytes,
                );
                crate::container::elf_writer::write_with_appended_segment_inner(
                    &updated, &image, segment, overrides,
                )
                .map_err(TextEditorError::from)
            }
        }
    }

    /// Pack extended `.dynsym` / `.dynstr` / `.gnu.version` /
    /// `.gnu.hash` blobs into the appended segment after the
    /// existing bytes, and stage a section-bytes override for
    /// `.dynamic` so its DT_SYMTAB / DT_STRTAB / DT_STRSZ /
    /// DT_GNU_HASH / DT_VERSYM tags follow the new copies.
    ///
    /// Returns the extended segment bytes. The original `.dynsym`
    /// etc. sections in the file stay in place but become
    /// orphaned — the dynamic linker follows the `.dynamic` tags
    /// (by virtual address) and never reads them again.
    fn extend_segment_for_exports(
        segment_vaddr: u64,
        existing_segment_bytes: &[u8],
        exports: &[ExportedSymbol],
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
        let mut dynsym_entries = dx::parse_dynsym(&source_dynsym.bytes);
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

        // Rebuild .gnu.hash from the full extended dynsym.
        // symbol_base is the index of the first hashable entry in
        // the source dynsym (read it from the existing .gnu.hash
        // header).
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

        // nbuckets=1 keeps the layout invariant simple regardless
        // of where new exports land. bloom_size=1 (one 64-bit
        // word) is fine for small export sets; bloom_shift=6
        // matches what GNU emits for ELF64.
        let new_gnu_hash_bytes =
            gnu_hash::build_gnu_hash(&hashable, symbol_base, 1, 1, 6);
        let new_dynsym_bytes = dx::encode_dynsym(&dynsym_entries);

        // Pack the four blobs into the segment with appropriate
        // alignment. Order: existing segment content → dynsym →
        // dynstr → versym → gnu_hash. dynsym and gnu_hash align
        // to 8; versym aligns to 2; dynstr aligns to 1.
        let mut segment_bytes = existing_segment_bytes.to_vec();

        let dynsym_vaddr = pack(&mut segment_bytes, segment_vaddr, &new_dynsym_bytes, 8);
        let dynstr_vaddr = pack(&mut segment_bytes, segment_vaddr, &dynstr_bytes, 1);
        let versym_vaddr = pack(&mut segment_bytes, segment_vaddr, &versym_bytes, 2);
        let gnu_hash_vaddr =
            pack(&mut segment_bytes, segment_vaddr, &new_gnu_hash_bytes, 8);

        // Build a fresh .dynamic tag list pointing at the new
        // copies. We update the address-bearing tags and DT_STRSZ
        // (since dynstr grew).
        use object::elf::*;
        let updates: &[(u64, u64)] = &[
            (DT_SYMTAB as u64, dynsym_vaddr),
            (DT_STRTAB as u64, dynstr_vaddr),
            (DT_STRSZ as u64, dynstr_bytes.len() as u64),
            (DT_GNU_HASH as u64, gnu_hash_vaddr),
            (DT_VERSYM as u64, versym_vaddr),
        ];
        let new_dynamic_entries = dx::update_dynamic_tags(&image.dynamic, updates);
        let new_dynamic_bytes = dx::encode_dynamic(&new_dynamic_entries);

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

        // Update or insert .dynamic tags.
        let has_init_array_tag = image
            .dynamic
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
        let mut new_dynamic_entries = dx::update_dynamic_tags(&image.dynamic, updates);
        if !has_init_array_tag {
            // Need to actually add the tags rather than rely on
            // update_dynamic_tags (which only modifies
            // existing). insert_dynamic_tags consumes trailing
            // DT_NULL slots so the byte length stays the same.
            let additions = &[
                (DT_INIT_ARRAY as u64, new_init_array_vaddr),
                (DT_INIT_ARRAYSZ as u64, new_init_array_size),
            ];
            new_dynamic_entries =
                dx::insert_dynamic_tags(&new_dynamic_entries, additions).ok_or(
                    TextEditorError::DynamicTooFull { needed: additions.len() },
                )?;
        }
        let new_dynamic_bytes = dx::encode_dynamic(&new_dynamic_entries);
        let dynamic_index = container
            .sections
            .iter()
            .position(|s| s.name == ".dynamic")
            .ok_or(TextEditorError::AppendMissingElfImage)?;
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

    /// In-place commit (no appended functions). Same as
    /// [`Self::commit`] without consuming `appended`. Used
    /// internally; not exposed because callers already have
    /// `commit` for this case.
    fn commit_in_place(self) -> Result<Container, TextEditorError> {
        let layout = lay_out(&self.plan, self.base_address, Some(&self.container))?;
        let output: EmitOutput = emit(&self.plan, &layout, Some(&self.container))?;
        let edited = commit_to_container(&self.container, self.section_id, output);
        Ok(edited)
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
