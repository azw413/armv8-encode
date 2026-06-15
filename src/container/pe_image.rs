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

/// Captured PE image: the raw input bytes plus each section's on-disk
/// placement.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PeImage {
    /// The input file's bytes, verbatim. The writer clones this and
    /// applies section overrides on top.
    pub raw_bytes: Vec<u8>,
    /// Per-section file placement, in section-table order.
    pub sections: Vec<PeSectionFile>,
}

impl PeImage {
    /// Look up a section's file placement by name.
    pub fn section_by_name(&self, name: &str) -> Option<&PeSectionFile> {
        self.sections.iter().find(|s| s.name == name)
    }
}
