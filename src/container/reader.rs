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
    let kind = classify_container(&file, bytes);

    // ET_DYN / ET_EXEC ELF inputs carry program headers, .dynamic,
    // GNU hash, version sections, build-ID, .interp, and .eh_frame_hdr
    // — none of which the neutral types model. Lift them into an
    // ElfImage companion so the Stage 5 writer can reproduce them.
    let elf_image = match (format, kind) {
        (
            BinaryFormat::Elf,
            ContainerKind::SharedObject | ContainerKind::Executable,
        ) => lift_elf_image(bytes, &sections, &symbols),
        _ => None,
    };

    // For Mach-O ET_DYN-shaped inputs, snapshot the raw bytes
    // so the round-trip writer can emit them verbatim with
    // section overrides applied at original file offsets. We
    // don't need parsed load-command metadata until later
    // phases.
    let macho_image = match (format, kind) {
        (
            BinaryFormat::Macho,
            ContainerKind::SharedObject | ContainerKind::Executable,
        ) => {
            // Best-effort parse: a load-command parse failure
            // doesn't fail the whole read since the rest of the
            // container (sections, symbols) is still useful.
            // The writer paths that need parsed metadata
            // explicitly check for `Some(layout)` and fall back
            // appropriately.
            crate::container::macho_image::MachOImage::parse(bytes.to_vec()).ok()
        }
        _ => None,
    };

    Ok(Container {
        format,
        architecture,
        kind,
        sections,
        symbols,
        relocations,
        file_flags,
        elf_image,
        macho_image,
        dwarf,
    })
}

/// Capture every ELF-specific blob the neutral types don't model.
/// Returns `None` when the input doesn't parse as ELF64; falling back
/// to no-image is safer than failing the entire read, since the rest
/// of the container (sections, symbols) is still useful.
fn lift_elf_image(
    bytes: &[u8],
    sections: &[Section],
    symbols: &[Symbol],
) -> Option<crate::container::elf_image::ElfImage> {
    use crate::container::elf_image::{
        DynamicEntry, ElfImage, ProgramHeader, RawNoteSection, SectionLayout,
    };
    use object::elf;
    use object::read::elf::{
        Dyn, ElfFile64, FileHeader, NoteIterator, ProgramHeader as _, SectionHeader,
    };
    use object::Endianness;

    let elf = ElfFile64::<Endianness>::parse(bytes).ok()?;
    let endian = elf.endian();
    let mut image = ElfImage::new();

    // 0. File-header fields the neutral types don't carry.
    image.e_entry = elf.elf_header().e_entry(endian);

    // 1. Program headers — verbatim copy of every entry.
    for phdr in elf.elf_program_headers() {
        image.program_headers.push(ProgramHeader {
            p_type: phdr.p_type(endian),
            p_flags: phdr.p_flags(endian),
            p_offset: phdr.p_offset(endian),
            p_vaddr: phdr.p_vaddr(endian),
            p_paddr: phdr.p_paddr(endian),
            p_filesz: phdr.p_filesz(endian),
            p_memsz: phdr.p_memsz(endian),
            p_align: phdr.p_align(endian),
        });
    }

    // 2. Section ordering — the neutral list happens to follow source
    // order today, but record it explicitly so the writer can
    // reproduce by index without depending on that invariant.
    image.section_order = sections.iter().map(|s| s.id).collect();

    // 2b. Per-section ELF header fields the neutral model doesn't
    // carry: sh_offset (file offset), sh_link, sh_info, sh_entsize.
    // Stage 5's writer needs these to reproduce the input layout.
    image.section_layout = elf
        .elf_section_table()
        .iter()
        .skip(1) // null section excluded from neutral model
        .map(|hdr| SectionLayout {
            sh_offset: hdr.sh_offset(endian),
            sh_size: hdr.sh_size(endian),
            sh_link: hdr.sh_link(endian),
            sh_info: hdr.sh_info(endian),
            sh_entsize: hdr.sh_entsize(endian),
        })
        .collect();

    // 3. Segment-to-section mapping. For each PT_LOAD segment, find
    // every section whose source `offset` falls inside its
    // `[p_offset, p_offset + p_filesz)` range. Sections covered by
    // multiple segments (rare; the GNU_RELRO overlap) are reported
    // under each.
    let section_table = elf.elf_section_table();
    image.segment_sections = elf
        .elf_program_headers()
        .iter()
        .map(|phdr| {
            let p_offset = phdr.p_offset(endian);
            let p_end = p_offset + phdr.p_filesz(endian);
            sections
                .iter()
                .zip(section_table.iter().skip(1)) // skip null section
                .filter_map(|(neutral, raw)| {
                    let sh_offset = raw.sh_offset(endian);
                    let sh_size = raw.sh_size(endian);
                    if sh_size == 0 {
                        return None;
                    }
                    if sh_offset >= p_offset && sh_offset + sh_size <= p_end {
                        Some(neutral.id)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .collect();

    // 4. `.dynamic` — parsed tag/value list plus the section id so
    // the writer knows where it lives.
    if let Ok(Some((entries, _link))) = section_table.dynamic(endian, bytes) {
        for entry in entries {
            image.dynamic.push(DynamicEntry {
                tag: entry.d_tag(endian),
                value: entry.d_val(endian),
            });
        }
        // The .dynamic section's neutral id, located by name.
        image.dynamic_section = sections
            .iter()
            .find(|s| s.name == ".dynamic")
            .map(|s| s.id);
    }

    // 5. Raw passthrough sections — find each by name and snapshot
    // its bytes. Using name-based lookup is fine here because these
    // sections have well-known canonical names.
    // 4b. PLT stub addresses for extern dynsym entries. Walk
    // `.rela.plt` in source order — each entry's index N lines up
    // with the PLT stub at `.plt + plt_header + N * plt_entry_size`.
    // The neutral `.symbols` list is keyed by `.symtab` index, so
    // we match dynsym entries to our symbols by name (preferring
    // undefined ones to disambiguate name collisions). This is
    // load-bearing for the appended-code-calls-extern path: the
    // emit pass folds Target::Symbol(extern) to bl <stub_addr>.
    image.plt_stubs = build_plt_stub_map(&elf, sections, symbols);

    image.gnu_hash = raw_section_bytes(sections, ".gnu.hash");
    image.sysv_hash = raw_section_bytes(sections, ".hash");
    image.gnu_versym = raw_section_bytes(sections, ".gnu.version");
    image.gnu_verdef = raw_section_bytes(sections, ".gnu.version_d");
    image.gnu_verneed = raw_section_bytes(sections, ".gnu.version_r");
    image.eh_frame_hdr = raw_section_bytes(sections, ".eh_frame_hdr");
    image.dynsym = raw_section_bytes(sections, ".dynsym");
    image.dynstr = raw_section_bytes(sections, ".dynstr");

    // 6. `.interp` — store as a String for ergonomics; the writer
    // re-adds the trailing NUL.
    image.interp = sections
        .iter()
        .find(|s| s.name == ".interp")
        .and_then(|s| {
            let trimmed = s
                .bytes
                .split(|&b| b == 0)
                .next()
                .unwrap_or(&[]);
            std::str::from_utf8(trimmed).ok().map(|s| s.to_string())
        });

    // 7. Build-ID note + other notes. Walk every SHT_NOTE section's
    // entries; route the `GNU` `NT_GNU_BUILD_ID` note to `build_id`
    // and everything else to `other_notes` keyed by source section.
    for header in section_table.iter().skip(1) {
        if header.sh_type(endian) != elf::SHT_NOTE {
            continue;
        }
        let Ok(data) = header.data(endian, bytes) else {
            continue;
        };
        let Ok(name_bytes) = section_table.section_name(endian, header) else {
            continue;
        };
        let Ok(section_name) = std::str::from_utf8(name_bytes) else {
            continue;
        };
        // Locate the matching neutral SectionId so other_notes can
        // be re-attached.
        let Some(neutral) = sections.iter().find(|s| s.name == section_name) else {
            continue;
        };
        let Ok(mut iter) = NoteIterator::<elf::FileHeader64<Endianness>>::new(
            endian,
            header.sh_addralign(endian),
            data,
        ) else {
            continue;
        };

        let mut consumed_as_build_id = false;
        while let Ok(Some(note)) = iter.next() {
            if note.name() == elf::ELF_NOTE_GNU
                && note.n_type(endian) == elf::NT_GNU_BUILD_ID
            {
                image.build_id = Some(note.desc().to_vec());
                consumed_as_build_id = true;
            }
        }
        if !consumed_as_build_id {
            image.other_notes.push(RawNoteSection {
                section: neutral.id,
                name: section_name.to_string(),
                bytes: data.to_vec(),
            });
        }
    }

    Some(image)
}

/// Snapshot the bytes of a named section, returning None when the
/// section isn't present. The neutral `Section.bytes` is already a
/// `Vec<u8>` we can clone — this is just a typed wrapper around that
/// lookup.
fn raw_section_bytes(
    sections: &[Section],
    name: &str,
) -> Option<crate::container::elf_image::RawSectionBytes> {
    use crate::container::elf_image::RawSectionBytes;
    sections.iter().find(|s| s.name == name).map(|s| RawSectionBytes {
        section: s.id,
        bytes: s.bytes.clone(),
    })
}

/// Walk `.rela.plt` and compute the PLT stub address for each
/// extern dynsym entry it references. Returns a map keyed by the
/// neutral [`SymbolId`] (from the static `.symtab`-derived
/// `Container.symbols` list).
///
/// AArch64's standard PLT layout per the ELF psABI:
/// - 32-byte header (PLT0 — lazy resolver trampoline).
/// - 16-byte stubs, one per `.rela.plt` entry, in source order.
///
/// The mapping from a `.rela.plt` index `N` to its stub address is
/// therefore `plt.address + 32 + N * 16`. We only handle this
/// standard layout; non-conforming inputs (e.g. PLT variants used
/// by some GNU IFUNC schemes) get an empty map and the rewriter
/// falls back to its existing relocation paths.
fn build_plt_stub_map(
    elf: &object::read::elf::ElfFile64<object::Endianness>,
    sections: &[Section],
    symbols: &[Symbol],
) -> std::collections::HashMap<crate::container::SymbolId, u64> {
    use object::elf;
    use object::read::elf::Sym;
    use std::collections::HashMap;

    let mut map = HashMap::new();
    let endian = elf.endian();

    let Some(plt) = sections.iter().find(|s| s.name == ".plt") else {
        return map;
    };
    const PLT_HEADER_SIZE: u64 = 32;
    const PLT_ENTRY_SIZE: u64 = 16;

    let Some(rela_plt) = sections.iter().find(|s| s.name == ".rela.plt") else {
        return map;
    };
    const RELA_ENTRY_SIZE: usize = 24; // sizeof Elf64_Rela
    if rela_plt.bytes.len() % RELA_ENTRY_SIZE != 0 {
        return map;
    }

    let dynsym = elf.elf_dynamic_symbol_table();

    let entries = rela_plt.bytes.len() / RELA_ENTRY_SIZE;
    for index in 0..entries {
        let off = index * RELA_ENTRY_SIZE;
        // r_offset = bytes[off..off+8], r_info = bytes[off+8..off+16].
        let r_info = u64::from_le_bytes(
            rela_plt.bytes[off + 8..off + 16].try_into().unwrap(),
        );
        let r_sym = (r_info >> 32) as u32;
        let r_type = (r_info & 0xffff_ffff) as u32;
        if r_type != elf::R_AARCH64_JUMP_SLOT {
            continue;
        }

        // Resolve the dynsym entry's name.
        let dyn_table = dynsym;
        let dyn_symbol = match dyn_table.symbol(object::read::SymbolIndex(r_sym as usize)) {
            Ok(sym) => sym,
            Err(_) => continue,
        };
        let Ok(name_bytes) = dyn_symbol.name(endian, dyn_table.strings()) else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };

        // Match by name in the static `.symtab`-derived neutral
        // list. Prefer the undefined entry (the dynamic import).
        // The dynsym typically stores the bare name (`puts`)
        // while static `.symtab` may carry the versioned form
        // (`puts@GLIBC_2.17`). Compare base names on both sides
        // so either spelling matches.
        let dynsym_base = name.split_once('@').map(|(b, _)| b).unwrap_or(name);
        let neutral_id = symbols
            .iter()
            .find(|s| {
                if !s.is_undefined {
                    return false;
                }
                let static_base = s
                    .name
                    .split_once('@')
                    .map(|(b, _)| b)
                    .unwrap_or(s.name.as_str());
                static_base == dynsym_base
            })
            .map(|s| s.id);

        let Some(neutral_id) = neutral_id else {
            continue;
        };

        let stub_addr = plt.address + PLT_HEADER_SIZE + index as u64 * PLT_ENTRY_SIZE;
        map.insert(neutral_id, stub_addr);
    }

    map
}

/// Classify a parsed file into the high-level [`ContainerKind`] taxonomy
/// the writer uses to gate ET_DYN / ET_EXEC paths. We pull the raw
/// header for ELF inputs because `object::read::Object` only exposes
/// the relocatable / executable distinction indirectly via
/// `is_executable`, and we want all four buckets explicit.
fn classify_container(file: &File<'_>, bytes: &[u8]) -> crate::container::ContainerKind {
    use crate::container::ContainerKind;
    match file.format() {
        object::BinaryFormat::Elf => classify_elf(bytes),
        object::BinaryFormat::MachO => classify_macho(bytes),
        _ => ContainerKind::Other,
    }
}

fn classify_elf(bytes: &[u8]) -> crate::container::ContainerKind {
    use crate::container::ContainerKind;
    use object::read::elf::{ElfFile64, FileHeader as _};
    use object::Endianness;

    let Ok(elf) = ElfFile64::<Endianness>::parse(bytes) else {
        return ContainerKind::Other;
    };
    let endian = elf.endian();
    let header = elf.elf_header();
    match header.e_type(endian) {
        object::elf::ET_REL => ContainerKind::Relocatable,
        object::elf::ET_DYN => ContainerKind::SharedObject,
        object::elf::ET_EXEC => ContainerKind::Executable,
        _ => ContainerKind::Other,
    }
}

fn classify_macho(bytes: &[u8]) -> crate::container::ContainerKind {
    use crate::container::ContainerKind;
    use object::macho;

    if bytes.len() < 8 {
        return ContainerKind::Other;
    }
    // Mach-O header layout: magic (4 bytes), cputype (4), cpusubtype
    // (4), filetype (4), ... `filetype` lives at offset 12. Both LE
    // and BE 64-bit headers have it at the same offset.
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let is_64 = magic == macho::MH_MAGIC_64 || magic == macho::MH_CIGAM_64;
    if !is_64 || bytes.len() < 16 {
        return ContainerKind::Other;
    }
    // Endianness for the filetype field follows the magic. We currently
    // only emit and inspect little-endian Mach-O.
    let filetype = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    match filetype {
        macho::MH_OBJECT => ContainerKind::Relocatable,
        macho::MH_DYLIB => ContainerKind::SharedObject,
        macho::MH_EXECUTE => ContainerKind::Executable,
        _ => ContainerKind::Other,
    }
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
