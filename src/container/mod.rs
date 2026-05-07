//! Binary-container parsing — Mach-O and ELF object files.
//!
//! This is the bottom of the user-facing stack: it turns on-disk bytes into
//! the neutral `Section` / `Symbol` / `Relocation` model that the analysis
//! and rewrite layers consume. No format-specific types leak out.
//!
//! ## Status
//!
//! - Read-only support for Mach-O and ELF AArch64 object files.
//! - Sections, symbols, relocations, and `Function` views derived from
//!   function-kind symbols.
//! - DWARF: not yet wired up (PR 2). Functions only appear here when
//!   symbols are present; stripped binaries will look empty.
//! - Write side: not implemented (PR 4). Use the rewriter on raw bytes for
//!   now.
//!
//! ## Example
//!
//! ```ignore
//! use armv8_encode::container::Container;
//! use armv8_encode::isa::aarch64;
//!
//! let bytes = std::fs::read("hello.o")?;
//! let container = Container::from_bytes(&bytes)?;
//!
//! for section in container.text_sections() {
//!     let (base, code) = section.for_disassembly().unwrap();
//!     let instructions = aarch64::disassemble_bytes(base, code)?;
//!     // ... feed into mc::build_cfg, rewrite, …
//! }
//! ```

mod dwarf;
mod reader;
mod types;
mod writer;

pub use types::{
    Architecture, BinaryFormat, Container, ContainerError, DwarfFunction, DwarfInfo, FileFlags,
    Function, FunctionProvenance, Relocation, RelocationId, RelocationKind, Section,
    SectionFlags, SectionId, SectionKind, Symbol, SymbolBinding, SymbolExtraFlags, SymbolId,
    SymbolKind,
};
pub use writer::ContainerWriteError;

impl Container {
    /// Parse a Mach-O or ELF byte slice. The format is auto-detected from
    /// the file header.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ContainerError> {
        reader::parse(bytes)
    }

    /// Serialize back to a Mach-O or ELF byte stream.
    ///
    /// Round-trip is "compatible," not byte-identical: the writer
    /// reconstructs sections, symbols, and relocations through
    /// `object::write` but does not preserve every header detail of the
    /// source file. Use this together with [`Self::with_section_bytes`] to
    /// land rewriter output back into a valid object file.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ContainerWriteError> {
        writer::write(self)
    }

    /// Return a clone of this container with one section's bytes
    /// replaced. The section's `size` is updated to match the new length.
    /// Other sections are untouched (no shifting), so this is suitable
    /// for in-place rewriter output where the new bytes are the same
    /// length as the original — or for cases where the caller has
    /// already laid the new content out within the original extent.
    pub fn with_section_bytes(&self, section: SectionId, new_bytes: Vec<u8>) -> Self {
        let mut out = self.clone();
        let target = &mut out.sections[section.0];
        target.size = new_bytes.len() as u64;
        target.bytes = new_bytes;
        out
    }
}
