//! Swift metadata reader for 64-bit Mach-O images.
//!
//! Walks the Swift v5 reflection sections — `__swift5_types`,
//! `__swift5_proto`, `__swift5_protos`, `__swift5_fieldmd`,
//! `__swift5_reflstr` — and returns an owned [`SwiftMetadata`]
//! snapshot covering classes, structs, enums, protocols, and
//! conformance records.
//!
//! Scope of v1 mirrors [`super::macho_objc`]:
//!
//! * 64-bit Mach-O (`MH_MAGIC_64`) only.
//! * Modern pointer encoding — `LC_DYLD_CHAINED_FIXUPS`.
//!   Cross-image type / protocol references in conformance
//!   descriptors are resolved through the chained-fixup import
//!   table.
//! * Read-only — no mutation API.
//! * Mangled names are returned verbatim. Demangling is the
//!   caller's responsibility.
//!
//! Out of scope for v1: `__swift5_capture`, `__swift5_mpenum`,
//! `__swift5_assocty`, generic instantiation patterns, and
//! resilient witness table contents.

use std::collections::HashMap;

use crate::container::macho_image::MachOImage;
use crate::container::macho_objc::{
    build_chained_fixup_map, ChainedFixupMap, ChainedFixupTarget,
};

/// Owned snapshot of an image's Swift metadata, built once via
/// [`read_swift_metadata`] and then read freely.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SwiftMetadata {
    pub types: Vec<SwiftType>,
    pub protocols: Vec<SwiftProtocol>,
    pub conformances: Vec<SwiftConformance>,
}

/// A nominal Swift type — class, struct, or enum.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftType {
    pub mangled_name: String,
    pub descriptor_vaddr: u64,
    pub kind: SwiftTypeKind,
    /// Parent context descriptor vaddr (module or enclosing
    /// type). `None` when the parent reference was zero or
    /// unresolved.
    pub parent_vaddr: Option<u64>,
    pub fields: Vec<SwiftField>,
    pub metadata_accessor_vaddr: Option<u64>,
    /// Class-only: virtual method slots, in declaration order.
    /// Empty for structs / enums and for classes with no
    /// override-able methods.
    pub vtable: Vec<SwiftVTableEntry>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum SwiftTypeKind {
    Class,
    Struct,
    Enum,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftField {
    pub name: String,
    pub mangled_type: String,
    pub flags: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftVTableEntry {
    pub impl_vaddr: u64,
    pub flags: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftProtocol {
    pub mangled_name: String,
    pub descriptor_vaddr: u64,
    pub num_requirements: u32,
    pub inherited: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftConformance {
    pub descriptor_vaddr: u64,
    pub protocol_ref: SwiftRef,
    pub type_ref: SwiftRef,
    pub witness_table_vaddr: Option<u64>,
}

/// In-image or external symbol reference, paralleling the ObjC
/// reader's class-name / class-vaddr split.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SwiftRef {
    InImage { vaddr: u64 },
    Imported { symbol_name: String },
}

#[derive(Debug)]
pub enum SwiftReadError {
    NotMachO,
    /// No `LC_DYLD_CHAINED_FIXUPS` — Swift cross-image
    /// references rely on the modern fixup format.
    ChainedFixupsMissing,
    /// Either the image is well-formed but contains no Swift
    /// metadata at all, or only `__swift5_reflstr` is present
    /// (no types). Callers can treat this as "no Swift" and
    /// move on.
    NoSwiftMetadata,
    Truncated(String),
}

/// Swift type-descriptor `Flags` decoding. See
/// `swift/include/swift/ABI/MetadataValues.h`,
/// `ContextDescriptorFlags`.
#[allow(dead_code)]
mod cdflags {
    pub const KIND_MASK: u32 = 0x1F;
    pub const KIND_MODULE: u32 = 0;
    pub const KIND_EXTENSION: u32 = 1;
    pub const KIND_ANONYMOUS: u32 = 2;
    pub const KIND_PROTOCOL: u32 = 3;
    pub const KIND_OPAQUE_TYPE: u32 = 4;
    pub const KIND_CLASS: u32 = 16;
    pub const KIND_STRUCT: u32 = 17;
    pub const KIND_ENUM: u32 = 18;

    /// Bit set in the high 16 bits of `Flags` for type
    /// descriptors that carry a generic-context trailer.
    pub const TYPE_GENERIC: u32 = 0x0000_8000;

    // Class-only kind-specific flags (high 16 bits of Flags).
    pub const CLASS_HAS_VTABLE: u32 = 0x8000_0000;
    pub const CLASS_HAS_OVERRIDE_TABLE: u32 = 0x4000_0000;
    pub const CLASS_HAS_RESILIENT_SUPERCLASS: u32 = 0x2000_0000;
    pub const CLASS_AREIMMEDIATE_MEMBERS_NEGATIVE: u32 = 0x1000_0000;
    pub const CLASS_RESILIENT_SUPERCLASS_REF_MASK: u32 = 0x0E00_0000;
}

/// Build the snapshot. Returns `Err(SwiftReadError::NoSwiftMetadata)`
/// if the image contains no `__swift5_types` (and no protocols).
/// Callers that prefer "absent" to "error" should treat
/// `NoSwiftMetadata` as an empty result.
pub fn read_swift_metadata(image: &MachOImage) -> Result<SwiftMetadata, SwiftReadError> {
    if image.layout.chained_fixups.is_none() {
        return Err(SwiftReadError::ChainedFixupsMissing);
    }
    let fixups = build_chained_fixup_map(image)
        .map_err(|e| SwiftReadError::Truncated(format!("chained fixup map: {e:?}")))?;

    let reader = SwiftReader {
        image,
        fixups: &fixups,
    };

    let reflstr = reader.read_reflstr();
    let fields = reader.read_field_descriptors(&reflstr);

    let types = reader.read_types(&fields);
    let protocols = reader.read_protocols();
    let conformances = reader.read_conformances();

    if types.is_empty() && protocols.is_empty() && conformances.is_empty() {
        return Err(SwiftReadError::NoSwiftMetadata);
    }

    Ok(SwiftMetadata {
        types,
        protocols,
        conformances,
    })
}

struct SwiftReader<'a> {
    image: &'a MachOImage,
    fixups: &'a ChainedFixupMap,
}

/// Parsed field descriptor entry — built once, looked up by
/// type descriptors via the descriptor's vaddr.
struct ParsedFieldDescriptor {
    fields: Vec<SwiftField>,
}

impl<'a> SwiftReader<'a> {
    fn find_section(&self, sectname: &str) -> Option<&'a crate::container::MachOSection> {
        self.image
            .layout
            .sections
            .iter()
            .find(|s| s.sectname == sectname)
    }

    fn section_bytes(&self, sectname: &str) -> Option<(u64, &'a [u8])> {
        let s = self.find_section(sectname)?;
        if s.size == 0 || s.file_offset == 0 {
            return None;
        }
        let off = s.file_offset as usize;
        let end = off + s.size as usize;
        let buf = self.image.raw_bytes.get(off..end)?;
        Some((s.vaddr, buf))
    }

    fn read_u32_at_vaddr(&self, vaddr: u64) -> Option<u32> {
        let off = self.image.layout.file_offset_for_vaddr(vaddr)? as usize;
        let buf = self.image.raw_bytes.get(off..off + 4)?;
        Some(u32_le(buf))
    }

    fn read_i32_at_vaddr(&self, vaddr: u64) -> Option<i32> {
        self.read_u32_at_vaddr(vaddr).map(|u| u as i32)
    }

    fn read_cstr_at_vaddr(&self, vaddr: u64) -> Option<String> {
        let off = self.image.layout.file_offset_for_vaddr(vaddr)? as usize;
        let tail = self.image.raw_bytes.get(off..)?;
        let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        std::str::from_utf8(&tail[..end]).ok().map(|s| s.to_string())
    }

    /// Resolve a 32-bit relative pointer field whose value lives
    /// at `field_vaddr`. Returns `field_vaddr + offset` when the
    /// offset is non-zero; `None` when the offset is zero (the
    /// Swift convention for "null pointer"). Errors that prevent
    /// reading the offset also yield `None`.
    fn resolve_relative(&self, field_vaddr: u64) -> Option<u64> {
        let off = self.read_i32_at_vaddr(field_vaddr)?;
        if off == 0 {
            return None;
        }
        let signed = field_vaddr as i64 + off as i64;
        if signed < 0 {
            return None;
        }
        Some(signed as u64)
    }

    /// Like [`resolve_relative`] but interprets the low bit as
    /// the "indirect" flag. When set, the offset points to a
    /// pointer slot (subject to chained fixups) which we
    /// dereference via the fixup map.
    fn resolve_relative_indirectable(&self, field_vaddr: u64) -> Option<SwiftRef> {
        let raw = self.read_i32_at_vaddr(field_vaddr)?;
        if raw == 0 {
            return None;
        }
        let indirect = raw & 1 != 0;
        let offset = (raw & !1) as i64;
        let target = field_vaddr as i64 + offset;
        if target < 0 {
            return None;
        }
        let target = target as u64;
        if !indirect {
            return Some(SwiftRef::InImage { vaddr: target });
        }
        // Indirect: target is a pointer slot. Consult the fixup
        // map first (chained binaries) and fall back to reading
        // the raw 64-bit value.
        match self.fixups.get(target) {
            Some(ChainedFixupTarget::Rebase { target_vaddr }) => Some(SwiftRef::InImage {
                vaddr: *target_vaddr,
            }),
            Some(ChainedFixupTarget::Bind { symbol_name, .. }) => Some(SwiftRef::Imported {
                symbol_name: symbol_name.clone(),
            }),
            None => {
                let off = self.image.layout.file_offset_for_vaddr(target)? as usize;
                let buf = self.image.raw_bytes.get(off..off + 8)?;
                let v = u64_le(buf);
                if v == 0 {
                    None
                } else {
                    Some(SwiftRef::InImage { vaddr: v })
                }
            }
        }
    }

    /// Build a `vaddr → string` map from `__swift5_reflstr`. The
    /// section is a tightly-packed NUL-terminated table; each
    /// run's start vaddr is the key field-descriptors point at.
    fn read_reflstr(&self) -> HashMap<u64, String> {
        let mut out = HashMap::new();
        let Some((base_vaddr, buf)) = self.section_bytes("__swift5_reflstr") else {
            return out;
        };
        let mut i = 0usize;
        while i < buf.len() {
            let start = i;
            while i < buf.len() && buf[i] != 0 {
                i += 1;
            }
            if let Ok(s) = std::str::from_utf8(&buf[start..i]) {
                out.insert(base_vaddr + start as u64, s.to_string());
            }
            // Skip NUL terminator(s).
            while i < buf.len() && buf[i] == 0 {
                i += 1;
            }
        }
        out
    }

    /// Build a `descriptor_vaddr → ParsedFieldDescriptor` map by
    /// walking `__swift5_fieldmd`. Type descriptors carry a
    /// 32-bit relative offset that resolves to one of these
    /// descriptors.
    ///
    /// `FieldDescriptor` layout (16-byte header + N field records):
    ///
    ///   i32 MangledTypeName     (relative)
    ///   i32 Superclass          (relative)
    ///   u16 Kind
    ///   u16 FieldRecordSize     (12 in current Swift)
    ///   u32 NumFields
    ///   FieldRecord[NumFields]
    ///
    /// `FieldRecord`:
    ///
    ///   u32 Flags
    ///   i32 MangledTypeName     (relative)
    ///   i32 FieldName           (relative, into __swift5_reflstr)
    fn read_field_descriptors(
        &self,
        reflstr: &HashMap<u64, String>,
    ) -> HashMap<u64, ParsedFieldDescriptor> {
        let mut out = HashMap::new();
        let Some((base_vaddr, buf)) = self.section_bytes("__swift5_fieldmd") else {
            return out;
        };
        let mut i = 0usize;
        while i + 16 <= buf.len() {
            let descriptor_vaddr = base_vaddr + i as u64;
            let _mangled_type_off = i32_le(&buf[i..i + 4]);
            let _superclass_off = i32_le(&buf[i + 4..i + 8]);
            let _kind = u16_le(&buf[i + 8..i + 10]);
            let record_size = u16_le(&buf[i + 10..i + 12]) as usize;
            let num_fields = u32_le(&buf[i + 12..i + 16]) as usize;
            i += 16;
            if record_size == 0 {
                // Defensive: avoid infinite loops on malformed input.
                continue;
            }
            let mut fields = Vec::with_capacity(num_fields);
            let mut j = 0usize;
            while j < num_fields && i + record_size <= buf.len() {
                let record_vaddr = base_vaddr + i as u64;
                let flags = u32_le(&buf[i..i + 4]);
                let type_off = i32_le(&buf[i + 4..i + 8]);
                let name_off = i32_le(&buf[i + 8..i + 12]);

                let name = if name_off != 0 {
                    let name_vaddr = (record_vaddr + 8) as i64 + name_off as i64;
                    if name_vaddr >= 0 {
                        reflstr
                            .get(&(name_vaddr as u64))
                            .cloned()
                            .or_else(|| self.read_cstr_at_vaddr(name_vaddr as u64))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let mangled_type = if type_off != 0 {
                    let tv = (record_vaddr + 4) as i64 + type_off as i64;
                    if tv >= 0 {
                        // Mangled type symbols are NUL-terminated
                        // C strings in __swift5_typeref or similar.
                        // The leading byte may be a control byte
                        // (0x01..=0x1F) prefix; preserve verbatim
                        // and let the caller demangle.
                        self.read_swift_mangled(tv as u64)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                fields.push(SwiftField {
                    name,
                    mangled_type,
                    flags,
                });
                i += record_size;
                j += 1;
            }
            out.insert(descriptor_vaddr, ParsedFieldDescriptor { fields });
        }
        out
    }

    /// Swift mangled-type references in `__swift5_typeref` use
    /// a variable-length encoding: bytes 0x01–0x17 are
    /// "symbolic references" (followed by a 4-byte relative
    /// offset to a context descriptor), and the rest is a plain
    /// mangled symbol string terminated by NUL or end-of-section.
    ///
    /// For phase 1 we return the bytes verbatim (as a
    /// best-effort UTF-8 string when possible). Symbolic-reference
    /// bytes are encoded as `\x01..\x17` placeholders; downstream
    /// demanglers (e.g. `symbolic-demangle`) understand the same
    /// raw form.
    fn read_swift_mangled(&self, vaddr: u64) -> String {
        let Some(off) = self.image.layout.file_offset_for_vaddr(vaddr) else {
            return String::new();
        };
        let off = off as usize;
        let Some(tail) = self.image.raw_bytes.get(off..) else {
            return String::new();
        };
        // Find the terminator. The mangled grammar terminates
        // at NUL; symbolic refs are length-1 control byte + 4
        // bytes of i32, which we copy verbatim.
        let mut i = 0usize;
        while i < tail.len() {
            let b = tail[i];
            if b == 0 {
                break;
            }
            if (0x01..=0x17).contains(&b) {
                // Control byte + 4-byte relative offset.
                if i + 5 > tail.len() {
                    break;
                }
                i += 5;
            } else {
                i += 1;
            }
            // Soft cap so we don't read forever on malformed data.
            if i > 4096 {
                break;
            }
        }
        String::from_utf8_lossy(&tail[..i]).into_owned()
    }

    /// Walk `__swift5_types` — an array of 32-bit relative
    /// offsets to type-context descriptors.
    fn read_types(&self, fields: &HashMap<u64, ParsedFieldDescriptor>) -> Vec<SwiftType> {
        let mut out = Vec::new();
        let Some((base_vaddr, buf)) = self.section_bytes("__swift5_types") else {
            return out;
        };
        let mut i = 0usize;
        while i + 4 <= buf.len() {
            let entry_vaddr = base_vaddr + i as u64;
            let off = i32_le(&buf[i..i + 4]);
            i += 4;
            if off == 0 {
                continue;
            }
            let desc_vaddr = entry_vaddr as i64 + off as i64;
            if desc_vaddr < 0 {
                continue;
            }
            if let Some(t) = self.parse_type_descriptor(desc_vaddr as u64, fields) {
                out.push(t);
            }
        }
        out
    }

    /// Common type-context descriptor prefix is:
    ///   u32 Flags
    ///   i32 Parent              (relative)
    ///   i32 Name                (relative)
    ///   i32 AccessFunction      (relative)
    ///   i32 FieldDescriptor     (relative)
    /// Beyond this, class / struct / enum diverge.
    fn parse_type_descriptor(
        &self,
        desc_vaddr: u64,
        fields: &HashMap<u64, ParsedFieldDescriptor>,
    ) -> Option<SwiftType> {
        let flags = self.read_u32_at_vaddr(desc_vaddr)?;
        let kind_bits = flags & cdflags::KIND_MASK;
        let kind = match kind_bits {
            cdflags::KIND_CLASS => SwiftTypeKind::Class,
            cdflags::KIND_STRUCT => SwiftTypeKind::Struct,
            cdflags::KIND_ENUM => SwiftTypeKind::Enum,
            _ => return None,
        };

        let parent_vaddr = self.resolve_relative(desc_vaddr + 4);
        let name_vaddr = self.resolve_relative(desc_vaddr + 8)?;
        let access_vaddr = self.resolve_relative(desc_vaddr + 12);
        let field_desc_vaddr = self.resolve_relative(desc_vaddr + 16);

        let mangled_name = self
            .read_cstr_at_vaddr(name_vaddr)
            .unwrap_or_default();

        let field_list = field_desc_vaddr
            .and_then(|v| fields.get(&v))
            .map(|p| p.fields.clone())
            .unwrap_or_default();

        let vtable = if kind == SwiftTypeKind::Class {
            self.parse_class_vtable(desc_vaddr, flags).unwrap_or_default()
        } else {
            Vec::new()
        };

        Some(SwiftType {
            mangled_name,
            descriptor_vaddr: desc_vaddr,
            kind,
            parent_vaddr,
            fields: field_list,
            metadata_accessor_vaddr: access_vaddr,
            vtable,
        })
    }

    /// `TargetClassDescriptor` trailing layout after the common
    /// prefix (`flags / parent / name / access / fieldmd`, 20
    /// bytes total):
    ///
    ///   i32 Superclass          (relative)
    ///   u32 MetadataNegativeSize / ResilientMetadataBoundsRef
    ///   u32 MetadataPositiveSize / ExtraClassFlags
    ///   u32 NumImmediateMembers
    ///   u32 NumFields
    ///   u32 FieldOffsetVectorOffset / ResilientFieldOffsetsRef
    ///   (optional generic context trailer)
    ///   (optional resilient-superclass trailer)
    ///   (optional foreign / singleton metadata-init trailer)
    ///   (optional VTableDescriptorHeader + VTableEntry[])
    ///   (optional OverrideTable)
    ///
    /// For phase 1 we parse only the v-table trailer and skip
    /// generic contexts / resilient-superclass refs / metadata-
    /// init blocks (binaries we care about, like Glass's target
    /// apps, rarely combine them with virtual methods on the
    /// same class — the lookups would be best-effort even if we
    /// implemented them). When unsupported trailers are present
    /// we conservatively return no v-table.
    fn parse_class_vtable(
        &self,
        desc_vaddr: u64,
        flags: u32,
    ) -> Option<Vec<SwiftVTableEntry>> {
        if flags & cdflags::CLASS_HAS_VTABLE == 0 {
            return None;
        }
        // Refuse cases we don't model — caller treats as "no vtable".
        if flags & cdflags::CLASS_HAS_RESILIENT_SUPERCLASS != 0 {
            return None;
        }
        if flags & cdflags::TYPE_GENERIC != 0 {
            return None;
        }
        if flags & cdflags::CLASS_RESILIENT_SUPERCLASS_REF_MASK != 0 {
            return None;
        }

        // Common prefix is 20 bytes; class trailing fixed fields
        // are 24 bytes (six u32s).
        let trailer_off = desc_vaddr + 20 + 24;
        // VTableDescriptorHeader { u32 VTableOffset; u32 VTableSize; }
        let _vt_offset = self.read_u32_at_vaddr(trailer_off)?;
        let vt_size = self.read_u32_at_vaddr(trailer_off + 4)? as usize;
        if vt_size == 0 {
            return Some(Vec::new());
        }
        // VTableEntry { i32 Impl (relative, points to function);
        //               u32 Flags; }
        // Stride = 8.
        let entries_base = trailer_off + 8;
        let mut out = Vec::with_capacity(vt_size);
        for k in 0..vt_size {
            let entry_vaddr = entries_base + (k as u64) * 8;
            let impl_field_vaddr = entry_vaddr;
            let flags_vaddr = entry_vaddr + 4;
            let impl_vaddr = self
                .resolve_relative(impl_field_vaddr)
                .or_else(|| {
                    // Some entries are absolute pointers under
                    // chained fixups (e.g. resilient overrides);
                    // we don't try to handle those — record 0.
                    None
                })
                .unwrap_or(0);
            let entry_flags = self.read_u32_at_vaddr(flags_vaddr).unwrap_or(0);
            out.push(SwiftVTableEntry {
                impl_vaddr,
                flags: entry_flags,
            });
        }
        Some(out)
    }

    /// Walk `__swift5_protos` — array of i32 relative offsets to
    /// `TargetProtocolDescriptor` records. (Note: section naming
    /// is the opposite of what one might guess —
    /// `__swift5_protos` (plural) holds protocol *descriptors*,
    /// while `__swift5_proto` (singular) holds *conformance*
    /// records. Swift's own codegen uses this convention.)
    fn read_protocols(&self) -> Vec<SwiftProtocol> {
        let mut out = Vec::new();
        let Some((base_vaddr, buf)) = self.section_bytes("__swift5_protos") else {
            return out;
        };
        let mut i = 0usize;
        while i + 4 <= buf.len() {
            let entry_vaddr = base_vaddr + i as u64;
            let off = i32_le(&buf[i..i + 4]);
            i += 4;
            if off == 0 {
                continue;
            }
            let desc_vaddr = entry_vaddr as i64 + off as i64;
            if desc_vaddr < 0 {
                continue;
            }
            if let Some(p) = self.parse_protocol_descriptor(desc_vaddr as u64) {
                out.push(p);
            }
        }
        out
    }

    /// `TargetProtocolDescriptor`:
    ///   u32 Flags
    ///   i32 Parent             (relative)
    ///   i32 Name               (relative)
    ///   u32 NumRequirementsInSignature
    ///   u32 NumRequirements
    ///   i32 AssociatedTypeNames (relative, optional)
    ///   ProtocolRequirement[NumRequirements] trailing
    fn parse_protocol_descriptor(&self, desc_vaddr: u64) -> Option<SwiftProtocol> {
        let flags = self.read_u32_at_vaddr(desc_vaddr)?;
        if flags & cdflags::KIND_MASK != cdflags::KIND_PROTOCOL {
            return None;
        }
        let _parent = self.resolve_relative(desc_vaddr + 4);
        let name_vaddr = self.resolve_relative(desc_vaddr + 8)?;
        let _num_in_sig = self.read_u32_at_vaddr(desc_vaddr + 12)?;
        let num_requirements = self.read_u32_at_vaddr(desc_vaddr + 16)?;

        let mangled_name = self.read_cstr_at_vaddr(name_vaddr).unwrap_or_default();

        Some(SwiftProtocol {
            mangled_name,
            descriptor_vaddr: desc_vaddr,
            num_requirements,
            inherited: Vec::new(),
        })
    }

    /// Walk `__swift5_proto` — array of i32 relative offsets
    /// to `TargetProtocolConformanceDescriptor` records.
    fn read_conformances(&self) -> Vec<SwiftConformance> {
        let mut out = Vec::new();
        let Some((base_vaddr, buf)) = self.section_bytes("__swift5_proto") else {
            return out;
        };
        let mut i = 0usize;
        while i + 4 <= buf.len() {
            let entry_vaddr = base_vaddr + i as u64;
            let off = i32_le(&buf[i..i + 4]);
            i += 4;
            if off == 0 {
                continue;
            }
            let desc_vaddr = entry_vaddr as i64 + off as i64;
            if desc_vaddr < 0 {
                continue;
            }
            if let Some(c) = self.parse_conformance_descriptor(desc_vaddr as u64) {
                out.push(c);
            }
        }
        out
    }

    /// `TargetProtocolConformanceDescriptor`:
    ///   i32 ProtocolDescriptor (relative, indirectable)
    ///   i32 TypeRef            (relative, indirectable; low bits
    ///                           also encode TypeReferenceKind)
    ///   i32 WitnessTablePattern (relative)
    ///   u32 ConformanceFlags
    ///   (trailing fields gated on flags — generic context,
    ///    retroactive context, conditional reqs, resilient
    ///    witnesses)
    fn parse_conformance_descriptor(&self, desc_vaddr: u64) -> Option<SwiftConformance> {
        let protocol_ref = self.resolve_relative_indirectable(desc_vaddr)?;

        // TypeRef uses the low 3 bits as TypeReferenceKind and
        // the rest as a relative offset. We treat the low bit
        // identically to indirectable refs (low-bit = indirect),
        // which is correct for the two dominant kinds
        // (DirectTypeDescriptor=0, IndirectTypeDescriptor=1).
        // Other kinds (DirectObjCClassName=2, IndirectObjCClass=3)
        // are signalled by the conformance flags; we fall back
        // to a direct read for unknown kinds.
        let type_ref = self.parse_typeref_field(desc_vaddr + 4)?;

        let witness_off = self.read_i32_at_vaddr(desc_vaddr + 8)?;
        let witness_table_vaddr = if witness_off == 0 {
            None
        } else {
            let v = (desc_vaddr + 8) as i64 + witness_off as i64;
            if v < 0 {
                None
            } else {
                Some(v as u64)
            }
        };

        Some(SwiftConformance {
            descriptor_vaddr: desc_vaddr,
            protocol_ref,
            type_ref,
            witness_table_vaddr,
        })
    }

    /// Decode a TypeRef field. ConformanceFlags carries the
    /// `TypeReferenceKind` in the top byte, but a sufficient
    /// approximation for read-only consumers is to interpret
    /// the low bit as "indirect" — direct kinds have low bit 0,
    /// indirect kinds have low bit 1. ObjC-class kinds (2/3)
    /// rarely appear in modern Swift; we treat them as direct.
    fn parse_typeref_field(&self, field_vaddr: u64) -> Option<SwiftRef> {
        let raw = self.read_i32_at_vaddr(field_vaddr)?;
        if raw == 0 {
            return None;
        }
        let indirect = raw & 1 != 0;
        let offset = (raw & !1) as i64;
        let target = field_vaddr as i64 + offset;
        if target < 0 {
            return None;
        }
        let target = target as u64;
        if !indirect {
            return Some(SwiftRef::InImage { vaddr: target });
        }
        match self.fixups.get(target) {
            Some(ChainedFixupTarget::Rebase { target_vaddr }) => Some(SwiftRef::InImage {
                vaddr: *target_vaddr,
            }),
            Some(ChainedFixupTarget::Bind { symbol_name, .. }) => Some(SwiftRef::Imported {
                symbol_name: symbol_name.clone(),
            }),
            None => {
                let off = self.image.layout.file_offset_for_vaddr(target)? as usize;
                let buf = self.image.raw_bytes.get(off..off + 8)?;
                let v = u64_le(buf);
                if v == 0 {
                    None
                } else {
                    Some(SwiftRef::InImage { vaddr: v })
                }
            }
        }
    }
}

fn u16_le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn i32_le(b: &[u8]) -> i32 {
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn u64_le(b: &[u8]) -> u64 {
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/macho_swift_fixture/libgreet_swift.dylib")
    }

    fn load_fixture() -> Option<MachOImage> {
        let bytes = std::fs::read(fixture_path()).ok()?;
        MachOImage::parse(bytes).ok()
    }

    #[test]
    fn reads_struct_class_enum_with_fields() {
        let Some(img) = load_fixture() else {
            eprintln!("skip: fixture not present");
            return;
        };
        let md = read_swift_metadata(&img).expect("read swift metadata");

        let point = md
            .types
            .iter()
            .find(|t| t.mangled_name == "Point")
            .expect("Point struct present");
        assert_eq!(point.kind, SwiftTypeKind::Struct);
        let field_names: Vec<&str> = point.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(field_names, vec!["x", "y"], "struct field order");

        let mood = md
            .types
            .iter()
            .find(|t| t.mangled_name == "Mood")
            .expect("Mood enum present");
        assert_eq!(mood.kind, SwiftTypeKind::Enum);
        let case_names: Vec<&str> = mood.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(case_names, vec!["happy", "sad", "neutral"], "enum case order");

        let greeting = md
            .types
            .iter()
            .find(|t| t.mangled_name == "Greeting")
            .expect("Greeting class present");
        assert_eq!(greeting.kind, SwiftTypeKind::Class);
        let cls_fields: Vec<&str> = greeting.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(cls_fields, vec!["name", "mood", "origin"]);
        assert!(
            greeting.metadata_accessor_vaddr.is_some(),
            "class metadata accessor populated"
        );
    }

    #[test]
    fn parses_class_vtable_entries() {
        let Some(img) = load_fixture() else {
            return;
        };
        let md = read_swift_metadata(&img).expect("read swift metadata");
        let greeting = md
            .types
            .iter()
            .find(|t| t.mangled_name == "Greeting")
            .expect("Greeting present");
        assert!(
            !greeting.vtable.is_empty(),
            "Greeting should expose v-table entries"
        );
        // Every entry should point inside the image's __TEXT or be
        // a recognised null/imported slot. We accept zero for
        // resilient overrides, but the majority should be live.
        let live = greeting
            .vtable
            .iter()
            .filter(|e| e.impl_vaddr != 0)
            .count();
        assert!(
            live >= 1,
            "expected at least one live vtable impl: {:?}",
            greeting.vtable
        );

        let fancy = md
            .types
            .iter()
            .find(|t| t.mangled_name == "FancyGreeting")
            .expect("FancyGreeting present");
        assert!(!fancy.vtable.is_empty(), "subclass has its own slots");
    }

    #[test]
    fn parses_protocol_and_conformances() {
        let Some(img) = load_fixture() else {
            return;
        };
        let md = read_swift_metadata(&img).expect("read swift metadata");

        let greeter = md
            .protocols
            .iter()
            .find(|p| p.mangled_name == "Greeter")
            .expect("Greeter protocol present");
        assert_eq!(
            greeter.num_requirements, 2,
            "Greeter declares greet + farewell"
        );

        // At least one in-image conformance — Greeting : Greeter.
        let in_image_conformance = md
            .conformances
            .iter()
            .find(|c| {
                matches!(&c.protocol_ref, SwiftRef::InImage { vaddr } if *vaddr == greeter.descriptor_vaddr)
            })
            .expect("Greeting : Greeter conformance present");
        match &in_image_conformance.type_ref {
            SwiftRef::InImage { .. } => {}
            other => panic!("conforming type should be in-image, got {other:?}"),
        }

        // At least one imported conformance ref — Mood synthesises
        // Equatable / Hashable, which reference the stdlib protocols.
        let any_imported = md.conformances.iter().any(|c| {
            matches!(&c.protocol_ref, SwiftRef::Imported { symbol_name }
                if symbol_name.contains("$sSQMp") || symbol_name.contains("$sSHMp"))
        });
        assert!(
            any_imported,
            "expected synthesised Equatable/Hashable conformance via import: {:?}",
            md.conformances
                .iter()
                .map(|c| &c.protocol_ref)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_swift_metadata_on_objc_fixture() {
        // The ObjC-only dylib has no Swift sections, so reader
        // should return NoSwiftMetadata. Cross-checks that our
        // section discovery does not mis-claim ObjC sections.
        let objc_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/macho_objc_fixture/libgreet_objc.dylib");
        let Ok(bytes) = std::fs::read(&objc_path) else {
            return;
        };
        let img = MachOImage::parse(bytes).expect("parse objc fixture");
        match read_swift_metadata(&img) {
            Err(SwiftReadError::NoSwiftMetadata) => {}
            other => panic!("expected NoSwiftMetadata, got {other:?}"),
        }
    }
}
