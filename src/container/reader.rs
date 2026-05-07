//! Parse a Mach-O or ELF byte slice into the neutral [`Container`] model.
//!
//! Implementation notes:
//!
//! - The `object` crate's read API does the heavy lifting. We deliberately
//!   round-trip every section, symbol, and relocation through our own ID
//!   space so callers above this layer never see `object` types.
//! - Format-specific relocation type codes (`R_AARCH64_*`, `ARM64_RELOC_*`)
//!   are mapped onto a small neutral enum covering the AArch64 shapes the
//!   rewriter cares about. Anything outside that set is preserved as
//!   `RelocationKind::Other(raw)` so callers can still observe it.

use crate::container::types::*;
use object::read::{File, Object, ObjectSection, ObjectSymbol};
use object::{
    FileFlags as ObjFileFlags, RelocationFlags, RelocationTarget, SectionFlags as ObjSectionFlags,
    SymbolFlags as ObjSymbolFlags,
};
use std::collections::HashMap;

pub fn parse(bytes: &[u8]) -> Result<Container, ContainerError> {
    let file = File::parse(bytes).map_err(|err| ContainerError::Parse(err.to_string()))?;

    let format = match file.format() {
        object::BinaryFormat::MachO => BinaryFormat::Macho,
        object::BinaryFormat::Elf => BinaryFormat::Elf,
        _ => return Err(ContainerError::UnsupportedFormat),
    };

    let architecture = match file.architecture() {
        object::Architecture::Aarch64 => Architecture::Aarch64,
        other => Architecture::from_object(other),
    };
    // Don't error on unknown arch — the container is still inspectable;
    // analysis layers will refuse to disassemble.
    let _ = architecture;

    let (mut sections, section_index_to_id) = lift_sections(&file);
    populate_elf_raw_sh_type(bytes, &mut sections);
    let (symbols, symbol_index_to_id) = lift_symbols(&file, &section_index_to_id);
    let relocations = lift_relocations(&file, &section_index_to_id, &symbol_index_to_id);
    let dwarf = crate::container::dwarf::parse(&file);
    let file_flags = lift_file_flags(&file);

    Ok(Container {
        format,
        architecture,
        sections,
        symbols,
        relocations,
        file_flags,
        dwarf,
    })
}

/// Pull the raw `sh_type` for any section whose neutral `kind` is `Other`
/// from the underlying ELF section header. Skipped on non-ELF inputs.
///
/// We only need this for sections the writer would otherwise emit as
/// `SectionKind::Other` — for stock kinds (Text/Data/Rodata/etc.)
/// `object::write` already picks the right `sh_type`.
fn populate_elf_raw_sh_type(bytes: &[u8], sections: &mut [Section]) {
    use object::read::elf::{ElfFile64, SectionHeader};
    use object::Endianness;

    let Ok(elf) = ElfFile64::<Endianness>::parse(bytes) else {
        return;
    };
    let endian = elf.endian();
    // `elf_section_table().iter()` includes the null section at index 0,
    // which the neutral lift skips. Drop it so positional zip lines up.
    let table = elf.elf_section_table();
    for (section, header) in sections.iter_mut().zip(table.iter().skip(1)) {
        if matches!(section.kind, SectionKind::Other) {
            section.raw_sh_type = Some(header.sh_type(endian));
        }
    }
}

fn lift_file_flags(file: &File<'_>) -> Option<FileFlags> {
    match file.flags() {
        ObjFileFlags::Elf {
            os_abi,
            abi_version,
            e_flags,
        } => Some(FileFlags::Elf {
            os_abi,
            abi_version,
            e_flags,
        }),
        _ => None,
    }
}

fn lift_sections(
    file: &File<'_>,
) -> (Vec<Section>, HashMap<object::SectionIndex, SectionId>) {
    let mut sections = Vec::new();
    let mut section_index_to_id = HashMap::new();

    for section in file.sections() {
        let id = SectionId(sections.len());
        section_index_to_id.insert(section.index(), id);

        let name = section.name().unwrap_or("").to_string();
        let address = section.address();
        let size = section.size();
        let bytes = section
            .uncompressed_data()
            .map(|cow| cow.into_owned())
            .unwrap_or_default();
        let kind = map_section_kind(section.kind(), &name);
        let align = section.align();
        let flags = match section.flags() {
            ObjSectionFlags::Elf { sh_flags } => Some(SectionFlags::Elf { sh_flags }),
            _ => None,
        };

        sections.push(Section {
            id,
            name,
            address,
            size,
            bytes,
            kind,
            align,
            flags,
            // Populated below for ELF sections whose `kind` is `Other` —
            // those are the ones where stock `SectionKind` would lose the
            // raw `sh_type` on round-trip.
            raw_sh_type: None,
        });
    }

    (sections, section_index_to_id)
}

fn lift_symbols(
    file: &File<'_>,
    section_index_to_id: &HashMap<object::SectionIndex, SectionId>,
) -> (Vec<Symbol>, HashMap<object::SymbolIndex, SymbolId>) {
    let mut symbols = Vec::new();
    let mut symbol_index_to_id = HashMap::new();

    for symbol in file.symbols() {
        let id = SymbolId(symbols.len());
        symbol_index_to_id.insert(symbol.index(), id);

        let name = symbol.name().unwrap_or("").to_string();
        let address = symbol.address();
        let size = symbol.size();
        let kind = map_symbol_kind(symbol.kind());
        let binding = if symbol.is_weak() {
            SymbolBinding::Weak
        } else if symbol.is_global() {
            SymbolBinding::Global
        } else if symbol.is_local() {
            SymbolBinding::Local
        } else {
            SymbolBinding::Unknown
        };
        let section = match symbol.section() {
            object::SymbolSection::Section(index) => section_index_to_id.get(&index).copied(),
            _ => None,
        };
        let is_undefined = symbol.is_undefined();
        let flags = match symbol.flags() {
            ObjSymbolFlags::Elf { st_info, st_other } => {
                Some(SymbolExtraFlags::Elf { st_info, st_other })
            }
            _ => None,
        };

        symbols.push(Symbol {
            id,
            name,
            address,
            size,
            kind,
            binding,
            section,
            is_undefined,
            flags,
        });
    }

    (symbols, symbol_index_to_id)
}

fn lift_relocations(
    file: &File<'_>,
    section_index_to_id: &HashMap<object::SectionIndex, SectionId>,
    symbol_index_to_id: &HashMap<object::SymbolIndex, SymbolId>,
) -> Vec<Relocation> {
    let mut relocations = Vec::new();

    for section in file.sections() {
        let Some(&section_id) = section_index_to_id.get(&section.index()) else {
            continue;
        };
        for (offset, reloc) in section.relocations() {
            let kind = map_relocation_kind(&reloc);
            let symbol = match reloc.target() {
                RelocationTarget::Symbol(index) => symbol_index_to_id.get(&index).copied(),
                _ => None,
            };
            relocations.push(Relocation {
                id: RelocationId(relocations.len()),
                section: section_id,
                offset,
                kind,
                size: reloc.size(),
                addend: reloc.addend(),
                symbol,
            });
        }
    }

    relocations
}

fn map_section_kind(kind: object::SectionKind, name: &str) -> SectionKind {
    use object::SectionKind as Obj;
    match kind {
        Obj::Text => SectionKind::Text,
        Obj::Data => SectionKind::Data,
        Obj::ReadOnlyData | Obj::ReadOnlyString | Obj::ReadOnlyDataWithRel => SectionKind::Rodata,
        Obj::UninitializedData | Obj::Common => SectionKind::Bss,
        Obj::Debug | Obj::DebugString => SectionKind::Debug,
        _ => {
            // Some DWARF sections come through as Other depending on
            // format/version. Disambiguate by name.
            if name.starts_with(".debug_") || name.starts_with("__debug_") {
                SectionKind::Debug
            } else {
                SectionKind::Other
            }
        }
    }
}

fn map_symbol_kind(kind: object::SymbolKind) -> SymbolKind {
    use object::SymbolKind as Obj;
    match kind {
        Obj::Text => SymbolKind::Function,
        Obj::Data => SymbolKind::Object,
        Obj::Section => SymbolKind::Section,
        Obj::File => SymbolKind::File,
        _ => SymbolKind::Unknown,
    }
}

fn map_relocation_kind(reloc: &object::Relocation) -> RelocationKind {
    match reloc.flags() {
        RelocationFlags::Elf { r_type } => map_elf_relocation(r_type),
        RelocationFlags::MachO { r_type, .. } => map_macho_relocation(r_type),
        _ => match reloc.kind() {
            object::RelocationKind::Absolute => RelocationKind::Absolute,
            _ => RelocationKind::Other(0),
        },
    }
}

fn map_elf_relocation(r_type: u32) -> RelocationKind {
    use object::elf;
    match r_type {
        elf::R_AARCH64_CALL26 | elf::R_AARCH64_JUMP26 => RelocationKind::Branch26,
        elf::R_AARCH64_CONDBR19 => RelocationKind::Branch19,
        elf::R_AARCH64_TSTBR14 => RelocationKind::Branch14,
        elf::R_AARCH64_ADR_PREL_PG_HI21 => RelocationKind::AdrpPage21,
        elf::R_AARCH64_ADD_ABS_LO12_NC => RelocationKind::AddPageOffset12,
        elf::R_AARCH64_LDST8_ABS_LO12_NC => {
            RelocationKind::LoadStorePageOffset12 { access_width_bytes: 1 }
        }
        elf::R_AARCH64_LDST16_ABS_LO12_NC => {
            RelocationKind::LoadStorePageOffset12 { access_width_bytes: 2 }
        }
        elf::R_AARCH64_LDST32_ABS_LO12_NC => {
            RelocationKind::LoadStorePageOffset12 { access_width_bytes: 4 }
        }
        elf::R_AARCH64_LDST64_ABS_LO12_NC => {
            RelocationKind::LoadStorePageOffset12 { access_width_bytes: 8 }
        }
        elf::R_AARCH64_LDST128_ABS_LO12_NC => {
            RelocationKind::LoadStorePageOffset12 { access_width_bytes: 16 }
        }
        elf::R_AARCH64_ABS64 | elf::R_AARCH64_ABS32 => RelocationKind::Absolute,
        other => RelocationKind::Other(other),
    }
}

fn map_macho_relocation(r_type: u8) -> RelocationKind {
    use object::macho;
    match r_type {
        macho::ARM64_RELOC_UNSIGNED => RelocationKind::Absolute,
        macho::ARM64_RELOC_BRANCH26 => RelocationKind::Branch26,
        macho::ARM64_RELOC_PAGE21 | macho::ARM64_RELOC_GOT_LOAD_PAGE21 => {
            RelocationKind::AdrpPage21
        }
        macho::ARM64_RELOC_PAGEOFF12 | macho::ARM64_RELOC_GOT_LOAD_PAGEOFF12 => {
            // Mach-O uses one r_type for all lo-12 page-offset variants
            // and disambiguates the access width via `r_length`. We
            // collapse to the add-form here; cross-format work that
            // needs LDST disambiguation should look at the bytes at
            // `relocation.offset` to pick the right variant.
            RelocationKind::AddPageOffset12
        }
        other => RelocationKind::Other(other as u32),
    }
}

impl Architecture {
    fn from_object(arch: object::Architecture) -> Self {
        match arch {
            object::Architecture::Aarch64 => Architecture::Aarch64,
            _ => Architecture::Other,
        }
    }
}
