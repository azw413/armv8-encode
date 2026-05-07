//! Emit ET_DYN / ET_EXEC ELF byte streams from a [`Container`] +
//! [`ElfImage`] pair.
//!
//! `object::write::Object` (the high-level builder used by
//! [`super::writer`]) is hard-wired to ET_REL; producing a shared
//! library or executable requires the lower-level
//! `object::write::elf::Writer`, which exposes a two-phase
//! reserve/write API and full control over file/program/section
//! headers.
//!
//! ## Strategy
//!
//! Identity round-trip uses a deliberately conservative pattern:
//! reproduce the input's *layout* — every section keeps its source
//! `sh_offset`, virtual address, alignment, and content bytes — so
//! the captured program headers (which reference file offsets) and
//! the dynamic linker's view (which references virtual addresses)
//! both stay valid. We use the Writer's `reserve_until(offset)` to
//! place each section at its source file offset rather than letting
//! Writer pack them densely.
//!
//! `Writer` still owns the `.shstrtab` it generates (with our
//! captured section names) and the section header table at the end.
//! Everything else — `.text`, `.rodata`, `.dynsym`, `.dynstr`,
//! `.dynamic`, `.gnu.hash`, `.gnu.version*`, `.eh_frame_hdr`,
//! `.note.*` — is paste-through bytes from the input.
//!
//! ## What this module deliberately doesn't do
//!
//! - **Editing.** The writer takes a `Container` after edits have
//!   already been applied. Stage 6 will need to teach it how to
//!   recompute layout when text grows.
//! - **Hash table regeneration.** We byte-copy `.gnu.hash` from
//!   input. Bucket / chain / bloom contents reference dynsym
//!   indices, not addresses, so they survive identity round-trip.
//! - **`.shstrtab` placement.** Writer puts it wherever it likes;
//!   the input's position isn't preserved. Doesn't affect runtime
//!   behaviour — `.shstrtab` is metadata for tools, not the loader.

use crate::container::elf_image::ElfImage;
use crate::container::{
    Container, ContainerKind, ContainerWriteError, FileFlags, Section, SectionId, SectionKind,
};
use object::elf;
use object::write::elf::{FileHeader, ProgramHeader, SectionHeader, Writer};
use object::write::StringId;
use object::Endianness;
use std::collections::HashMap;

pub fn write(container: &Container) -> Result<Vec<u8>, ContainerWriteError> {
    let image = container
        .elf_image
        .as_ref()
        .ok_or(ContainerWriteError::ElfImageMissing)?;

    let mut buffer = Vec::new();
    let mut writer = Writer::new(Endianness::Little, /*is_64=*/ true, &mut buffer);

    // ---------------------------------------------------------------
    // RESERVE PHASE
    // ---------------------------------------------------------------
    //
    // Each section is placed at its source file offset via
    // `reserve_until(offset)` so program-header `p_offset` values
    // stay valid. Writer's internal cursor advances monotonically;
    // the input's section ordering must therefore be sorted by
    // `sh_offset` for this to work — sections with `sh_offset = 0`
    // (NOBITS, group, and a few other types) are placed at the
    // current cursor without forcing a back-jump.

    writer.reserve_file_header();
    writer.reserve_program_headers(image.program_headers.len() as u32);

    // Allocate section indices in source order. Index 0 is the
    // null section.
    let _null_index = writer.reserve_null_section_index();
    let mut planned: Vec<PlannedSection> = Vec::with_capacity(container.sections.len());
    let mut section_index_lookup: HashMap<SectionId, u32> = HashMap::new();

    for (i, section) in container.sections.iter().enumerate() {
        let layout = image
            .section_layout
            .get(i)
            .copied()
            .unwrap_or(crate::container::SectionLayout {
                sh_offset: 0,
                sh_link: 0,
                sh_info: 0,
                sh_entsize: 0,
            });

        let name_id = if section.name.is_empty() {
            None
        } else {
            // `Writer::add_section_name` borrows the slice; we leak
            // the section name's bytes for the lifetime of the
            // writer call. The leak ends with the function (Vec
            // dropped at end of `write`); since the writer doesn't
            // outlive this scope, it's safe in practice.
            //
            // Note: leaking is the wrong tool long-term. Stage 6
            // will replace this with an arena owned by the writer
            // function. For now it keeps the change minimal.
            Some(writer.add_section_name(section.name.as_bytes().to_vec().leak()))
        };
        let section_index = writer.reserve_section_index();
        section_index_lookup.insert(section.id, section_index.0);

        let is_nobits = matches!(section.kind, SectionKind::Bss);
        let file_offset = if is_nobits {
            // SHT_NOBITS sections record an `sh_offset` but consume
            // no file bytes. Pass through the captured value.
            layout.sh_offset
        } else if section.bytes.is_empty() {
            // Section that contributes neither file bytes nor
            // memory image (rare — SHT_NULL, SHT_GROUP). Place at
            // the captured offset; reserve no space.
            layout.sh_offset
        } else if layout.sh_offset > 0 {
            // Place at the source file offset so program-header
            // `p_offset` values stay valid.
            writer.reserve_until(layout.sh_offset as usize);
            writer.reserve(section.bytes.len(), 1) as u64
        } else {
            // No source offset (SHT_NULL or freshly-synthesised
            // section). Take whatever offset Writer assigns.
            writer.reserve(section.bytes.len(), section.align.max(1) as usize) as u64
        };

        planned.push(PlannedSection {
            section_id: section.id,
            section_index: section_index.0,
            name_id,
            file_offset,
            sh_offset: layout.sh_offset,
            sh_link: layout.sh_link,
            sh_info: layout.sh_info,
            sh_entsize: layout.sh_entsize,
            is_nobits,
        });
    }

    // Section name table + section header table.
    writer.reserve_shstrtab_section_index();
    writer.reserve_shstrtab();
    writer.reserve_section_headers();

    // ---------------------------------------------------------------
    // WRITE PHASE
    // ---------------------------------------------------------------

    writer
        .write_file_header(&build_file_header(container, image))
        .map_err(|err| ContainerWriteError::ObjectWrite(err.to_string()))?;

    writer.write_align_program_headers();
    for phdr in &image.program_headers {
        writer.write_program_header(&ProgramHeader {
            p_type: phdr.p_type,
            p_flags: phdr.p_flags,
            p_offset: phdr.p_offset,
            p_vaddr: phdr.p_vaddr,
            p_paddr: phdr.p_paddr,
            p_filesz: phdr.p_filesz,
            p_memsz: phdr.p_memsz,
            p_align: phdr.p_align,
        });
    }

    // Section bytes, in the same order they were reserved. Each
    // section is padded out to its source offset before its bytes
    // are written.
    for (plan, section) in planned.iter().zip(container.sections.iter()) {
        if plan.is_nobits || section.bytes.is_empty() {
            continue;
        }
        if plan.sh_offset > 0 {
            writer.pad_until(plan.sh_offset as usize);
        } else {
            writer.write_align(section.align.max(1) as usize);
        }
        writer.write(&section.bytes);
    }

    // .shstrtab content.
    writer.write_shstrtab();

    // Section header table.
    writer.write_null_section_header();
    for (plan, section) in planned.iter().zip(container.sections.iter()) {
        writer.write_section_header(&build_section_header(section, plan));
    }
    writer.write_shstrtab_section_header();

    drop(writer);
    Ok(buffer)
}

/// Per-section bookkeeping carried from reserve into write.
struct PlannedSection {
    #[allow(dead_code)]
    section_id: SectionId,
    #[allow(dead_code)]
    section_index: u32,
    name_id: Option<StringId>,
    file_offset: u64,
    sh_offset: u64,
    sh_link: u32,
    sh_info: u32,
    sh_entsize: u64,
    is_nobits: bool,
}

fn build_file_header(container: &Container, image: &ElfImage) -> FileHeader {
    let (os_abi, abi_version, e_flags) = match container.file_flags {
        Some(FileFlags::Elf {
            os_abi,
            abi_version,
            e_flags,
        }) => (os_abi, abi_version, e_flags),
        None => (elf::ELFOSABI_NONE, 0, 0),
    };
    let e_type = match container.kind {
        ContainerKind::SharedObject => elf::ET_DYN,
        ContainerKind::Executable => elf::ET_EXEC,
        // The dispatcher in `Container::to_bytes` only routes
        // SharedObject / Executable here; other kinds shouldn't
        // reach this far. Default conservatively to ET_DYN.
        _ => elf::ET_DYN,
    };

    FileHeader {
        os_abi,
        abi_version,
        e_type,
        e_machine: elf::EM_AARCH64,
        e_entry: image.e_entry,
        e_flags,
    }
}

fn build_section_header(section: &Section, plan: &PlannedSection) -> SectionHeader {
    // sh_type: prefer the captured raw value when present; else map
    // from the neutral kind.
    let sh_type = section.raw_sh_type.unwrap_or_else(|| match section.kind {
        SectionKind::Text | SectionKind::Data | SectionKind::Rodata => elf::SHT_PROGBITS,
        SectionKind::Bss => elf::SHT_NOBITS,
        SectionKind::Debug => elf::SHT_PROGBITS,
        SectionKind::Other => elf::SHT_PROGBITS,
    });
    let sh_flags = match section.flags {
        Some(crate::container::SectionFlags::Elf { sh_flags }) => sh_flags,
        None => default_sh_flags_for_kind(section.kind),
    };
    let sh_offset = if plan.is_nobits {
        plan.sh_offset
    } else if plan.sh_offset > 0 {
        plan.sh_offset
    } else {
        plan.file_offset
    };

    SectionHeader {
        name: plan.name_id,
        sh_type,
        sh_flags,
        sh_addr: section.address,
        sh_offset,
        sh_size: section.size,
        sh_link: plan.sh_link,
        sh_info: plan.sh_info,
        sh_addralign: section.align.max(1),
        sh_entsize: plan.sh_entsize,
    }
}

fn default_sh_flags_for_kind(kind: SectionKind) -> u64 {
    use object::elf::*;
    match kind {
        SectionKind::Text => u64::from(SHF_ALLOC | SHF_EXECINSTR),
        SectionKind::Data => u64::from(SHF_ALLOC | SHF_WRITE),
        SectionKind::Rodata => u64::from(SHF_ALLOC),
        SectionKind::Bss => u64::from(SHF_ALLOC | SHF_WRITE),
        SectionKind::Debug => 0,
        SectionKind::Other => 0,
    }
}
