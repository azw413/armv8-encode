//! Write a [`Container`] back to Mach-O / ELF bytes.
//!
//! This is the inverse of `reader.rs`: it walks our neutral model and
//! drives `object::write` to produce a valid object file. Round-trip is
//! "compatible," not byte-identical — the writer reconstructs the
//! structural content (sections, symbols, relocations) but doesn't
//! preserve every header detail of the source file. For analysis and
//! rewriter use cases, that's the contract callers want.
//!
//! ## Limitations
//!
//! - Mach-O segment names are derived from section name + kind. Sources
//!   that put sections in unusual segments won't survive round-trip.
//! - Branch19 / Branch14 relocations are emitted only for ELF — Mach-O's
//!   AArch64 relocation set has no native equivalents (the assembler
//!   resolves these locally), and we error if the caller asks for them.
//! - `RelocationKind::Other(raw)` is passed through with the raw type
//!   code, which preserves it but doesn't validate the surrounding flag
//!   bits — fine for round-tripping content we read, may not be enough
//!   for synthesized relocations.

use crate::container::types::*;
use object::write::{
    Object as WriteObject, Relocation as WriteRelocation, Symbol as WriteSymbol,
    SymbolSection as WriteSymbolSection,
};
use object::{
    Architecture as ObjArch, BinaryFormat as ObjFormat, Endianness, RelocationFlags,
    SectionKind as ObjSectionKind, SymbolFlags, SymbolKind as ObjSymbolKind, SymbolScope,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ContainerWriteError {
    /// `architecture` is not one we know how to emit.
    UnsupportedArchitecture,
    /// A relocation kind has no representation in the target format.
    /// Mainly relevant for Mach-O, which lacks Branch19 / Branch14.
    UnsupportedRelocation {
        format: BinaryFormat,
        kind: RelocationKind,
    },
    /// A relocation references a symbol that isn't in the container's
    /// symbol table. Indicates a malformed input.
    DanglingSymbol { symbol_index: usize },
    /// `object::write` rejected the input.
    ObjectWrite(String),
}

impl std::fmt::Display for ContainerWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerWriteError::UnsupportedArchitecture => {
                write!(f, "container architecture is not supported by the writer")
            }
            ContainerWriteError::UnsupportedRelocation { format, kind } => write!(
                f,
                "relocation kind {kind:?} cannot be emitted for {format:?}"
            ),
            ContainerWriteError::DanglingSymbol { symbol_index } => write!(
                f,
                "relocation references symbol id {symbol_index}, which is not in the symbol table"
            ),
            ContainerWriteError::ObjectWrite(detail) => write!(f, "object::write error: {detail}"),
        }
    }
}

impl std::error::Error for ContainerWriteError {}

pub fn write(container: &Container) -> Result<Vec<u8>, ContainerWriteError> {
    let format = match container.format {
        BinaryFormat::Elf => ObjFormat::Elf,
        BinaryFormat::Macho => ObjFormat::MachO,
    };
    let architecture = match container.architecture {
        Architecture::Aarch64 => ObjArch::Aarch64,
        Architecture::Other => return Err(ContainerWriteError::UnsupportedArchitecture),
    };

    let mut obj = WriteObject::new(format, architecture, Endianness::Little);

    let section_map = add_sections(container, &mut obj);
    let symbol_map = add_symbols(container, &mut obj, &section_map);
    add_relocations(container, &mut obj, &section_map, &symbol_map)?;

    obj.write()
        .map_err(|err| ContainerWriteError::ObjectWrite(err.to_string()))
}

fn add_sections(
    container: &Container,
    obj: &mut WriteObject<'_>,
) -> HashMap<SectionId, object::write::SectionId> {
    let mut map = HashMap::new();
    for section in &container.sections {
        let segment = match container.format {
            BinaryFormat::Macho => macho_segment_for(section).to_vec(),
            BinaryFormat::Elf => Vec::new(),
        };
        let kind = section_kind_to_obj(section.kind);
        let obj_id = obj.add_section(segment, section.name.as_bytes().to_vec(), kind);
        // `Bss`-kind sections in object::write should be marked as having
        // no on-disk bytes; for our model that means an empty `bytes`
        // vector. `append_section_bss` reserves the size; otherwise, push
        // bytes directly.
        if matches!(section.kind, SectionKind::Bss) {
            obj.section_mut(obj_id).append_bss(section.size, 4);
        } else if !section.bytes.is_empty() {
            obj.append_section_data(obj_id, &section.bytes, 4);
        }
        map.insert(section.id, obj_id);
    }
    map
}

fn add_symbols(
    container: &Container,
    obj: &mut WriteObject<'_>,
    section_map: &HashMap<SectionId, object::write::SectionId>,
) -> HashMap<SymbolId, object::write::SymbolId> {
    let mut map = HashMap::new();
    for symbol in &container.symbols {
        let section = match (symbol.section, symbol.is_undefined) {
            (Some(section_id), false) => match section_map.get(&section_id) {
                Some(&obj_id) => WriteSymbolSection::Section(obj_id),
                None => WriteSymbolSection::Undefined,
            },
            _ => WriteSymbolSection::Undefined,
        };
        let scope = match symbol.binding {
            SymbolBinding::Local => SymbolScope::Compilation,
            SymbolBinding::Global => SymbolScope::Linkage,
            SymbolBinding::Weak => SymbolScope::Linkage,
            SymbolBinding::Unknown => SymbolScope::Unknown,
        };
        let obj_id = obj.add_symbol(WriteSymbol {
            name: symbol.name.as_bytes().to_vec(),
            value: symbol.address,
            size: symbol.size,
            kind: symbol_kind_to_obj(symbol.kind),
            scope,
            weak: symbol.binding == SymbolBinding::Weak,
            section,
            flags: SymbolFlags::None,
        });
        map.insert(symbol.id, obj_id);
    }
    map
}

fn add_relocations(
    container: &Container,
    obj: &mut WriteObject<'_>,
    section_map: &HashMap<SectionId, object::write::SectionId>,
    symbol_map: &HashMap<SymbolId, object::write::SymbolId>,
) -> Result<(), ContainerWriteError> {
    for relocation in &container.relocations {
        let Some(&section_id) = section_map.get(&relocation.section) else {
            continue;
        };
        let Some(symbol) = relocation.symbol else {
            // Section-relative relocations would need additional plumbing;
            // skip for now so the file is still valid.
            continue;
        };
        let &symbol_id = symbol_map.get(&symbol).ok_or(ContainerWriteError::DanglingSymbol {
            symbol_index: symbol.0,
        })?;
        let flags = relocation_flags(relocation.kind, container.format)?;
        obj.add_relocation(
            section_id,
            WriteRelocation {
                offset: relocation.offset,
                symbol: symbol_id,
                addend: relocation.addend,
                flags,
            },
        )
        .map_err(|err| ContainerWriteError::ObjectWrite(err.to_string()))?;
    }
    Ok(())
}

fn section_kind_to_obj(kind: SectionKind) -> ObjSectionKind {
    match kind {
        SectionKind::Text => ObjSectionKind::Text,
        SectionKind::Data => ObjSectionKind::Data,
        SectionKind::Rodata => ObjSectionKind::ReadOnlyData,
        SectionKind::Bss => ObjSectionKind::UninitializedData,
        SectionKind::Debug => ObjSectionKind::Debug,
        SectionKind::Other => ObjSectionKind::Other,
    }
}

fn symbol_kind_to_obj(kind: SymbolKind) -> ObjSymbolKind {
    match kind {
        SymbolKind::Function => ObjSymbolKind::Text,
        SymbolKind::Object => ObjSymbolKind::Data,
        SymbolKind::Section => ObjSymbolKind::Section,
        SymbolKind::File => ObjSymbolKind::File,
        SymbolKind::Unknown => ObjSymbolKind::Unknown,
    }
}

/// Best-effort segment placement for Mach-O. Real binaries have more
/// nuanced layout that we don't yet track in the neutral model.
fn macho_segment_for(section: &Section) -> &'static [u8] {
    match section.kind {
        SectionKind::Text => b"__TEXT",
        SectionKind::Data => b"__DATA",
        SectionKind::Rodata => b"__TEXT",
        SectionKind::Bss => b"__DATA",
        SectionKind::Debug => b"__DWARF",
        SectionKind::Other => {
            // Heuristic on the leading underscore convention.
            if section.name.starts_with("__debug_") {
                b"__DWARF"
            } else {
                b"__TEXT"
            }
        }
    }
}

fn relocation_flags(
    kind: RelocationKind,
    format: BinaryFormat,
) -> Result<RelocationFlags, ContainerWriteError> {
    match format {
        BinaryFormat::Elf => Ok(elf_flags(kind)),
        BinaryFormat::Macho => macho_flags(kind),
    }
}

fn elf_flags(kind: RelocationKind) -> RelocationFlags {
    use object::elf;
    let r_type = match kind {
        RelocationKind::Branch26 => elf::R_AARCH64_CALL26,
        RelocationKind::Branch19 => elf::R_AARCH64_CONDBR19,
        RelocationKind::Branch14 => elf::R_AARCH64_TSTBR14,
        RelocationKind::AdrpPage21 => elf::R_AARCH64_ADR_PREL_PG_HI21,
        RelocationKind::PageOffset12 => elf::R_AARCH64_ADD_ABS_LO12_NC,
        RelocationKind::Absolute => elf::R_AARCH64_ABS64,
        RelocationKind::Other(raw) => raw,
    };
    RelocationFlags::Elf { r_type }
}

fn macho_flags(kind: RelocationKind) -> Result<RelocationFlags, ContainerWriteError> {
    use object::macho;
    let (r_type, r_pcrel, r_length) = match kind {
        RelocationKind::Branch26 => (macho::ARM64_RELOC_BRANCH26, true, 2),
        RelocationKind::AdrpPage21 => (macho::ARM64_RELOC_PAGE21, true, 2),
        RelocationKind::PageOffset12 => (macho::ARM64_RELOC_PAGEOFF12, false, 2),
        RelocationKind::Absolute => (macho::ARM64_RELOC_UNSIGNED, false, 3),
        RelocationKind::Other(raw) => (raw as u8, false, 2),
        // Mach-O's standard ARM64 relocation set has no Branch19 /
        // Branch14: the assembler resolves these locally before emitting
        // the object file. If a caller asks for them, refuse rather than
        // pick something wrong.
        RelocationKind::Branch19 | RelocationKind::Branch14 => {
            return Err(ContainerWriteError::UnsupportedRelocation {
                format: BinaryFormat::Macho,
                kind,
            });
        }
    };
    Ok(RelocationFlags::MachO {
        r_type,
        r_pcrel,
        r_length,
    })
}
