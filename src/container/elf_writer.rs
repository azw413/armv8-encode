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

/// Page alignment for appended segments. The input's PT_LOAD
/// segments use 64 KiB alignment on aarch64; we match that so the
/// dynamic linker accepts the new segment without complaint.
pub(crate) const APPEND_PAGE_ALIGN: u64 = 0x10000;

/// Describes a fresh PT_LOAD R-X segment to append to the output.
/// Used by both the grown-text path (where the bytes are
/// rewriter-relocated text) and the add-function path (where the
/// bytes are user-supplied new functions). The caller pre-computes
/// the segment's virtual address (typically via [`pick_append_vaddr`])
/// and lays out the bytes against that address.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct AppendedSegment {
    /// Virtual address the new segment loads at. Must be page-aligned
    /// to [`APPEND_PAGE_ALIGN`] and beyond the input's mapped range.
    pub vaddr: u64,
    /// Bytes of the new segment, laid out so any internal
    /// PC-relative references resolve correctly when interpreted
    /// at virtual address `vaddr + offset`.
    pub bytes: Vec<u8>,
    /// Section name to give this segment in the output's section
    /// header table. Defaults to `.text.armv8_encode_appended` when
    /// constructed via [`AppendedSegment::new`].
    pub section_name: String,
    /// PT_LOAD `p_flags` for the appended segment. Defaults to
    /// `PF_R | PF_W | PF_X` for back-compat with the add_function
    /// / add_initialiser paths. Android (post-API 26 strict, fully
    /// enforced on Android 10+) refuses to load shared objects
    /// with W+X PT_LOADs (`has load segments that are both writable
    /// and executable`), so callers that don't append executable
    /// content (e.g. add_library_dependency) should drop PF_X.
    pub p_flags: u32,
}

impl AppendedSegment {
    pub(crate) fn new(vaddr: u64, bytes: Vec<u8>) -> Self {
        Self {
            vaddr,
            bytes,
            section_name: ".text.armv8_encode_appended".to_string(),
            p_flags: elf::PF_R | elf::PF_W | elf::PF_X,
        }
    }
}

/// Pick a virtual address for an appended segment: the maximum
/// vaddr the input's PT_LOAD segments reach, page-aligned up.
pub(crate) fn pick_append_vaddr(image: &ElfImage) -> u64 {
    let max_input_vaddr = image
        .program_headers
        .iter()
        .filter(|p| p.p_type == elf::PT_LOAD)
        .map(|p| p.p_vaddr.saturating_add(p.p_memsz))
        .max()
        .unwrap_or(0);
    align_up(max_input_vaddr, APPEND_PAGE_ALIGN)
}

pub fn write(container: &Container) -> Result<Vec<u8>, ContainerWriteError> {
    let image = container
        .elf_image
        .as_ref()
        .ok_or(ContainerWriteError::ElfImageMissing)?;

    // Classify the rewrite intent up-front. If anything grew we
    // route to the append-PT_LOAD path (Stage 6.3) which builds an
    // extra segment for the grown content and forwards via
    // trampolines from each grown function's original entry. The
    // common case — no growth — uses the simpler in-place path.
    let grew = container
        .sections
        .iter()
        .enumerate()
        .find_map(|(i, section)| {
            let original = image
                .section_layout
                .get(i)
                .map(|l| l.sh_size)
                .unwrap_or(0);
            if section.bytes.len() as u64 > original && original > 0 {
                Some(GrowingSection {
                    index: i,
                    original_extent: original,
                })
            } else {
                None
            }
        });

    if let Some(grew) = grew {
        return write_with_appended_segment(container, image, grew);
    }

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
                sh_size: 0,
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
            // Note: leaking is the wrong tool long-term. A future
            // refactor will replace this with an arena owned by
            // the writer function. For now it keeps the change
            // minimal.
            Some(writer.add_section_name(section.name.as_bytes().to_vec().leak()))
        };
        let section_index = writer.reserve_section_index();
        section_index_lookup.insert(section.id, section_index.0);

        let is_nobits = matches!(section.kind, SectionKind::Bss);

        // Length-change classification. The writer reserves the
        // *source extent* for in-place edits — when the new bytes
        // shrunk, we still occupy the same file region (padded
        // with NOPs/zeros below) so program-header `p_offset` /
        // `p_filesz` references stay valid. Grown sections are
        // routed to the append-PT_LOAD path before this loop runs
        // (see `write_with_appended_segment`), so we should never
        // see them here.
        let new_size = section.bytes.len() as u64;
        let original_extent = layout.sh_size;
        debug_assert!(
            new_size <= original_extent || original_extent == 0,
            "section {} grew but in-place dispatch wasn't taken",
            section.name,
        );
        let in_place_extent = if is_nobits || section.bytes.is_empty() {
            // Nothing to write to file. The section header still
            // reports the (possibly grown) size.
            0
        } else if original_extent > 0 {
            // Shrunk or unchanged: occupy the source extent.
            original_extent
        } else {
            // No captured extent (SHT_NULL / SHT_GROUP / freshly
            // synthesised). Use the new bytes verbatim.
            new_size
        };

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
            writer.reserve(in_place_extent as usize, 1) as u64
        } else {
            // No source offset (SHT_NULL or freshly-synthesised
            // section). Take whatever offset Writer assigns.
            writer.reserve(in_place_extent as usize, section.align.max(1) as usize) as u64
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
            in_place_extent,
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
    // section is padded out to its source offset before writing,
    // and trailing space inside its in-place extent is filled with
    // NOPs (executable) or zero bytes (other) so the section's
    // file region matches the program-header references.
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

        // Pad out to in_place_extent. For SHF_EXECINSTR sections
        // we pad with AArch64 NOPs (`0xd503201f` little-endian) so
        // execution that falls through landing there is well-
        // defined; everything else gets zeros.
        let trailing = plan.in_place_extent.saturating_sub(section.bytes.len() as u64);
        if trailing > 0 {
            if section_is_executable(section) {
                emit_nop_padding(&mut writer, trailing as usize);
            } else {
                writer.write(&vec![0u8; trailing as usize]);
            }
        }
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
    /// Bytes occupied in the file for this section. For shrunk
    /// in-place edits this is the *source* extent (we pad to
    /// match); for grown edits this would be the new size (Stage
    /// 6.3 / append-PT_LOAD path).
    in_place_extent: u64,
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
    // For shrunk-in-place sections, report the section's nominal
    // (source) size so consumers see it as logically unchanged —
    // the padded NOPs within the original extent fall under the
    // section, not "between" it and the next.
    let sh_size = if plan.in_place_extent > section.size {
        plan.in_place_extent
    } else {
        section.size
    };

    SectionHeader {
        name: plan.name_id,
        sh_type,
        sh_flags,
        sh_addr: section.address,
        sh_offset,
        sh_size,
        sh_link: plan.sh_link,
        sh_info: plan.sh_info,
        sh_addralign: section.align.max(1),
        sh_entsize: plan.sh_entsize,
    }
}

/// Identifies the section that grew past its source extent.
struct GrowingSection {
    /// Index into `container.sections` of the grown section.
    index: usize,
    /// Source `sh_size` — the file extent the section originally
    /// occupied. The append-PT_LOAD path fills this region with
    /// NOPs + trampolines; the actual grown bytes go into a fresh
    /// segment at high virtual address.
    original_extent: u64,
}

/// Stage 6.3: emit an ELF byte stream when one section's content
/// has grown past its source extent. Strategy:
///
///   1. Original section keeps its source `sh_offset` / `sh_addr`,
///      gets filled with NOPs everywhere except at known function
///      entry points, where a trampoline (`b <new_addr>`) forwards
///      execution to the relocated function body.
///   2. The grown content goes into a freshly-allocated PT_LOAD
///      R-X segment at a high, page-aligned virtual address (and a
///      file offset past the end of the source layout).
///   3. The program header table is moved to the end of the file
///      so we have room for the extra entry without colliding with
///      whatever section sits right after the original phdr range.
///
/// Limitations of this first iteration (worth widening later):
/// - Only the first growing section is handled. A rewrite that
///   grew two sections simultaneously would need richer planning.
/// - PT_PHDR is dropped from the output rather than rewritten.
///   The new program-header table lives at file end (after the
///   appended segment) and the dynamic linker uses
///   `e_phoff` for the canonical lookup; PT_PHDR is the
///   runtime-introspection convenience copy and bionic / glibc
///   handle its absence (`dl_iterate_phdr` falls back, libgcc
///   unwinder copes). This is necessary for Android NDK inputs,
///   which carry PT_PHDR by default.
/// - Trampolines are unconditional 26-bit branches (`b imm`),
///   reaching ±128 MiB. If the new segment ends up further than
///   that, callers would silently misroute. This first version
///   places the new segment immediately after a 4 KiB page boundary
///   beyond original `sh_addr`, well within range.
fn write_with_appended_segment(
    container: &Container,
    image: &ElfImage,
    grown: GrowingSection,
) -> Result<Vec<u8>, ContainerWriteError> {
    let grown_section = &container.sections[grown.index];

    // Pick the new segment's virtual address. Make sure we're past
    // the grown section's nominal end too, since some inputs have a
    // lone trailing section that's not in any PT_LOAD's memsz.
    let base_vaddr = pick_append_vaddr(image);
    let new_seg_vaddr =
        base_vaddr.max(grown_section.address + grown.original_extent);
    let new_seg_vaddr = align_up(new_seg_vaddr, APPEND_PAGE_ALIGN);

    // Build the original-extent stub bytes: NOPs everywhere except
    // at function entry points, which become trampolines forwarding
    // to the corresponding offset in the new segment.
    let stub_bytes = build_original_extent_stub(
        grown_section,
        grown.original_extent,
        new_seg_vaddr,
        container,
    )?;

    let appended = AppendedSegment::new(new_seg_vaddr, grown_section.bytes.clone());
    let mut overrides = HashMap::new();
    overrides.insert(grown.index, stub_bytes);
    write_with_appended_segment_inner(container, image, appended, overrides)
}

/// Shared driver for "emit ELF with one extra appended PT_LOAD R-X
/// segment." Used by the Stage 6.3 grown-text path and the
/// add-function path. The original sections are written at their
/// source offsets (with optional byte overrides per section index
/// in `section_byte_overrides`); the appended segment's bytes go in
/// a fresh PT_LOAD past the input's mapped range.
pub(crate) fn write_with_appended_segment_inner(
    container: &Container,
    image: &ElfImage,
    appended: AppendedSegment,
    section_byte_overrides: HashMap<usize, Vec<u8>>,
) -> Result<Vec<u8>, ContainerWriteError> {
    // Drop the input's PT_PHDR (file_offset/vaddr point at the
    // input's old PHT location, not the new one we'll emit at
    // file end) and emit a fresh PT_PHDR below pointing at the
    // new PHT.
    //
    // Earlier this path simply omitted PT_PHDR, on the assumption
    // bionic falls back to `e_phoff`. It does — but the fallback
    // anchors off the PT_LOAD with `p_offset == 0` and computes
    // `loaded = load_bias + e_phoff`. That implicitly assumes
    // file_offset == vaddr, which is false for our appended
    // segment (vaddr=new_seg_vaddr, file_offset >> vaddr after
    // page alignment). Result on Android: `loaded phdr ... not
    // in loadable segment`. A correct PT_PHDR avoids the
    // fallback entirely.
    let phdrs_to_emit: Vec<&crate::container::ProgramHeader> = image
        .program_headers
        .iter()
        .filter(|p| p.p_type != elf::PT_PHDR)
        .collect();

    let new_seg_vaddr = appended.vaddr;
    let new_seg_filesz = appended.bytes.len() as u64;
    let new_seg_memsz = new_seg_filesz;

    // ---------------------------------------------------------------
    // Drive Writer through reserve / write phases. Original sections
    // land at their source offsets (with optional byte overrides);
    // the appended segment's bytes follow the section header table.
    // ---------------------------------------------------------------

    let mut buffer = Vec::new();
    let mut writer = Writer::new(Endianness::Little, true, &mut buffer);

    writer.reserve_file_header();

    let _null_index = writer.reserve_null_section_index();
    let mut planned: Vec<PlannedSection> = Vec::with_capacity(container.sections.len());

    // Pass 1 (source order): allocate section names + indices.
    // This MUST iterate in container.sections order so the
    // resulting section indices match the source — sh_link /
    // sh_info references and any caller code that uses
    // SectionId.0 as the section index keep working.
    struct SectionPrep {
        name_id: Option<object::write::StringId>,
        section_index: u32,
        layout: crate::container::SectionLayout,
        is_nobits: bool,
        effective_bytes_len: u64,
        in_place_extent: u64,
    }
    let mut prepped: Vec<SectionPrep> = Vec::with_capacity(container.sections.len());
    for (i, section) in container.sections.iter().enumerate() {
        let layout = image
            .section_layout
            .get(i)
            .copied()
            .unwrap_or(crate::container::SectionLayout {
                sh_offset: 0,
                sh_size: 0,
                sh_link: 0,
                sh_info: 0,
                sh_entsize: 0,
            });
        let name_id = if section.name.is_empty() {
            None
        } else {
            Some(writer.add_section_name(section.name.as_bytes().to_vec().leak()))
        };
        let section_index = writer.reserve_section_index();
        let is_nobits = matches!(section.kind, SectionKind::Bss);
        let effective_bytes_len = section_byte_overrides
            .get(&i)
            .map(|v| v.len() as u64)
            .unwrap_or(section.bytes.len() as u64);
        let in_place_extent = if is_nobits || effective_bytes_len == 0 {
            0
        } else if layout.sh_size > 0 {
            layout.sh_size.max(effective_bytes_len)
        } else {
            effective_bytes_len
        };
        prepped.push(SectionPrep {
            name_id,
            section_index: section_index.0,
            layout,
            is_nobits,
            effective_bytes_len,
            in_place_extent,
        });
    }

    // Pass 2 (sh_offset order): reserve file space.
    // `Writer::reserve_until` advances the cursor monotonically
    // and asserts cursor ≤ target, so sections with content
    // (`sh_offset > 0`, non-NOBITS, non-empty bytes) must be
    // placed in ascending sh_offset order even when that
    // differs from source-index order. Real-world ELFs that
    // round-trip through `object::build::elf::Builder` can have
    // section header entries whose indices don't match file
    // offset order — a perfectly legal layout the previous
    // single-pass writer panicked on.
    //
    // Sections that don't reserve file space (NOBITS, empty
    // bytes, sh_offset == 0) are accumulated in source order
    // after the placement pass with `file_offset = sh_offset`.
    let mut placement_order: Vec<usize> = (0..container.sections.len())
        .filter(|&i| {
            !prepped[i].is_nobits
                && prepped[i].effective_bytes_len != 0
                && prepped[i].layout.sh_offset > 0
        })
        .collect();
    placement_order.sort_by_key(|&i| prepped[i].layout.sh_offset);

    let mut file_offsets: Vec<u64> = vec![0; container.sections.len()];
    for &i in &placement_order {
        let layout = &prepped[i].layout;
        writer.reserve_until(layout.sh_offset as usize);
        let off = writer.reserve(prepped[i].in_place_extent as usize, 1) as u64;
        file_offsets[i] = off;
    }
    // Sections with sh_offset = 0 but non-empty bytes get
    // appended at the current cursor in source order
    // (rare — typically synthetic relocatable sections).
    for (i, section) in container.sections.iter().enumerate() {
        if prepped[i].is_nobits || prepped[i].effective_bytes_len == 0 {
            file_offsets[i] = prepped[i].layout.sh_offset;
            continue;
        }
        if prepped[i].layout.sh_offset > 0 {
            // Already placed in pass 2.
            continue;
        }
        let off = writer.reserve(
            prepped[i].in_place_extent as usize,
            section.align.max(1) as usize,
        ) as u64;
        file_offsets[i] = off;
    }

    // Pass 3 (source order): build the `planned` vec so
    // subsequent emission code keeps using source indices.
    for (i, section) in container.sections.iter().enumerate() {
        planned.push(PlannedSection {
            section_id: section.id,
            section_index: prepped[i].section_index,
            name_id: prepped[i].name_id,
            file_offset: file_offsets[i],
            sh_offset: prepped[i].layout.sh_offset,
            sh_link: prepped[i].layout.sh_link,
            sh_info: prepped[i].layout.sh_info,
            sh_entsize: prepped[i].layout.sh_entsize,
            in_place_extent: prepped[i].in_place_extent,
            is_nobits: prepped[i].is_nobits,
        });
    }

    // Add the appended segment's section name *before*
    // reserve_shstrtab — Writer's `add_section_name` is illegal
    // afterwards.
    let appended_name_id =
        writer.add_section_name(appended.section_name.as_bytes().to_vec().leak());
    let appended_section_index = writer.reserve_section_index();

    writer.reserve_shstrtab_section_index();
    writer.reserve_shstrtab();
    writer.reserve_section_headers();

    // Reserve the appended segment's bytes at a fresh, page-aligned
    // file offset. Place it after the section header table so all
    // earlier offsets stay valid.
    writer.write_align(APPEND_PAGE_ALIGN as usize);
    let appended_file_offset =
        writer.reserve(new_seg_filesz as usize, APPEND_PAGE_ALIGN as usize) as u64;

    // Finally reserve the program header table at the end. We
    // reserve_program_headers *late* so its offset reflects the
    // post-content position; Writer reads `e_phoff` from this.
    // Count: filtered phdrs (no PT_PHDR) + 1 fresh PT_PHDR
    // pointing at the new PHT location + 1 for the appended-
    // segment PT_LOAD.
    writer.reserve_program_headers((phdrs_to_emit.len() + 2) as u32);

    // ---------------------------------------------------------------
    // Write phase
    // ---------------------------------------------------------------

    writer
        .write_file_header(&build_file_header(container, image))
        .map_err(|err| ContainerWriteError::ObjectWrite(err.to_string()))?;

    // Original sections, with per-section byte overrides where
    // requested. Iteration order MUST match the sh_offset order
    // we used in the reserve phase — `Writer::pad_until` /
    // `write` advance the buffer cursor monotonically and
    // assert against backwards moves. Source-index order is
    // preserved by the section header table separately
    // (planned[i].section_index is set during reserve), so it's
    // safe to write content out of source order.
    let mut write_order: Vec<usize> = (0..container.sections.len())
        .filter(|&i| {
            !planned[i].is_nobits
                && {
                    let bytes = section_byte_overrides
                        .get(&i)
                        .map(|v| v.as_slice())
                        .unwrap_or(&container.sections[i].bytes);
                    !bytes.is_empty()
                }
                && planned[i].sh_offset > 0
        })
        .collect();
    write_order.sort_by_key(|&i| planned[i].sh_offset);
    let mut tail: Vec<usize> = (0..container.sections.len())
        .filter(|&i| {
            !planned[i].is_nobits
                && {
                    let bytes = section_byte_overrides
                        .get(&i)
                        .map(|v| v.as_slice())
                        .unwrap_or(&container.sections[i].bytes);
                    !bytes.is_empty()
                }
                && planned[i].sh_offset == 0
        })
        .collect();
    write_order.append(&mut tail);

    for &i in &write_order {
        let plan = &planned[i];
        let section = &container.sections[i];
        let bytes_to_write: &[u8] = section_byte_overrides
            .get(&i)
            .map(|v| v.as_slice())
            .unwrap_or(&section.bytes);
        if plan.sh_offset > 0 {
            writer.pad_until(plan.sh_offset as usize);
        } else {
            writer.write_align(section.align.max(1) as usize);
        }
        writer.write(bytes_to_write);

        let trailing = plan
            .in_place_extent
            .saturating_sub(bytes_to_write.len() as u64);
        if trailing > 0 {
            if section_is_executable(section) {
                emit_nop_padding(&mut writer, trailing as usize);
            } else {
                writer.write(&vec![0u8; trailing as usize]);
            }
        }
    }

    writer.write_shstrtab();

    // Section headers. Order matches the reservation order:
    // null → source sections → appended → shstrtab.
    writer.write_null_section_header();
    for (plan, section) in planned.iter().zip(container.sections.iter()) {
        writer.write_section_header(&build_section_header(section, plan));
    }
    writer.write_section_header(&SectionHeader {
        name: Some(appended_name_id),
        sh_type: elf::SHT_PROGBITS,
        sh_flags: u64::from(elf::SHF_ALLOC | elf::SHF_EXECINSTR),
        sh_addr: new_seg_vaddr,
        sh_offset: appended_file_offset,
        sh_size: new_seg_filesz,
        sh_link: 0,
        sh_info: 0,
        sh_addralign: 4,
        sh_entsize: 0,
    });
    writer.write_shstrtab_section_header();
    let _ = appended_section_index;

    // Appended segment content. write_align matches the reserve call.
    writer.write_align(APPEND_PAGE_ALIGN as usize);
    writer.write(&appended.bytes);

    // Program header table at end. If a phdr's p_vaddr falls
    // inside the appended segment (e.g. PT_DYNAMIC after a
    // `.dynamic` relocation), recompute its p_offset from the
    // appended segment's file_offset so static tooling and
    // strict loaders find the on-disk content where the phdr
    // says it should be.
    writer.write_align_program_headers();
    // Emit a fresh PT_PHDR pointing at the new PHT location.
    // Without it, bionic's FindPhdr falls back to anchoring
    // off the PT_LOAD with `p_offset == 0` and computes
    // `loaded = load_bias + e_phoff`, which assumes
    // file_offset == vaddr — false for our appended segment
    // (vaddr=new_seg_vaddr, file_offset=appended_file_offset).
    // Result: bionic computes a phdr address inside the first
    // PT_LOAD's vaddr range, then CheckPhdr fails because the
    // address is past that segment's `p_filesz`.
    //
    // PT_PHDR must precede the first PT_LOAD per ELF spec
    // (and SysV uses it for AT_PHDR). Counted in
    // `reserve_program_headers((N + 2) as u32)` above.
    let phdr_entry_size: u64 = 56; // sizeof(Elf64_Phdr)
    let phdr_count: u64 = (phdrs_to_emit.len() + 2) as u64;
    let elf_align: u64 = 8;
    let pht_file_offset =
        (appended_file_offset + new_seg_filesz + elf_align - 1) & !(elf_align - 1);
    let pht_size = phdr_count * phdr_entry_size;
    let pht_vaddr = new_seg_vaddr + (pht_file_offset - appended_file_offset);
    writer.write_program_header(&ProgramHeader {
        p_type: elf::PT_PHDR,
        p_flags: elf::PF_R,
        p_offset: pht_file_offset,
        p_vaddr: pht_vaddr,
        p_paddr: pht_vaddr,
        p_filesz: pht_size,
        p_memsz: pht_size,
        p_align: 8,
    });
    for phdr in &phdrs_to_emit {
        let p_offset = if phdr.p_vaddr >= new_seg_vaddr
            && phdr.p_vaddr + phdr.p_filesz <= new_seg_vaddr + new_seg_filesz
        {
            appended_file_offset + (phdr.p_vaddr - new_seg_vaddr)
        } else {
            phdr.p_offset
        };
        writer.write_program_header(&ProgramHeader {
            p_type: phdr.p_type,
            p_flags: phdr.p_flags,
            p_offset,
            p_vaddr: phdr.p_vaddr,
            p_paddr: phdr.p_paddr,
            p_filesz: phdr.p_filesz,
            p_memsz: phdr.p_memsz,
            p_align: phdr.p_align,
        });
    }
    // Permissions on the appended segment come from
    // `appended.p_flags`. Default is RWX (back-compat with
    // add_function + add_initialiser, which need both X for
    // appended code and W for `R_AARCH64_RELATIVE` writes
    // into appended `.init_array`). Android refuses W+X
    // PT_LOADs, so callers that append only data
    // (add_library_dependency) drop PF_X.
    //
    // Extend filesz/memsz to include the program-header table
    // that we just wrote at the file's end. Bionic's loader
    // verifies the runtime PHT address lies within some
    // PT_LOAD; otherwise it rejects with `loaded phdr ... not
    // in loadable segment`. `pht_file_offset`/`phdr_count`
    // were computed above (where PT_PHDR was emitted).
    let pht_end = pht_file_offset + phdr_count * phdr_entry_size;
    let cover_filesz = pht_end - appended_file_offset;
    let cover_memsz = cover_filesz;
    writer.write_program_header(&ProgramHeader {
        p_type: elf::PT_LOAD,
        p_flags: appended.p_flags,
        p_offset: appended_file_offset,
        p_vaddr: new_seg_vaddr,
        p_paddr: new_seg_vaddr,
        p_filesz: cover_filesz,
        p_memsz: cover_memsz,
        p_align: APPEND_PAGE_ALIGN,
    });
    let _ = new_seg_memsz;

    drop(writer);
    Ok(buffer)
}

/// Build the bytes that occupy the original extent of a grown
/// section. The first instruction of every function whose body now
/// lives in the appended segment is replaced with a `b imm26`
/// trampoline pointing at the corresponding offset in the new
/// segment; everything else becomes NOP padding.
///
/// "Function entry" is identified by walking the container's
/// symbol table for `STT_FUNC` symbols whose section is the grown
/// section. Each symbol's `address` gives its source virtual
/// address; the new segment's matching offset is
/// `new_seg_vaddr + (symbol.address - section.address)`.
fn build_original_extent_stub(
    section: &Section,
    original_extent: u64,
    new_seg_vaddr: u64,
    container: &Container,
) -> Result<Vec<u8>, ContainerWriteError> {
    let mut bytes = vec![0u8; original_extent as usize];
    // Fill with NOPs first.
    for chunk in bytes.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[0x1f, 0x20, 0x03, 0xd5]);
    }

    for symbol in &container.symbols {
        use crate::container::SymbolKind;
        if symbol.kind != SymbolKind::Function {
            continue;
        }
        if symbol.section != Some(section.id) {
            continue;
        }
        if symbol.address < section.address
            || symbol.address >= section.address + original_extent
        {
            continue;
        }
        let symbol_offset = symbol.address - section.address;
        let trampoline_address = section.address + symbol_offset;
        let target_address = new_seg_vaddr + symbol_offset;

        let displacement_bytes = target_address as i64 - trampoline_address as i64;
        if displacement_bytes % 4 != 0 {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "trampoline target {target_address:#x} for {} not 4-aligned \
                 vs source {trampoline_address:#x}",
                symbol.name,
            )));
        }
        let displacement_words = displacement_bytes / 4;
        if !(-(1 << 25)..(1 << 25)).contains(&displacement_words) {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "trampoline displacement for {} ({} words) exceeds \
                 b imm26 range",
                symbol.name, displacement_words,
            )));
        }
        // `b imm26`: opcode 0b000101, then sign-extended 26-bit
        // word displacement.
        let imm26 = (displacement_words as u32) & 0x3ff_ffff;
        let word: u32 = 0x14000000 | imm26;
        let dst = symbol_offset as usize;
        if dst + 4 > bytes.len() {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "function {} at offset {dst:#x} doesn't fit in original extent {original_extent}",
                symbol.name,
            )));
        }
        bytes[dst..dst + 4].copy_from_slice(&word.to_le_bytes());
    }

    Ok(bytes)
}

fn align_up(value: u64, align: u64) -> u64 {
    let r = value % align;
    if r == 0 {
        value
    } else {
        value + (align - r)
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

/// True for sections that need NOP padding (rather than zeros) when
/// the rewrite shrinks them inside their original file extent.
/// Falling-through code on a SHF_EXECINSTR section needs valid
/// instructions in the trailing space; everything else is fine
/// with zeros.
fn section_is_executable(section: &Section) -> bool {
    use object::elf::SHF_EXECINSTR;
    match section.flags {
        Some(crate::container::SectionFlags::Elf { sh_flags }) => {
            sh_flags & u64::from(SHF_EXECINSTR) != 0
        }
        // Fall back to neutral kind. SectionKind::Text always
        // implies executable; everything else doesn't.
        None => matches!(section.kind, SectionKind::Text),
    }
}

/// Write `count` bytes of AArch64 NOP padding (`0xd503201f`,
/// little-endian, 4 bytes per NOP) via `Writer::write`. Caller
/// guarantees `count` is a multiple of 4 — true for any source ELF
/// where text starts/ends aligned, which is the only case the
/// shrink path applies to.
fn emit_nop_padding(writer: &mut Writer<'_>, count: usize) {
    debug_assert!(count.is_multiple_of(4), "NOP padding must be 4-aligned");
    const NOP: [u8; 4] = [0x1f, 0x20, 0x03, 0xd5];
    let mut padding = Vec::with_capacity(count);
    while padding.len() < count {
        padding.extend_from_slice(&NOP);
    }
    writer.write(&padding[..count]);
}
