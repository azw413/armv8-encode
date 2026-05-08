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

                // If exports were registered, extend the dynsym
                // family and pack the new copies into the
                // appended segment at the end. This produces
                // additional bytes that follow the function/data
                // bodies.
                let segment_bytes = if appended.exports.is_empty() {
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
