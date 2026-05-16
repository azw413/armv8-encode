//! Mach-O–specific data captured at parse time for ET_DYN-shaped
//! inputs (MH_DYLIB / MH_EXECUTE) — analogue of
//! [`crate::container::elf_image::ElfImage`].
//!
//! Stores both the original file bytes (for the Phase-1 passthrough
//! writer) and a parsed view of the load commands (for the Phase-3
//! append-segment writer). Parsing is done once at read time so
//! later passes don't have to re-walk the load-command list.
//!
//! Phase 3+ writers consume the parsed layout to:
//!
//! - find the highest existing vmaddr (so the new segment can be
//!   placed past the input's mapped range without collision);
//! - find `__LINKEDIT.fileoff + filesize` (so the new segment's
//!   file region lands beyond the existing content);
//! - validate that headerpad has room for an extra `LC_SEGMENT_64`
//!   entry before growing `sizeofcmds`;
//! - locate the `LC_CODE_SIGNATURE` byte range so the signature can
//!   be re-emitted in place by `codesign --force`.
//!
//! The parsed view deliberately captures only what writers need.
//! Round-trip preserves anything not modelled here byte-for-byte
//! via [`MachOImage::raw_bytes`].

use std::collections::HashMap;

use crate::container::{ContainerWriteError, SymbolId};

/// Mach-O file metadata captured at parse time.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MachOImage {
    /// The original file bytes captured at parse time. The Phase-1
    /// writer copies these verbatim and applies any caller-staged
    /// section-byte overrides at their original file offsets.
    pub raw_bytes: Vec<u8>,
    /// Parsed load-command metadata. Populated by the reader for
    /// any input that parses as a 64-bit Mach-O.
    pub layout: MachOLayout,
    /// Import name → `__stubs` trampoline address. Mirrors
    /// `ElfImage::plt_stubs`. Populated by the high-level reader
    /// from the indirect symbol table once neutral `SymbolId`s
    /// exist. Empty until then.
    pub stubs: HashMap<SymbolId, u64>,
    /// Import name → `__got` / `__la_symbol_ptr` slot address.
    /// Lets the disassembler resolve `adrp/ldr` pairs that load an
    /// imported function pointer directly (no `__stubs` hop) to the
    /// import name.
    pub import_pointers: HashMap<SymbolId, u64>,
}

impl MachOImage {
    /// Construct from raw input bytes by parsing the load
    /// commands once. Errors if the input isn't a recognisable
    /// 64-bit Mach-O — the caller should fall back to a writer
    /// path that doesn't depend on the parsed layout.
    pub fn parse(raw_bytes: Vec<u8>) -> Result<Self, ContainerWriteError> {
        let layout = MachOLayout::parse(&raw_bytes)?;
        Ok(Self {
            raw_bytes,
            layout,
            stubs: HashMap::new(),
            import_pointers: HashMap::new(),
        })
    }
}

/// Subset of the load-command structure that the writer needs.
/// Everything else is preserved by emitting [`MachOImage::raw_bytes`]
/// verbatim.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct MachOLayout {
    /// `mach_header_64.ncmds`.
    pub ncmds: u32,
    /// `mach_header_64.sizeofcmds`. Bytes after the
    /// `mach_header_64` (which is itself 32 bytes) consumed by
    /// the load-command list.
    pub sizeofcmds: u32,
    /// File offset where the load-command list starts. Always
    /// 32 (immediately after the 64-bit mach_header) for
    /// well-formed inputs.
    pub load_commands_offset: u64,
    /// Per-segment entries from every `LC_SEGMENT_64`.
    pub segments: Vec<MachOSegment>,
    /// Per-section entries — the union of every `LC_SEGMENT_64`
    /// section. Used by the Phase-1 writer to map a Container
    /// section back to its file offset for byte overrides.
    pub sections: Vec<MachOSection>,
    /// `LC_CODE_SIGNATURE` data range, if present. The signer
    /// rewrites this on every commit; we capture it so a future
    /// strip-and-resign path can locate the existing signature.
    pub code_signature: Option<MachOLinkeditData>,
    /// `LC_DYLD_EXPORTS_TRIE` data range. The export trie maps
    /// symbol names to vmaddr offsets; dyld walks it during
    /// `dlsym` lookups. Phase-5 (export) extends this trie.
    pub exports_trie: Option<MachOLinkeditData>,
    /// `LC_SYMTAB` parameters. Captured so Phase-5 can locate
    /// the existing symbol table + string table to extend
    /// them.
    pub symtab: Option<MachOSymtab>,
    /// `LC_DYSYMTAB` external-symbol counts. Captured so
    /// Phase-5 can update the iextdefsym/nextdefsym range
    /// when a new defined external is added.
    pub dysymtab: Option<MachODysymtab>,
}

/// `LC_SYMTAB` parameters (offsets, counts) captured at
/// parse time. Phase-5 needs all four values to extend the
/// symbol table.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MachOSymtab {
    pub symoff: u64,
    pub nsyms: u32,
    pub stroff: u64,
    pub strsize: u32,
}

/// `LC_DYSYMTAB` external-symbol range captured at parse
/// time. Phase-5 uses iextdefsym + nextdefsym to know where
/// in the symbol table the new export should land
/// (immediately after existing externals, before
/// undefineds).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MachODysymtab {
    pub ilocalsym: u32,
    pub nlocalsym: u32,
    pub iextdefsym: u32,
    pub nextdefsym: u32,
    pub iundefsym: u32,
    pub nundefsym: u32,
    /// File offset of the indirect symbol table. Each entry is a
    /// `u32` index into `LC_SYMTAB`'s symbol table; sections with
    /// type `S_SYMBOL_STUBS` / `S_*_SYMBOL_POINTERS` use
    /// `reserved1 + i` to index into it.
    pub indirectsymoff: u32,
    /// Number of `u32` entries at `indirectsymoff`. Bounds the
    /// indirect-table walk so a malformed `reserved1` can't read
    /// past the table.
    pub nindirectsyms: u32,
}

/// One contiguous range of bytes inside an existing segment
/// that isn't claimed by any section's declared range. Used
/// by the intra-segment placement strategy: callers can put
/// new content into these gaps without adding a fresh
/// `LC_SEGMENT_64`, which is required for App Store
/// submissions (rejected if more than one R-X segment is
/// present).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MachOFreeRegion {
    /// Start vmaddr of the free region.
    pub vaddr: u64,
    /// Start file offset of the free region.
    pub file_offset: u64,
    /// Size in bytes of the free region.
    pub size: u64,
    /// Name of the segment this region lives inside (e.g.
    /// `__TEXT`). Determines what permissions content placed
    /// here will have at runtime.
    pub segment_name: String,
}

impl MachOLayout {
    /// Compute free regions inside `__TEXT` — the gaps
    /// between consecutive sections (sorted by file offset)
    /// plus the gap from the last section's end to the
    /// segment's file end. Section alignment + segment-end
    /// padding routinely leaves several KB of free space in
    /// most dylibs, which fits an initialiser wrapper or
    /// small payload comfortably.
    pub fn text_free_regions(&self) -> Vec<MachOFreeRegion> {
        let Some(text_seg) = self.segments.iter().find(|s| s.name == "__TEXT") else {
            return Vec::new();
        };
        // Sections inside __TEXT, sorted by file offset.
        let mut text_sections: Vec<&MachOSection> = self
            .sections
            .iter()
            .filter(|s| s.segname == "__TEXT" && s.file_offset > 0)
            .collect();
        text_sections.sort_by_key(|s| s.file_offset);

        let mut out = Vec::new();
        // Cursor walks file offsets; we record any gap
        // between the cursor and the next section's
        // file_offset.
        let mut cursor_file = text_seg.fileoff;
        let mut cursor_vaddr = text_seg.vmaddr;
        // Skip past the mach_header and load commands at the
        // start of __TEXT — that region is reserved.
        if cursor_file == 0 {
            // mach_header_64 is 32 bytes; load commands
            // follow. The first section's file_offset already
            // accounts for them. Just leave the cursor where
            // the segment starts and let the loop skip ahead
            // to the first section's offset (which is
            // typically 0x40 + sizeofcmds).
        }
        for section in &text_sections {
            if section.file_offset > cursor_file {
                let gap = section.file_offset - cursor_file;
                let gap_vaddr = section.vaddr - (section.file_offset - cursor_file);
                // The first gap is the mach_header + load
                // commands area; skip it (we don't want
                // callers to overwrite headers).
                if cursor_file > text_seg.fileoff {
                    out.push(MachOFreeRegion {
                        vaddr: cursor_vaddr,
                        file_offset: cursor_file,
                        size: gap,
                        segment_name: "__TEXT".to_string(),
                    });
                }
                let _ = gap_vaddr;
            }
            cursor_file = section.file_offset + section.size;
            cursor_vaddr = section.vaddr + section.size;
        }
        // Tail gap from the last section's end to the
        // segment's file end.
        let seg_file_end = text_seg.fileoff + text_seg.filesize;
        let seg_vmaddr_end = text_seg.vmaddr + text_seg.vmsize;
        if cursor_file < seg_file_end {
            out.push(MachOFreeRegion {
                vaddr: cursor_vaddr,
                file_offset: cursor_file,
                size: seg_file_end - cursor_file,
                segment_name: "__TEXT".to_string(),
            });
        }
        let _ = seg_vmaddr_end;
        out
    }

    /// Best-fit allocate `bytes_needed` (aligned to
    /// `align_to`) inside `__TEXT`'s free regions. Returns
    /// the chosen `(vaddr, file_offset)`, or `None` if no
    /// single region is large enough.
    pub fn allocate_in_text(
        &self,
        bytes_needed: u64,
        align_to: u64,
    ) -> Option<(u64, u64)> {
        let regions = self.text_free_regions();
        // Best-fit: pick the smallest region that has enough
        // post-alignment room.
        let mut best: Option<(u64, u64, u64)> = None; // (size_after_align, vaddr, file_offset)
        for region in &regions {
            let aligned_vaddr = align_up(region.vaddr, align_to);
            let aligned_off = aligned_vaddr - region.vaddr + region.file_offset;
            let alignment_padding = aligned_vaddr - region.vaddr;
            if region.size < alignment_padding + bytes_needed {
                continue;
            }
            let usable = region.size - alignment_padding;
            if let Some((best_size, _, _)) = best {
                if usable >= best_size {
                    continue;
                }
            }
            best = Some((usable, aligned_vaddr, aligned_off));
        }
        best.map(|(_, vaddr, off)| (vaddr, off))
    }
}

fn align_up(value: u64, align: u64) -> u64 {
    if align <= 1 {
        return value;
    }
    (value + align - 1) & !(align - 1)
}

/// One `LC_SEGMENT_64` entry's interesting fields.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MachOSegment {
    /// 16-byte fixed-length segment name (e.g. `__TEXT`,
    /// `__DATA`, `__LINKEDIT`). Trailing NULs stripped.
    pub name: String,
    pub vmaddr: u64,
    pub vmsize: u64,
    pub fileoff: u64,
    pub filesize: u64,
    /// `VM_PROT_*` bitmask: READ=1, WRITE=2, EXECUTE=4.
    pub maxprot: u32,
    pub initprot: u32,
    pub flags: u32,
}

/// One section_64 entry's location in the file. The vaddr is
/// the dyld virtual address; file_offset is where to read /
/// write the section's bytes.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MachOSection {
    pub sectname: String,
    pub segname: String,
    pub vaddr: u64,
    pub size: u64,
    /// File offset of the section's bytes. `0` for zerofill
    /// sections (e.g. `__bss`, `__common`) which have no
    /// on-disk content.
    pub file_offset: u64,
    pub flags: u32,
    /// `section_64.reserved1`. Type-dependent:
    ///   - `S_SYMBOL_STUBS` / `S_LAZY_SYMBOL_POINTERS` /
    ///     `S_NON_LAZY_SYMBOL_POINTERS`: index into the indirect
    ///     symbol table for the section's first entry.
    /// Captured so the stub-address → SymbolId map can be built
    /// without re-parsing the file.
    pub reserved1: u32,
    /// `section_64.reserved2`. Type-dependent:
    ///   - `S_SYMBOL_STUBS`: stub size in bytes (12 for arm64).
    /// Captured to walk `__stubs` in strides without hard-coding
    /// per-arch sizes.
    pub reserved2: u32,
}

/// `section_64.flags` low-byte section types we care about for
/// stub / import-pointer resolution. Values match `<mach-o/loader.h>`.
pub const S_NON_LAZY_SYMBOL_POINTERS: u32 = 0x06;
pub const S_LAZY_SYMBOL_POINTERS: u32 = 0x07;
pub const S_SYMBOL_STUBS: u32 = 0x08;

/// Mask isolating the section type from `section_64.flags`.
pub const SECTION_TYPE_MASK: u32 = 0x0000_00ff;

/// Sentinel values in indirect-symbol-table entries that mean
/// "this slot doesn't bind to a named import" — skip them.
pub const INDIRECT_SYMBOL_LOCAL: u32 = 0x8000_0000;
pub const INDIRECT_SYMBOL_ABS: u32 = 0x4000_0000;

/// `LC_CODE_SIGNATURE` (and similar `linkedit_data_command`-
/// shaped commands like `LC_FUNCTION_STARTS`, `LC_DATA_IN_CODE`,
/// `LC_DYLD_EXPORTS_TRIE`, `LC_DYLD_CHAINED_FIXUPS`) byte range
/// inside `__LINKEDIT`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MachOLinkeditData {
    pub dataoff: u64,
    pub datasize: u64,
}

impl MachOLayout {
    /// Parse the mach_header_64 + load commands and extract the
    /// pieces the writer needs.
    pub fn parse(bytes: &[u8]) -> Result<Self, ContainerWriteError> {
        use object::macho;

        if bytes.len() < 32 {
            return Err(ContainerWriteError::ObjectWrite(
                "Mach-O image: input too short for mach_header_64".into(),
            ));
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != macho::MH_MAGIC_64 {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O image: unsupported magic 0x{magic:08x} (only MH_MAGIC_64 supported)",
            )));
        }
        let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let load_commands_offset = 32u64;
        let cmds_end = (load_commands_offset as usize) + (sizeofcmds as usize);
        if cmds_end > bytes.len() {
            return Err(ContainerWriteError::ObjectWrite(
                "Mach-O image: load commands extend past file end".into(),
            ));
        }

        let mut segments: Vec<MachOSegment> = Vec::new();
        let mut sections: Vec<MachOSection> = Vec::new();
        let mut code_signature: Option<MachOLinkeditData> = None;
        let mut exports_trie: Option<MachOLinkeditData> = None;
        let mut symtab: Option<MachOSymtab> = None;
        let mut dysymtab: Option<MachODysymtab> = None;

        let mut cursor = load_commands_offset as usize;
        for _ in 0..ncmds {
            if cursor + 8 > cmds_end {
                return Err(ContainerWriteError::ObjectWrite(
                    "Mach-O image: truncated load command header".into(),
                ));
            }
            let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            let cmdsize =
                u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
            if cmdsize == 0 || cursor + cmdsize > cmds_end {
                return Err(ContainerWriteError::ObjectWrite(format!(
                    "Mach-O image: invalid cmdsize {cmdsize} at offset {cursor}",
                )));
            }

            match cmd {
                macho::LC_SEGMENT_64 => {
                    parse_segment_64(bytes, cursor, cmdsize, &mut segments, &mut sections)?;
                }
                macho::LC_CODE_SIGNATURE => {
                    // linkedit_data_command: u32 cmd, u32
                    // cmdsize, u32 dataoff, u32 datasize.
                    if cmdsize < 16 {
                        return Err(ContainerWriteError::ObjectWrite(
                            "Mach-O image: LC_CODE_SIGNATURE cmdsize < 16".into(),
                        ));
                    }
                    let dataoff =
                        u32::from_le_bytes(bytes[cursor + 8..cursor + 12].try_into().unwrap())
                            as u64;
                    let datasize =
                        u32::from_le_bytes(bytes[cursor + 12..cursor + 16].try_into().unwrap())
                            as u64;
                    code_signature = Some(MachOLinkeditData { dataoff, datasize });
                }
                macho::LC_DYLD_EXPORTS_TRIE => {
                    if cmdsize < 16 {
                        return Err(ContainerWriteError::ObjectWrite(
                            "Mach-O image: LC_DYLD_EXPORTS_TRIE cmdsize < 16".into(),
                        ));
                    }
                    let dataoff = u32::from_le_bytes(
                        bytes[cursor + 8..cursor + 12].try_into().unwrap(),
                    ) as u64;
                    let datasize = u32::from_le_bytes(
                        bytes[cursor + 12..cursor + 16].try_into().unwrap(),
                    ) as u64;
                    exports_trie = Some(MachOLinkeditData { dataoff, datasize });
                }
                macho::LC_SYMTAB => {
                    // symtab_command: u32 cmd, cmdsize, u32
                    // symoff, nsyms, stroff, strsize.
                    if cmdsize < 24 {
                        return Err(ContainerWriteError::ObjectWrite(
                            "Mach-O image: LC_SYMTAB cmdsize < 24".into(),
                        ));
                    }
                    let symoff = u32::from_le_bytes(
                        bytes[cursor + 8..cursor + 12].try_into().unwrap(),
                    ) as u64;
                    let nsyms = u32::from_le_bytes(
                        bytes[cursor + 12..cursor + 16].try_into().unwrap(),
                    );
                    let stroff = u32::from_le_bytes(
                        bytes[cursor + 16..cursor + 20].try_into().unwrap(),
                    ) as u64;
                    let strsize = u32::from_le_bytes(
                        bytes[cursor + 20..cursor + 24].try_into().unwrap(),
                    );
                    symtab = Some(MachOSymtab {
                        symoff,
                        nsyms,
                        stroff,
                        strsize,
                    });
                }
                macho::LC_DYSYMTAB => {
                    if cmdsize < 80 {
                        return Err(ContainerWriteError::ObjectWrite(
                            "Mach-O image: LC_DYSYMTAB cmdsize < 80".into(),
                        ));
                    }
                    let ilocalsym = u32::from_le_bytes(
                        bytes[cursor + 8..cursor + 12].try_into().unwrap(),
                    );
                    let nlocalsym = u32::from_le_bytes(
                        bytes[cursor + 12..cursor + 16].try_into().unwrap(),
                    );
                    let iextdefsym = u32::from_le_bytes(
                        bytes[cursor + 16..cursor + 20].try_into().unwrap(),
                    );
                    let nextdefsym = u32::from_le_bytes(
                        bytes[cursor + 20..cursor + 24].try_into().unwrap(),
                    );
                    let iundefsym = u32::from_le_bytes(
                        bytes[cursor + 24..cursor + 28].try_into().unwrap(),
                    );
                    let nundefsym = u32::from_le_bytes(
                        bytes[cursor + 28..cursor + 32].try_into().unwrap(),
                    );
                    // `dysymtab_command` continues with toc/modtab/
                    // extrefsym pairs (24 bytes), then the indirect
                    // symbol table pair at offsets 56/60.
                    let indirectsymoff = u32::from_le_bytes(
                        bytes[cursor + 56..cursor + 60].try_into().unwrap(),
                    );
                    let nindirectsyms = u32::from_le_bytes(
                        bytes[cursor + 60..cursor + 64].try_into().unwrap(),
                    );
                    dysymtab = Some(MachODysymtab {
                        ilocalsym,
                        nlocalsym,
                        iextdefsym,
                        nextdefsym,
                        iundefsym,
                        nundefsym,
                        indirectsymoff,
                        nindirectsyms,
                    });
                }
                _ => {}
            }
            cursor += cmdsize;
        }

        Ok(Self {
            ncmds,
            sizeofcmds,
            load_commands_offset,
            segments,
            sections,
            code_signature,
            exports_trie,
            symtab,
            dysymtab,
        })
    }

    /// Bytes available between the end of the existing
    /// load-command list and the lowest content file offset
    /// (typically the first segment's first section). Tells
    /// callers whether they have room to grow `sizeofcmds`
    /// without shifting downstream content.
    pub fn headerpad(&self) -> u64 {
        // Lowest section file_offset; if no sections have
        // content, fall back to the lowest non-zero segment
        // fileoff. (`__PAGEZERO` has fileoff=0; ignore it.)
        let lowest_section = self
            .sections
            .iter()
            .filter(|s| s.file_offset > 0)
            .map(|s| s.file_offset)
            .min();
        let lowest_segment = self
            .segments
            .iter()
            .filter(|s| s.fileoff > 0)
            .map(|s| s.fileoff)
            .min();
        let lowest = lowest_section
            .or(lowest_segment)
            .unwrap_or(self.load_commands_offset + self.sizeofcmds as u64);
        let end_of_load_commands = self.load_commands_offset + self.sizeofcmds as u64;
        lowest.saturating_sub(end_of_load_commands)
    }

    /// Highest vaddr+vmsize across all segments. Phase 3+ uses
    /// this as the lower bound when picking a fresh vaddr for
    /// the appended segment (page-aligned up).
    pub fn max_vaddr_end(&self) -> u64 {
        self.segments
            .iter()
            .map(|s| s.vmaddr.saturating_add(s.vmsize))
            .max()
            .unwrap_or(0)
    }

    /// Highest fileoff+filesize across all segments. Phase 3+
    /// uses this as the lower bound when picking the file
    /// offset for the appended segment's bytes.
    pub fn max_fileoff_end(&self) -> u64 {
        self.segments
            .iter()
            .map(|s| s.fileoff.saturating_add(s.filesize))
            .max()
            .unwrap_or(0)
    }

    /// Lookup a section by `(segname, sectname)` pair, e.g.
    /// `("__TEXT", "__text")`. Useful when the writer needs to
    /// find a particular section's file offset.
    pub fn section(&self, segname: &str, sectname: &str) -> Option<&MachOSection> {
        self.sections
            .iter()
            .find(|s| s.segname == segname && s.sectname == sectname)
    }

    /// Decode the indirect symbol table referenced by `LC_DYSYMTAB`.
    /// Each entry is a `u32` index into the Mach-O symbol table
    /// (`LC_SYMTAB`), used by sections of type `S_SYMBOL_STUBS`,
    /// `S_LAZY_SYMBOL_POINTERS`, and `S_NON_LAZY_SYMBOL_POINTERS`
    /// to identify which import each slot binds to.
    ///
    /// Returns an empty vec when `LC_DYSYMTAB` was absent or the
    /// table's declared range extends past the file.
    pub fn read_indirect_symtab(&self, bytes: &[u8]) -> Vec<u32> {
        let Some(dyn_) = self.dysymtab else { return Vec::new() };
        let off = dyn_.indirectsymoff as usize;
        let n = dyn_.nindirectsyms as usize;
        let end = off.saturating_add(n.saturating_mul(4));
        if n == 0 || end > bytes.len() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let base = off + i * 4;
            out.push(u32::from_le_bytes(bytes[base..base + 4].try_into().unwrap()));
        }
        out
    }

    /// Enumerate every `__stubs`-style trampoline in the image, one
    /// entry per stub slot. Walks each section whose type is
    /// [`S_SYMBOL_STUBS`] in `reserved2`-byte strides; the entry's
    /// indirect-table slot (`reserved1 + i`) yields the symbol-table
    /// index of the import the stub binds to.
    ///
    /// Sentinel slots (`INDIRECT_SYMBOL_LOCAL` / `_ABS`) are
    /// dropped so callers only see real imports. Bad `reserved2`
    /// values (zero or larger than the section) skip that section
    /// rather than abort — a malformed binary shouldn't break
    /// disassembly of the rest of the image.
    pub fn stub_entries(&self, bytes: &[u8]) -> Vec<MachOStubEntry> {
        let indirect = self.read_indirect_symtab(bytes);
        if indirect.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for sec in &self.sections {
            if sec.flags & SECTION_TYPE_MASK != S_SYMBOL_STUBS {
                continue;
            }
            let stride = sec.reserved2 as u64;
            if stride == 0 || stride > sec.size {
                continue;
            }
            let count = (sec.size / stride) as u32;
            let base_idx = sec.reserved1;
            for i in 0..count {
                let slot = match base_idx.checked_add(i).and_then(|j| indirect.get(j as usize)) {
                    Some(&v) => v,
                    None => break,
                };
                if slot & (INDIRECT_SYMBOL_LOCAL | INDIRECT_SYMBOL_ABS) != 0 {
                    continue;
                }
                let address = sec.vaddr + (i as u64) * stride;
                out.push(MachOStubEntry {
                    address,
                    symtab_index: slot,
                });
            }
        }
        out
    }

    /// Same idea as `stub_entries`, but for `__got` / `__la_symbol_ptr`
    /// pointer slots. Each entry is one 8-byte slot whose runtime
    /// value will be filled in by dyld from the named import. Used
    /// by the disassembler to resolve `adrp x16, __got` /
    /// `ldr x16, [x16, #imp_off]` pairs to the import symbol.
    pub fn import_pointer_entries(&self, bytes: &[u8]) -> Vec<MachOStubEntry> {
        let indirect = self.read_indirect_symtab(bytes);
        if indirect.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for sec in &self.sections {
            let kind = sec.flags & SECTION_TYPE_MASK;
            if kind != S_NON_LAZY_SYMBOL_POINTERS && kind != S_LAZY_SYMBOL_POINTERS {
                continue;
            }
            // Pointer slots are always 8 bytes wide on 64-bit Mach-O.
            const PTR_SIZE: u64 = 8;
            let count = (sec.size / PTR_SIZE) as u32;
            let base_idx = sec.reserved1;
            for i in 0..count {
                let slot = match base_idx.checked_add(i).and_then(|j| indirect.get(j as usize)) {
                    Some(&v) => v,
                    None => break,
                };
                if slot & (INDIRECT_SYMBOL_LOCAL | INDIRECT_SYMBOL_ABS) != 0 {
                    continue;
                }
                let address = sec.vaddr + (i as u64) * PTR_SIZE;
                out.push(MachOStubEntry {
                    address,
                    symtab_index: slot,
                });
            }
        }
        out
    }
}

/// One resolved stub or import-pointer slot. `address` is the
/// run-time address of the trampoline / pointer; `symtab_index` is
/// the entry's slot in the Mach-O `LC_SYMTAB` symbol table — the
/// reader maps this back to a neutral `SymbolId` via the same
/// `object::SymbolIndex` mapping used everywhere else.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MachOStubEntry {
    pub address: u64,
    pub symtab_index: u32,
}

fn parse_segment_64(
    bytes: &[u8],
    cursor: usize,
    cmdsize: usize,
    segments: &mut Vec<MachOSegment>,
    sections: &mut Vec<MachOSection>,
) -> Result<(), ContainerWriteError> {
    // segment_command_64 is 72 bytes:
    //   u32 cmd, u32 cmdsize,
    //   char segname[16],
    //   u64 vmaddr, u64 vmsize,
    //   u64 fileoff, u64 filesize,
    //   u32 maxprot, u32 initprot,
    //   u32 nsects, u32 flags
    const SEG_HEADER: usize = 72;
    if cmdsize < SEG_HEADER {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O image: LC_SEGMENT_64 cmdsize < 72".into(),
        ));
    }
    let segname = read_fixed_name(&bytes[cursor + 8..cursor + 24]);
    let vmaddr = u64::from_le_bytes(bytes[cursor + 24..cursor + 32].try_into().unwrap());
    let vmsize = u64::from_le_bytes(bytes[cursor + 32..cursor + 40].try_into().unwrap());
    let fileoff = u64::from_le_bytes(bytes[cursor + 40..cursor + 48].try_into().unwrap());
    let filesize = u64::from_le_bytes(bytes[cursor + 48..cursor + 56].try_into().unwrap());
    let maxprot = u32::from_le_bytes(bytes[cursor + 56..cursor + 60].try_into().unwrap());
    let initprot = u32::from_le_bytes(bytes[cursor + 60..cursor + 64].try_into().unwrap());
    let nsects = u32::from_le_bytes(bytes[cursor + 64..cursor + 68].try_into().unwrap()) as usize;
    let flags = u32::from_le_bytes(bytes[cursor + 68..cursor + 72].try_into().unwrap());

    // section_64 is 80 bytes:
    //   char sectname[16],
    //   char segname[16],
    //   u64 addr, u64 size,
    //   u32 offset, u32 align,
    //   u32 reloff, u32 nreloc,
    //   u32 flags,
    //   u32 reserved1, u32 reserved2, u32 reserved3
    const SECT_SIZE: usize = 80;
    if cmdsize < SEG_HEADER + nsects * SECT_SIZE {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O image: LC_SEGMENT_64 cmdsize doesn't fit nsects".into(),
        ));
    }
    for i in 0..nsects {
        let s = cursor + SEG_HEADER + i * SECT_SIZE;
        let sectname = read_fixed_name(&bytes[s..s + 16]);
        let sect_segname = read_fixed_name(&bytes[s + 16..s + 32]);
        let addr = u64::from_le_bytes(bytes[s + 32..s + 40].try_into().unwrap());
        let size = u64::from_le_bytes(bytes[s + 40..s + 48].try_into().unwrap());
        let file_offset =
            u32::from_le_bytes(bytes[s + 48..s + 52].try_into().unwrap()) as u64;
        let sect_flags = u32::from_le_bytes(bytes[s + 64..s + 68].try_into().unwrap());
        let reserved1 = u32::from_le_bytes(bytes[s + 68..s + 72].try_into().unwrap());
        let reserved2 = u32::from_le_bytes(bytes[s + 72..s + 76].try_into().unwrap());
        sections.push(MachOSection {
            sectname,
            segname: sect_segname,
            vaddr: addr,
            size,
            file_offset,
            flags: sect_flags,
            reserved1,
            reserved2,
        });
    }

    segments.push(MachOSegment {
        name: segname,
        vmaddr,
        vmsize,
        fileoff,
        filesize,
        maxprot,
        initprot,
        flags,
    });
    Ok(())
}

/// Decode a Mach-O fixed-length 16-byte name field by
/// stripping trailing NULs. Mach-O zero-pads names; we trim so
/// downstream string comparisons work.
fn read_fixed_name(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::macho;

    /// Build a minimal valid 64-bit Mach-O byte stream with the
    /// given load commands appended. The mach_header is filled
    /// with sensible defaults; `ncmds` and `sizeofcmds` are
    /// derived from `cmds`.
    fn synth_macho(cmds: &[Vec<u8>]) -> Vec<u8> {
        let sizeofcmds: u32 = cmds.iter().map(|c| c.len() as u32).sum();
        let mut bytes = Vec::new();
        // mach_header_64: magic, cputype, cpusubtype, filetype,
        // ncmds, sizeofcmds, flags, reserved.
        bytes.extend_from_slice(&macho::MH_MAGIC_64.to_le_bytes());
        bytes.extend_from_slice(&0x0100_000c_u32.to_le_bytes()); // CPU_TYPE_ARM64
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&macho::MH_DYLIB.to_le_bytes());
        bytes.extend_from_slice(&(cmds.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&sizeofcmds.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // flags
        bytes.extend_from_slice(&0u32.to_le_bytes()); // reserved
        for cmd in cmds {
            bytes.extend_from_slice(cmd);
        }
        bytes
    }

    /// Build an LC_SEGMENT_64 with the given fields and zero
    /// sections. 72 bytes total.
    fn segment_64(
        segname: &str,
        vmaddr: u64,
        vmsize: u64,
        fileoff: u64,
        filesize: u64,
        maxprot: u32,
        initprot: u32,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(72);
        out.extend_from_slice(&macho::LC_SEGMENT_64.to_le_bytes());
        out.extend_from_slice(&72u32.to_le_bytes()); // cmdsize
        // 16-byte segname, NUL-padded.
        let mut name = [0u8; 16];
        let bytes = segname.as_bytes();
        name[..bytes.len().min(16)].copy_from_slice(&bytes[..bytes.len().min(16)]);
        out.extend_from_slice(&name);
        out.extend_from_slice(&vmaddr.to_le_bytes());
        out.extend_from_slice(&vmsize.to_le_bytes());
        out.extend_from_slice(&fileoff.to_le_bytes());
        out.extend_from_slice(&filesize.to_le_bytes());
        out.extend_from_slice(&maxprot.to_le_bytes());
        out.extend_from_slice(&initprot.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // nsects
        out.extend_from_slice(&0u32.to_le_bytes()); // flags
        out
    }

    /// Build an LC_SEGMENT_64 with one section_64 attached.
    /// 72 + 80 = 152 bytes total.
    #[allow(clippy::too_many_arguments)]
    fn segment_64_with_section(
        segname: &str,
        vmaddr: u64,
        vmsize: u64,
        fileoff: u64,
        filesize: u64,
        sect_name: &str,
        sect_addr: u64,
        sect_size: u64,
        sect_offset: u32,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(152);
        out.extend_from_slice(&macho::LC_SEGMENT_64.to_le_bytes());
        out.extend_from_slice(&152u32.to_le_bytes()); // cmdsize
        let mut name = [0u8; 16];
        name[..segname.len().min(16)].copy_from_slice(&segname.as_bytes()[..segname.len().min(16)]);
        out.extend_from_slice(&name);
        out.extend_from_slice(&vmaddr.to_le_bytes());
        out.extend_from_slice(&vmsize.to_le_bytes());
        out.extend_from_slice(&fileoff.to_le_bytes());
        out.extend_from_slice(&filesize.to_le_bytes());
        out.extend_from_slice(&5u32.to_le_bytes()); // maxprot R-X
        out.extend_from_slice(&5u32.to_le_bytes()); // initprot R-X
        out.extend_from_slice(&1u32.to_le_bytes()); // nsects
        out.extend_from_slice(&0u32.to_le_bytes()); // flags

        // section_64
        let mut sectname_field = [0u8; 16];
        sectname_field[..sect_name.len().min(16)]
            .copy_from_slice(&sect_name.as_bytes()[..sect_name.len().min(16)]);
        out.extend_from_slice(&sectname_field);
        out.extend_from_slice(&name); // segname (matches segment)
        out.extend_from_slice(&sect_addr.to_le_bytes());
        out.extend_from_slice(&sect_size.to_le_bytes());
        out.extend_from_slice(&sect_offset.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes()); // align 2^2
        out.extend_from_slice(&0u32.to_le_bytes()); // reloff
        out.extend_from_slice(&0u32.to_le_bytes()); // nreloc
        out.extend_from_slice(&0u32.to_le_bytes()); // flags
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved1
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved2
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved3
        out
    }

    /// Build an LC_CODE_SIGNATURE linkedit_data_command. 16
    /// bytes total.
    fn code_signature(dataoff: u32, datasize: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(16);
        out.extend_from_slice(&macho::LC_CODE_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&dataoff.to_le_bytes());
        out.extend_from_slice(&datasize.to_le_bytes());
        out
    }

    #[test]
    fn parse_rejects_short_input() {
        let err = MachOLayout::parse(&[0u8; 4]).expect_err("too short");
        let msg = format!("{err}");
        assert!(msg.contains("too short"), "got {msg:?}");
    }

    #[test]
    fn parse_rejects_wrong_magic() {
        let mut bytes = vec![0u8; 32];
        bytes[0..4].copy_from_slice(&0xdead_beef_u32.to_le_bytes());
        let err = MachOLayout::parse(&bytes).expect_err("wrong magic");
        assert!(format!("{err}").contains("magic"));
    }

    #[test]
    fn parse_rejects_truncated_load_commands() {
        // Mach header says sizeofcmds=72 (one segment) but file
        // truncates after the header.
        let header = synth_macho(&[]);
        let mut bytes = header.clone();
        // Patch sizeofcmds to claim a load command exists.
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes()); // ncmds
        bytes[20..24].copy_from_slice(&72u32.to_le_bytes()); // sizeofcmds
        let err = MachOLayout::parse(&bytes).expect_err("truncated cmds");
        let msg = format!("{err}");
        assert!(
            msg.contains("past file end") || msg.contains("truncated"),
            "got {msg:?}",
        );
    }

    #[test]
    fn parse_captures_single_segment() {
        let bytes = synth_macho(&[segment_64(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, 5, 5,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert_eq!(layout.ncmds, 1);
        assert_eq!(layout.sizeofcmds, 72);
        assert_eq!(layout.load_commands_offset, 32);
        assert_eq!(layout.segments.len(), 1);
        assert_eq!(layout.segments[0].name, "__TEXT");
        assert_eq!(layout.segments[0].vmaddr, 0x0);
        assert_eq!(layout.segments[0].vmsize, 0x4000);
        assert_eq!(layout.segments[0].maxprot, 5);
        assert_eq!(layout.segments[0].initprot, 5);
        assert!(layout.code_signature.is_none());
    }

    #[test]
    fn parse_captures_section_64() {
        let bytes = synth_macho(&[segment_64_with_section(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, "__text", 0x1000, 0x100, 0x1000,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert_eq!(layout.sections.len(), 1);
        let sect = &layout.sections[0];
        assert_eq!(sect.sectname, "__text");
        assert_eq!(sect.segname, "__TEXT");
        assert_eq!(sect.vaddr, 0x1000);
        assert_eq!(sect.size, 0x100);
        assert_eq!(sect.file_offset, 0x1000);
    }

    #[test]
    fn parse_captures_code_signature() {
        let bytes = synth_macho(&[
            segment_64("__TEXT", 0x0, 0x4000, 0x0, 0x4000, 5, 5),
            code_signature(0x4000, 0x1000),
        ]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        let sig = layout.code_signature.expect("LC_CODE_SIGNATURE present");
        assert_eq!(sig.dataoff, 0x4000);
        assert_eq!(sig.datasize, 0x1000);
    }

    #[test]
    fn section_lookup_by_segname_and_sectname() {
        let bytes = synth_macho(&[segment_64_with_section(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, "__text", 0x1000, 0x100, 0x1000,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert!(layout.section("__TEXT", "__text").is_some());
        assert!(layout.section("__TEXT", "__data").is_none());
        assert!(layout.section("__DATA", "__text").is_none());
    }

    #[test]
    fn headerpad_reports_room_between_load_commands_and_first_section() {
        // mach_header (32) + LC_SEGMENT_64 with one section
        // (152) = 184 bytes used. First section's content lives
        // at file offset 0x1000. headerpad = 0x1000 - 184 = 3912.
        let bytes = synth_macho(&[segment_64_with_section(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, "__text", 0x1000, 0x100, 0x1000,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert_eq!(layout.headerpad(), 0x1000 - (32 + 152));
    }

    #[test]
    fn headerpad_falls_back_to_segment_fileoff_when_no_sections() {
        // Segment with no sections, fileoff=0x500, no content
        // at 0x0. headerpad = 0x500 - (32 + 72).
        let bytes = synth_macho(&[segment_64(
            "__TEXT", 0x0, 0x4000, 0x500, 0x100, 5, 5,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert_eq!(layout.headerpad(), 0x500 - (32 + 72));
    }

    #[test]
    fn max_vaddr_end_is_highest_segment_extent() {
        let bytes = synth_macho(&[
            segment_64("__TEXT", 0x0, 0x4000, 0x0, 0x4000, 5, 5),
            segment_64("__DATA", 0x4000, 0x4000, 0x4000, 0x4000, 3, 3),
            segment_64("__LINKEDIT", 0x8000, 0x4000, 0x8000, 0x2000, 1, 1),
        ]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert_eq!(layout.max_vaddr_end(), 0xc000);
    }

    #[test]
    fn max_fileoff_end_is_highest_segment_file_extent() {
        let bytes = synth_macho(&[
            segment_64("__TEXT", 0x0, 0x4000, 0x0, 0x4000, 5, 5),
            segment_64("__LINKEDIT", 0x4000, 0x2000, 0x4000, 0x1234, 1, 1),
        ]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert_eq!(layout.max_fileoff_end(), 0x4000 + 0x1234);
    }

    #[test]
    fn parse_skips_unknown_load_commands() {
        // Synthesise an unknown load command (cmd=0xdeadbeef,
        // cmdsize=24) between two segments. The parser should
        // skip it, not error.
        let mut unknown = Vec::with_capacity(24);
        unknown.extend_from_slice(&0xdeadbeef_u32.to_le_bytes());
        unknown.extend_from_slice(&24u32.to_le_bytes());
        unknown.extend_from_slice(&[0u8; 16]);
        let bytes = synth_macho(&[
            segment_64("__TEXT", 0x0, 0x4000, 0x0, 0x4000, 5, 5),
            unknown,
            segment_64("__DATA", 0x4000, 0x4000, 0x4000, 0x4000, 3, 3),
        ]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert_eq!(layout.segments.len(), 2);
        assert_eq!(layout.segments[0].name, "__TEXT");
        assert_eq!(layout.segments[1].name, "__DATA");
    }

    #[test]
    fn parse_rejects_zero_cmdsize() {
        // Truncate cmdsize to 0 — must error rather than loop.
        let mut bytes = synth_macho(&[segment_64("__TEXT", 0, 0, 0, 0, 5, 5)]);
        bytes[36..40].copy_from_slice(&0u32.to_le_bytes()); // cmdsize at offset 32+4
        let err = MachOLayout::parse(&bytes).expect_err("zero cmdsize");
        assert!(format!("{err}").contains("cmdsize"));
    }

    #[test]
    fn parse_rejects_segment_too_small() {
        // cmd=LC_SEGMENT_64 with cmdsize=16 (less than 72).
        let mut bad = Vec::with_capacity(16);
        bad.extend_from_slice(&macho::LC_SEGMENT_64.to_le_bytes());
        bad.extend_from_slice(&16u32.to_le_bytes());
        bad.extend_from_slice(&[0u8; 8]);
        let bytes = synth_macho(&[bad]);
        let err = MachOLayout::parse(&bytes).expect_err("too-small segment");
        assert!(format!("{err}").contains("LC_SEGMENT_64"));
    }

    #[test]
    fn text_free_regions_finds_tail_gap() {
        // __TEXT segment 0x4000 long, one section at the
        // start with size 0x100. Expected tail gap is the
        // remaining 0x4000 - 0x100 - section_offset bytes.
        let bytes = synth_macho(&[segment_64_with_section(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, "__text", 0x1000, 0x100, 0x1000,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        let regions = layout.text_free_regions();
        // One tail gap from end-of-__text (file 0x1100) to
        // end-of-__TEXT (file 0x4000) = 0x2f00 bytes.
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].file_offset, 0x1100);
        assert_eq!(regions[0].size, 0x4000 - 0x1100);
        assert_eq!(regions[0].vaddr, 0x1100);
        assert_eq!(regions[0].segment_name, "__TEXT");
    }

    #[test]
    fn allocate_in_text_returns_aligned_offset_when_fits() {
        let bytes = synth_macho(&[segment_64_with_section(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, "__text", 0x1000, 0x100, 0x1000,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        // Request 0x100 bytes aligned to 4. The tail gap
        // starts at file 0x1100 (already 4-aligned), so the
        // returned (vaddr, file_offset) should be (0x1100,
        // 0x1100).
        let (vaddr, file_offset) = layout
            .allocate_in_text(0x100, 4)
            .expect("should fit");
        assert_eq!(vaddr, 0x1100);
        assert_eq!(file_offset, 0x1100);
    }

    #[test]
    fn allocate_in_text_returns_none_when_no_region_fits() {
        let bytes = synth_macho(&[segment_64_with_section(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, "__text", 0x1000, 0x100, 0x1000,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        // Request 1 GB — won't fit anywhere.
        assert!(layout.allocate_in_text(0x4000_0000, 4).is_none());
    }

    #[test]
    fn text_free_regions_skips_header_gap() {
        // The mach_header + load commands sit at the start
        // of __TEXT before the first section. We don't want
        // callers placing content there because that would
        // overwrite the header.
        let bytes = synth_macho(&[segment_64_with_section(
            "__TEXT", 0x0, 0x4000, 0x0, 0x4000, "__text", 0x1000, 0x100, 0x1000,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        let regions = layout.text_free_regions();
        for region in &regions {
            assert!(
                region.file_offset >= 0x1000,
                "free region at file {} shouldn't include the header area",
                region.file_offset,
            );
        }
    }

    // ---- LC_DYSYMTAB + stub enumeration --------------------------------

    /// Build an `LC_DYSYMTAB` load command. 80 bytes total.
    /// All fields zero except those passed in — enough to exercise
    /// indirect-table reads.
    fn dysymtab(indirectsymoff: u32, nindirectsyms: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(80);
        out.extend_from_slice(&macho::LC_DYSYMTAB.to_le_bytes());
        out.extend_from_slice(&80u32.to_le_bytes()); // cmdsize
        // 6 × u32 symbol-range counts.
        for _ in 0..6 {
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        // 3 × (offset, count) pairs we don't care about (toc, modtab,
        // extrefsym) — 24 bytes.
        for _ in 0..6 {
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        // indirectsymoff, nindirectsyms.
        out.extend_from_slice(&indirectsymoff.to_le_bytes());
        out.extend_from_slice(&nindirectsyms.to_le_bytes());
        // extreloff, nextrel, locreloff, nlocrel.
        for _ in 0..4 {
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        out
    }

    /// Build a one-section segment with caller-supplied
    /// `flags` / `reserved1` / `reserved2` — used to drop a
    /// `__TEXT,__stubs` (or pointer) section into the synthetic
    /// image. 72 + 80 = 152 bytes.
    #[allow(clippy::too_many_arguments)]
    fn segment_64_with_typed_section(
        segname: &str,
        sect_name: &str,
        sect_addr: u64,
        sect_size: u64,
        sect_offset: u32,
        sect_flags: u32,
        reserved1: u32,
        reserved2: u32,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(152);
        out.extend_from_slice(&macho::LC_SEGMENT_64.to_le_bytes());
        out.extend_from_slice(&152u32.to_le_bytes());
        let mut name = [0u8; 16];
        name[..segname.len().min(16)]
            .copy_from_slice(&segname.as_bytes()[..segname.len().min(16)]);
        out.extend_from_slice(&name);
        out.extend_from_slice(&0u64.to_le_bytes()); // vmaddr
        out.extend_from_slice(&0x4000u64.to_le_bytes()); // vmsize
        out.extend_from_slice(&0u64.to_le_bytes()); // fileoff
        out.extend_from_slice(&0x4000u64.to_le_bytes()); // filesize
        out.extend_from_slice(&5u32.to_le_bytes()); // maxprot
        out.extend_from_slice(&5u32.to_le_bytes()); // initprot
        out.extend_from_slice(&1u32.to_le_bytes()); // nsects
        out.extend_from_slice(&0u32.to_le_bytes()); // flags

        let mut sectname_field = [0u8; 16];
        sectname_field[..sect_name.len().min(16)]
            .copy_from_slice(&sect_name.as_bytes()[..sect_name.len().min(16)]);
        out.extend_from_slice(&sectname_field);
        out.extend_from_slice(&name);
        out.extend_from_slice(&sect_addr.to_le_bytes());
        out.extend_from_slice(&sect_size.to_le_bytes());
        out.extend_from_slice(&sect_offset.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes()); // align
        out.extend_from_slice(&0u32.to_le_bytes()); // reloff
        out.extend_from_slice(&0u32.to_le_bytes()); // nreloc
        out.extend_from_slice(&sect_flags.to_le_bytes());
        out.extend_from_slice(&reserved1.to_le_bytes());
        out.extend_from_slice(&reserved2.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved3
        out
    }

    #[test]
    fn parse_captures_section_reserved_fields() {
        // __stubs section with reserved1=3, reserved2=12.
        let bytes = synth_macho(&[segment_64_with_typed_section(
            "__TEXT",
            "__stubs",
            0x1000,
            0x60,
            0x1000,
            S_SYMBOL_STUBS,
            3,
            12,
        )]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        let sect = layout.section("__TEXT", "__stubs").expect("__stubs");
        assert_eq!(sect.flags & SECTION_TYPE_MASK, S_SYMBOL_STUBS);
        assert_eq!(sect.reserved1, 3);
        assert_eq!(sect.reserved2, 12);
    }

    #[test]
    fn parse_captures_dysymtab_indirect_pointers() {
        let bytes = synth_macho(&[dysymtab(0x2000, 5)]);
        let layout = MachOLayout::parse(&bytes).expect("parse");
        let dyn_ = layout.dysymtab.expect("LC_DYSYMTAB");
        assert_eq!(dyn_.indirectsymoff, 0x2000);
        assert_eq!(dyn_.nindirectsyms, 5);
    }

    #[test]
    fn read_indirect_symtab_returns_entries_in_order() {
        // Build a file with a DYSYMTAB pointing at a trailing 5-entry
        // indirect table laid out immediately after the load commands.
        // To keep the synth_macho helper simple, we splice the table
        // into a fixed offset.
        let cmds = vec![dysymtab(0, 5)]; // patch indirectsymoff after build
        let mut bytes = synth_macho(&cmds);
        // Indirect table at end of file: 5 × u32 = 20 bytes.
        let indirect_off = bytes.len() as u32;
        for v in [10u32, 11, 12, 0x8000_0000, 13] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        // Patch the LC_DYSYMTAB's indirectsymoff field.
        // Layout: 32 (hdr) + 0 cmd-start + 8 (cmd, cmdsize) + 6*4 + 6*4
        //       = 32 + 56 = 88.
        bytes[88..92].copy_from_slice(&indirect_off.to_le_bytes());

        let layout = MachOLayout::parse(&bytes).expect("parse");
        let table = layout.read_indirect_symtab(&bytes);
        assert_eq!(table, vec![10, 11, 12, 0x8000_0000, 13]);
    }

    #[test]
    fn stub_entries_walks_symbol_stubs_section() {
        // __stubs with vaddr 0x1000, size 36 = 3 × 12-byte stubs.
        // reserved1=2 means the first stub binds to indirect[2].
        // Indirect table = [99, 99, 7, 8, 9] so the three stubs map
        // to symbol-table indices 7, 8, 9 at addresses 0x1000, 0x100c,
        // 0x1018.
        let cmds = vec![
            segment_64_with_typed_section(
                "__TEXT",
                "__stubs",
                0x1000,
                36,
                0x1000,
                S_SYMBOL_STUBS,
                2,
                12,
            ),
            dysymtab(0, 5),
        ];
        let mut bytes = synth_macho(&cmds);
        let indirect_off = bytes.len() as u32;
        for v in [99u32, 99, 7, 8, 9] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        // dysymtab is the 2nd load command (offset 32 + 152 = 184),
        // indirectsymoff field is 56 bytes into the cmd body.
        bytes[184 + 56..184 + 60].copy_from_slice(&indirect_off.to_le_bytes());

        let layout = MachOLayout::parse(&bytes).expect("parse");
        let stubs = layout.stub_entries(&bytes);
        assert_eq!(stubs.len(), 3);
        assert_eq!(stubs[0], MachOStubEntry { address: 0x1000, symtab_index: 7 });
        assert_eq!(stubs[1], MachOStubEntry { address: 0x100c, symtab_index: 8 });
        assert_eq!(stubs[2], MachOStubEntry { address: 0x1018, symtab_index: 9 });
    }

    #[test]
    fn stub_entries_drops_sentinel_slots() {
        let cmds = vec![
            segment_64_with_typed_section(
                "__TEXT",
                "__stubs",
                0x2000,
                24,
                0x2000,
                S_SYMBOL_STUBS,
                0,
                12,
            ),
            dysymtab(0, 2),
        ];
        let mut bytes = synth_macho(&cmds);
        let indirect_off = bytes.len() as u32;
        // Both slots are sentinels — stub_entries should return empty.
        for v in [INDIRECT_SYMBOL_LOCAL, INDIRECT_SYMBOL_ABS] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes[184 + 56..184 + 60].copy_from_slice(&indirect_off.to_le_bytes());

        let layout = MachOLayout::parse(&bytes).expect("parse");
        assert!(layout.stub_entries(&bytes).is_empty());
    }

    #[test]
    fn import_pointer_entries_walks_nonlazy_section() {
        // __got with vaddr 0x3000, size 24 = 3 × 8-byte pointers.
        // reserved1=1, indirect = [99, 21, 22, 23] → entries at
        // 0x3000/21, 0x3008/22, 0x3010/23.
        let cmds = vec![
            segment_64_with_typed_section(
                "__DATA_CONST",
                "__got",
                0x3000,
                24,
                0x3000,
                S_NON_LAZY_SYMBOL_POINTERS,
                1,
                0,
            ),
            dysymtab(0, 4),
        ];
        let mut bytes = synth_macho(&cmds);
        let indirect_off = bytes.len() as u32;
        for v in [99u32, 21, 22, 23] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes[184 + 56..184 + 60].copy_from_slice(&indirect_off.to_le_bytes());

        let layout = MachOLayout::parse(&bytes).expect("parse");
        let ptrs = layout.import_pointer_entries(&bytes);
        assert_eq!(ptrs.len(), 3);
        assert_eq!(ptrs[0].address, 0x3000);
        assert_eq!(ptrs[0].symtab_index, 21);
        assert_eq!(ptrs[2].address, 0x3010);
        assert_eq!(ptrs[2].symtab_index, 23);
    }
}
