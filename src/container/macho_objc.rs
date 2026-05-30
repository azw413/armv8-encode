//! Objective-C metadata reader for 64-bit Mach-O images.
//!
//! Walks `__objc_classlist`, `__objc_catlist`, and
//! `__objc_protolist` (plus the `__objc_const` / `__objc_data`
//! / `__objc_methname` / `__objc_methtype` sections they
//! reference) and returns an owned [`ObjCMetadata`] snapshot:
//! classes with their methods / ivars / properties / protocol
//! lists, categories that extend an existing class with extra
//! methods or protocols, and the protocols themselves.
//!
//! Scope of v1:
//!
//! * 64-bit Mach-O (`MH_MAGIC_64`) only.
//! * Modern pointer encoding — `LC_DYLD_CHAINED_FIXUPS`.
//!   Pre-Big-Sur images that still use `LC_DYLD_INFO_ONLY`
//!   rebase opcodes are not supported yet; the walker will
//!   refuse them with [`ObjCReadError::ChainedFixupsMissing`]
//!   so a caller can fall back rather than mis-interpret raw
//!   pointer fields.
//! * Both method-list encodings (legacy absolute pointers and
//!   iOS 14+/macOS 11+ relative `i32` offsets, signalled by
//!   bit 0x80000000 of `entsize`).
//! * Reads only — no mutation API. Renaming, injection, and
//!   the corresponding fixup-list rewrites are deferred.
//!
//! Swift metadata (`__swift5_types`, `__swift5_proto`, …) is
//! intentionally out of scope: Swift classes appear here only
//! to the extent their backing ObjC `class_ro_t` is present.

use std::collections::HashMap;

use crate::container::macho_image::MachOImage;
use crate::container::ContainerWriteError;

/// Owned snapshot of an image's Objective-C metadata, built
/// once via [`read_objc_metadata`] and then read freely.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ObjCMetadata {
    pub classes: Vec<ObjCClass>,
    pub categories: Vec<ObjCCategory>,
    pub protocols: Vec<ObjCProtocol>,
    /// `__objc_imageinfo` flags + version. `None` if the
    /// section is absent.
    pub image_info: Option<ObjCImageInfo>,
}

/// `objc_image_info`: version word + flags word. The runtime
/// inspects `flags` for the Swift ABI version, "has category
/// class properties," "optimized by dyld," etc.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ObjCImageInfo {
    pub version: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObjCClass {
    /// `class_ro_t.name`.
    pub name: String,
    /// vmaddr of the `objc_class` struct in `__objc_data`.
    pub vaddr: u64,
    /// vmaddr of the metaclass's `objc_class` struct, or
    /// `None` if the metaclass pointer was zero / unresolved
    /// (typically: external metaclass we don't have a binding
    /// name for).
    pub metaclass_vaddr: Option<u64>,
    /// vmaddr of the superclass `objc_class`, or `None` for
    /// root classes / external superclasses we don't resolve.
    pub superclass_vaddr: Option<u64>,
    /// Name of the external superclass when the pointer was a
    /// bind to another image (e.g. `"NSObject"`). Resolved via
    /// the chained-fixups import table.
    pub superclass_name: Option<String>,
    /// `class_ro_t.flags` (RO_META, RO_ROOT, etc.).
    pub flags: u32,
    pub instance_start: u32,
    pub instance_size: u32,
    pub instance_methods: Vec<ObjCMethod>,
    pub class_methods: Vec<ObjCMethod>,
    pub ivars: Vec<ObjCIvar>,
    pub properties: Vec<ObjCProperty>,
    pub adopted_protocols: Vec<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObjCCategory {
    pub name: String,
    pub vaddr: u64,
    /// vmaddr of the class this category extends, when the
    /// class pointer resolves to an in-image class.
    pub class_vaddr: Option<u64>,
    /// Name of the extended class when the class pointer was
    /// a bind to another image.
    pub class_name: Option<String>,
    pub instance_methods: Vec<ObjCMethod>,
    pub class_methods: Vec<ObjCMethod>,
    pub protocols: Vec<u64>,
    pub instance_properties: Vec<ObjCProperty>,
    pub class_properties: Vec<ObjCProperty>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObjCProtocol {
    pub name: String,
    pub vaddr: u64,
    pub instance_methods: Vec<ObjCMethod>,
    pub class_methods: Vec<ObjCMethod>,
    pub optional_instance_methods: Vec<ObjCMethod>,
    pub optional_class_methods: Vec<ObjCMethod>,
    pub instance_properties: Vec<ObjCProperty>,
    pub adopted_protocols: Vec<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObjCMethod {
    /// Selector, e.g. `"initWithFrame:"`.
    pub name: String,
    /// Type encoding string, e.g. `"@16@0:8"`.
    pub types: String,
    /// vmaddr of the implementing function, or 0 / `None`
    /// for protocol methods which carry no IMP.
    pub imp: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObjCIvar {
    pub name: String,
    pub types: String,
    /// vmaddr of the `*offset` slot the runtime fixes up.
    pub offset_ptr_vaddr: u64,
    /// The offset value read from `*offset` at parse time.
    pub offset: u32,
    pub alignment: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObjCProperty {
    pub name: String,
    pub attributes: String,
}

#[derive(Debug)]
pub enum ObjCReadError {
    /// Input isn't a 64-bit Mach-O / has no Mach-O image
    /// attached. Caller should check `Container::macho_image`
    /// before calling.
    NotMachO,
    /// No `LC_DYLD_CHAINED_FIXUPS` present. v1 only supports
    /// the modern fixup format; legacy `LC_DYLD_INFO_ONLY`
    /// images fall here.
    ChainedFixupsMissing,
    /// Some structural field (an `entsize`, a list count,
    /// a pointer offset) doesn't fit in the file. Bundles the
    /// offending byte range for diagnostics.
    Truncated(String),
    /// A vmaddr pointer in a metadata struct didn't resolve to
    /// any in-image section AND wasn't a bind. Usually a sign
    /// the fixup chain walker missed the entry — bug, not a
    /// real malformed-binary case.
    UnresolvedPointer { at_file_offset: u64, raw: u64 },
}

impl std::fmt::Display for ObjCReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMachO => f.write_str("ObjC reader: input is not a 64-bit Mach-O"),
            Self::ChainedFixupsMissing => f.write_str(
                "ObjC reader: image has no LC_DYLD_CHAINED_FIXUPS (legacy rebase opcodes not yet supported)",
            ),
            Self::Truncated(m) => write!(f, "ObjC reader: truncated: {m}"),
            Self::UnresolvedPointer { at_file_offset, raw } => write!(
                f,
                "ObjC reader: unresolved pointer at file offset {at_file_offset:#x} (raw {raw:#018x})",
            ),
        }
    }
}

impl std::error::Error for ObjCReadError {}

impl From<ObjCReadError> for ContainerWriteError {
    fn from(e: ObjCReadError) -> Self {
        ContainerWriteError::ObjectWrite(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Chained-fixups parser
// ---------------------------------------------------------------------------

/// What a single chained-fixup entry resolves to. Either a
/// rebase (in-image target vmaddr) or a bind (named import).
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ChainedFixupTarget {
    /// Rebase: the on-disk pointer slot should be read as
    /// `target_vaddr` (already relative to the preferred load
    /// address — same value the runtime sees post-rebase).
    Rebase { target_vaddr: u64 },
    /// Bind: the pointer slot will be filled with the address
    /// of `symbol_name` from `dylib_ordinal` at load time.
    Bind {
        dylib_ordinal: i32,
        symbol_name: String,
        addend: i64,
    },
}

/// Map from a fixup-slot's *vaddr* (not file offset, since
/// callers always look up by vaddr) to the resolved target.
/// Holds every slot dyld will rewrite when the image is mapped.
#[derive(Debug, Clone, Default)]
pub struct ChainedFixupMap {
    entries: HashMap<u64, ChainedFixupTarget>,
}

impl ChainedFixupMap {
    pub fn get(&self, vaddr: u64) -> Option<&ChainedFixupTarget> {
        self.entries.get(&vaddr)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `dyld_chained_fixups_header` (libdyld `mach-o/fixup-chains.h`).
const FIXUPS_HEADER_SIZE: usize = 32;

/// Pointer-format constants we handle. ARM64E / firmware
/// variants are recognised but treated as opaque (we still
/// walk the chain, we just decode targets via the common
/// arm64/x86_64 paths).
const DYLD_CHAINED_PTR_ARM64E: u16 = 1;
const DYLD_CHAINED_PTR_64: u16 = 2;
const DYLD_CHAINED_PTR_32: u16 = 3;
const DYLD_CHAINED_PTR_64_OFFSET: u16 = 6;
const DYLD_CHAINED_PTR_ARM64E_KERNEL: u16 = 7;
const DYLD_CHAINED_PTR_ARM64E_USERLAND: u16 = 9;
const DYLD_CHAINED_PTR_ARM64E_FIRMWARE: u16 = 10;
const DYLD_CHAINED_PTR_64_KERNEL_CACHE: u16 = 8;
const DYLD_CHAINED_PTR_ARM64E_USERLAND24: u16 = 12;

const DYLD_CHAINED_IMPORT: u32 = 1;
const DYLD_CHAINED_IMPORT_ADDEND: u32 = 2;
const DYLD_CHAINED_IMPORT_ADDEND64: u32 = 3;

/// Parse `LC_DYLD_CHAINED_FIXUPS` and walk every chain to
/// build a vaddr → target map. Returns `Ok(default)` (i.e.
/// empty) if the image has no chained-fixups load command,
/// since the read paths treat "no fixups" as "all pointer
/// fields are raw vmaddrs."
pub fn build_chained_fixup_map(image: &MachOImage) -> Result<ChainedFixupMap, ObjCReadError> {
    let Some(fixups) = image.layout.chained_fixups else {
        return Err(ObjCReadError::ChainedFixupsMissing);
    };
    let bytes = &image.raw_bytes;
    let base = fixups.dataoff as usize;
    let end = base + fixups.datasize as usize;
    if end > bytes.len() || fixups.datasize < FIXUPS_HEADER_SIZE as u64 {
        return Err(ObjCReadError::Truncated(format!(
            "LC_DYLD_CHAINED_FIXUPS extends past file ({}..{} of {})",
            base,
            end,
            bytes.len(),
        )));
    }
    let header = &bytes[base..base + FIXUPS_HEADER_SIZE];
    let fixups_version = u32_le(&header[0..4]);
    if fixups_version != 0 {
        return Err(ObjCReadError::Truncated(format!(
            "LC_DYLD_CHAINED_FIXUPS unsupported fixups_version {fixups_version}"
        )));
    }
    let starts_offset = u32_le(&header[4..8]) as usize;
    let imports_offset = u32_le(&header[8..12]) as usize;
    let symbols_offset = u32_le(&header[12..16]) as usize;
    let imports_count = u32_le(&header[16..20]) as usize;
    let imports_format = u32_le(&header[20..24]);
    let _symbols_format = u32_le(&header[24..28]); // 0 = uncompressed

    // Decode imports — a flat array of import descriptors, each
    // referencing a NUL-terminated symbol in the strings pool
    // that follows.
    let imports = decode_imports(
        bytes,
        base,
        imports_offset,
        symbols_offset,
        imports_count,
        imports_format,
        end,
    )?;

    // Walk per-segment starts.
    let starts_in_image_abs = base + starts_offset;
    if starts_in_image_abs + 4 > end {
        return Err(ObjCReadError::Truncated(
            "starts_in_image header past file end".into(),
        ));
    }
    let seg_count = u32_le(&bytes[starts_in_image_abs..starts_in_image_abs + 4]) as usize;
    let seg_offsets_base = starts_in_image_abs + 4;
    if seg_offsets_base + seg_count * 4 > end {
        return Err(ObjCReadError::Truncated(
            "starts_in_image seg_info_offset[] past file end".into(),
        ));
    }
    let mut map = HashMap::new();
    for seg_idx in 0..seg_count {
        let off_field =
            u32_le(&bytes[seg_offsets_base + seg_idx * 4..seg_offsets_base + seg_idx * 4 + 4])
                as usize;
        if off_field == 0 {
            // Segment has no fixup chains (e.g. __PAGEZERO).
            continue;
        }
        let seg_info_abs = starts_in_image_abs + off_field;
        walk_segment_chains(
            bytes,
            seg_info_abs,
            seg_idx,
            end,
            &image.layout.segments,
            &imports,
            &mut map,
        )?;
    }
    Ok(ChainedFixupMap { entries: map })
}

#[derive(Debug, Clone)]
struct DecodedImport {
    lib_ordinal: i32,
    symbol: String,
    addend: i64,
}

fn decode_imports(
    bytes: &[u8],
    fixups_base: usize,
    imports_offset: usize,
    symbols_offset: usize,
    imports_count: usize,
    imports_format: u32,
    fixups_end: usize,
) -> Result<Vec<DecodedImport>, ObjCReadError> {
    let entry_size = match imports_format {
        DYLD_CHAINED_IMPORT => 4,
        DYLD_CHAINED_IMPORT_ADDEND => 8,
        DYLD_CHAINED_IMPORT_ADDEND64 => 16,
        other => {
            return Err(ObjCReadError::Truncated(format!(
                "unknown chained-fixups imports_format {other}"
            )))
        }
    };
    let imports_abs = fixups_base + imports_offset;
    let imports_end_abs = imports_abs + imports_count * entry_size;
    let symbols_abs = fixups_base + symbols_offset;
    if imports_end_abs > fixups_end || symbols_abs > fixups_end {
        return Err(ObjCReadError::Truncated("imports/strings past file end".into()));
    }
    let symbols_pool = &bytes[symbols_abs..fixups_end];

    let mut out = Vec::with_capacity(imports_count);
    for i in 0..imports_count {
        let e = imports_abs + i * entry_size;
        let (lib_ordinal, name_offset, addend) = match imports_format {
            DYLD_CHAINED_IMPORT => {
                // bit-packed: lib_ordinal:8, weak_import:1, name_offset:23
                let raw = u32_le(&bytes[e..e + 4]);
                let lib_ordinal = sign_extend_8(raw & 0xff);
                let name_offset = (raw >> 9) & 0x007f_ffff;
                (lib_ordinal, name_offset as usize, 0i64)
            }
            DYLD_CHAINED_IMPORT_ADDEND => {
                // lib_ordinal:8, weak:1, name_offset:23, addend:i32
                let raw = u32_le(&bytes[e..e + 4]);
                let lib_ordinal = sign_extend_8(raw & 0xff);
                let name_offset = (raw >> 9) & 0x007f_ffff;
                let addend = i32::from_le_bytes(bytes[e + 4..e + 8].try_into().unwrap()) as i64;
                (lib_ordinal, name_offset as usize, addend)
            }
            DYLD_CHAINED_IMPORT_ADDEND64 => {
                // lib_ordinal:16, weak:1, reserved:15, name_offset:u32, addend:u64
                let raw = u32_le(&bytes[e..e + 4]);
                let lib_ordinal = sign_extend_16(raw & 0xffff);
                let name_offset = u32_le(&bytes[e + 4..e + 8]) as usize;
                let addend = u64::from_le_bytes(bytes[e + 8..e + 16].try_into().unwrap()) as i64;
                (lib_ordinal, name_offset, addend)
            }
            _ => unreachable!(),
        };
        let symbol = read_cstr(symbols_pool, name_offset)
            .ok_or_else(|| ObjCReadError::Truncated("import string past end of pool".into()))?
            .to_string();
        out.push(DecodedImport {
            lib_ordinal,
            symbol,
            addend,
        });
    }
    Ok(out)
}

fn walk_segment_chains(
    bytes: &[u8],
    seg_info_abs: usize,
    seg_idx: usize,
    fixups_end: usize,
    segments: &[crate::container::macho_image::MachOSegment],
    imports: &[DecodedImport],
    out: &mut HashMap<u64, ChainedFixupTarget>,
) -> Result<(), ObjCReadError> {
    // dyld_chained_starts_in_segment: u32 size, u16 page_size,
    // u16 pointer_format, u64 segment_offset, u32 max_valid_pointer,
    // u16 page_count, u16 page_start[page_count].
    if seg_info_abs + 22 > fixups_end {
        return Err(ObjCReadError::Truncated(
            "starts_in_segment header past file end".into(),
        ));
    }
    let _size = u32_le(&bytes[seg_info_abs..seg_info_abs + 4]);
    let page_size = u16_le(&bytes[seg_info_abs + 4..seg_info_abs + 6]) as u64;
    let pointer_format = u16_le(&bytes[seg_info_abs + 6..seg_info_abs + 8]);
    let _segment_offset = u64_le(&bytes[seg_info_abs + 8..seg_info_abs + 16]);
    let _max_valid_pointer = u32_le(&bytes[seg_info_abs + 16..seg_info_abs + 20]);
    let page_count = u16_le(&bytes[seg_info_abs + 20..seg_info_abs + 22]) as usize;
    let page_start_base = seg_info_abs + 22;
    if page_start_base + page_count * 2 > fixups_end {
        return Err(ObjCReadError::Truncated(
            "starts_in_segment page_start[] past file end".into(),
        ));
    }
    let seg = segments
        .get(seg_idx)
        .ok_or_else(|| ObjCReadError::Truncated(format!("seg index {seg_idx} out of range")))?;
    let stride = pointer_stride(pointer_format)?;
    const DYLD_CHAINED_PTR_START_NONE: u16 = 0xFFFF;

    let seg_end_file_off = seg.fileoff + seg.filesize;
    for page_idx in 0..page_count {
        let page_start =
            u16_le(&bytes[page_start_base + page_idx * 2..page_start_base + page_idx * 2 + 2]);
        if page_start == DYLD_CHAINED_PTR_START_NONE {
            continue;
        }
        // segment_offset is the segment's start offset relative
        // to the *first* segment that has chained fixups, NOT
        // the segment's own fileoff. Pages walk relative to the
        // segment's vmaddr base.
        let page_off_in_seg = page_idx as u64 * page_size + page_start as u64;
        let mut chain_off_in_seg = page_off_in_seg;
        loop {
            let chain_vaddr = seg.vmaddr + chain_off_in_seg;
            let chain_file_off = seg.fileoff + chain_off_in_seg;
            // A malformed `next` (or our misdecode of it) can
            // walk past the segment. Stop the chain at the
            // segment boundary rather than failing the whole
            // parse — the entries we already collected are still
            // valid, and downstream lookups will simply miss
            // anything past this point.
            if chain_file_off + 8 > seg_end_file_off
                || chain_file_off as usize + 8 > bytes.len()
            {
                break;
            }
            let raw = u64_le(&bytes[chain_file_off as usize..chain_file_off as usize + 8]);
            let decoded = decode_chained_pointer(raw, pointer_format, imports, seg.vmaddr)?;
            out.insert(chain_vaddr, decoded.target);
            if decoded.next == 0 {
                break;
            }
            chain_off_in_seg += decoded.next as u64 * stride;
        }
    }
    Ok(())
}

fn pointer_stride(pointer_format: u16) -> Result<u64, ObjCReadError> {
    match pointer_format {
        DYLD_CHAINED_PTR_ARM64E
        | DYLD_CHAINED_PTR_ARM64E_KERNEL
        | DYLD_CHAINED_PTR_ARM64E_USERLAND
        | DYLD_CHAINED_PTR_ARM64E_FIRMWARE
        | DYLD_CHAINED_PTR_ARM64E_USERLAND24 => Ok(8),
        DYLD_CHAINED_PTR_64 | DYLD_CHAINED_PTR_64_OFFSET | DYLD_CHAINED_PTR_64_KERNEL_CACHE => {
            Ok(4)
        }
        DYLD_CHAINED_PTR_32 => Ok(4),
        other => Err(ObjCReadError::Truncated(format!(
            "unsupported pointer_format {other}"
        ))),
    }
}

struct DecodedChainStep {
    target: ChainedFixupTarget,
    /// `next` field from the chain entry. 0 terminates the chain.
    next: u32,
}

fn decode_chained_pointer(
    raw: u64,
    pointer_format: u16,
    imports: &[DecodedImport],
    seg_vmaddr_base: u64,
) -> Result<DecodedChainStep, ObjCReadError> {
    // We support two main encodings here: arm64e (auth + non-auth)
    // and the plain DYLD_CHAINED_PTR_64* family. Both pack target/
    // ordinal + next + a couple flag bits into a 64-bit word.
    match pointer_format {
        DYLD_CHAINED_PTR_64 | DYLD_CHAINED_PTR_64_OFFSET => {
            let bind = (raw >> 63) & 1 != 0;
            let next = ((raw >> 51) & 0xfff) as u32;
            if bind {
                let ordinal = (raw & 0x00ff_ffff) as usize;
                let addend = ((raw >> 24) & 0xff) as i64;
                let imp = imports.get(ordinal).ok_or_else(|| {
                    ObjCReadError::Truncated(format!("bind ordinal {ordinal} out of range"))
                })?;
                Ok(DecodedChainStep {
                    target: ChainedFixupTarget::Bind {
                        dylib_ordinal: imp.lib_ordinal,
                        symbol_name: imp.symbol.clone(),
                        addend: addend + imp.addend,
                    },
                    next,
                })
            } else {
                // Rebase. target is a 36-bit unsigned vmaddr (PTR_64
                // is absolute) or unsigned file offset (PTR_64_OFFSET
                // is relative to the load address, which on disk we
                // treat as identical to a vmaddr — both end up as
                // "the slot will be filled with this address").
                let target_vaddr = raw & 0x0000_000f_ffff_ffff;
                Ok(DecodedChainStep {
                    target: ChainedFixupTarget::Rebase { target_vaddr },
                    next,
                })
            }
        }
        DYLD_CHAINED_PTR_ARM64E
        | DYLD_CHAINED_PTR_ARM64E_USERLAND
        | DYLD_CHAINED_PTR_ARM64E_USERLAND24
        | DYLD_CHAINED_PTR_ARM64E_KERNEL
        | DYLD_CHAINED_PTR_ARM64E_FIRMWARE => {
            let auth = (raw >> 63) & 1 != 0;
            let bind = (raw >> 62) & 1 != 0;
            let next = ((raw >> 51) & 0x7ff) as u32;
            if bind {
                let ordinal_bits = if pointer_format == DYLD_CHAINED_PTR_ARM64E_USERLAND24 {
                    0x00ff_ffff
                } else {
                    0x0000_ffff
                };
                let ordinal = (raw & ordinal_bits) as usize;
                let imp = imports.get(ordinal).ok_or_else(|| {
                    ObjCReadError::Truncated(format!(
                        "arm64e bind ordinal {ordinal} out of range"
                    ))
                })?;
                let addend = if auth {
                    0
                } else {
                    // 19-bit signed addend in non-auth bind.
                    let raw_add = ((raw >> 24) & 0x7_ffff) as i64;
                    if raw_add & (1 << 18) != 0 {
                        raw_add - (1 << 19)
                    } else {
                        raw_add
                    }
                };
                Ok(DecodedChainStep {
                    target: ChainedFixupTarget::Bind {
                        dylib_ordinal: imp.lib_ordinal,
                        symbol_name: imp.symbol.clone(),
                        addend: addend + imp.addend,
                    },
                    next,
                })
            } else {
                // arm64e rebase: target is 36-bit relative to
                // the segment base (kernel) or absolute vmaddr
                // (userland). We always recover an absolute
                // vmaddr by adding the segment base for the
                // _OFFSET-style formats and leaving the value
                // alone for absolute formats.
                let target_low = raw & 0x0000_000f_ffff_ffff;
                let target_vaddr = match pointer_format {
                    DYLD_CHAINED_PTR_ARM64E_USERLAND
                    | DYLD_CHAINED_PTR_ARM64E_USERLAND24
                    | DYLD_CHAINED_PTR_ARM64E_KERNEL => seg_vmaddr_base + target_low,
                    _ => target_low,
                };
                Ok(DecodedChainStep {
                    target: ChainedFixupTarget::Rebase { target_vaddr },
                    next,
                })
            }
        }
        other => Err(ObjCReadError::Truncated(format!(
            "pointer_format {other} not yet decoded"
        ))),
    }
}

// ---------------------------------------------------------------------------
// ObjC metadata walker
// ---------------------------------------------------------------------------

/// Top-level entry point. Build an [`ObjCMetadata`] snapshot
/// from `image`'s `__objc_*` sections.
pub fn read_objc_metadata(image: &MachOImage) -> Result<ObjCMetadata, ObjCReadError> {
    let fixups = build_chained_fixup_map(image)?;
    let ctx = WalkCtx {
        image,
        fixups: &fixups,
    };

    let classes = ctx
        .read_pointer_section("__DATA", "__objc_classlist")
        .or_else(|| ctx.read_pointer_section("__DATA_CONST", "__objc_classlist"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| ctx.read_class(p).transpose())
        .collect::<Result<Vec<_>, _>>()?;

    let categories = ctx
        .read_pointer_section("__DATA", "__objc_catlist")
        .or_else(|| ctx.read_pointer_section("__DATA_CONST", "__objc_catlist"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| ctx.read_category(p).transpose())
        .collect::<Result<Vec<_>, _>>()?;

    let protocols = ctx
        .read_pointer_section("__DATA", "__objc_protolist")
        .or_else(|| ctx.read_pointer_section("__DATA_CONST", "__objc_protolist"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| ctx.read_protocol(p).transpose())
        .collect::<Result<Vec<_>, _>>()?;

    let image_info = ctx.read_image_info();

    Ok(ObjCMetadata {
        classes,
        categories,
        protocols,
        image_info,
    })
}

struct WalkCtx<'a> {
    image: &'a MachOImage,
    fixups: &'a ChainedFixupMap,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum ResolvedPointer {
    /// In-image rebase — the field holds a vmaddr we can follow.
    Vaddr(u64),
    /// Bind — the field will be filled by dyld with `symbol`'s
    /// address. We can't follow it but we can report the name.
    External(String),
    /// Slot was zero (or unresolved).
    Null,
}

impl<'a> WalkCtx<'a> {
    fn read_pointer_section(&self, segname: &str, sectname: &str) -> Option<Vec<u64>> {
        let sect = self.image.layout.section(segname, sectname)?;
        if sect.size == 0 || sect.file_offset == 0 {
            return Some(Vec::new());
        }
        let n = (sect.size / 8) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let slot_vaddr = sect.vaddr + (i as u64) * 8;
            match self.resolve_pointer(slot_vaddr) {
                Some(ResolvedPointer::Vaddr(v)) => out.push(v),
                _ => {}
            }
        }
        Some(out)
    }

    fn resolve_pointer(&self, slot_vaddr: u64) -> Option<ResolvedPointer> {
        // First consult the fixup map — modern arm64 binaries
        // store packed chain descriptors on disk, not raw pointers.
        if let Some(target) = self.fixups.get(slot_vaddr) {
            return Some(match target {
                ChainedFixupTarget::Rebase { target_vaddr } => {
                    if *target_vaddr == 0 {
                        ResolvedPointer::Null
                    } else {
                        ResolvedPointer::Vaddr(*target_vaddr)
                    }
                }
                ChainedFixupTarget::Bind { symbol_name, .. } => {
                    ResolvedPointer::External(symbol_name.clone())
                }
            });
        }
        // No fixup entry — read the raw 8 bytes (this covers
        // static binaries / .o files without LC_DYLD_*).
        let off = self.image.layout.file_offset_for_vaddr(slot_vaddr)?;
        let off = off as usize;
        if off + 8 > self.image.raw_bytes.len() {
            return None;
        }
        let raw = u64_le(&self.image.raw_bytes[off..off + 8]);
        if raw == 0 {
            Some(ResolvedPointer::Null)
        } else {
            Some(ResolvedPointer::Vaddr(raw))
        }
    }

    fn read_u32_at_vaddr(&self, vaddr: u64) -> Option<u32> {
        let off = self.image.layout.file_offset_for_vaddr(vaddr)? as usize;
        self.image
            .raw_bytes
            .get(off..off + 4)
            .map(|s| u32_le(s))
    }

    fn read_cstr_at_vaddr(&self, vaddr: u64) -> Option<String> {
        let off = self.image.layout.file_offset_for_vaddr(vaddr)? as usize;
        let tail = self.image.raw_bytes.get(off..)?;
        let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        std::str::from_utf8(&tail[..end]).ok().map(|s| s.to_string())
    }

    fn read_image_info(&self) -> Option<ObjCImageInfo> {
        let s = self
            .image
            .layout
            .section("__DATA", "__objc_imageinfo")
            .or_else(|| self.image.layout.section("__DATA_CONST", "__objc_imageinfo"))?;
        if s.size < 8 || s.file_offset == 0 {
            return None;
        }
        let off = s.file_offset as usize;
        let buf = self.image.raw_bytes.get(off..off + 8)?;
        Some(ObjCImageInfo {
            version: u32_le(&buf[0..4]),
            flags: u32_le(&buf[4..8]),
        })
    }

    /// `objc_class` (`__objc_data`):
    ///   isa, superclass, cache, vtable, data
    /// `data` is a `class_rw_t*` at runtime but on disk holds
    /// a `class_ro_t*` (the low bit is reserved for the
    /// `RW_REALIZED` flag the runtime sets later).
    fn read_class(&self, class_vaddr: u64) -> Result<Option<ObjCClass>, ObjCReadError> {
        let off = match self.image.layout.file_offset_for_vaddr(class_vaddr) {
            Some(o) => o as usize,
            None => return Ok(None),
        };
        if off + 40 > self.image.raw_bytes.len() {
            return Ok(None);
        }
        let isa = self.resolve_pointer(class_vaddr);
        let superclass = self.resolve_pointer(class_vaddr + 8);
        // cache + vtable at +16/+24 are runtime-only.
        let data = self.resolve_pointer(class_vaddr + 32);

        let ro_vaddr = match data {
            Some(ResolvedPointer::Vaddr(v)) => v & !0x7, // mask FAST_DATA flag bits
            _ => return Ok(None),
        };
        let ro = self.read_class_ro(ro_vaddr)?;

        let metaclass_vaddr = match isa {
            Some(ResolvedPointer::Vaddr(v)) => Some(v),
            _ => None,
        };
        let (superclass_vaddr, superclass_name) = match superclass {
            Some(ResolvedPointer::Vaddr(v)) => (Some(v), None),
            Some(ResolvedPointer::External(name)) => (None, Some(strip_objc_class_prefix(&name))),
            _ => (None, None),
        };

        // class_methods come from the metaclass's class_ro_t.
        let class_methods = if let Some(mv) = metaclass_vaddr {
            self.read_metaclass_instance_methods(mv)?
        } else {
            Vec::new()
        };

        Ok(Some(ObjCClass {
            name: ro.name,
            vaddr: class_vaddr,
            metaclass_vaddr,
            superclass_vaddr,
            superclass_name,
            flags: ro.flags,
            instance_start: ro.instance_start,
            instance_size: ro.instance_size,
            instance_methods: ro.methods,
            class_methods,
            ivars: ro.ivars,
            properties: ro.properties,
            adopted_protocols: ro.protocols,
        }))
    }

    fn read_metaclass_instance_methods(
        &self,
        metaclass_vaddr: u64,
    ) -> Result<Vec<ObjCMethod>, ObjCReadError> {
        let data = self.resolve_pointer(metaclass_vaddr + 32);
        let Some(ResolvedPointer::Vaddr(ro_vaddr)) = data else {
            return Ok(Vec::new());
        };
        let ro_vaddr = ro_vaddr & !0x7;
        let ro = self.read_class_ro(ro_vaddr)?;
        Ok(ro.methods)
    }

    /// `class_ro_t`:
    ///   u32 flags, u32 instanceStart, u32 instanceSize, u32 reserved,
    ///   const uint8_t *ivarLayout,
    ///   const char *name,
    ///   const method_list_t *baseMethods,
    ///   const protocol_list_t *baseProtocols,
    ///   const ivar_list_t *ivars,
    ///   const uint8_t *weakIvarLayout,
    ///   const property_list_t *baseProperties
    fn read_class_ro(&self, ro_vaddr: u64) -> Result<ClassRo, ObjCReadError> {
        let off = self
            .image
            .layout
            .file_offset_for_vaddr(ro_vaddr)
            .ok_or(ObjCReadError::UnresolvedPointer {
                at_file_offset: 0,
                raw: ro_vaddr,
            })? as usize;
        let buf = self.image.raw_bytes.get(off..off + 16).ok_or_else(|| {
            ObjCReadError::Truncated(format!("class_ro_t header at vaddr {ro_vaddr:#x}"))
        })?;
        let flags = u32_le(&buf[0..4]);
        let instance_start = u32_le(&buf[4..8]);
        let instance_size = u32_le(&buf[8..12]);
        // pointers start at +24 on 64-bit (after 16 bytes of u32s
        // and 8-byte ivarLayout pointer).
        let name = match self.resolve_pointer(ro_vaddr + 24) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v).unwrap_or_default(),
            _ => String::new(),
        };
        let methods = match self.resolve_pointer(ro_vaddr + 32) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_method_list(v)?,
            _ => Vec::new(),
        };
        let protocols = match self.resolve_pointer(ro_vaddr + 40) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_protocol_list(v)?,
            _ => Vec::new(),
        };
        let ivars = match self.resolve_pointer(ro_vaddr + 48) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_ivar_list(v)?,
            _ => Vec::new(),
        };
        let properties = match self.resolve_pointer(ro_vaddr + 64) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_property_list(v)?,
            _ => Vec::new(),
        };
        Ok(ClassRo {
            flags,
            instance_start,
            instance_size,
            name,
            methods,
            protocols,
            ivars,
            properties,
        })
    }

    /// `method_list_t`: u32 entsize_and_flags, u32 count, then
    /// `count` entries. entries are either 3 pointers (legacy)
    /// or 3 i32 relative offsets (bit 0x80000000 of entsize).
    fn read_method_list(&self, list_vaddr: u64) -> Result<Vec<ObjCMethod>, ObjCReadError> {
        let off = match self.image.layout.file_offset_for_vaddr(list_vaddr) {
            Some(o) => o as usize,
            None => return Ok(Vec::new()),
        };
        let buf = self.image.raw_bytes.get(off..off + 8).ok_or_else(|| {
            ObjCReadError::Truncated(format!("method_list header at {list_vaddr:#x}"))
        })?;
        let entsize_and_flags = u32_le(&buf[0..4]);
        let count = u32_le(&buf[4..8]) as usize;
        let relative = entsize_and_flags & 0x8000_0000 != 0;
        let entsize = (entsize_and_flags & 0x0000_ffff) as usize;
        let mut out = Vec::with_capacity(count);
        let entries_off = off + 8;
        let entries_vaddr = list_vaddr + 8;
        for i in 0..count {
            let entry_off = entries_off + i * entsize;
            let entry_vaddr = entries_vaddr + (i as u64) * entsize as u64;
            if entry_off + entsize > self.image.raw_bytes.len() {
                return Err(ObjCReadError::Truncated(format!(
                    "method entry {i} past file end"
                )));
            }
            let entry = &self.image.raw_bytes[entry_off..entry_off + entsize];
            if relative {
                // Each i32 is a signed offset from the slot's
                // own address. name → SEL (a pointer to a
                // C-string in __objc_methname) for iOS 14+/
                // macOS 11+; we follow it once.
                let name_off_rel = i32::from_le_bytes(entry[0..4].try_into().unwrap()) as i64;
                let types_off_rel = i32::from_le_bytes(entry[4..8].try_into().unwrap()) as i64;
                let imp_off_rel = i32::from_le_bytes(entry[8..12].try_into().unwrap()) as i64;
                let name_slot_vaddr = (entry_vaddr as i64 + name_off_rel) as u64;
                let types_vaddr = (entry_vaddr as i64 + 4 + types_off_rel) as u64;
                let imp_vaddr = if imp_off_rel == 0 {
                    None
                } else {
                    Some((entry_vaddr as i64 + 8 + imp_off_rel) as u64)
                };
                // name_slot_vaddr points at a SEL slot (a u64
                // holding the address of the c-string). Follow
                // through the fixup map to the string.
                let name = match self.resolve_pointer(name_slot_vaddr) {
                    Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v),
                    _ => self.read_cstr_at_vaddr(name_slot_vaddr),
                }
                .unwrap_or_default();
                let types = self.read_cstr_at_vaddr(types_vaddr).unwrap_or_default();
                out.push(ObjCMethod {
                    name,
                    types,
                    imp: imp_vaddr,
                });
            } else {
                // Legacy: three absolute pointers. Each is a
                // fixup slot.
                let name = match self.resolve_pointer(entry_vaddr) {
                    Some(ResolvedPointer::Vaddr(v)) => {
                        self.read_cstr_at_vaddr(v).unwrap_or_default()
                    }
                    _ => String::new(),
                };
                let types = match self.resolve_pointer(entry_vaddr + 8) {
                    Some(ResolvedPointer::Vaddr(v)) => {
                        self.read_cstr_at_vaddr(v).unwrap_or_default()
                    }
                    _ => String::new(),
                };
                let imp = match self.resolve_pointer(entry_vaddr + 16) {
                    Some(ResolvedPointer::Vaddr(v)) => Some(v),
                    _ => None,
                };
                out.push(ObjCMethod { name, types, imp });
            }
        }
        Ok(out)
    }

    /// `protocol_list_t`: uintptr_t count, then `count`
    /// `protocol_ref_t` (just a pointer-sized vaddr each).
    fn read_protocol_list(&self, list_vaddr: u64) -> Result<Vec<u64>, ObjCReadError> {
        let off = match self.image.layout.file_offset_for_vaddr(list_vaddr) {
            Some(o) => o as usize,
            None => return Ok(Vec::new()),
        };
        let buf = self.image.raw_bytes.get(off..off + 8).ok_or_else(|| {
            ObjCReadError::Truncated(format!("protocol_list count at {list_vaddr:#x}"))
        })?;
        let count = u64_le(buf) as usize;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let slot = list_vaddr + 8 + (i as u64) * 8;
            if let Some(ResolvedPointer::Vaddr(v)) = self.resolve_pointer(slot) {
                out.push(v);
            }
        }
        Ok(out)
    }

    /// `ivar_list_t`: u32 entsize, u32 count, then entries of
    /// `ivar_t` (4 pointers + 3 u32s = 32 bytes on 64-bit:
    ///   offset_ptr, name, type, alignment, size).
    fn read_ivar_list(&self, list_vaddr: u64) -> Result<Vec<ObjCIvar>, ObjCReadError> {
        let off = match self.image.layout.file_offset_for_vaddr(list_vaddr) {
            Some(o) => o as usize,
            None => return Ok(Vec::new()),
        };
        let buf = self.image.raw_bytes.get(off..off + 8).ok_or_else(|| {
            ObjCReadError::Truncated(format!("ivar_list header at {list_vaddr:#x}"))
        })?;
        let entsize = u32_le(&buf[0..4]) as usize;
        let count = u32_le(&buf[4..8]) as usize;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let entry_vaddr = list_vaddr + 8 + (i as u64) * entsize as u64;
            let offset_ptr = match self.resolve_pointer(entry_vaddr) {
                Some(ResolvedPointer::Vaddr(v)) => v,
                _ => 0,
            };
            let name = match self.resolve_pointer(entry_vaddr + 8) {
                Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v).unwrap_or_default(),
                _ => String::new(),
            };
            let types = match self.resolve_pointer(entry_vaddr + 16) {
                Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v).unwrap_or_default(),
                _ => String::new(),
            };
            let alignment = self
                .read_u32_at_vaddr(entry_vaddr + 24)
                .unwrap_or(0);
            let size = self.read_u32_at_vaddr(entry_vaddr + 28).unwrap_or(0);
            let offset = if offset_ptr != 0 {
                self.read_u32_at_vaddr(offset_ptr).unwrap_or(0)
            } else {
                0
            };
            out.push(ObjCIvar {
                name,
                types,
                offset_ptr_vaddr: offset_ptr,
                offset,
                alignment,
                size,
            });
        }
        Ok(out)
    }

    /// `property_list_t`: u32 entsize, u32 count, then 2-ptr
    /// entries: name, attributes.
    fn read_property_list(&self, list_vaddr: u64) -> Result<Vec<ObjCProperty>, ObjCReadError> {
        let off = match self.image.layout.file_offset_for_vaddr(list_vaddr) {
            Some(o) => o as usize,
            None => return Ok(Vec::new()),
        };
        let buf = self.image.raw_bytes.get(off..off + 8).ok_or_else(|| {
            ObjCReadError::Truncated(format!("property_list header at {list_vaddr:#x}"))
        })?;
        let entsize = u32_le(&buf[0..4]) as usize;
        let count = u32_le(&buf[4..8]) as usize;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let entry_vaddr = list_vaddr + 8 + (i as u64) * entsize as u64;
            let name = match self.resolve_pointer(entry_vaddr) {
                Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v).unwrap_or_default(),
                _ => String::new(),
            };
            let attributes = match self.resolve_pointer(entry_vaddr + 8) {
                Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v).unwrap_or_default(),
                _ => String::new(),
            };
            out.push(ObjCProperty { name, attributes });
        }
        Ok(out)
    }

    /// `category_t`:
    ///   const char *name,
    ///   classref_t cls,
    ///   const method_list_t *instanceMethods,
    ///   const method_list_t *classMethods,
    ///   const protocol_list_t *protocols,
    ///   const property_list_t *instanceProperties,
    ///   const property_list_t *_classProperties
    fn read_category(&self, cat_vaddr: u64) -> Result<Option<ObjCCategory>, ObjCReadError> {
        if self.image.layout.file_offset_for_vaddr(cat_vaddr).is_none() {
            return Ok(None);
        }
        let name = match self.resolve_pointer(cat_vaddr) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v).unwrap_or_default(),
            _ => String::new(),
        };
        let (class_vaddr, class_name) = match self.resolve_pointer(cat_vaddr + 8) {
            Some(ResolvedPointer::Vaddr(v)) => (Some(v), None),
            Some(ResolvedPointer::External(n)) => (None, Some(strip_objc_class_prefix(&n))),
            _ => (None, None),
        };
        let instance_methods = match self.resolve_pointer(cat_vaddr + 16) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_method_list(v)?,
            _ => Vec::new(),
        };
        let class_methods = match self.resolve_pointer(cat_vaddr + 24) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_method_list(v)?,
            _ => Vec::new(),
        };
        let protocols = match self.resolve_pointer(cat_vaddr + 32) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_protocol_list(v)?,
            _ => Vec::new(),
        };
        let instance_properties = match self.resolve_pointer(cat_vaddr + 40) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_property_list(v)?,
            _ => Vec::new(),
        };
        let class_properties = match self.resolve_pointer(cat_vaddr + 48) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_property_list(v)?,
            _ => Vec::new(),
        };
        Ok(Some(ObjCCategory {
            name,
            vaddr: cat_vaddr,
            class_vaddr,
            class_name,
            instance_methods,
            class_methods,
            protocols,
            instance_properties,
            class_properties,
        }))
    }

    /// `protocol_t`:
    ///   isa,
    ///   const char *mangledName,
    ///   const protocol_list_t *protocols,
    ///   const method_list_t *instanceMethods,
    ///   const method_list_t *classMethods,
    ///   const method_list_t *optionalInstanceMethods,
    ///   const method_list_t *optionalClassMethods,
    ///   const property_list_t *instanceProperties,
    ///   u32 size, u32 flags, …
    fn read_protocol(&self, p_vaddr: u64) -> Result<Option<ObjCProtocol>, ObjCReadError> {
        if self.image.layout.file_offset_for_vaddr(p_vaddr).is_none() {
            return Ok(None);
        }
        let name = match self.resolve_pointer(p_vaddr + 8) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_cstr_at_vaddr(v).unwrap_or_default(),
            _ => String::new(),
        };
        let adopted_protocols = match self.resolve_pointer(p_vaddr + 16) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_protocol_list(v)?,
            _ => Vec::new(),
        };
        let instance_methods = match self.resolve_pointer(p_vaddr + 24) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_method_list(v)?,
            _ => Vec::new(),
        };
        let class_methods = match self.resolve_pointer(p_vaddr + 32) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_method_list(v)?,
            _ => Vec::new(),
        };
        let optional_instance_methods = match self.resolve_pointer(p_vaddr + 40) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_method_list(v)?,
            _ => Vec::new(),
        };
        let optional_class_methods = match self.resolve_pointer(p_vaddr + 48) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_method_list(v)?,
            _ => Vec::new(),
        };
        let instance_properties = match self.resolve_pointer(p_vaddr + 56) {
            Some(ResolvedPointer::Vaddr(v)) => self.read_property_list(v)?,
            _ => Vec::new(),
        };
        Ok(Some(ObjCProtocol {
            name,
            vaddr: p_vaddr,
            instance_methods,
            class_methods,
            optional_instance_methods,
            optional_class_methods,
            instance_properties,
            adopted_protocols,
        }))
    }
}

struct ClassRo {
    flags: u32,
    instance_start: u32,
    instance_size: u32,
    name: String,
    methods: Vec<ObjCMethod>,
    protocols: Vec<u64>,
    ivars: Vec<ObjCIvar>,
    properties: Vec<ObjCProperty>,
}

// ---------------------------------------------------------------------------
// Helpers
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

/// Strip the `OBJC_CLASS_$_` prefix that imports use on the
/// link-edit side, leaving the bare class name an ObjC user
/// would recognise.
fn strip_objc_class_prefix(s: &str) -> String {
    s.strip_prefix("_OBJC_CLASS_$_")
        .or_else(|| s.strip_prefix("OBJC_CLASS_$_"))
        .unwrap_or(s)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/macho_objc_fixture/libgreet_objc.dylib")
    }

    fn load_fixture() -> Option<MachOImage> {
        let bytes = std::fs::read(fixture_path()).ok()?;
        MachOImage::parse(bytes).ok()
    }

    #[test]
    fn parses_chained_fixups_without_panic() {
        let Some(img) = load_fixture() else {
            eprintln!("skip: fixture not present");
            return;
        };
        let map = build_chained_fixup_map(&img).expect("chained fixups");
        assert!(!map.is_empty(), "expected non-empty fixup map");
        // There should be at least one bind entry pointing at a
        // libobjc / Foundation symbol (e.g. _OBJC_CLASS_$_NSObject).
        let any_objc_bind = map.entries.values().any(|t| match t {
            ChainedFixupTarget::Bind { symbol_name, .. } => {
                symbol_name.contains("OBJC_CLASS_$_NSObject")
                    || symbol_name.contains("_objc_msgSend")
            }
            _ => false,
        });
        assert!(any_objc_bind, "expected at least one ObjC bind in the map");
    }

    #[test]
    fn reads_class_with_methods_ivars_properties_protocols() {
        let Some(img) = load_fixture() else {
            eprintln!("skip: fixture not present");
            return;
        };
        let md = read_objc_metadata(&img).expect("read metadata");
        // The fixture has exactly one user-defined class: Greet.
        let greet = md
            .classes
            .iter()
            .find(|c| c.name == "Greet")
            .expect("class `Greet` parsed");
        // Superclass is NSObject from the runtime — comes through
        // as a bind, so `superclass_name` is populated and the
        // `_vaddr` is None.
        assert_eq!(
            greet.superclass_name.as_deref(),
            Some("NSObject"),
            "superclass via bind"
        );
        let method_names: Vec<&str> =
            greet.instance_methods.iter().map(|m| m.name.as_str()).collect();
        assert!(method_names.contains(&"hello"), "missing -hello: {method_names:?}");
        assert!(method_names.contains(&"bye"), "missing -bye: {method_names:?}");
        // +shared lives on the metaclass and lands in class_methods.
        let class_method_names: Vec<&str> =
            greet.class_methods.iter().map(|m| m.name.as_str()).collect();
        assert!(
            class_method_names.contains(&"shared"),
            "missing +shared: {class_method_names:?}"
        );
        // One ivar, one property.
        assert!(greet.ivars.iter().any(|i| i.name == "_count"), "ivar _count");
        assert!(
            greet.properties.iter().any(|p| p.name == "count"),
            "property count"
        );
        // Adopts the Greeter protocol declared in the same fixture.
        assert!(
            !greet.adopted_protocols.is_empty(),
            "expected at least one adopted protocol"
        );
    }

    #[test]
    fn reads_category_extending_greet() {
        let Some(img) = load_fixture() else {
            eprintln!("skip: fixture not present");
            return;
        };
        let md = read_objc_metadata(&img).expect("read metadata");
        let util = md
            .categories
            .iter()
            .find(|c| c.name == "Util")
            .expect("category `Util` parsed");
        let names: Vec<&str> =
            util.instance_methods.iter().map(|m| m.name.as_str()).collect();
        assert!(
            names.contains(&"util_value"),
            "missing -util_value in category: {names:?}",
        );
        // Category extends an external class (NSObject), so the
        // class pointer resolves through a bind, not an in-image
        // vmaddr.
        assert_eq!(util.class_name.as_deref(), Some("NSObject"));
        assert!(util.class_vaddr.is_none());
    }

    #[test]
    fn reads_protocol_with_required_and_optional_methods() {
        let Some(img) = load_fixture() else {
            eprintln!("skip: fixture not present");
            return;
        };
        let md = read_objc_metadata(&img).expect("read metadata");
        let greeter = md
            .protocols
            .iter()
            .find(|p| p.name == "Greeter")
            .expect("protocol `Greeter` parsed");
        let required: Vec<&str> =
            greeter.instance_methods.iter().map(|m| m.name.as_str()).collect();
        let optional: Vec<&str> = greeter
            .optional_instance_methods
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert!(
            required.contains(&"hello"),
            "required must include -hello: {required:?}"
        );
        assert!(
            optional.contains(&"bye"),
            "optional must include -bye: {optional:?}"
        );
    }

    #[test]
    fn image_info_present() {
        let Some(img) = load_fixture() else {
            eprintln!("skip: fixture not present");
            return;
        };
        let md = read_objc_metadata(&img).expect("read metadata");
        let info = md.image_info.expect("__objc_imageinfo present");
        // version is always 0 currently; flags non-zero on
        // Swift-bridged / category-class-properties / etc. We
        // only assert the section was parsed.
        assert_eq!(info.version, 0, "objc image_info version");
        let _ = info.flags;
    }
}

