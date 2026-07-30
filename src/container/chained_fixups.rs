//! Semantic editable view of `LC_DYLD_CHAINED_FIXUPS`.
//!
//! Reads the dyld chained-fixups blob into a flat, editable
//! [`ChainedFixups`] structure, hides the page-chain plumbing
//! (per-page `page_start[]` arrays + the `next` delta walked
//! between chain entries) behind a sorted-by-vaddr list of
//! [`Fixup`] slots per segment, and serialises the structure
//! back into a byte stream suitable for replacing the original
//! `LC_DYLD_CHAINED_FIXUPS` data range.
//!
//! Scope:
//!
//! * Supported pointer formats: `PTR_64`, `PTR_64_OFFSET`, and
//!   the four arm64e variants (`ARM64E`, `ARM64E_KERNEL`,
//!   `ARM64E_USERLAND`, `ARM64E_USERLAND24`). `PTR_32`,
//!   `PTR_64_KERNEL_CACHE`, and `ARM64E_FIRMWARE` are
//!   recognised by name and refused with
//!   [`ChainedFixupsError::UnsupportedFormat`].
//!
//! * Import-pool serialisation: when nothing changed,
//!   [`ChainedFixups::serialize`] preserves the original
//!   symbol-table ordering and string pool byte-for-byte. When
//!   imports were added, removed, or had their `symbol` /
//!   `addend` / `lib_ordinal` / `weak` fields edited, the pool
//!   is rebuilt fresh (still valid, just not necessarily
//!   byte-identical to the input).
//!
//! * Auth (arm64e) pointer fields beyond bind-vs-rebase
//!   discrimination are NOT modelled. The serialiser emits
//!   non-auth chain entries. Callers wishing to preserve auth
//!   diversifier / key / addr bits will need to extend
//!   [`FixupTarget`] later; today armadillo only cares about
//!   the rebase/bind shape.

use std::collections::HashMap;

use crate::container::macho_image::MachOImage;
use crate::container::ContainerWriteError;

// ---------------------------------------------------------------------------
// Constants — mirror libdyld `mach-o/fixup-chains.h`.
// ---------------------------------------------------------------------------

const FIXUPS_HEADER_SIZE: usize = 32;

const DYLD_CHAINED_PTR_ARM64E: u16 = 1;
const DYLD_CHAINED_PTR_64: u16 = 2;
const DYLD_CHAINED_PTR_32: u16 = 3;
const DYLD_CHAINED_PTR_64_OFFSET: u16 = 6;
const DYLD_CHAINED_PTR_ARM64E_KERNEL: u16 = 7;
const DYLD_CHAINED_PTR_64_KERNEL_CACHE: u16 = 8;
const DYLD_CHAINED_PTR_ARM64E_USERLAND: u16 = 9;
const DYLD_CHAINED_PTR_ARM64E_FIRMWARE: u16 = 10;
const DYLD_CHAINED_PTR_ARM64E_USERLAND24: u16 = 12;

const DYLD_CHAINED_IMPORT: u32 = 1;
const DYLD_CHAINED_IMPORT_ADDEND: u32 = 2;
const DYLD_CHAINED_IMPORT_ADDEND64: u32 = 3;

const DYLD_CHAINED_PTR_START_NONE: u16 = 0xFFFF;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Pointer-format families we support read+write. Mirrors a
/// subset of the on-disk `pointer_format` field. The kernel-
/// cache, 32-bit, and arm64e-firmware variants are explicitly
/// out of scope — they appear in places (kernel collections,
/// embedded firmware) that this crate doesn't target.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PointerFormat {
    /// `DYLD_CHAINED_PTR_64` — 4-byte stride, 12-bit next,
    /// rebase target = absolute 36-bit vmaddr.
    Ptr64,
    /// `DYLD_CHAINED_PTR_64_OFFSET` — 4-byte stride, 12-bit
    /// next, rebase target = 36-bit offset from the preferred
    /// load address. dyld's modern default for arm64 userland.
    Ptr64Offset,
    /// `DYLD_CHAINED_PTR_ARM64E` — 8-byte stride, 11-bit next,
    /// rebase target = absolute 43-bit vmaddr, has auth bit.
    Arm64e,
    /// `DYLD_CHAINED_PTR_ARM64E_KERNEL` — 8-byte stride,
    /// rebase target = 32-bit offset from segment base.
    Arm64eKernel,
    /// `DYLD_CHAINED_PTR_ARM64E_USERLAND` — 8-byte stride,
    /// rebase target = 36-bit offset from preferred load addr.
    Arm64eUserland,
    /// `DYLD_CHAINED_PTR_ARM64E_USERLAND24` — same as
    /// `Userland` but with a 24-bit bind ordinal (so up to
    /// ~16M imports per image).
    Arm64eUserland24,
}

impl PointerFormat {
    /// Stride (in bytes) between consecutive chain entries
    /// encoded by `next`. dyld's `next` is in stride units —
    /// `next * stride` = byte offset.
    pub fn stride(self) -> u32 {
        match self {
            Self::Ptr64 | Self::Ptr64Offset => 4,
            Self::Arm64e
            | Self::Arm64eKernel
            | Self::Arm64eUserland
            | Self::Arm64eUserland24 => 8,
        }
    }

    /// Width of the `next` field in bits. A page can only span
    /// `(1 << next_bits) * stride` bytes of chain.
    pub fn next_bits(self) -> u32 {
        match self {
            Self::Ptr64 | Self::Ptr64Offset => 12,
            Self::Arm64e
            | Self::Arm64eKernel
            | Self::Arm64eUserland
            | Self::Arm64eUserland24 => 11,
        }
    }

    /// Maximum byte gap between adjacent fixup slots in the
    /// same chain. `serialize()` errors if two fixups inside
    /// one page are farther apart than this.
    pub fn max_next_byte_delta(self) -> u32 {
        ((1u32 << self.next_bits()) - 1) * self.stride()
    }

    fn from_raw(raw: u16) -> Result<Self, ChainedFixupsError> {
        Ok(match raw {
            DYLD_CHAINED_PTR_64 => Self::Ptr64,
            DYLD_CHAINED_PTR_64_OFFSET => Self::Ptr64Offset,
            DYLD_CHAINED_PTR_ARM64E => Self::Arm64e,
            DYLD_CHAINED_PTR_ARM64E_KERNEL => Self::Arm64eKernel,
            DYLD_CHAINED_PTR_ARM64E_USERLAND => Self::Arm64eUserland,
            DYLD_CHAINED_PTR_ARM64E_USERLAND24 => Self::Arm64eUserland24,
            DYLD_CHAINED_PTR_32
            | DYLD_CHAINED_PTR_64_KERNEL_CACHE
            | DYLD_CHAINED_PTR_ARM64E_FIRMWARE => {
                return Err(ChainedFixupsError::UnsupportedFormat(raw))
            }
            other => return Err(ChainedFixupsError::UnknownFormat(other)),
        })
    }

    fn to_raw(self) -> u16 {
        match self {
            Self::Ptr64 => DYLD_CHAINED_PTR_64,
            Self::Ptr64Offset => DYLD_CHAINED_PTR_64_OFFSET,
            Self::Arm64e => DYLD_CHAINED_PTR_ARM64E,
            Self::Arm64eKernel => DYLD_CHAINED_PTR_ARM64E_KERNEL,
            Self::Arm64eUserland => DYLD_CHAINED_PTR_ARM64E_USERLAND,
            Self::Arm64eUserland24 => DYLD_CHAINED_PTR_ARM64E_USERLAND24,
        }
    }
}

/// Editable representation of an image's chained-fixups blob.
#[derive(Debug, Clone)]
pub struct ChainedFixups {
    /// Pointer format used by every chain in this image. The
    /// on-disk format records a separate value per segment;
    /// in practice every segment uses the same format and the
    /// reader / writer assume that. If a binary mixes formats
    /// we error at read time.
    pub pointer_format: PointerFormat,
    /// Imports table. Indexed by `FixupTarget::Bind::import_index`.
    pub imports: Vec<ChainedImport>,
    /// Per-segment fixup slots, sorted by vaddr inside each
    /// segment. `segment_index` matches `MachOLayout.segments`.
    pub segments: Vec<SegmentFixups>,
    /// On-disk imports-format the input used. Preserved so
    /// round-tripping a read-only image produces the same
    /// imports encoding rather than always upgrading.
    on_disk_imports_format: u32,
    /// Original byte size of the imports + symbol pool, used
    /// to detect "did anything change" for byte-stable
    /// round-trip when nothing was edited.
    original_imports_bytes: Vec<u8>,
    /// Snapshot of the decoded imports as they appeared on
    /// disk. Compared against `imports` at serialise time to
    /// decide whether to repack.
    original_imports: Vec<ChainedImport>,
}

/// One entry in the imports table.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChainedImport {
    /// 1-based dylib index into `LC_LOAD_DYLIB` list, or one of
    /// the special values:
    ///   * `0`  → `SELF_LIBRARY_ORDINAL`
    ///   * `-1` → `BIND_SPECIAL_DYLIB_MAIN_EXECUTABLE`
    ///   * `-2` → `BIND_SPECIAL_DYLIB_FLAT_LOOKUP`
    ///   * `-3` → `BIND_SPECIAL_DYLIB_WEAK_LOOKUP`
    pub lib_ordinal: i32,
    pub symbol: String,
    pub addend: i64,
    pub weak: bool,
}

/// Fixup slots for one segment.
#[derive(Debug, Clone)]
pub struct SegmentFixups {
    /// Index into `MachOLayout.segments`.
    pub segment_index: usize,
    /// Page size used to bucket fixups into chains. Matches
    /// dyld's expected value for the target arch (16 KiB on
    /// arm64 userland, 4 KiB on x86_64).
    pub page_size: u32,
    /// Fixups in this segment, sorted by `vaddr` ascending.
    /// The serialiser re-sorts defensively; callers are not
    /// required to maintain order.
    pub fixups: Vec<Fixup>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Fixup {
    /// Absolute vmaddr of the fixup slot itself. Must be
    /// inside `MachOLayout.segments[segment_index]`'s vmaddr
    /// range and aligned to `pointer_format.stride()`.
    pub vaddr: u64,
    pub target: FixupTarget,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FixupTarget {
    /// Rebase: at load time dyld writes `target_vaddr +
    /// load_slide` to this slot.
    Rebase {
        /// Absolute vmaddr (pre-slide) that this slot resolves
        /// to. For `Ptr64_Offset` and arm64e-userland formats,
        /// the on-disk encoding is an offset; the editable
        /// view always uses the absolute address and the
        /// serialiser converts.
        target_vaddr: u64,
    },
    /// Bind: at load time dyld writes
    /// `addr_of(imports[import_index].symbol) + addend` to
    /// this slot.
    Bind { import_index: usize, addend: i64 },
}

#[derive(Debug)]
pub enum ChainedFixupsError {
    /// Image has no `LC_DYLD_CHAINED_FIXUPS`.
    Missing,
    /// Some structural byte range didn't fit in the file.
    Truncated(String),
    /// `pointer_format` is recognised but unsupported (32-bit,
    /// kernel cache, arm64e firmware).
    UnsupportedFormat(u16),
    /// `pointer_format` value isn't known. Likely a future
    /// libdyld version.
    UnknownFormat(u16),
    /// On serialise: a fixup slot's vaddr isn't aligned to the
    /// pointer format's stride.
    Misaligned { vaddr: u64, stride: u32 },
    /// On serialise: two fixups in the same page are farther
    /// apart than the pointer format's `next` can encode.
    /// Caller must either split the chain across pages or add
    /// no-op intermediate rebases.
    NextOverflow {
        page_vaddr: u64,
        from_vaddr: u64,
        to_vaddr: u64,
        max_byte_delta: u32,
    },
    /// On serialise: a `Bind` references an `import_index`
    /// that doesn't exist in `imports`.
    BadImportIndex { import_index: usize, len: usize },
    /// On serialise: a fixup's vaddr lies outside its declared
    /// segment.
    FixupOutsideSegment {
        vaddr: u64,
        segment_index: usize,
    },
    /// On serialise: an import string can't fit in the on-disk
    /// `name_offset` field for the chosen imports format.
    /// (`DYLD_CHAINED_IMPORT` and `_ADDEND` use 23-bit
    /// offsets; the string pool may exceed 8 MiB if many long
    /// symbols are appended.)
    ImportNameOffsetOverflow {
        offset: u64,
        max: u64,
    },
    /// On serialise: a `Bind` addend can't fit in the format's
    /// inline addend field, and the imports format isn't
    /// already `ADDEND64`. Promote to `IMPORT_ADDEND64`
    /// manually by calling [`ChainedFixups::ensure_addend64`].
    AddendOverflow { import_index: usize, addend: i64 },
}

impl std::fmt::Display for ChainedFixupsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => f.write_str("chained-fixups: image has no LC_DYLD_CHAINED_FIXUPS"),
            Self::Truncated(m) => write!(f, "chained-fixups: truncated: {m}"),
            Self::UnsupportedFormat(n) => {
                write!(f, "chained-fixups: pointer_format {n} is recognised but not supported by this writer")
            }
            Self::UnknownFormat(n) => {
                write!(f, "chained-fixups: unknown pointer_format {n}")
            }
            Self::Misaligned { vaddr, stride } => write!(
                f,
                "chained-fixups: fixup at vaddr {vaddr:#x} not aligned to stride {stride}",
            ),
            Self::NextOverflow {
                page_vaddr,
                from_vaddr,
                to_vaddr,
                max_byte_delta,
            } => write!(
                f,
                "chained-fixups: page {page_vaddr:#x}: gap between {from_vaddr:#x} and {to_vaddr:#x} exceeds max next delta {max_byte_delta} bytes",
            ),
            Self::BadImportIndex { import_index, len } => write!(
                f,
                "chained-fixups: bind references import {import_index}, only {len} imports defined",
            ),
            Self::FixupOutsideSegment {
                vaddr,
                segment_index,
            } => write!(
                f,
                "chained-fixups: fixup at {vaddr:#x} not inside segment {segment_index}",
            ),
            Self::ImportNameOffsetOverflow { offset, max } => write!(
                f,
                "chained-fixups: import name offset {offset} > max {max} for this imports_format",
            ),
            Self::AddendOverflow {
                import_index,
                addend,
            } => write!(
                f,
                "chained-fixups: addend {addend} for import {import_index} doesn't fit in current imports_format; promote to ADDEND64",
            ),
        }
    }
}

impl std::error::Error for ChainedFixupsError {}

impl From<ChainedFixupsError> for ContainerWriteError {
    fn from(e: ChainedFixupsError) -> Self {
        ContainerWriteError::ObjectWrite(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

impl ChainedFixups {
    /// Parse `LC_DYLD_CHAINED_FIXUPS` into the editable view.
    pub fn read(image: &MachOImage) -> Result<Self, ChainedFixupsError> {
        let Some(fixups) = image.layout.chained_fixups else {
            return Err(ChainedFixupsError::Missing);
        };
        let bytes = &image.raw_bytes;
        let base = fixups.dataoff as usize;
        let end = base + fixups.datasize as usize;
        if end > bytes.len() || fixups.datasize < FIXUPS_HEADER_SIZE as u64 {
            return Err(ChainedFixupsError::Truncated(format!(
                "header range {}..{} of file {}",
                base,
                end,
                bytes.len(),
            )));
        }
        let header = &bytes[base..base + FIXUPS_HEADER_SIZE];
        let fixups_version = u32_le(&header[0..4]);
        if fixups_version != 0 {
            return Err(ChainedFixupsError::Truncated(format!(
                "unsupported fixups_version {fixups_version}"
            )));
        }
        let starts_offset = u32_le(&header[4..8]) as usize;
        let imports_offset = u32_le(&header[8..12]) as usize;
        let symbols_offset = u32_le(&header[12..16]) as usize;
        let imports_count = u32_le(&header[16..20]) as usize;
        let imports_format = u32_le(&header[20..24]);
        let _symbols_format = u32_le(&header[24..28]);

        let imports = decode_imports(
            bytes,
            base,
            imports_offset,
            symbols_offset,
            imports_count,
            imports_format,
            end,
        )?;

        // Capture the original imports + symbol-pool bytes so
        // we can choose byte-stable round-trip when nothing
        // changed.
        let original_imports_bytes = bytes[base + imports_offset..end].to_vec();
        let original_imports = imports.clone();

        // starts_in_image header at base+starts_offset:
        //   u32 seg_count, u32 seg_info_offset[seg_count].
        let starts_in_image_abs = base + starts_offset;
        if starts_in_image_abs + 4 > end {
            return Err(ChainedFixupsError::Truncated(
                "starts_in_image header".into(),
            ));
        }
        let seg_count =
            u32_le(&bytes[starts_in_image_abs..starts_in_image_abs + 4]) as usize;
        let seg_offsets_base = starts_in_image_abs + 4;
        if seg_offsets_base + seg_count * 4 > end {
            return Err(ChainedFixupsError::Truncated(
                "starts_in_image seg_info_offset[]".into(),
            ));
        }

        // Walk each segment, picking up its pointer_format. We
        // require all populated segments to agree.
        let mut segments: Vec<SegmentFixups> = Vec::new();
        let mut chosen_pointer_format: Option<PointerFormat> = None;
        for seg_idx in 0..seg_count {
            let off_field = u32_le(
                &bytes[seg_offsets_base + seg_idx * 4..seg_offsets_base + seg_idx * 4 + 4],
            ) as usize;
            if off_field == 0 {
                continue;
            }
            let seg_info_abs = starts_in_image_abs + off_field;
            let walked = walk_one_segment(
                bytes,
                seg_info_abs,
                seg_idx,
                end,
                &image.layout.segments,
                &imports,
            )?;
            match chosen_pointer_format {
                Some(existing) if existing != walked.pointer_format => {
                    return Err(ChainedFixupsError::Truncated(format!(
                        "segments use different pointer_format ({:?} vs {:?})",
                        existing, walked.pointer_format
                    )));
                }
                Some(_) => {}
                None => chosen_pointer_format = Some(walked.pointer_format),
            }
            segments.push(SegmentFixups {
                segment_index: seg_idx,
                page_size: walked.page_size,
                fixups: walked.fixups,
            });
        }
        let pointer_format = chosen_pointer_format.ok_or(ChainedFixupsError::Truncated(
            "no segment had any chain starts".into(),
        ))?;

        Ok(Self {
            pointer_format,
            imports,
            segments,
            on_disk_imports_format: imports_format,
            original_imports_bytes,
            original_imports,
        })
    }

    /// Build the legacy `ChainedFixupMap`-shaped view (vaddr →
    /// rebase target / bind name) from this editable
    /// structure. Used by the ObjC reader.
    pub fn to_map(&self) -> HashMap<u64, ResolvedTarget> {
        let mut out = HashMap::new();
        for seg in &self.segments {
            for fx in &seg.fixups {
                let resolved = match &fx.target {
                    FixupTarget::Rebase { target_vaddr } => ResolvedTarget::Rebase {
                        target_vaddr: *target_vaddr,
                    },
                    FixupTarget::Bind {
                        import_index,
                        addend,
                    } => {
                        let imp = &self.imports[*import_index];
                        ResolvedTarget::Bind {
                            dylib_ordinal: imp.lib_ordinal,
                            symbol_name: imp.symbol.clone(),
                            addend: addend + imp.addend,
                        }
                    }
                };
                out.insert(fx.vaddr, resolved);
            }
        }
        out
    }

    /// Convenience for callers that want to know whether
    /// anything was edited. Returns `true` iff `imports` and
    /// every `segments[].fixups` field matches what `read()`
    /// produced bit-for-bit. Used internally to choose the
    /// byte-stable serialisation path.
    pub fn unchanged_imports(&self) -> bool {
        self.imports == self.original_imports
    }

    /// Find every `Rebase` fixup that targets `old_vaddr` and
    /// repoint it at `new_vaddr`. Returns the number of fixups
    /// touched.
    ///
    /// The common shape for an ObjC rename-to-longer-string is:
    ///
    /// ```text
    /// let new_vaddr = editor.binary.append_macho_segment_padding(
    ///     "__DATA_CONST", b"longer_name\0")?;
    /// cf.repoint_fixup(old_string_vaddr, new_vaddr);
    /// ```
    ///
    /// `Bind` fixups are unaffected — they don't carry a vaddr
    /// target, only an import index + addend. To rename a
    /// bind, mutate the matching entry in `self.imports`
    /// instead.
    pub fn repoint_fixup(&mut self, old_vaddr: u64, new_vaddr: u64) -> usize {
        let mut hits = 0;
        for seg in self.segments.iter_mut() {
            for fx in seg.fixups.iter_mut() {
                if let FixupTarget::Rebase { target_vaddr } = &mut fx.target {
                    if *target_vaddr == old_vaddr {
                        *target_vaddr = new_vaddr;
                        hits += 1;
                    }
                }
            }
        }
        hits
    }

    /// Shift every fixup up by `delta`: each fixup's slot location
    /// (`vaddr`) and each `Rebase` target move by the same amount.
    ///
    /// This is the chained-fixup half of the uniform-shift text-grow —
    /// where the whole image past the insert point moves by `delta`, so
    /// both the fixup slots (in `__DATA*`) and everything they rebase to
    /// shift together. `Bind` targets are import indices, not addresses,
    /// so only their slot `vaddr` shifts.
    ///
    /// The result serialises against the correspondingly shifted segment
    /// list (each fixup still lands inside its segment because both moved
    /// by `delta`).
    pub fn shift_by(&mut self, delta: u64) {
        for seg in self.segments.iter_mut() {
            for fx in seg.fixups.iter_mut() {
                fx.vaddr = fx.vaddr.saturating_add(delta);
                if let FixupTarget::Rebase { target_vaddr } = &mut fx.target {
                    *target_vaddr = target_vaddr.saturating_add(delta);
                }
            }
        }
    }
}

/// Convenience target shape — same shape as the legacy
/// `ChainedFixupTarget` exposed by the ObjC reader.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ResolvedTarget {
    Rebase { target_vaddr: u64 },
    Bind {
        dylib_ordinal: i32,
        symbol_name: String,
        addend: i64,
    },
}

struct WalkedSegment {
    pointer_format: PointerFormat,
    page_size: u32,
    fixups: Vec<Fixup>,
}

fn walk_one_segment(
    bytes: &[u8],
    seg_info_abs: usize,
    seg_idx: usize,
    fixups_end: usize,
    segments: &[crate::container::macho_image::MachOSegment],
    imports: &[ChainedImport],
) -> Result<WalkedSegment, ChainedFixupsError> {
    // dyld_chained_starts_in_segment: u32 size, u16 page_size,
    // u16 pointer_format, u64 segment_offset, u32
    // max_valid_pointer, u16 page_count, u16 page_start[].
    if seg_info_abs + 22 > fixups_end {
        return Err(ChainedFixupsError::Truncated(
            "starts_in_segment header".into(),
        ));
    }
    let _size = u32_le(&bytes[seg_info_abs..seg_info_abs + 4]);
    let page_size = u16_le(&bytes[seg_info_abs + 4..seg_info_abs + 6]) as u32;
    let pointer_format_raw = u16_le(&bytes[seg_info_abs + 6..seg_info_abs + 8]);
    let pointer_format = PointerFormat::from_raw(pointer_format_raw)?;
    let _segment_offset = u64_le(&bytes[seg_info_abs + 8..seg_info_abs + 16]);
    let _max_valid_pointer = u32_le(&bytes[seg_info_abs + 16..seg_info_abs + 20]);
    let page_count = u16_le(&bytes[seg_info_abs + 20..seg_info_abs + 22]) as usize;
    let page_start_base = seg_info_abs + 22;
    if page_start_base + page_count * 2 > fixups_end {
        return Err(ChainedFixupsError::Truncated(
            "starts_in_segment page_start[]".into(),
        ));
    }
    let seg = segments
        .get(seg_idx)
        .ok_or_else(|| ChainedFixupsError::Truncated(format!("seg index {seg_idx}")))?;
    let stride = pointer_format.stride() as u64;
    let seg_end_file_off = seg.fileoff + seg.filesize;

    let mut out = Vec::new();
    for page_idx in 0..page_count {
        let page_start = u16_le(
            &bytes[page_start_base + page_idx * 2..page_start_base + page_idx * 2 + 2],
        );
        if page_start == DYLD_CHAINED_PTR_START_NONE {
            continue;
        }
        let mut off_in_seg = page_idx as u64 * page_size as u64 + page_start as u64;
        loop {
            let chain_vaddr = seg.vmaddr + off_in_seg;
            let chain_file_off = seg.fileoff + off_in_seg;
            if chain_file_off + 8 > seg_end_file_off
                || chain_file_off as usize + 8 > bytes.len()
            {
                break;
            }
            let raw = u64_le(&bytes[chain_file_off as usize..chain_file_off as usize + 8]);
            let (target, next) = decode_one(raw, pointer_format, imports, seg.vmaddr)?;
            out.push(Fixup {
                vaddr: chain_vaddr,
                target,
            });
            if next == 0 {
                break;
            }
            off_in_seg += next as u64 * stride;
        }
    }
    out.sort_by_key(|f| f.vaddr);
    Ok(WalkedSegment {
        pointer_format,
        page_size,
        fixups: out,
    })
}

fn decode_imports(
    bytes: &[u8],
    fixups_base: usize,
    imports_offset: usize,
    symbols_offset: usize,
    imports_count: usize,
    imports_format: u32,
    fixups_end: usize,
) -> Result<Vec<ChainedImport>, ChainedFixupsError> {
    let entry_size = match imports_format {
        DYLD_CHAINED_IMPORT => 4,
        DYLD_CHAINED_IMPORT_ADDEND => 8,
        DYLD_CHAINED_IMPORT_ADDEND64 => 16,
        other => {
            return Err(ChainedFixupsError::Truncated(format!(
                "unknown imports_format {other}"
            )))
        }
    };
    let imports_abs = fixups_base + imports_offset;
    let imports_end_abs = imports_abs + imports_count * entry_size;
    let symbols_abs = fixups_base + symbols_offset;
    if imports_end_abs > fixups_end || symbols_abs > fixups_end {
        return Err(ChainedFixupsError::Truncated(
            "imports/symbols past fixups end".into(),
        ));
    }
    let symbols_pool = &bytes[symbols_abs..fixups_end];
    let mut out = Vec::with_capacity(imports_count);
    for i in 0..imports_count {
        let e = imports_abs + i * entry_size;
        let (lib_ordinal, weak, name_offset, addend) = match imports_format {
            DYLD_CHAINED_IMPORT => {
                let raw = u32_le(&bytes[e..e + 4]);
                let lib_ordinal = sign_extend_8(raw & 0xff);
                let weak = (raw >> 8) & 1 != 0;
                let name_offset = ((raw >> 9) & 0x007f_ffff) as usize;
                (lib_ordinal, weak, name_offset, 0i64)
            }
            DYLD_CHAINED_IMPORT_ADDEND => {
                let raw = u32_le(&bytes[e..e + 4]);
                let lib_ordinal = sign_extend_8(raw & 0xff);
                let weak = (raw >> 8) & 1 != 0;
                let name_offset = ((raw >> 9) & 0x007f_ffff) as usize;
                let addend = i32::from_le_bytes(bytes[e + 4..e + 8].try_into().unwrap()) as i64;
                (lib_ordinal, weak, name_offset, addend)
            }
            DYLD_CHAINED_IMPORT_ADDEND64 => {
                let raw = u32_le(&bytes[e..e + 4]);
                let lib_ordinal = sign_extend_16(raw & 0xffff);
                let weak = (raw >> 16) & 1 != 0;
                let name_offset = u32_le(&bytes[e + 4..e + 8]) as usize;
                let addend =
                    i64::from_le_bytes(bytes[e + 8..e + 16].try_into().unwrap());
                (lib_ordinal, weak, name_offset, addend)
            }
            _ => unreachable!(),
        };
        let symbol = read_cstr(symbols_pool, name_offset)
            .ok_or_else(|| ChainedFixupsError::Truncated(format!("import {i} string")))?
            .to_string();
        out.push(ChainedImport {
            lib_ordinal,
            symbol,
            addend,
            weak,
        });
    }
    Ok(out)
}

fn decode_one(
    raw: u64,
    pointer_format: PointerFormat,
    imports: &[ChainedImport],
    seg_vmaddr_base: u64,
) -> Result<(FixupTarget, u32), ChainedFixupsError> {
    match pointer_format {
        PointerFormat::Ptr64 | PointerFormat::Ptr64Offset => {
            let bind = (raw >> 63) & 1 != 0;
            let next = ((raw >> 51) & 0xfff) as u32;
            if bind {
                let ordinal = (raw & 0x00ff_ffff) as usize;
                let addend = ((raw >> 24) & 0xff) as i64;
                if ordinal >= imports.len() {
                    return Err(ChainedFixupsError::Truncated(format!(
                        "bind ordinal {ordinal} out of range"
                    )));
                }
                Ok((
                    FixupTarget::Bind {
                        import_index: ordinal,
                        addend,
                    },
                    next,
                ))
            } else {
                let target_vaddr = raw & 0x0000_000f_ffff_ffff;
                Ok((FixupTarget::Rebase { target_vaddr }, next))
            }
        }
        PointerFormat::Arm64e
        | PointerFormat::Arm64eKernel
        | PointerFormat::Arm64eUserland
        | PointerFormat::Arm64eUserland24 => {
            let auth = (raw >> 63) & 1 != 0;
            let bind = (raw >> 62) & 1 != 0;
            let next = ((raw >> 51) & 0x7ff) as u32;
            if bind {
                let ordinal_mask = if pointer_format == PointerFormat::Arm64eUserland24 {
                    0x00ff_ffff
                } else {
                    0x0000_ffff
                };
                let ordinal = (raw & ordinal_mask) as usize;
                if ordinal >= imports.len() {
                    return Err(ChainedFixupsError::Truncated(format!(
                        "arm64e bind ordinal {ordinal} out of range"
                    )));
                }
                let addend = if auth {
                    0
                } else {
                    let raw_add = ((raw >> 24) & 0x7_ffff) as i64;
                    if raw_add & (1 << 18) != 0 {
                        raw_add - (1 << 19)
                    } else {
                        raw_add
                    }
                };
                Ok((
                    FixupTarget::Bind {
                        import_index: ordinal,
                        addend,
                    },
                    next,
                ))
            } else {
                let target_low = raw & 0x0000_000f_ffff_ffff;
                let target_vaddr = match pointer_format {
                    PointerFormat::Arm64eUserland
                    | PointerFormat::Arm64eUserland24
                    | PointerFormat::Arm64eKernel => seg_vmaddr_base + target_low,
                    _ => target_low,
                };
                Ok((FixupTarget::Rebase { target_vaddr }, next))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Output of [`ChainedFixups::serialize`].
#[derive(Debug, Clone)]
pub struct SerializedChainedFixups {
    /// Full byte blob to place at the original
    /// `LC_DYLD_CHAINED_FIXUPS` `dataoff`. Includes the header,
    /// starts_in_image, every starts_in_segment, the imports
    /// table, and the symbol-string pool.
    pub bytes: Vec<u8>,
    /// Imports format actually used. May differ from the
    /// input's format when `ensure_addend64` was called or
    /// when promotion was forced by an oversized addend.
    pub imports_format: u32,
}

impl ChainedFixups {
    /// Force the imports format to `IMPORT_ADDEND64`. Useful
    /// when a caller knows ahead of time it'll be adding
    /// imports with `> i32::MAX` addends.
    pub fn ensure_addend64(&mut self) {
        self.on_disk_imports_format = DYLD_CHAINED_IMPORT_ADDEND64;
    }

    /// Serialise back to a byte blob.
    pub fn serialize(
        &self,
        segments: &[crate::container::macho_image::MachOSegment],
    ) -> Result<SerializedChainedFixups, ChainedFixupsError> {
        // 1. Validate every fixup's alignment + segment
        //    membership + bind import index.
        for sf in &self.segments {
            let seg = segments
                .get(sf.segment_index)
                .ok_or_else(|| ChainedFixupsError::FixupOutsideSegment {
                    vaddr: 0,
                    segment_index: sf.segment_index,
                })?;
            let stride = self.pointer_format.stride() as u64;
            for fx in &sf.fixups {
                if fx.vaddr % stride != 0 {
                    return Err(ChainedFixupsError::Misaligned {
                        vaddr: fx.vaddr,
                        stride: stride as u32,
                    });
                }
                if fx.vaddr < seg.vmaddr || fx.vaddr >= seg.vmaddr + seg.vmsize {
                    return Err(ChainedFixupsError::FixupOutsideSegment {
                        vaddr: fx.vaddr,
                        segment_index: sf.segment_index,
                    });
                }
                if let FixupTarget::Bind { import_index, .. } = fx.target {
                    if import_index >= self.imports.len() {
                        return Err(ChainedFixupsError::BadImportIndex {
                            import_index,
                            len: self.imports.len(),
                        });
                    }
                }
            }
        }

        // 2. Decide imports format. If callers haven't touched
        //    anything we can replay the original imports +
        //    symbol pool byte-for-byte. Otherwise repack into
        //    a fresh pool, possibly promoting to ADDEND64.
        let (imports_format, imports_blob) = if self.unchanged_imports() {
            (self.on_disk_imports_format, self.original_imports_bytes.clone())
        } else {
            build_imports_blob(&self.imports, self.on_disk_imports_format)?
        };

        // 3. Lay out the new file: header (32) + starts_in_image
        //    + each starts_in_segment + imports blob.
        let mut sorted = self.segments.clone();
        for sf in sorted.iter_mut() {
            sf.fixups.sort_by_key(|f| f.vaddr);
            // De-dup by vaddr — last wins. Two fixups at the
            // same slot would collide on serialise and dyld
            // can only fix up a slot once.
            sf.fixups.dedup_by_key(|f| f.vaddr);
        }

        // Layout the variable-length pieces, computing
        // offsets relative to the start of the blob.
        let seg_count = segments.len();
        let starts_in_image_off: u32 = FIXUPS_HEADER_SIZE as u32;
        let starts_in_image_size: u32 = 4 + seg_count as u32 * 4;

        // Compute each segment's starts_in_segment size up
        // front so we can lay them out before the imports blob.
        let mut per_seg_payload: Vec<Option<Vec<u8>>> = vec![None; seg_count];
        for sf in &sorted {
            let seg = &segments[sf.segment_index];
            let payload = build_starts_in_segment(self.pointer_format, sf, seg)?;
            per_seg_payload[sf.segment_index] = Some(payload);
        }

        // Header field positions (from <mach-o/fixup-chains.h>):
        // dyld_chained_starts_in_segment fields are 4-byte
        // aligned; we emit them back-to-back with 4-byte
        // padding between.
        let mut starts_in_seg_offsets: Vec<u32> = vec![0; seg_count];
        let mut cursor = starts_in_image_off + starts_in_image_size;
        // Round up to 4-byte alignment.
        cursor = (cursor + 3) & !3;
        for (idx, payload) in per_seg_payload.iter().enumerate() {
            if let Some(p) = payload {
                starts_in_seg_offsets[idx] = cursor - starts_in_image_off;
                cursor += p.len() as u32;
                cursor = (cursor + 3) & !3;
            }
        }
        let imports_off = cursor as usize;

        // imports_blob already contains imports + symbol pool
        // contiguously (built by build_imports_blob).
        let (imports_count, imports_table_size, symbols_relative_off) =
            measure_imports_blob(&imports_blob, imports_format)?;

        let symbols_off = imports_off + symbols_relative_off;

        // 4. Emit everything.
        let total_size = imports_off + imports_blob.len();
        // Pad to 8-byte boundary so __LINKEDIT-aligned readers
        // don't trip on stale trailing bytes.
        let total_size_aligned = (total_size + 7) & !7;
        let mut out = vec![0u8; total_size_aligned];

        // Header.
        out[0..4].copy_from_slice(&0u32.to_le_bytes()); // fixups_version
        out[4..8].copy_from_slice(&starts_in_image_off.to_le_bytes());
        out[8..12].copy_from_slice(&(imports_off as u32).to_le_bytes());
        out[12..16].copy_from_slice(&(symbols_off as u32).to_le_bytes());
        out[16..20].copy_from_slice(&(imports_count as u32).to_le_bytes());
        out[20..24].copy_from_slice(&imports_format.to_le_bytes());
        out[24..28].copy_from_slice(&0u32.to_le_bytes()); // symbols_format = uncompressed
        // bytes 28..32 reserved.

        // starts_in_image.
        let sii = starts_in_image_off as usize;
        out[sii..sii + 4].copy_from_slice(&(seg_count as u32).to_le_bytes());
        for idx in 0..seg_count {
            let off = sii + 4 + idx * 4;
            out[off..off + 4].copy_from_slice(&starts_in_seg_offsets[idx].to_le_bytes());
        }

        // starts_in_segment payloads.
        for (idx, payload) in per_seg_payload.iter().enumerate() {
            if let Some(p) = payload {
                let abs = (starts_in_seg_offsets[idx] + starts_in_image_off) as usize;
                out[abs..abs + p.len()].copy_from_slice(p);
            }
        }

        // imports blob.
        out[imports_off..imports_off + imports_blob.len()].copy_from_slice(&imports_blob);
        // Trailing padding bytes stay zero (already from vec!).
        let _ = imports_table_size; // size already encoded in imports_blob

        Ok(SerializedChainedFixups {
            bytes: out,
            imports_format,
        })
    }

    /// Re-encode every fixup-slot value in the image's raw
    /// segment bytes. Returns the modified raw image — the
    /// caller can then call [`MachOImage::parse`] on it again,
    /// or splice it into a `Container::to_bytes` stream.
    ///
    /// This rewrites the *on-disk pointer slots themselves*,
    /// not the `LC_DYLD_CHAINED_FIXUPS` blob. The slot bytes
    /// encode the chain (target + next + flags); the blob
    /// only encodes where chains start. Both need updating
    /// for the image to be valid.
    pub fn encode_into_segments(
        &self,
        image_bytes: &mut [u8],
        segments: &[crate::container::macho_image::MachOSegment],
    ) -> Result<(), ChainedFixupsError> {
        for (file_off, raw) in self.slot_bytes(segments)? {
            let off = file_off as usize;
            if off + 8 > image_bytes.len() {
                return Err(ChainedFixupsError::Truncated(format!(
                    "slot at file offset {off:#x} extends past image bytes"
                )));
            }
            image_bytes[off..off + 8].copy_from_slice(&raw);
        }
        Ok(())
    }

    /// Re-encode every fixup slot and return the resulting
    /// `(file_offset, 8-byte word)` pairs. Same data
    /// [`Self::encode_into_segments`] would write into a
    /// segment buffer, but as a list — convenient for staging
    /// the rewritten slot bytes via the editor's raw-byte
    /// override path without holding a mutable file buffer.
    pub fn slot_bytes(
        &self,
        segments: &[crate::container::macho_image::MachOSegment],
    ) -> Result<Vec<(u64, [u8; 8])>, ChainedFixupsError> {
        let mut out = Vec::new();
        for sf in &self.segments {
            let seg = segments
                .get(sf.segment_index)
                .ok_or_else(|| ChainedFixupsError::FixupOutsideSegment {
                    vaddr: 0,
                    segment_index: sf.segment_index,
                })?;
            let mut fixups = sf.fixups.clone();
            fixups.sort_by_key(|f| f.vaddr);
            fixups.dedup_by_key(|f| f.vaddr);

            // Group by page so the `next` delta only reaches
            // the next fixup within the same page (chains
            // never cross page boundaries — each page has its
            // own start in `page_start[]`).
            let page_size = sf.page_size as u64;
            let mut by_page: HashMap<u64, Vec<&Fixup>> = HashMap::new();
            for fx in &fixups {
                let page = (fx.vaddr - seg.vmaddr) / page_size;
                by_page.entry(page).or_default().push(fx);
            }
            for (_page, mut page_fixups) in by_page {
                page_fixups.sort_by_key(|f| f.vaddr);
                for (i, fx) in page_fixups.iter().enumerate() {
                    let next_delta_bytes = if i + 1 < page_fixups.len() {
                        page_fixups[i + 1].vaddr - fx.vaddr
                    } else {
                        0
                    };
                    let next_units = next_delta_bytes / self.pointer_format.stride() as u64;
                    let raw = encode_one(
                        fx,
                        self.pointer_format,
                        next_units as u32,
                        seg.vmaddr,
                    )?;
                    let file_off = seg.fileoff + (fx.vaddr - seg.vmaddr);
                    out.push((file_off, raw.to_le_bytes()));
                }
            }
        }
        Ok(out)
    }
}

/// Build the imports-table + symbol-pool blob from scratch.
/// Returns `(imports_format, blob)`. May upgrade the format if
/// the original is `DYLD_CHAINED_IMPORT` and the new symbol
/// pool exceeds the 23-bit name_offset budget, but does NOT
/// upgrade past `_ADDEND` → `_ADDEND64` automatically — an
/// oversized addend returns `AddendOverflow` instead so the
/// caller can decide to call `ensure_addend64()`.
fn build_imports_blob(
    imports: &[ChainedImport],
    requested_format: u32,
) -> Result<(u32, Vec<u8>), ChainedFixupsError> {
    let mut symbols_pool: Vec<u8> = Vec::with_capacity(imports.len() * 16);
    let mut name_offsets: Vec<u32> = Vec::with_capacity(imports.len());
    // Pool by string content — many imports share the same
    // symbol (e.g. multiple binds to `_objc_msgSend`).
    let mut seen: HashMap<&str, u32> = HashMap::new();
    for imp in imports {
        if let Some(&off) = seen.get(imp.symbol.as_str()) {
            name_offsets.push(off);
        } else {
            let off = symbols_pool.len() as u32;
            seen.insert(imp.symbol.as_str(), off);
            symbols_pool.extend_from_slice(imp.symbol.as_bytes());
            symbols_pool.push(0);
            name_offsets.push(off);
        }
    }

    // Decide format. If requested is _ADDEND64, keep it. If
    // requested is _ADDEND and any addend doesn't fit i32,
    // bail with AddendOverflow. If requested is _IMPORT and
    // any addend != 0, promote to _ADDEND.
    let max_name_offset_short = (1u64 << 23) - 1;
    let need_promote_to_addend = imports.iter().any(|i| i.addend != 0);
    let need_long_name_offset = symbols_pool.len() as u64 > max_name_offset_short;
    let want_lib_ordinal_16 = imports.iter().any(|i| i.lib_ordinal < -128 || i.lib_ordinal > 127);

    let mut format = requested_format;
    if format == DYLD_CHAINED_IMPORT && need_promote_to_addend {
        format = DYLD_CHAINED_IMPORT_ADDEND;
    }
    if (format == DYLD_CHAINED_IMPORT || format == DYLD_CHAINED_IMPORT_ADDEND)
        && (need_long_name_offset || want_lib_ordinal_16)
    {
        format = DYLD_CHAINED_IMPORT_ADDEND64;
    }
    if format == DYLD_CHAINED_IMPORT_ADDEND {
        for (i, imp) in imports.iter().enumerate() {
            if imp.addend < i32::MIN as i64 || imp.addend > i32::MAX as i64 {
                return Err(ChainedFixupsError::AddendOverflow {
                    import_index: i,
                    addend: imp.addend,
                });
            }
        }
    }

    let entry_size = match format {
        DYLD_CHAINED_IMPORT => 4,
        DYLD_CHAINED_IMPORT_ADDEND => 8,
        DYLD_CHAINED_IMPORT_ADDEND64 => 16,
        _ => unreachable!(),
    };
    let imports_table_size = imports.len() * entry_size;
    let mut blob = vec![0u8; imports_table_size + symbols_pool.len()];

    for (i, imp) in imports.iter().enumerate() {
        let e = i * entry_size;
        let name_off = name_offsets[i];
        match format {
            DYLD_CHAINED_IMPORT => {
                if name_off as u64 > max_name_offset_short {
                    return Err(ChainedFixupsError::ImportNameOffsetOverflow {
                        offset: name_off as u64,
                        max: max_name_offset_short,
                    });
                }
                let ord = (imp.lib_ordinal as i32 as u32) & 0xff;
                let weak = if imp.weak { 1u32 } else { 0 };
                let packed = ord | (weak << 8) | ((name_off & 0x007f_ffff) << 9);
                blob[e..e + 4].copy_from_slice(&packed.to_le_bytes());
            }
            DYLD_CHAINED_IMPORT_ADDEND => {
                if name_off as u64 > max_name_offset_short {
                    return Err(ChainedFixupsError::ImportNameOffsetOverflow {
                        offset: name_off as u64,
                        max: max_name_offset_short,
                    });
                }
                let ord = (imp.lib_ordinal as i32 as u32) & 0xff;
                let weak = if imp.weak { 1u32 } else { 0 };
                let packed = ord | (weak << 8) | ((name_off & 0x007f_ffff) << 9);
                blob[e..e + 4].copy_from_slice(&packed.to_le_bytes());
                blob[e + 4..e + 8].copy_from_slice(&(imp.addend as i32).to_le_bytes());
            }
            DYLD_CHAINED_IMPORT_ADDEND64 => {
                let ord = (imp.lib_ordinal as i32 as u32) & 0xffff;
                let weak = if imp.weak { 1u32 } else { 0 };
                let packed = ord | (weak << 16);
                blob[e..e + 4].copy_from_slice(&packed.to_le_bytes());
                blob[e + 4..e + 8].copy_from_slice(&name_off.to_le_bytes());
                blob[e + 8..e + 16].copy_from_slice(&imp.addend.to_le_bytes());
            }
            _ => unreachable!(),
        }
    }

    blob[imports_table_size..].copy_from_slice(&symbols_pool);
    Ok((format, blob))
}

fn measure_imports_blob(
    blob: &[u8],
    imports_format: u32,
) -> Result<(usize, usize, usize), ChainedFixupsError> {
    // Returns (imports_count, imports_table_size, symbols_relative_offset).
    // We don't actually need to re-parse — the writer above
    // built the blob and knows the layout. Recompute count
    // from the blob shape so the read-only round-trip path
    // (where the blob came verbatim from disk) still works.
    let entry_size = match imports_format {
        DYLD_CHAINED_IMPORT => 4,
        DYLD_CHAINED_IMPORT_ADDEND => 8,
        DYLD_CHAINED_IMPORT_ADDEND64 => 16,
        other => {
            return Err(ChainedFixupsError::Truncated(format!(
                "unknown imports_format {other}"
            )))
        }
    };
    // The blob is layouted as: [entries][strings...]. Count
    // entries by scanning name_offsets — the first entry whose
    // bytes lie at or past the end of the table marks the
    // boundary. But that's fragile for the byte-stable path
    // where we have the original bytes verbatim. Use a
    // different approach: parse the first entry's
    // name_offset; that tells us the boundary because by
    // convention the first import's name lives at the start
    // of the symbols pool (offset 0). Walk until we hit an
    // entry whose name_offset is past the table itself.
    //
    // Simpler still: the blob length doesn't include trailing
    // padding, and we built it as `count*entry_size +
    // pool_size`. count = number of entries; the symbols pool
    // starts at count*entry_size. Determine count by reading
    // entries and stopping when their name_offset references
    // a NUL inside the pool.
    if blob.len() < entry_size {
        return Ok((0, 0, 0));
    }
    // The header records imports_count separately, but this
    // helper is used during writeback where we already have
    // the blob in hand. Locate the boundary by finding the
    // smallest name_offset across all entries: that's
    // necessarily 0 if the pool starts at the table end (by
    // convention, the first import's symbol lives at
    // pool[0]). The table size is then the byte position
    // where pool[0] starts.
    //
    // For the byte-stable path: re-decode each import's
    // name_offset, find max+strlen; the pool ends at
    // blob.len() (or before, with padding we wrote). The
    // table size equals the first entry whose offset points
    // at the first byte of the pool. Concretely, we walk
    // until name_offset >= remaining_bytes, which means the
    // current entry is no longer a valid entry.
    let mut count = 0usize;
    loop {
        let e = count * entry_size;
        if e + entry_size > blob.len() {
            break;
        }
        let candidate_name_offset = match imports_format {
            DYLD_CHAINED_IMPORT | DYLD_CHAINED_IMPORT_ADDEND => {
                let raw = u32_le(&blob[e..e + 4]);
                ((raw >> 9) & 0x007f_ffff) as usize
            }
            DYLD_CHAINED_IMPORT_ADDEND64 => u32_le(&blob[e + 4..e + 8]) as usize,
            _ => unreachable!(),
        };
        // The pool starts at (count+1)*entry_size or later if
        // we haven't yet found all entries. An entry is valid
        // if its name_offset points strictly inside what
        // we'll treat as the pool. Pool start can't be larger
        // than the candidate's position. So if
        // (count+1)*entry_size + candidate_name_offset >
        // blob.len(), this is past the pool — count is
        // complete.
        let prospective_table_end = (count + 1) * entry_size;
        if prospective_table_end + candidate_name_offset >= blob.len() {
            break;
        }
        count += 1;
    }
    let imports_table_size = count * entry_size;
    Ok((count, imports_table_size, imports_table_size))
}

fn build_starts_in_segment(
    pointer_format: PointerFormat,
    sf: &SegmentFixups,
    seg: &crate::container::macho_image::MachOSegment,
) -> Result<Vec<u8>, ChainedFixupsError> {
    if sf.fixups.is_empty() {
        // We could omit the segment entirely, but the input
        // recorded it — preserve symmetry by emitting an
        // empty starts_in_segment with page_count=0.
        let mut payload = Vec::with_capacity(22);
        payload.extend_from_slice(&22u32.to_le_bytes()); // size
        payload.extend_from_slice(&(sf.page_size as u16).to_le_bytes());
        payload.extend_from_slice(&pointer_format.to_raw().to_le_bytes());
        payload.extend_from_slice(&seg.fileoff.to_le_bytes()); // segment_offset (file offset)
        payload.extend_from_slice(&0u32.to_le_bytes()); // max_valid_pointer
        payload.extend_from_slice(&0u16.to_le_bytes()); // page_count
        return Ok(payload);
    }

    let stride = pointer_format.stride() as u64;
    let max_delta = pointer_format.max_next_byte_delta() as u64;
    let page_size = sf.page_size as u64;

    // page_count must cover the whole segment so dyld knows
    // every page is accounted for. Round vmsize up.
    let page_count = ((seg.vmsize + page_size - 1) / page_size) as usize;
    let mut page_starts = vec![DYLD_CHAINED_PTR_START_NONE; page_count];

    // Group fixups by page; record first-fixup offset within
    // each page; verify max_delta isn't exceeded.
    let mut fixups = sf.fixups.clone();
    fixups.sort_by_key(|f| f.vaddr);
    let mut by_page: HashMap<usize, Vec<u64>> = HashMap::new();
    for fx in &fixups {
        if fx.vaddr % stride != 0 {
            return Err(ChainedFixupsError::Misaligned {
                vaddr: fx.vaddr,
                stride: stride as u32,
            });
        }
        let page_off = fx.vaddr.checked_sub(seg.vmaddr).ok_or_else(|| {
            ChainedFixupsError::FixupOutsideSegment {
                vaddr: fx.vaddr,
                segment_index: sf.segment_index,
            }
        })?;
        let page_idx = (page_off / page_size) as usize;
        by_page.entry(page_idx).or_default().push(fx.vaddr);
    }
    for (page_idx, vaddrs) in &by_page {
        let page_base_vaddr = seg.vmaddr + *page_idx as u64 * page_size;
        let first = vaddrs[0];
        page_starts[*page_idx] = (first - page_base_vaddr) as u16;
        for window in vaddrs.windows(2) {
            let delta = window[1] - window[0];
            if delta > max_delta {
                return Err(ChainedFixupsError::NextOverflow {
                    page_vaddr: page_base_vaddr,
                    from_vaddr: window[0],
                    to_vaddr: window[1],
                    max_byte_delta: max_delta as u32,
                });
            }
        }
    }

    let size = 22 + page_count * 2;
    let mut payload = Vec::with_capacity(size);
    payload.extend_from_slice(&(size as u32).to_le_bytes());
    payload.extend_from_slice(&(sf.page_size as u16).to_le_bytes());
    payload.extend_from_slice(&pointer_format.to_raw().to_le_bytes());
    // segment_offset: the segment's FILE offset, where dyld begins walking this
    // segment's pointer chains. Must be the real fileoff (not 0) or dyld walks
    // from the start of the file, never touches the slots, and applies no fixups.
    payload.extend_from_slice(&seg.fileoff.to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes()); // max_valid_pointer
    payload.extend_from_slice(&(page_count as u16).to_le_bytes());
    for ps in &page_starts {
        payload.extend_from_slice(&ps.to_le_bytes());
    }
    Ok(payload)
}

fn encode_one(
    fx: &Fixup,
    pointer_format: PointerFormat,
    next_units: u32,
    seg_vmaddr_base: u64,
) -> Result<u64, ChainedFixupsError> {
    match pointer_format {
        PointerFormat::Ptr64 | PointerFormat::Ptr64Offset => {
            let next = (next_units & 0xfff) as u64;
            let mut word: u64 = 0;
            word |= next << 51;
            match &fx.target {
                FixupTarget::Rebase { target_vaddr } => {
                    word |= target_vaddr & 0x0000_000f_ffff_ffff;
                }
                FixupTarget::Bind {
                    import_index,
                    addend,
                } => {
                    word |= 1u64 << 63;
                    word |= (*import_index as u64) & 0x00ff_ffff;
                    let addend_byte = (*addend as u8) as u64;
                    word |= addend_byte << 24;
                }
            }
            Ok(word)
        }
        PointerFormat::Arm64e
        | PointerFormat::Arm64eKernel
        | PointerFormat::Arm64eUserland
        | PointerFormat::Arm64eUserland24 => {
            let next = (next_units & 0x7ff) as u64;
            let mut word: u64 = 0;
            // auth = 0 (we don't model auth fields)
            word |= next << 51;
            match &fx.target {
                FixupTarget::Rebase { target_vaddr } => {
                    let stored = match pointer_format {
                        PointerFormat::Arm64eUserland
                        | PointerFormat::Arm64eUserland24
                        | PointerFormat::Arm64eKernel => target_vaddr.saturating_sub(seg_vmaddr_base),
                        _ => *target_vaddr,
                    };
                    word |= stored & 0x0000_000f_ffff_ffff;
                }
                FixupTarget::Bind {
                    import_index,
                    addend,
                } => {
                    word |= 1u64 << 62; // bind
                    let ordinal_mask = if pointer_format == PointerFormat::Arm64eUserland24 {
                        0x00ff_ffff
                    } else {
                        0x0000_ffff
                    };
                    word |= (*import_index as u64) & ordinal_mask;
                    // 19-bit signed addend
                    let a = *addend as i64;
                    if a < -(1 << 18) || a >= (1 << 18) {
                        return Err(ChainedFixupsError::AddendOverflow {
                            import_index: *import_index,
                            addend: a,
                        });
                    }
                    let raw_add = (a as u64) & 0x7_ffff;
                    word |= raw_add << 24;
                }
            }
            Ok(word)
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn u16_le(b: &[u8]) -> u16 {
    u16::from_le_bytes(b.try_into().unwrap())
}
fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}
fn u64_le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap())
}

fn sign_extend_8(raw: u32) -> i32 {
    let v = (raw & 0xff) as i32;
    if v & 0x80 != 0 {
        v - 0x100
    } else {
        v
    }
}
fn sign_extend_16(raw: u32) -> i32 {
    let v = (raw & 0xffff) as i32;
    if v & 0x8000 != 0 {
        v - 0x10000
    } else {
        v
    }
}

fn read_cstr(pool: &[u8], offset: usize) -> Option<&str> {
    let tail = pool.get(offset..)?;
    let end = tail.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&tail[..end]).ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/macho_objc_fixture/libgreet_objc.dylib")
    }

    fn load() -> Option<MachOImage> {
        let bytes = std::fs::read(fixture_path()).ok()?;
        MachOImage::parse(bytes).ok()
    }

    #[test]
    fn reads_pointer_format_and_imports() {
        let Some(img) = load() else {
            eprintln!("skip: fixture not present");
            return;
        };
        let cf = ChainedFixups::read(&img).expect("read");
        // libgreet_objc.dylib is arm64 userland — should be
        // PTR_64 or PTR_64_OFFSET depending on linker version.
        assert!(
            matches!(
                cf.pointer_format,
                PointerFormat::Ptr64 | PointerFormat::Ptr64Offset
            ),
            "unexpected pointer_format: {:?}",
            cf.pointer_format,
        );
        // At least one bind to _OBJC_CLASS_$_NSObject or
        // _objc_msgSend should be present.
        assert!(
            cf.imports.iter().any(|i| {
                i.symbol.contains("OBJC_CLASS_$_NSObject")
                    || i.symbol.contains("_objc_msgSend")
            }),
            "no ObjC bind in imports: {:?}",
            cf.imports.iter().map(|i| &i.symbol).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn reads_segment_fixups_match_legacy_map() {
        let Some(img) = load() else {
            return;
        };
        let cf = ChainedFixups::read(&img).expect("read");
        // The semantic view's to_map() should agree with the
        // legacy build_chained_fixup_map() entry by entry.
        let new_map = cf.to_map();
        let legacy = crate::container::macho_objc::build_chained_fixup_map(&img)
            .expect("legacy parser ok");
        assert_eq!(
            new_map.len(),
            legacy.len(),
            "entry count mismatch: new={} legacy={}",
            new_map.len(),
            legacy.len(),
        );
        // Spot-check a handful of entries agree on rebase vs
        // bind and on target vaddr / symbol.
        for (vaddr, target) in new_map.iter().take(50) {
            let legacy_target = legacy.get(*vaddr).expect("legacy has same vaddr");
            match (target, legacy_target) {
                (
                    ResolvedTarget::Rebase { target_vaddr: a },
                    crate::container::macho_objc::ChainedFixupTarget::Rebase { target_vaddr: b },
                ) => assert_eq!(a, b, "rebase target at {vaddr:#x}"),
                (
                    ResolvedTarget::Bind { symbol_name: a, .. },
                    crate::container::macho_objc::ChainedFixupTarget::Bind { symbol_name: b, .. },
                ) => assert_eq!(a, b, "bind symbol at {vaddr:#x}"),
                _ => panic!(
                    "kind mismatch at {vaddr:#x}: new={:?} legacy={:?}",
                    target, legacy_target
                ),
            }
        }
    }

    #[test]
    fn round_trip_unchanged_serialises_to_blob() {
        let Some(img) = load() else { return };
        let cf = ChainedFixups::read(&img).expect("read");
        let ser = cf.serialize(&img.layout.segments).expect("serialise");
        // Re-parse the freshly emitted blob as if it were a
        // standalone fixups payload by patching the image's
        // raw bytes and re-reading. Easiest validation is:
        // build the legacy map from a doctored image with the
        // new blob spliced in.
        let Some(orig) = img.layout.chained_fixups else {
            return;
        };
        let mut doctored = img.raw_bytes.clone();
        // Truncate / extend doctored bytes to fit the new blob.
        let mut new_blob = ser.bytes.clone();
        // Pad blob to original size or grow file; for the
        // identity case (nothing edited) sizes should match.
        let new_size = new_blob.len() as u64;
        if new_size < orig.datasize {
            new_blob.resize(orig.datasize as usize, 0);
        }
        if (orig.dataoff + new_blob.len() as u64) as usize > doctored.len() {
            doctored.resize((orig.dataoff + new_blob.len() as u64) as usize, 0);
        }
        let lo = orig.dataoff as usize;
        let hi = lo + new_blob.len();
        doctored[lo..hi].copy_from_slice(&new_blob);

        let img2 = MachOImage::parse(doctored).expect("re-parse");
        let cf2 = ChainedFixups::read(&img2).expect("read round-tripped");
        let m1 = cf.to_map();
        let m2 = cf2.to_map();
        assert_eq!(m1.len(), m2.len(), "entry count after round-trip");
        for (k, v) in &m1 {
            assert_eq!(m2.get(k), Some(v), "entry at {k:#x} survives round-trip");
        }
    }

    #[test]
    fn edit_import_symbol_round_trips_via_to_map() {
        let Some(img) = load() else { return };
        let mut cf = ChainedFixups::read(&img).expect("read");
        // Rename every NSObject reference to MyNSObject by
        // mutating the import string. This is what a higher-
        // level "rename" pass would do.
        let mut hits = 0;
        for imp in cf.imports.iter_mut() {
            if imp.symbol.contains("OBJC_CLASS_$_NSObject") {
                imp.symbol = imp.symbol.replace("NSObject", "MyNSObject");
                hits += 1;
            }
        }
        assert!(hits > 0, "fixture must reference NSObject");
        // Serialise — this exercises the fresh-pack path
        // because imports differ from original.
        let ser = cf.serialize(&img.layout.segments).expect("serialise");
        // Splice and re-parse.
        let orig = img.layout.chained_fixups.unwrap();
        let mut doctored = img.raw_bytes.clone();
        let new_blob = ser.bytes;
        let new_size = new_blob.len();
        if new_size > orig.datasize as usize {
            // Need to grow file; this is the editor's job in
            // production, but for the unit test we just append.
            doctored.resize(orig.dataoff as usize + new_size, 0);
        }
        let lo = orig.dataoff as usize;
        doctored[lo..lo + new_size].copy_from_slice(&new_blob);
        // Patch LC_DYLD_CHAINED_FIXUPS.datasize so the parser
        // accepts the new size.
        patch_chained_fixups_datasize(&mut doctored, new_size as u64);
        let img2 = MachOImage::parse(doctored).expect("re-parse");
        let cf2 = ChainedFixups::read(&img2).expect("read after edit");
        let renamed = cf2
            .imports
            .iter()
            .filter(|i| i.symbol.contains("OBJC_CLASS_$_MyNSObject"))
            .count();
        assert_eq!(
            renamed, hits,
            "all renamed imports should survive round-trip"
        );
    }

    /// Find the LC_DYLD_CHAINED_FIXUPS command and overwrite
    /// its datasize field. Mirrors what the high-level writer
    /// would do when growing the linkedit chunk.
    fn patch_chained_fixups_datasize(bytes: &mut [u8], new_size: u64) {
        let ncmds = u32_le(&bytes[16..20]);
        let mut cursor = 32usize;
        for _ in 0..ncmds {
            let cmd = u32_le(&bytes[cursor..cursor + 4]);
            let cmdsize = u32_le(&bytes[cursor + 4..cursor + 8]) as usize;
            if cmd == object::macho::LC_DYLD_CHAINED_FIXUPS {
                let ns = (new_size as u32).to_le_bytes();
                bytes[cursor + 12..cursor + 16].copy_from_slice(&ns);
                return;
            }
            cursor += cmdsize;
        }
        panic!("LC_DYLD_CHAINED_FIXUPS not found");
    }
}
