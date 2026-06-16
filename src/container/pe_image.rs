//! Format-specific capture for linked PE images (`.exe` / `.dll`),
//! the PE analogue of [`ElfImage`](super::elf_image::ElfImage) and
//! [`MachOImage`](super::macho_image::MachOImage).
//!
//! ## Strategy (round-trip + section overrides)
//!
//! Like the Mach-O writer's first stage, the PE writer takes the
//! conservative path: snapshot the input's raw bytes verbatim and, on
//! write, splice any edited section's bytes back in at that section's
//! original file offset. Section file offsets/sizes come straight from
//! `object`'s `ObjectSection::file_range()` — no hand-rolled PE header
//! parsing — so the captured layout matches what the loader expects.
//!
//! This supports **in-place** edits (a rewritten section whose bytes are
//! the same length as the original). Length-changing edits — which would
//! require recomputing the section table, data directories, and the
//! optional header's `SizeOfImage` — are a later stage; they're rejected
//! loudly rather than silently corrupting the image.

/// One section's on-disk placement, captured for the override pass.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PeSectionFile {
    /// Section name (e.g. `.text`). Used to match the neutral
    /// [`Section`](super::types::Section) back to its file location.
    pub name: String,
    /// File offset of the section's raw data (`PointerToRawData`).
    pub file_offset: u64,
    /// On-disk size of the section's raw data (`SizeOfRawData`).
    pub file_size: u64,
}

/// One imported function from the PE import directory.
///
/// Linked-PE imports don't fit the neutral COFF [`Symbol`](super::types::Symbol)
/// model — they live in the import directory's descriptor/thunk tables, not the
/// symbol table — so they're captured here alongside the rest of the PE-specific
/// data. Either `name` (import by name) or `ordinal` (import by ordinal) is set.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PeImport {
    /// Name of the DLL this import resolves from (e.g. `KERNEL32.dll`).
    pub library: String,
    /// Function name, when imported by name.
    pub name: Option<String>,
    /// Ordinal, when imported by ordinal.
    pub ordinal: Option<u16>,
    /// Virtual address of this import's IAT slot (the pointer the loader
    /// patches and that call sites dereference).
    pub iat_address: u64,
}

/// One entry from the PE export directory.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PeExport {
    /// Export name, when the export is named (name-table entries). `None`
    /// for exports reachable by ordinal only.
    pub name: Option<String>,
    /// Export ordinal (already biased by the directory's ordinal base).
    pub ordinal: u32,
    /// Virtual address of the exported item. `0` when this export is a
    /// forwarder (see `forward`).
    pub address: u64,
    /// `"OtherDll.Func"` / `"OtherDll.#42"` when this export forwards to
    /// another module instead of pointing at local code/data.
    pub forward: Option<String>,
}

/// One base relocation from the `.reloc` directory.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PeBaseReloc {
    /// Virtual address the fix-up applies to.
    pub address: u64,
    /// `IMAGE_REL_BASED_*` type code (e.g. `HIGHLOW=3`, `DIR64=10`).
    /// `IMAGE_REL_BASED_ABSOLUTE` (0, padding) entries are dropped.
    pub typ: u16,
}

/// Captured PE image: the raw input bytes plus each section's on-disk
/// placement.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PeImage {
    /// The input file's bytes, verbatim. The writer clones this and
    /// applies section overrides on top.
    pub raw_bytes: Vec<u8>,
    /// Per-section file placement, in section-table order.
    pub sections: Vec<PeSectionFile>,
    /// Preferred load address from the optional header (`ImageBase`).
    pub image_base: u64,
    /// Entry-point virtual address (`image_base + AddressOfEntryPoint`),
    /// or `0` for a DLL/object with no entry point.
    pub entry_point: u64,
    /// Imported functions, lifted from the import directory. Empty when the
    /// image has no imports or the directory failed to parse.
    pub imports: Vec<PeImport>,
    /// Exported symbols, lifted from the export directory.
    pub exports: Vec<PeExport>,
    /// Base relocations, lifted from the `.reloc` directory (padding
    /// `ABSOLUTE` entries excluded).
    pub base_relocs: Vec<PeBaseReloc>,
}

impl PeImage {
    /// Look up a section's file placement by name.
    pub fn section_by_name(&self, name: &str) -> Option<&PeSectionFile> {
        self.sections.iter().find(|s| s.name == name)
    }
}
