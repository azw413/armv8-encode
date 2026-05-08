//! Mach-O–specific data captured at parse time for ET_DYN-shaped
//! inputs (MH_DYLIB / MH_EXECUTE) — analogue of
//! [`crate::container::elf_image::ElfImage`].
//!
//! Phase 1 stores only the raw input bytes so the round-trip
//! writer can emit the file verbatim with section-byte overrides
//! applied at their original file offsets. Phases 2+ will extend
//! this with parsed load-command metadata (segment offsets, the
//! LC_CODE_SIGNATURE byte range, LC_SYMTAB / LC_DYSYMTAB offsets,
//! the LC_DYLD_INFO_ONLY / LC_DYLD_CHAINED_FIXUPS / LC_DYLD_EXPORTS_TRIE
//! data ranges, etc.) once we need to actually rewrite metadata
//! rather than just copy it.
//!
//! Why store the raw bytes rather than parsing once and
//! re-emitting:
//!
//! - For Phase 1 (round-trip + section overrides) we don't need
//!   a load-command-aware writer. Bytes-out-as-bytes-in is
//!   correct and trivially preserves things the neutral types
//!   don't model (LC_UUID, LC_BUILD_VERSION, embedded strings,
//!   alignment padding, etc.).
//! - LC_CODE_SIGNATURE will be invalidated by any byte change,
//!   including overrides, so we always re-sign after writing —
//!   no point preserving the signature blob.

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MachOImage {
    /// The original file bytes captured at parse time. The Phase-1
    /// writer copies these verbatim and applies any caller-staged
    /// section-byte overrides at their original file offsets.
    pub raw_bytes: Vec<u8>,
}

impl MachOImage {
    pub fn new(raw_bytes: Vec<u8>) -> Self {
        Self { raw_bytes }
    }
}
