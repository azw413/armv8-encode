//! Symbolic, relocation-aware data-section rewriting.
//!
//! The data layer is to `.rodata`/`.data` what [`crate::rewrite::plan`] is
//! to `.text`: it turns a section's raw bytes plus relocations into an
//! editable IR whose pointer-shaped slots carry [`Target`] values rather
//! than frozen 64-bit addresses. Edit, lay out, emit, splice back.
//!
//! ## What it covers
//!
//! - Sections whose internal layout is a sequence of `(label, payload)`
//!   items: vtables, function-pointer arrays, `__init_array`-style
//!   tables, opaque byte blobs. Pointer slots are recovered from
//!   `R_AARCH64_ABS64` relocations.
//! - Atomic byte+relocation replacement on commit (the same contract as
//!   the text rewriter — see [`commit_to_data_container`]).
//!
//! ## What it does *not* cover (yet)
//!
//! - Mergeable string sections (`SHF_MERGE | SHF_STRINGS`,
//!   e.g. `.rodata.str1.1`). The linker may dedup identical strings
//!   across compilation units; rebuilding such a section opaquely
//!   could break that. We treat any section we don't know how to
//!   structure as a single [`DataItem::Bytes`] — round-trips correctly
//!   so long as it has no internal relocations, which is the common
//!   case for `.rodata.str*` (relocations point *at* it, not *into* it).
//! - Alignment-derived padding inserted *between* items. Lift records
//!   the alignment the source had per item; emit honors it. We don't
//!   try to recover or invent padding the source didn't already have.
//! - Non-Absolute relocation kinds inside data. AArch64 ELF data
//!   sections almost exclusively use `R_AARCH64_ABS64`/`ABS32`; if a
//!   data section carries something else we lift the slot as opaque
//!   bytes and pass the relocation through structurally via
//!   [`commit_to_data_container`]'s "unhandled" passthrough.

use crate::container::{
    Container, Relocation, RelocationId, RelocationKind, Section, SectionId, SymbolId,
};
use crate::rewrite::ir::Target;

/// An editable, symbolic representation of a data section.
///
/// Items appear in source order; emit reproduces them in the same
/// order. Each item knows its own alignment requirement so that
/// inserting or resizing an item doesn't silently shift others.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DataSection {
    /// Source section this plan was lifted from. Used by
    /// [`commit_to_data_container`] to splice the emitted bytes and
    /// relocations back into the right slot. `None` for plans built
    /// from scratch.
    pub source_section: Option<SectionId>,
    pub items: Vec<DataItem>,
}

/// One unit of content inside a data section.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DataItem {
    /// Optional label for this item — populated when a defined symbol
    /// in the source container pointed at this exact byte offset. Lets
    /// callers find an item by name (e.g. `vtable_for_Foo`) and
    /// preserves the symbol's section-internal address through
    /// edit/emit.
    pub label: Option<SymbolId>,
    /// Required alignment in bytes. Lift records this from the
    /// source's section alignment plus the item's offset; emit honors
    /// it by inserting padding bytes when the running offset isn't
    /// aligned.
    pub align: u64,
    pub payload: DataPayload,
}

/// Either raw bytes or a symbolic pointer.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DataPayload {
    /// Opaque byte content — strings, integer constants, padding,
    /// anything the rewriter doesn't model symbolically.
    Bytes(Vec<u8>),
    /// A pointer slot. Emit produces `width_bytes` zero bytes plus a
    /// matching [`crate::rewrite::EmittedRelocation`]; the linker
    /// patches the actual address at link time.
    Pointer {
        target: Target,
        addend: i64,
        width_bytes: u8,
    },
}

impl DataPayload {
    pub fn byte_size(&self) -> usize {
        match self {
            DataPayload::Bytes(bytes) => bytes.len(),
            DataPayload::Pointer { width_bytes, .. } => *width_bytes as usize,
        }
    }
}

impl DataItem {
    pub fn byte_size(&self) -> usize {
        self.payload.byte_size()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DataLiftError {
    /// The given section id doesn't exist in the container.
    UnknownSection(SectionId),
    /// A relocation in this section pointed at an offset beyond the
    /// section's byte content. Indicates a malformed input.
    RelocationOutOfBounds { offset: u64, section_size: u64 },
    /// Two relocations claimed overlapping byte ranges in the same
    /// section. AArch64 data relocations don't overlap in well-formed
    /// input; if this fires the input is malformed or the rewriter is
    /// looking at a section it doesn't understand.
    OverlappingRelocations { first_offset: u64, second_offset: u64 },
}

impl std::fmt::Display for DataLiftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSection(id) => write!(f, "no section with id {id:?}"),
            Self::RelocationOutOfBounds {
                offset,
                section_size,
            } => write!(
                f,
                "relocation offset {offset:#x} is past section end {section_size:#x}",
            ),
            Self::OverlappingRelocations {
                first_offset,
                second_offset,
            } => write!(
                f,
                "overlapping data relocations at {first_offset:#x} and \
                 {second_offset:#x}",
            ),
        }
    }
}

impl std::error::Error for DataLiftError {}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DataEditError {
    /// No item with the requested label.
    LabelNotFound(SymbolId),
    /// The item at this index isn't a `Pointer` — caller asked for a
    /// pointer-only edit on a `Bytes` item.
    NotAPointer(usize),
    /// Index out of bounds for the items list.
    IndexOutOfBounds(usize),
}

impl std::fmt::Display for DataEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LabelNotFound(id) => write!(f, "no item labelled with symbol {id:?}"),
            Self::NotAPointer(idx) => write!(f, "item at index {idx} is not a Pointer"),
            Self::IndexOutOfBounds(idx) => write!(f, "item index {idx} out of bounds"),
        }
    }
}

impl std::error::Error for DataEditError {}

impl DataSection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lift `section_id` into a data plan, splitting at every
    /// `Absolute`/64-bit relocation into a `Pointer` item. Bytes
    /// outside relocation ranges become `Bytes` items.
    ///
    /// Relocations whose kind isn't a recognized data shape (anything
    /// other than `Absolute`) are *not* lifted — they stay attached to
    /// the original container and survive via the structural relocation
    /// list. Lift logs them via the result's
    /// [`DataLift::unhandled_relocations`] so callers can route them
    /// through `commit_to_data_container`.
    pub fn lift(container: &Container, section_id: SectionId) -> Result<DataLift, DataLiftError> {
        let section = container
            .sections
            .iter()
            .find(|s| s.id == section_id)
            .ok_or(DataLiftError::UnknownSection(section_id))?;

        let mut handled = Vec::new();
        let mut unhandled = Vec::new();
        for relocation in container.relocations_for(section_id) {
            if relocation.offset >= section.size {
                return Err(DataLiftError::RelocationOutOfBounds {
                    offset: relocation.offset,
                    section_size: section.size,
                });
            }
            match data_relocation_width(relocation.kind) {
                Some(width) => handled.push(LiftedReloc {
                    offset: relocation.offset,
                    width,
                    target: relocation
                        .symbol
                        .map(Target::Symbol)
                        .unwrap_or(Target::Absolute(0)),
                    addend: relocation.addend,
                }),
                None => unhandled.push(relocation.clone()),
            }
        }

        // Sort by offset so we can walk both bytes and relocations in
        // tandem. Detect overlaps while we're at it.
        handled.sort_by_key(|r| r.offset);
        for window in handled.windows(2) {
            let a = &window[0];
            let b = &window[1];
            if a.offset + a.width as u64 > b.offset {
                return Err(DataLiftError::OverlappingRelocations {
                    first_offset: a.offset,
                    second_offset: b.offset,
                });
            }
        }

        // Build a "labels at offset" map from defined symbols in this
        // section. A symbol whose section is `section_id` and whose
        // address falls inside the section's range labels the item
        // starting at that offset.
        let labels = symbol_labels_for_section(container, section);

        let items = lift_items(section, &handled, &labels);

        Ok(DataLift {
            plan: DataSection {
                source_section: Some(section_id),
                items,
            },
            unhandled_relocations: unhandled,
        })
    }

    /// Find an item by its source symbol label.
    pub fn find_by_label(&self, label: SymbolId) -> Option<usize> {
        self.items
            .iter()
            .position(|item| item.label == Some(label))
    }

    /// Convenience: change the target of an existing pointer item by
    /// label. Returns the new target's old value.
    pub fn redirect_pointer(
        &mut self,
        label: SymbolId,
        new_target: Target,
    ) -> Result<Target, DataEditError> {
        let index = self.find_by_label(label).ok_or(DataEditError::LabelNotFound(label))?;
        self.redirect_pointer_at(index, new_target)
    }

    /// Like [`Self::redirect_pointer`] but indexed positionally.
    pub fn redirect_pointer_at(
        &mut self,
        index: usize,
        new_target: Target,
    ) -> Result<Target, DataEditError> {
        let item = self
            .items
            .get_mut(index)
            .ok_or(DataEditError::IndexOutOfBounds(index))?;
        match &mut item.payload {
            DataPayload::Pointer { target, .. } => {
                let old = *target;
                *target = new_target;
                Ok(old)
            }
            DataPayload::Bytes(_) => Err(DataEditError::NotAPointer(index)),
        }
    }
}

/// Result of lifting one section: the editable plan plus any
/// relocations the lift didn't recognize. Callers route the unhandled
/// ones through [`commit_to_data_container`] so they survive the
/// round-trip even when we can't model them symbolically.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DataLift {
    pub plan: DataSection,
    pub unhandled_relocations: Vec<Relocation>,
}

/// Result of laying out and emitting a [`DataSection`].
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DataEmitOutput {
    pub bytes: Vec<u8>,
    pub relocations: Vec<crate::rewrite::EmittedRelocation>,
}

/// Lay out `plan` and emit its bytes + relocations. Items are written
/// in order with alignment padding (`0x00` bytes) inserted as needed.
pub fn emit_data_section(plan: &DataSection) -> DataEmitOutput {
    let mut output = DataEmitOutput::default();

    for item in &plan.items {
        if item.align > 1 {
            let padding = align_padding(output.bytes.len() as u64, item.align);
            output.bytes.resize(output.bytes.len() + padding as usize, 0);
        }
        match &item.payload {
            DataPayload::Bytes(bytes) => {
                output.bytes.extend_from_slice(bytes);
            }
            DataPayload::Pointer {
                target,
                addend,
                width_bytes,
            } => {
                let offset = output.bytes.len() as u64;
                output
                    .bytes
                    .extend(std::iter::repeat(0u8).take(*width_bytes as usize));
                if let Target::Symbol(symbol) = target {
                    output.relocations.push(crate::rewrite::EmittedRelocation {
                        offset,
                        kind: relocation_kind_for_pointer_width(*width_bytes),
                        symbol: *symbol,
                        addend: *addend,
                    });
                }
                // Target::Absolute / Block / Constant inside data
                // aren't yet handled — they'd need a constant pool or
                // a way to compute the absolute address at emit time.
                // For now we emit zero bytes and no relocation; the
                // linker leaves them as zero, which is the same as
                // before stage 3. Symbolic edits via `redirect_pointer`
                // can produce these states; we accept the silent zero
                // rather than erroring so partial editing flows still
                // make progress.
            }
        }
    }

    output
}

/// Splice the result of [`emit_data_section`] back into a container.
///
/// Mirrors [`crate::rewrite::commit_to_container`]'s contract: the
/// targeted section's bytes are replaced wholesale, its existing
/// relocations are dropped, and the freshly-emitted ones replace them.
/// Relocations on other sections are untouched.
///
/// `extra_relocations` carries any relocations the lift step didn't
/// recognize (returned via [`DataLift::unhandled_relocations`]).
/// Callers should pass that vec verbatim; if they have post-edit
/// reasoning about whether to keep them, they can filter beforehand.
pub fn commit_to_data_container(
    container: &Container,
    section: SectionId,
    output: DataEmitOutput,
    extra_relocations: Vec<Relocation>,
) -> Container {
    let mut edited = container.with_section_bytes(section, output.bytes);

    edited
        .relocations
        .retain(|relocation| relocation.section != section);

    for emitted in output.relocations {
        edited.relocations.push(Relocation {
            id: RelocationId(edited.relocations.len()),
            section,
            offset: emitted.offset,
            kind: emitted.kind,
            size: relocation_size_bits(emitted.kind),
            addend: emitted.addend,
            symbol: Some(emitted.symbol),
        });
    }
    for mut extra in extra_relocations {
        // Re-id so the relocation list stays dense and the new id
        // doesn't collide with anything carried over from elsewhere.
        extra.id = RelocationId(edited.relocations.len());
        extra.section = section;
        edited.relocations.push(extra);
    }

    edited
}

// ---- internals --------------------------------------------------------

#[derive(Debug, Clone)]
struct LiftedReloc {
    offset: u64,
    width: u8,
    target: Target,
    addend: i64,
}

/// Map a relocation kind to its byte width if it represents a symbolic
/// pointer slot in a data section. `None` for kinds that aren't
/// pointer-shaped (branch displacements, page-relative references).
fn data_relocation_width(kind: RelocationKind) -> Option<u8> {
    match kind {
        RelocationKind::Absolute => Some(8),
        // 32-bit absolute is `R_AARCH64_ABS32`; the reader collapses
        // it to RelocationKind::Absolute today (see
        // map_elf_relocation), so the 8-byte assumption is correct
        // for the inputs we currently produce. If/when the reader
        // gains an `Absolute32` variant, add a 4-byte arm here.
        _ => None,
    }
}

fn relocation_kind_for_pointer_width(width_bytes: u8) -> RelocationKind {
    match width_bytes {
        // Fall back to Absolute (64-bit) for anything we don't
        // explicitly recognise. Only 8 is reachable today; 4 is
        // reserved for when we split Absolute into 32/64 variants.
        _ => RelocationKind::Absolute,
    }
}

fn relocation_size_bits(kind: RelocationKind) -> u8 {
    match kind {
        RelocationKind::Absolute => 64,
        RelocationKind::Branch26 => 26,
        RelocationKind::Branch19 => 19,
        RelocationKind::Branch14 => 14,
        RelocationKind::AdrpPage21 => 21,
        RelocationKind::AddPageOffset12 => 12,
        RelocationKind::LoadStorePageOffset12 { .. } => 12,
        // ARMv7.
        RelocationKind::ArmCall
        | RelocationKind::ArmJump24
        | RelocationKind::ArmPc24 => 24,
        RelocationKind::ArmRelative
        | RelocationKind::ArmGlobData
        | RelocationKind::ArmJumpSlot
        | RelocationKind::ArmAbs32 => 32,
        RelocationKind::ArmMovwAbsNc | RelocationKind::ArmMovtAbs => 16,
        RelocationKind::ThumbCall | RelocationKind::ThumbJump24 => 24,
        RelocationKind::ThumbJump19 => 19,
        RelocationKind::ThumbMovwAbsNc | RelocationKind::ThumbMovtAbs => 16,
        // x86.
        RelocationKind::X86Pc32
        | RelocationKind::X86Plt32
        | RelocationKind::X86GotPcRel
        | RelocationKind::X86Abs32 => 32,
        RelocationKind::X86Abs64 => 64,
        RelocationKind::Other(_) => 32,
    }
}

fn symbol_labels_for_section(
    container: &Container,
    section: &Section,
) -> std::collections::BTreeMap<u64, SymbolId> {
    use std::collections::BTreeMap;
    let mut labels = BTreeMap::new();
    for symbol in container.defined_symbols() {
        if symbol.section != Some(section.id) {
            continue;
        }
        let section_offset = symbol.address.checked_sub(section.address);
        let Some(offset) = section_offset else { continue };
        if offset >= section.size {
            continue;
        }
        labels.entry(offset).or_insert(symbol.id);
    }
    labels
}

fn lift_items(
    section: &Section,
    handled: &[LiftedReloc],
    labels: &std::collections::BTreeMap<u64, SymbolId>,
) -> Vec<DataItem> {
    let mut items = Vec::new();
    let mut cursor = 0u64;

    let push_bytes = |items: &mut Vec<DataItem>,
                      bytes: Vec<u8>,
                      offset: u64,
                      labels: &std::collections::BTreeMap<u64, SymbolId>,
                      section: &Section| {
        if bytes.is_empty() {
            return;
        }
        items.push(DataItem {
            label: labels.get(&offset).copied(),
            align: align_for_offset(offset, section.align),
            payload: DataPayload::Bytes(bytes),
        });
    };

    for reloc in handled {
        if reloc.offset > cursor {
            let chunk = section.bytes[cursor as usize..reloc.offset as usize].to_vec();
            push_bytes(&mut items, chunk, cursor, labels, section);
        }
        items.push(DataItem {
            label: labels.get(&reloc.offset).copied(),
            align: align_for_offset(reloc.offset, section.align),
            payload: DataPayload::Pointer {
                target: reloc.target,
                addend: reloc.addend,
                width_bytes: reloc.width,
            },
        });
        cursor = reloc.offset + reloc.width as u64;
    }

    if (cursor as usize) < section.bytes.len() {
        let chunk = section.bytes[cursor as usize..].to_vec();
        push_bytes(&mut items, chunk, cursor, labels, section);
    }

    items
}

/// Items inherit the section's alignment requirement when their offset
/// is itself aligned. For inner items whose offset isn't aligned to the
/// full section alignment, fall back to the largest power-of-two factor
/// of the offset (a conservative lower bound that round-trips
/// correctly).
fn align_for_offset(offset: u64, section_align: u64) -> u64 {
    if offset == 0 {
        return section_align.max(1);
    }
    if section_align <= 1 {
        return 1;
    }
    let factor = 1u64 << offset.trailing_zeros();
    factor.min(section_align.max(1))
}

fn align_padding(current_offset: u64, align: u64) -> u64 {
    if align <= 1 {
        return 0;
    }
    let remainder = current_offset % align;
    if remainder == 0 {
        0
    } else {
        align - remainder
    }
}
