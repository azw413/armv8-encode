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
    Architecture as ObjArch, BinaryFormat as ObjFormat, Endianness,
    FileFlags as ObjFileFlags, RelocationFlags, SectionFlags as ObjSectionFlags,
    SectionKind as ObjSectionKind, SymbolFlags as ObjSymbolFlags,
    SymbolKind as ObjSymbolKind, SymbolScope,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ContainerWriteError {
    /// `architecture` is not one we know how to emit.
    UnsupportedArchitecture,
    /// `kind` is something the writer can't emit yet — today only
    /// [`ContainerKind::Relocatable`] is supported. Shared libraries
    /// and executables (ET_DYN / ET_EXEC, MH_DYLIB / MH_EXECUTE) need
    /// the lower-level `object::write::elf::Writer` path that lands
    /// in Stage 5.
    UnsupportedKind {
        kind: crate::container::ContainerKind,
    },
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
    /// Caller asked for an ELF ET_DYN / ET_EXEC write but the
    /// container has no attached [`super::ElfImage`]. The reader
    /// populates it for non-relocatable ELF inputs; missing here
    /// indicates a reader bug or a hand-built container that lacks
    /// the necessary metadata.
    ElfImageMissing,
    /// A rewrite grew an ELF section past its source extent. The
    /// in-place writer can't accommodate this; an
    /// append-PT_LOAD-with-trampolines layout is needed. The first
    /// iteration of that path (Stage 6.3) handles only inputs
    /// without a PT_PHDR program header — see
    /// [`PtPhdrNotSupported`] for that limit. This variant
    /// preserved for the case where the grow-detection logic
    /// itself rejects an input.
    ElfTextGrewBeyondExtent {
        section_name: String,
        original_extent: u64,
        new_size: u64,
    },
    /// The append-PT_LOAD path doesn't yet handle inputs with a
    /// PT_PHDR program header, because relocating the program
    /// header table to file end requires either removing or
    /// rewriting that segment. Inputs without PT_PHDR (most
    /// shared libraries) work fine.
    PtPhdrNotSupported,
}

impl std::fmt::Display for ContainerWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerWriteError::UnsupportedArchitecture => {
                write!(f, "container architecture is not supported by the writer")
            }
            ContainerWriteError::UnsupportedKind { kind } => write!(
                f,
                "container kind {kind:?} is not yet supported by the writer; \
                 only Relocatable (ET_REL / MH_OBJECT) inputs can round-trip \
                 today (ET_DYN / ET_EXEC support is Stage 5)",
            ),
            ContainerWriteError::UnsupportedRelocation { format, kind } => write!(
                f,
                "relocation kind {kind:?} cannot be emitted for {format:?}"
            ),
            ContainerWriteError::DanglingSymbol { symbol_index } => write!(
                f,
                "relocation references symbol id {symbol_index}, which is not in the symbol table"
            ),
            ContainerWriteError::ObjectWrite(detail) => write!(f, "object::write error: {detail}"),
            ContainerWriteError::ElfImageMissing => write!(
                f,
                "container kind requires an ElfImage companion but none is attached",
            ),
            ContainerWriteError::ElfTextGrewBeyondExtent {
                section_name,
                original_extent,
                new_size,
            } => write!(
                f,
                "section {section_name:?} grew from {original_extent} to {new_size} \
                 bytes; the in-place writer can't accommodate this — append-PT_LOAD \
                 layout (Stage 6.3) handles this for non-PT_PHDR inputs",
            ),
            ContainerWriteError::PtPhdrNotSupported => write!(
                f,
                "input has a PT_PHDR program header; the Stage 6.3 \
                 append-PT_LOAD writer doesn't yet relocate it",
            ),
        }
    }
}

impl std::error::Error for ContainerWriteError {}

pub fn write(container: &Container) -> Result<Vec<u8>, ContainerWriteError> {
    use crate::container::ContainerKind;
    // Fail fast on inputs the writer can't handle. `object::write::Object`
    // is hard-wired to ET_REL on the ELF side and MH_OBJECT on the
    // Mach-O side; passing an ET_DYN or ET_EXEC input today would
    // silently re-emit it as a relocatable object — wrong shape, wrong
    // section layout, broken at load time. Surface that explicitly so
    // callers can detect the limit and route around it (or wait for
    // Stage 5).
    if !matches!(container.kind, ContainerKind::Relocatable) {
        return Err(ContainerWriteError::UnsupportedKind { kind: container.kind });
    }

    let format = match container.format {
        BinaryFormat::Elf => ObjFormat::Elf,
        BinaryFormat::Macho => ObjFormat::MachO,
    };
    let architecture = match container.architecture {
        Architecture::Aarch64 => ObjArch::Aarch64,
        Architecture::Other => return Err(ContainerWriteError::UnsupportedArchitecture),
    };

    let mut obj = WriteObject::new(format, architecture, Endianness::Little);

    if let Some(file_flags) = container.file_flags {
        obj.flags = match file_flags {
            FileFlags::Elf {
                os_abi,
                abi_version,
                e_flags,
            } => ObjFileFlags::Elf {
                os_abi,
                abi_version,
                e_flags,
            },
        };
    }

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
        // Skip sections `object::write` regenerates from the symbol and
        // relocation tables we hand it: the symbol/string tables and
        // section-name table for ELF, plus the relocation sections per
        // text/data section. Re-adding them would produce a malformed
        // output (duplicate section names, mismatched sh_link).
        if container.format == BinaryFormat::Elf && is_writer_managed_elf_section(section) {
            continue;
        }
        let segment = match container.format {
            BinaryFormat::Macho => macho_segment_for(section).to_vec(),
            BinaryFormat::Elf => Vec::new(),
        };
        // Pick the kind passed to `add_section`. For ELF inputs whose
        // neutral kind is `Other` and we captured a raw `sh_type`, fall
        // through to `SectionKind::Elf(raw)` so the writer emits the
        // exact section type. Otherwise stock kinds suffice.
        let kind = match (container.format, section.kind, section.raw_sh_type) {
            (BinaryFormat::Elf, SectionKind::Other, Some(raw)) => ObjSectionKind::Elf(raw),
            _ => section_kind_to_obj(section.kind),
        };
        let obj_id = obj.add_section(segment, section.name.as_bytes().to_vec(), kind);
        // Alignment passed to `append_section_data` becomes `sh_addralign`
        // for ELF and the section alignment for Mach-O. Default to 4 (the
        // historic default) when the section didn't carry one.
        let align = if section.align == 0 { 4 } else { section.align };
        // `Bss`-kind sections in object::write should be marked as having
        // no on-disk bytes; for our model that means an empty `bytes`
        // vector. `append_section_bss` reserves the size; otherwise, push
        // bytes directly.
        if matches!(section.kind, SectionKind::Bss) {
            obj.section_mut(obj_id).append_bss(section.size, align);
        } else if !section.bytes.is_empty() {
            obj.append_section_data(obj_id, &section.bytes, align);
        }
        // Apply raw section flags last, after the section is fully formed
        // — `add_section` may pick defaults from `kind` and we want the
        // source's `sh_flags` to win. Mach-O carries flag bits we don't
        // model yet; pass through as-is when present.
        if let Some(flags) = section.flags {
            let obj_flags = match flags {
                SectionFlags::Elf { sh_flags } => ObjSectionFlags::Elf { sh_flags },
            };
            obj.section_mut(obj_id).flags = obj_flags;
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
        let flags = match symbol.flags {
            Some(SymbolExtraFlags::Elf { st_info, st_other }) => {
                ObjSymbolFlags::Elf { st_info, st_other }
            }
            None => ObjSymbolFlags::None,
        };
        let obj_id = obj.add_symbol(WriteSymbol {
            name: symbol.name.as_bytes().to_vec(),
            value: symbol.address,
            size: symbol.size,
            kind: symbol_kind_to_obj(symbol.kind),
            scope,
            weak: symbol.binding == SymbolBinding::Weak,
            section,
            flags,
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
    // Cache for synthesized section symbols. ELF requires every relocation
    // to point at *some* symbol; section-relative relocations conventionally
    // target a synthetic STT_SECTION symbol bound to the destination
    // section. `object::write` exposes `Object::section_symbol(section_id)`
    // to materialize one on demand and reuse it across relocations.
    let mut section_symbol_cache: HashMap<object::write::SectionId, object::write::SymbolId> =
        HashMap::new();

    for relocation in &container.relocations {
        let Some(&section_id) = section_map.get(&relocation.section) else {
            continue;
        };
        let symbol_id = match relocation.symbol {
            Some(symbol) => *symbol_map.get(&symbol).ok_or(
                ContainerWriteError::DanglingSymbol {
                    symbol_index: symbol.0,
                },
            )?,
            None => {
                // Section-relative relocation. Pick a target section: prefer
                // the relocation's `addend` interpreted as an offset into a
                // known section, else fall back to the relocation's own
                // section (rare, but valid for self-referential fixups).
                // Without more context the best generic choice is the
                // relocation's own section — that's where stripped or
                // section-only relocations point in practice.
                *section_symbol_cache
                    .entry(section_id)
                    .or_insert_with(|| obj.section_symbol(section_id))
            }
        };
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

/// True when an ELF section is one `object::write::Object` regenerates
/// itself, so re-adding it from the parsed container would duplicate it.
/// Covers the symbol/string tables, the section-name table, the symbol
/// shndx-extension table, and per-section relocation sub-sections.
fn is_writer_managed_elf_section(section: &Section) -> bool {
    use object::elf;
    if let Some(sh_type) = section.raw_sh_type {
        if matches!(
            sh_type,
            elf::SHT_SYMTAB
                | elf::SHT_STRTAB
                | elf::SHT_SYMTAB_SHNDX
                | elf::SHT_REL
                | elf::SHT_RELA
        ) {
            return true;
        }
    }
    // The `.shstrtab` and `.strtab` are SHT_STRTAB so the check above
    // catches them; `.symtab` is SHT_SYMTAB. Fall back to name-based
    // matching for inputs that didn't carry a raw `sh_type` (i.e. were
    // mapped to a stock kind that obscured it).
    matches!(
        section.name.as_str(),
        ".symtab" | ".strtab" | ".shstrtab" | ".rela.text" | ".rel.text"
    ) || section.name.starts_with(".rela.")
        || section.name.starts_with(".rel.")
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
        RelocationKind::AddPageOffset12 => elf::R_AARCH64_ADD_ABS_LO12_NC,
        RelocationKind::LoadStorePageOffset12 { access_width_bytes } => match access_width_bytes {
            1 => elf::R_AARCH64_LDST8_ABS_LO12_NC,
            2 => elf::R_AARCH64_LDST16_ABS_LO12_NC,
            4 => elf::R_AARCH64_LDST32_ABS_LO12_NC,
            8 => elf::R_AARCH64_LDST64_ABS_LO12_NC,
            16 => elf::R_AARCH64_LDST128_ABS_LO12_NC,
            // Unknown widths fall back to the closest available
            // variant. In practice the reader only constructs this
            // variant from the five widths above.
            _ => elf::R_AARCH64_LDST32_ABS_LO12_NC,
        },
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
        RelocationKind::AddPageOffset12 => (macho::ARM64_RELOC_PAGEOFF12, false, 2),
        // Mach-O encodes the access width via `r_length`, not the
        // r_type. log2(width_bytes) gives the right field for the
        // standard 1/2/4/8-byte widths we model.
        RelocationKind::LoadStorePageOffset12 { access_width_bytes } => {
            let r_length = match access_width_bytes {
                1 => 0,
                2 => 1,
                4 => 2,
                8 => 3,
                _ => 2,
            };
            (macho::ARM64_RELOC_PAGEOFF12, false, r_length)
        }
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
