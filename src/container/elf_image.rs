//! Format-specific data captured from ET_DYN / ET_EXEC ELF inputs that
//! the neutral [`Container`](super::Container) model deliberately
//! discards.
//!
//! Stage 5's contract is "read a `.so`, round-trip it, dynamic linker
//! still loads and runs it." A real ET_DYN carries program headers,
//! a `.dynamic` tag list, `.gnu.version_d/r/sym` blobs, GNU hash
//! tables, build-ID notes, an `.interp` string, an `.eh_frame_hdr`
//! binary-search table, and per-section ordering — none of which the
//! linker-style neutral `Container` types model. We could lift each
//! of them into the neutral model, but that bloats the types every
//! analysis layer above this sees, and most are linker-policy artifacts
//! the rewriter wants to preserve verbatim, not reason about.
//!
//! Instead, [`ElfImage`] captures the lot as a side-companion. It's
//! populated for ET_DYN / ET_EXEC inputs by the reader, and consumed
//! by the ELF writer (Stage 5.5) which drives `object::write::elf::Writer`
//! through a reserve/write pipeline against this captured shape.
//!
//! ## Mutability
//!
//! Stage 5 treats `ElfImage` as **read-only** — the rewriter doesn't
//! edit it. Stage 6 (in-place layout policy) will need to update it
//! when text grows beyond its source extent and a new PT_LOAD is
//! appended. Until then, the writer reproduces what it was handed.
//!
//! ## Why the shape varies per field
//!
//! Some fields lift into typed structs (`ProgramHeader`, `DynamicEntry`)
//! because the writer emits each entry one at a time and needs to
//! understand the values. Others stay as raw byte blobs (`gnu_hash`,
//! `eh_frame_hdr`, `gnu_version*`) because:
//!
//!   1. We don't need to interpret them at write time — the writer
//!      copies them through verbatim while the contents they
//!      reference (text addresses, FDE offsets) haven't moved.
//!   2. The ELF spec for these structures is intricate; modelling
//!      them as Rust types this stage doesn't need is busywork that
//!      compounds the bug surface.
//!
//! When Stage 6 lands and text starts moving, we'll lift more fields
//! to typed form — beginning with `.eh_frame_hdr` (its FDE table
//! references `.eh_frame` PC ranges that shift with text). That work
//! happens then, not now.

use crate::container::{SectionId, SymbolId};
use std::collections::HashMap;

/// Companion data captured for ET_DYN / ET_EXEC inputs. Populated
/// by [`crate::container::reader::parse`]; consumed by the ELF
/// writer.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ElfImage {
    /// All program headers from the source, in source order. Stage
    /// 5's writer reproduces this list as-is; Stage 6 may insert /
    /// modify entries (e.g. an appended PT_LOAD for grown text).
    pub program_headers: Vec<ProgramHeader>,

    /// Parsed `.dynamic` tag list, in source order. We model each
    /// entry as `(tag, value)` because the writer emits them via
    /// `object::write::elf::Writer::write_dynamic_string` /
    /// `write_dynamic` which want the same shape.
    pub dynamic: Vec<DynamicEntry>,

    /// Index of `.dynamic` in the section list, if present. Cached
    /// here because Stage 5's writer may need to know it without
    /// re-scanning sections.
    pub dynamic_section: Option<SectionId>,

    /// Raw `.gnu.hash` bytes, if present. Stage 5 writes these
    /// verbatim — the bucket/chain/bloom contents reference dynsym
    /// indices, not addresses, so they survive identity round-trip.
    pub gnu_hash: Option<RawSectionBytes>,

    /// Raw SysV `.hash` bytes, if present. Older ELFs use this; we
    /// pass it through structurally.
    pub sysv_hash: Option<RawSectionBytes>,

    /// Raw `.gnu.version` (versym) bytes — one entry per dynsym.
    pub gnu_versym: Option<RawSectionBytes>,

    /// Raw `.gnu.version_d` (verdef) bytes.
    pub gnu_verdef: Option<RawSectionBytes>,

    /// Raw `.gnu.version_r` (verneed) bytes.
    pub gnu_verneed: Option<RawSectionBytes>,

    /// Raw `.eh_frame_hdr` bytes, if present. Stage 5 copies this
    /// verbatim — it references `.eh_frame` FDE offsets relative to
    /// itself, which don't shift in an identity round-trip. Stage 7
    /// will need to regenerate the binary-search table when FDEs
    /// move.
    pub eh_frame_hdr: Option<RawSectionBytes>,

    /// `.interp` string (the dynamic linker path). Stored without a
    /// trailing NUL — emit re-adds one.
    pub interp: Option<String>,

    /// Build-ID note bytes, if present. The full
    /// `.note.gnu.build-id` section gets reproduced via this plus
    /// the section ordering.
    pub build_id: Option<Vec<u8>>,

    /// Other ELF notes (`.note.stapsdt`, `.note.ABI-tag`, etc.) the
    /// reader didn't recognise specifically. Stored as raw section
    /// bytes keyed by section id so the writer can reproduce them
    /// without lifting their structure.
    pub other_notes: Vec<RawNoteSection>,

    /// `.dynsym` raw bytes. The neutral `Container.symbols` lifts
    /// `.symtab` (or `.dynsym` when no `.symtab` is present); for
    /// ET_DYN inputs both tables exist and the reader hands the
    /// neutral list `.dynsym` content. Stage 5's writer regenerates
    /// `.dynsym` from `Container.symbols`, but we keep the raw bytes
    /// here as a fallback / verification source for the equivalence
    /// checker.
    pub dynsym: Option<RawSectionBytes>,

    /// `.dynstr` raw bytes — the string table backing `.dynsym`,
    /// `.dynamic` (DT_NEEDED, DT_SONAME), and `.gnu.version*`. The
    /// writer will rebuild this from the symbol/dynamic name set;
    /// raw bytes are kept for verification.
    pub dynstr: Option<RawSectionBytes>,

    /// Section ordering vector — the indices the input's section
    /// header table had, in original order. The writer emits in this
    /// order so section indices in `sh_link` / `sh_info` references
    /// (especially from `.dynamic`) stay valid.
    pub section_order: Vec<SectionId>,

    /// Per-section captured ELF section header bytes the neutral
    /// model doesn't carry yet: source `sh_offset` (file offset),
    /// `sh_link`, `sh_info`, `sh_entsize`. The writer needs all of
    /// these to reproduce the input layout faithfully — `sh_offset`
    /// so program-header `p_offset` values still point at the right
    /// bytes; `sh_link` / `sh_info` so the dynamic linker can walk
    /// `.dynsym → .dynstr` and similar references.
    pub section_layout: Vec<SectionLayout>,

    /// File header e_entry. Captured here because the neutral types
    /// don't model it; ET_EXEC round-trip depends on preserving it.
    pub e_entry: u64,

    /// Map from extern dynsym entry → its `.plt` stub virtual
    /// address. Lets the rewriter call into existing PLT stubs
    /// from new (appended) code: emit folds
    /// `Target::Symbol(extern_id)` to a `bl <stub>` instead of
    /// emitting an unresolved relocation.
    ///
    /// Populated for ET_DYN / ET_EXEC inputs whose `.rela.plt`
    /// the reader could parse; empty otherwise.
    pub plt_stubs: HashMap<SymbolId, u64>,

    /// Mapping from program-header index to the list of section ids
    /// that segment contains, derived from PT_LOAD `p_offset` /
    /// `p_filesz` ranges. Stage 6 needs this to decide where to
    /// append text when the rewrite outgrows its source extent.
    pub segment_sections: Vec<Vec<SectionId>>,
}

/// One program header. Mirrors the relevant ELF fields — we don't
/// model the `Elf64_Phdr` struct directly because the writer's
/// reserve/write API takes named arguments anyway.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// One entry from the `.dynamic` tag table.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct DynamicEntry {
    /// `d_tag` — DT_NEEDED, DT_SONAME, DT_HASH, DT_GNU_HASH, etc.
    /// We keep the raw u64 because the reader saw it that way; the
    /// writer dispatches on common tags and passes the rest through.
    pub tag: u64,
    /// `d_val` / `d_ptr`. For string-valued tags (DT_NEEDED,
    /// DT_SONAME, DT_RUNPATH) this is an offset into `.dynstr`; the
    /// writer must re-resolve against the rebuilt string table.
    /// For pointer tags (DT_HASH, DT_STRTAB, DT_SYMTAB, etc.) this
    /// is a virtual address; Stage 5 carries it through verbatim
    /// because addresses don't shift in identity round-trip.
    pub value: u64,
}

/// Raw section bytes captured at read time, plus the section id they
/// came from so the writer can place them back in the right slot.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RawSectionBytes {
    pub section: SectionId,
    pub bytes: Vec<u8>,
}

/// One unrecognised `.note.*` section the reader passes through.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RawNoteSection {
    pub section: SectionId,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Per-section ELF header fields the neutral [`crate::container::Section`]
/// model doesn't carry. Indexed positionally with the section list:
/// `image.section_layout[i]` describes `container.sections[i]`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct SectionLayout {
    /// Source `sh_offset`. Stage 5 places each section at this
    /// offset in the output via `Writer::reserve_until` so program
    /// header `p_offset` values still point at the right bytes.
    pub sh_offset: u64,
    /// Source `sh_size` — the section's original extent in the
    /// source file. The neutral [`crate::container::Section::size`]
    /// updates whenever a rewrite shrinks or grows the section's
    /// content; this field preserves the original so the writer
    /// can detect length-changing edits and react (Stage 6).
    pub sh_size: u64,
    /// Source `sh_link` (raw section index from the input). The
    /// writer preserves section ordering 1:1, so this index is also
    /// valid in the output. `0` means "no link" (the null section).
    pub sh_link: u32,
    /// Source `sh_info` — semantic depends on `sh_type` (number of
    /// local symbols in `.symtab`, related section index for
    /// `.rela.*`, verneed count for `.gnu.version_r`, etc.).
    pub sh_info: u32,
    /// Source `sh_entsize` — entry size for table-shaped sections
    /// (`.dynsym`, `.dynamic`, `.rela.*`).
    pub sh_entsize: u64,
}

impl ElfImage {
    /// Empty image. Useful for tests; production code populates via
    /// the reader.
    pub fn new() -> Self {
        Self {
            program_headers: Vec::new(),
            dynamic: Vec::new(),
            dynamic_section: None,
            gnu_hash: None,
            sysv_hash: None,
            gnu_versym: None,
            gnu_verdef: None,
            gnu_verneed: None,
            eh_frame_hdr: None,
            interp: None,
            build_id: None,
            other_notes: Vec::new(),
            dynsym: None,
            dynstr: None,
            section_order: Vec::new(),
            segment_sections: Vec::new(),
            section_layout: Vec::new(),
            e_entry: 0,
            plt_stubs: HashMap::new(),
        }
    }
}

impl Default for ElfImage {
    fn default() -> Self {
        Self::new()
    }
}
