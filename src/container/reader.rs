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
        object::BinaryFormat::Pe | object::BinaryFormat::Coff => BinaryFormat::Pe,
        _ => return Err(ContainerError::UnsupportedFormat),
    };

    let architecture = match file.architecture() {
        object::Architecture::Aarch64 => Architecture::Aarch64,
        object::Architecture::Arm => Architecture::Arm,
        other => Architecture::from_object(other),
    };
    // Don't error on unknown arch — the container is still inspectable;
    // analysis layers will refuse to disassemble.

    let (mut sections, section_index_to_id) = lift_sections(&file);
    populate_elf_raw_sh_type(bytes, &mut sections);
    let (symbols, symbol_index_to_id) = lift_symbols(&file, &section_index_to_id);
    let relocations =
        lift_relocations(&file, &section_index_to_id, &symbol_index_to_id, architecture);
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
            let mut image =
                crate::container::macho_image::MachOImage::parse(bytes.to_vec()).ok();
            if let Some(img) = image.as_mut() {
                populate_macho_stubs(img, &symbol_index_to_id);
                reclassify_macho_text_sections(&img.layout, &mut sections);
            }
            image
        }
        _ => None,
    };

    // For linked PE inputs, snapshot the raw bytes plus each section's
    // on-disk placement so the PE writer can round-trip the image and
    // apply in-place section overrides at their original file offsets.
    let pe_image = match (format, kind) {
        (
            BinaryFormat::Pe,
            ContainerKind::SharedObject | ContainerKind::Executable,
        ) => Some(lift_pe_image(&file, bytes)),
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
        pe_image,
        dwarf,
    })
}

/// Capture a linked PE image for round-trip + in-place section
/// overrides: the raw bytes plus each section's on-disk placement
/// (`PointerToRawData` / `SizeOfRawData`), read straight off
/// `object`'s `file_range()`.
fn lift_pe_image(file: &File<'_>, bytes: &[u8]) -> crate::container::pe_image::PeImage {
    use crate::container::pe_image::{PeImage, PeSectionFile};
    let sections = file
        .sections()
        .filter_map(|section| {
            let name = section.name().ok()?.to_string();
            let (file_offset, file_size) = section.file_range()?;
            Some(PeSectionFile {
                name,
                file_offset,
                file_size,
            })
        })
        .collect();
    PeImage {
        raw_bytes: bytes.to_vec(),
        sections,
    }
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
    use object::read::elf::{ElfFile32, ElfFile64};
    use object::Endianness;

    // Try ELF64; fall back to ELF32. The body is generic
    // over the FileHeader trait so the same routine
    // populates the full image for either width.
    if let Ok(elf) = ElfFile64::<Endianness>::parse(bytes) {
        return lift_elf_image_typed(bytes, sections, symbols, &elf);
    }
    if let Ok(elf) = ElfFile32::<Endianness>::parse(bytes) {
        return lift_elf_image_typed(bytes, sections, symbols, &elf);
    }
    None
}

fn lift_elf_image_typed<'data, Elf>(
    bytes: &'data [u8],
    sections: &[Section],
    symbols: &[Symbol],
    elf: &object::read::elf::ElfFile<'data, Elf, &'data [u8]>,
) -> Option<crate::container::elf_image::ElfImage>
where
    Elf: object::read::elf::FileHeader<Endian = object::Endianness>,
{
    use crate::container::elf_image::{
        DynamicEntry, ElfImage, ProgramHeader, RawNoteSection, SectionLayout,
    };
    use object::elf;
    use object::read::elf::{
        Dyn, FileHeader, NoteIterator, ProgramHeader as _, SectionHeader,
    };
    use object::Endianness;

    let endian = elf.endian();
    let mut image = ElfImage::new();

    // 0. File-header fields the neutral types don't carry.
    image.e_entry = elf.elf_header().e_entry(endian).into();

    // 1. Program headers — verbatim copy of every entry.
    // ELF32 fields widen to u64 via the Word: Into<u64> bound.
    for phdr in elf.elf_program_headers() {
        image.program_headers.push(ProgramHeader {
            p_type: phdr.p_type(endian),
            p_flags: phdr.p_flags(endian),
            p_offset: phdr.p_offset(endian).into(),
            p_vaddr: phdr.p_vaddr(endian).into(),
            p_paddr: phdr.p_paddr(endian).into(),
            p_filesz: phdr.p_filesz(endian).into(),
            p_memsz: phdr.p_memsz(endian).into(),
            p_align: phdr.p_align(endian).into(),
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
            sh_offset: hdr.sh_offset(endian).into(),
            sh_size: hdr.sh_size(endian).into(),
            sh_link: hdr.sh_link(endian),
            sh_info: hdr.sh_info(endian),
            sh_entsize: hdr.sh_entsize(endian).into(),
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
            let p_offset: u64 = phdr.p_offset(endian).into();
            let p_filesz: u64 = phdr.p_filesz(endian).into();
            let p_end = p_offset + p_filesz;
            sections
                .iter()
                .zip(section_table.iter().skip(1)) // skip null section
                .filter_map(|(neutral, raw)| {
                    let sh_offset: u64 = raw.sh_offset(endian).into();
                    let sh_size: u64 = raw.sh_size(endian).into();
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
                tag: entry.d_tag(endian).into(),
                value: entry.d_val(endian).into(),
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
    // PLT stub layout differs between AArch64 and ARM. Both
    // walk `.rela.plt` (or `.rel.plt` on ARM ELF32) in source
    // order, but the header / entry sizes and the
    // SymbolIndex extraction from `r_info` are different.
    // Dispatch via the generic ELF type's 64-bitness: 64-bit
    // = AArch64 layout, 32-bit = ARM layout.
    image.plt_stubs = if Elf::is_type_64_sized() {
        // SAFETY-equivalent: re-parse as ELF64 since the
        // existing helper expects that concrete type.
        if let Ok(elf64) = object::read::elf::ElfFile64::<Endianness>::parse(bytes) {
            build_plt_stub_map(&elf64, sections, symbols)
        } else {
            std::collections::HashMap::new()
        }
    } else if let Ok(elf32) = object::read::elf::ElfFile32::<Endianness>::parse(bytes) {
        build_arm_plt_stub_map(&elf32, sections, symbols)
    } else {
        std::collections::HashMap::new()
    };

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
        let Ok(mut iter) = NoteIterator::<Elf>::new(
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
/// ARMv7 PLT stub layout: 20-byte header followed by 12-byte
/// entries. The Nth `.rel.plt` entry corresponds to the Nth
/// stub at `.plt + 20 + N*12`. Each `.rel.plt` entry is 8
/// bytes (`Elf32_Rel`: r_offset + r_info; no addend), with
/// the symbol index in the high 24 bits of `r_info`.
fn build_arm_plt_stub_map(
    elf: &object::read::elf::ElfFile32<object::Endianness>,
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
    const PLT_HEADER_SIZE: u64 = 20;
    const PLT_ENTRY_SIZE: u64 = 12;

    // ARMv7 uses REL (no addend), so the relocation table is
    // `.rel.plt` not `.rela.plt`.
    let Some(rel_plt) = sections.iter().find(|s| s.name == ".rel.plt") else {
        return map;
    };
    const REL_ENTRY_SIZE: usize = 8; // sizeof Elf32_Rel
    if rel_plt.bytes.len() % REL_ENTRY_SIZE != 0 {
        return map;
    }

    let dyn_table = elf.elf_dynamic_symbol_table();

    let entries = rel_plt.bytes.len() / REL_ENTRY_SIZE;
    for index in 0..entries {
        let off = index * REL_ENTRY_SIZE;
        let r_info = u32::from_le_bytes(
            rel_plt.bytes[off + 4..off + 8].try_into().unwrap(),
        );
        let r_sym = r_info >> 8;
        let r_type = (r_info & 0xff) as u32;
        if r_type != elf::R_ARM_JUMP_SLOT {
            continue;
        }
        let dyn_symbol =
            match dyn_table.symbol(object::read::SymbolIndex(r_sym as usize)) {
                Ok(sym) => sym,
                Err(_) => continue,
            };
        let Ok(name_bytes) = dyn_symbol.name(endian, dyn_table.strings()) else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };
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

/// Populate `MachOImage::stubs` and `MachOImage::import_pointers`
/// from the indirect symbol table. Each entry returned by
/// `MachOLayout::stub_entries` / `import_pointer_entries` carries a
/// `LC_SYMTAB` index; we translate that to a neutral `SymbolId`
/// using the same mapping `lift_symbols` built from
/// `object::SymbolIndex`. Entries whose symbol index doesn't
/// resolve are silently dropped — a malformed indirect table
/// shouldn't break disassembly of the rest of the image.
fn populate_macho_stubs(
    image: &mut crate::container::macho_image::MachOImage,
    symbol_index_to_id: &HashMap<object::SymbolIndex, SymbolId>,
) {
    // Borrow image fields disjointly: read from `layout` while
    // writing the destination maps. We can't call `&self`-style
    // methods on layout through `&mut image`, so snapshot the bytes
    // separately first.
    let bytes = image.raw_bytes.clone();
    for entry in image.layout.stub_entries(&bytes) {
        let neutral = symbol_index_to_id
            .get(&object::SymbolIndex(entry.symtab_index as usize));
        if let Some(&id) = neutral {
            image.stubs.insert(id, entry.address);
        }
    }
    for entry in image.layout.import_pointer_entries(&bytes) {
        let neutral = symbol_index_to_id
            .get(&object::SymbolIndex(entry.symtab_index as usize));
        if let Some(&id) = neutral {
            image.import_pointers.insert(id, entry.address);
        }
    }
}

/// `object` classifies Mach-O `__stubs` (and other `S_SYMBOL_STUBS`
/// sections) as `Other` because the load command doesn't tag them as
/// executable in the way `object`'s neutral kind enum recognises.
/// They're code though — three `adrp`/`ldr`/`br` instructions per
/// 12-byte stub — so reclassify them as `Text` after the Mach-O image
/// is parsed and we can see the section-type byte. Same treatment
/// for the rarer `__auth_stubs` (PAC-signed variant).
///
/// We match on flags rather than name so non-standard stub section
/// names still get routed to the disassembler.
fn reclassify_macho_text_sections(
    layout: &crate::container::macho_image::MachOLayout,
    sections: &mut [Section],
) {
    use crate::container::macho_image::{S_SYMBOL_STUBS, SECTION_TYPE_MASK};
    for macho_sec in &layout.sections {
        if macho_sec.flags & SECTION_TYPE_MASK != S_SYMBOL_STUBS {
            continue;
        }
        // Match the neutral section by (segname, sectname). Mach-O
        // section names aren't unique across segments — e.g. both
        // `__TEXT,__const` and `__DATA,__const` exist — but for our
        // purposes the neutral list uses just the sectname, which is
        // unique within `__TEXT` for stub sections.
        if let Some(section) =
            sections.iter_mut().find(|s| s.name == macho_sec.sectname)
        {
            section.kind = SectionKind::Text;
        }
    }
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
        object::BinaryFormat::Pe | object::BinaryFormat::Coff => classify_pe(file),
        _ => ContainerKind::Other,
    }
}

/// Classify a PE/COFF input. COFF objects (`.obj`) are relocatable;
/// linked PE images are an executable or a DLL (shared object). We
/// read this off `object`'s already-parsed `ObjectKind` rather than
/// re-parsing the headers.
fn classify_pe(file: &File<'_>) -> crate::container::ContainerKind {
    use crate::container::ContainerKind;
    match file.kind() {
        object::ObjectKind::Relocatable => ContainerKind::Relocatable,
        object::ObjectKind::Dynamic => ContainerKind::SharedObject,
        object::ObjectKind::Executable => ContainerKind::Executable,
        _ => ContainerKind::Other,
    }
}

fn classify_elf(bytes: &[u8]) -> crate::container::ContainerKind {
    use crate::container::ContainerKind;
    use object::read::elf::{ElfFile32, ElfFile64, FileHeader as _};
    use object::Endianness;

    // Try ELF64 first (the original implementation). Fall
    // back to ELF32 for ARMv7 / x86 / RISC-V32 inputs.
    let e_type = if let Ok(elf) = ElfFile64::<Endianness>::parse(bytes) {
        let endian = elf.endian();
        elf.elf_header().e_type(endian)
    } else if let Ok(elf) = ElfFile32::<Endianness>::parse(bytes) {
        let endian = elf.endian();
        elf.elf_header().e_type(endian)
    } else {
        return ContainerKind::Other;
    };
    match e_type {
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
    use object::read::elf::{ElfFile32, ElfFile64};
    use object::Endianness;

    if let Ok(elf) = ElfFile64::<Endianness>::parse(bytes) {
        populate_elf_raw_sh_type_typed(&elf, sections);
        return;
    }
    if let Ok(elf) = ElfFile32::<Endianness>::parse(bytes) {
        populate_elf_raw_sh_type_typed(&elf, sections);
    }
}

fn populate_elf_raw_sh_type_typed<'data, Elf>(
    elf: &object::read::elf::ElfFile<'data, Elf, &'data [u8]>,
    sections: &mut [Section],
) where
    Elf: object::read::elf::FileHeader<Endian = object::Endianness>,
{
    use object::read::elf::SectionHeader;
    let endian = elf.endian();
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

    // Static symbol table (`.symtab` on ELF). Empty for
    // stripped binaries, full of locals + globals on object
    // files and unstripped executables.
    for symbol in file.symbols() {
        push_symbol(symbol, section_index_to_id, &mut symbols, &mut symbol_index_to_id);
    }

    // Dynamic symbol table (`.dynsym` on ELF, dyld export
    // trie on Mach-O). Stripped Android `.so` files often
    // have only `.dynsym` — the static `.symtab` is empty.
    // Pulling these makes PLT stubs, imports, and exported
    // entry points visible to the analysis layers.
    for symbol in file.dynamic_symbols() {
        if symbol_index_to_id.contains_key(&symbol.index()) {
            continue;
        }
        push_symbol(symbol, section_index_to_id, &mut symbols, &mut symbol_index_to_id);
    }

    (symbols, symbol_index_to_id)
}

fn push_symbol<'data, 'file>(
    symbol: object::read::Symbol<'data, 'file, &'data [u8]>,
    section_index_to_id: &HashMap<object::SectionIndex, SectionId>,
    symbols: &mut Vec<Symbol>,
    symbol_index_to_id: &mut HashMap<object::SymbolIndex, SymbolId>,
) {
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

fn lift_relocations(
    file: &File<'_>,
    section_index_to_id: &HashMap<object::SectionIndex, SectionId>,
    symbol_index_to_id: &HashMap<object::SymbolIndex, SymbolId>,
    architecture: Architecture,
) -> Vec<Relocation> {
    let mut relocations = Vec::new();

    // Per-section relocations: these are the static
    // relocations object files carry in `.rela.text` / similar
    // companions to each section.
    for section in file.sections() {
        let Some(&section_id) = section_index_to_id.get(&section.index()) else {
            continue;
        };
        for (offset, reloc) in section.relocations() {
            let kind = map_relocation_kind(&reloc, architecture);
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

    // Dynamic relocations: linked .so / executables carry
    // these in `.rel.dyn`/`.rela.dyn` and `.rel.plt`/
    // `.rela.plt` tables. The `object` crate exposes them
    // separately from the per-section iterator. Their
    // `offset` is a virtual address; we resolve back to the
    // owning section by address range.
    if let Some(dyn_relocs) = file.dynamic_relocations() {
        let section_lookup: Vec<(u64, u64, SectionId)> = file
            .sections()
            .filter_map(|s| {
                let id = section_index_to_id.get(&s.index()).copied()?;
                Some((s.address(), s.address() + s.size(), id))
            })
            .collect();
        for (vaddr, reloc) in dyn_relocs {
            let kind = map_relocation_kind(&reloc, architecture);
            let symbol = match reloc.target() {
                RelocationTarget::Symbol(index) => symbol_index_to_id.get(&index).copied(),
                _ => None,
            };
            // Find which section owns this VA so the
            // relocation's `offset` becomes section-relative.
            let owning = section_lookup
                .iter()
                .find(|(start, end, _)| vaddr >= *start && vaddr < *end);
            let (section_id, offset) = match owning {
                Some((start, _, id)) => (*id, vaddr - start),
                None => continue, // dyn-reloc points outside any section
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

fn map_relocation_kind(reloc: &object::Relocation, architecture: Architecture) -> RelocationKind {
    match reloc.flags() {
        RelocationFlags::Elf { r_type } => map_elf_relocation(r_type, architecture),
        RelocationFlags::MachO { r_type, .. } => map_macho_relocation(r_type, architecture),
        RelocationFlags::Coff { typ } => map_coff_relocation(typ, architecture),
        _ => match reloc.kind() {
            object::RelocationKind::Absolute => RelocationKind::Absolute,
            _ => RelocationKind::Other(0),
        },
    }
}

fn map_elf_relocation(r_type: u32, architecture: Architecture) -> RelocationKind {
    use object::elf;
    // x86 ELF reloc codes occupy disjoint ABI code spaces that share
    // small numeric values (e.g. R_386_32 == R_X86_64_64 == 1), so we
    // must dispatch on the architecture before matching them.
    match architecture {
        Architecture::X86_64 => return map_elf_x86_64_relocation(r_type),
        Architecture::X86 => return map_elf_i386_relocation(r_type),
        _ => {}
    }
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

        // --- ARMv7 relocation types ---
        elf::R_ARM_CALL => RelocationKind::ArmCall,
        elf::R_ARM_JUMP24 => RelocationKind::ArmJump24,
        elf::R_ARM_PC24 => RelocationKind::ArmPc24,
        elf::R_ARM_RELATIVE => RelocationKind::ArmRelative,
        elf::R_ARM_GLOB_DAT => RelocationKind::ArmGlobData,
        elf::R_ARM_JUMP_SLOT => RelocationKind::ArmJumpSlot,
        elf::R_ARM_ABS32 => RelocationKind::ArmAbs32,
        elf::R_ARM_MOVW_ABS_NC => RelocationKind::ArmMovwAbsNc,
        elf::R_ARM_MOVT_ABS => RelocationKind::ArmMovtAbs,
        // Thumb relocations. `R_ARM_THM_PC22` (10) is the
        // legacy name for the same shape as ARM's `R_ARM_THM_CALL`
        // — the `object` crate exposes only the legacy const.
        elf::R_ARM_THM_PC22 => RelocationKind::ThumbCall,
        elf::R_ARM_THM_JUMP24 => RelocationKind::ThumbJump24,
        elf::R_ARM_THM_JUMP19 => RelocationKind::ThumbJump19,
        elf::R_ARM_THM_MOVW_ABS_NC => RelocationKind::ThumbMovwAbsNc,
        elf::R_ARM_THM_MOVT_ABS => RelocationKind::ThumbMovtAbs,

        other => RelocationKind::Other(other),
    }
}

/// x86_64 ELF relocations (`R_X86_64_*`).
fn map_elf_x86_64_relocation(r_type: u32) -> RelocationKind {
    use object::elf;
    match r_type {
        elf::R_X86_64_PC32 => RelocationKind::X86Pc32,
        elf::R_X86_64_PLT32 => RelocationKind::X86Plt32,
        elf::R_X86_64_GOTPCREL | elf::R_X86_64_GOTPCRELX | elf::R_X86_64_REX_GOTPCRELX => {
            RelocationKind::X86GotPcRel
        }
        elf::R_X86_64_32 | elf::R_X86_64_32S => RelocationKind::X86Abs32,
        elf::R_X86_64_64 => RelocationKind::X86Abs64,
        other => RelocationKind::Other(other),
    }
}

/// i386 ELF relocations (`R_386_*`).
fn map_elf_i386_relocation(r_type: u32) -> RelocationKind {
    use object::elf;
    match r_type {
        elf::R_386_PC32 => RelocationKind::X86Pc32,
        elf::R_386_PLT32 => RelocationKind::X86Plt32,
        elf::R_386_GOTPC => RelocationKind::X86GotPcRel,
        elf::R_386_32 => RelocationKind::X86Abs32,
        other => RelocationKind::Other(other),
    }
}

/// COFF relocations for PE inputs, dispatched on bitness
/// (`IMAGE_REL_AMD64_*` vs `IMAGE_REL_I386_*`).
fn map_coff_relocation(typ: u16, architecture: Architecture) -> RelocationKind {
    use object::pe;
    match architecture {
        Architecture::X86 => match typ {
            pe::IMAGE_REL_I386_REL32 => RelocationKind::X86Pc32,
            pe::IMAGE_REL_I386_DIR32 => RelocationKind::X86Abs32,
            other => RelocationKind::Other(other as u32),
        },
        // Default to the AMD64 code space for x86_64 (and as a
        // best-effort for unspecified architectures on a PE input).
        _ => match typ {
            pe::IMAGE_REL_AMD64_REL32 => RelocationKind::X86Pc32,
            pe::IMAGE_REL_AMD64_ADDR32 | pe::IMAGE_REL_AMD64_ADDR32NB => RelocationKind::X86Abs32,
            pe::IMAGE_REL_AMD64_ADDR64 => RelocationKind::X86Abs64,
            other => RelocationKind::Other(other as u32),
        },
    }
}

fn map_macho_relocation(r_type: u8, architecture: Architecture) -> RelocationKind {
    use object::macho;
    // x86_64 and ARM64 Mach-O relocations share r_type values
    // (e.g. UNSIGNED == 0, BRANCH(26) == 2), so dispatch on arch.
    if architecture == Architecture::X86_64 {
        return match r_type {
            // Width (4 vs 8) lives in r_length, which the neutral
            // model drops at this layer; default UNSIGNED to the
            // 64-bit absolute form (the common pointer case).
            macho::X86_64_RELOC_UNSIGNED => RelocationKind::X86Abs64,
            macho::X86_64_RELOC_BRANCH => RelocationKind::X86Pc32,
            macho::X86_64_RELOC_SIGNED
            | macho::X86_64_RELOC_SIGNED_1
            | macho::X86_64_RELOC_SIGNED_2
            | macho::X86_64_RELOC_SIGNED_4 => RelocationKind::X86Pc32,
            macho::X86_64_RELOC_GOT | macho::X86_64_RELOC_GOT_LOAD => RelocationKind::X86GotPcRel,
            other => RelocationKind::Other(other as u32),
        };
    }
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
            object::Architecture::Arm => Architecture::Arm,
            object::Architecture::X86_64 => Architecture::X86_64,
            object::Architecture::I386 => Architecture::X86,
            _ => Architecture::Other,
        }
    }
}
