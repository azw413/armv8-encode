//! Mach-O writer for ET_DYN-shaped inputs (MH_DYLIB / MH_EXECUTE).
//!
//! Phase 1 (round-trip + section overrides): copies the captured
//! [`MachOImage::raw_bytes`] verbatim, applies any caller-staged
//! section-byte overrides at their original file offsets, then
//! re-signs ad-hoc by shelling out to `codesign -s - --force`.
//! The result is byte-faithful for unmodified inputs and
//! correctly modified for in-place edits whose new bytes match
//! the original section's length.
//!
//! Why re-sign with `codesign -s -`:
//!
//! - macOS 10.15+ refuses to load arm64 dylibs without a valid
//!   ad-hoc-or-better signature. Any byte modification (even
//!   keeping the file size the same) invalidates the existing
//!   signature.
//! - `codesign --force` overwrites the existing
//!   LC_CODE_SIGNATURE blob and updates the load command's
//!   sizes in place; we don't need to track the signature byte
//!   range ourselves.
//! - The shell-out keeps Phase 1 dependency-free at runtime
//!   (no inline CMS implementation). A fully self-contained
//!   signer is future work.
//!
//! Limits:
//!
//! - Length-changing edits (overrides whose bytes don't match
//!   the original section size) aren't supported here. Phase 3
//!   will add an append-segment writer for that case, similar
//!   to the ELF append-PT_LOAD path.
//! - We don't yet validate that overrides are confined to
//!   in-bounds section offsets — the caller is trusted to
//!   produce well-formed override bytes the same length as the
//!   target section. Mismatches would silently corrupt
//!   neighbouring section data.

use crate::container::{Container, ContainerWriteError};
use std::io::Write;

/// Page alignment for appended Mach-O segments. dyld maps
/// segments at page granularity; mismatched alignment between
/// vmaddr and fileoff would either fail to load or trigger a
/// copy-on-write split that we don't want. 16K matches arm64
/// macOS.
pub(crate) const APPEND_PAGE_ALIGN: u64 = 0x4000;

/// One symbol the caller wants to expose via the Mach-O
/// export trie + symbol table. Mach-O's external-defined
/// symbols all have the same shape from the writer's view:
/// a name + a vmaddr.
#[derive(Debug, Clone)]
pub struct MachOExportRequest {
    /// Full symbol name including the leading underscore
    /// (e.g. `_greet_quintuple`). Mach-O symbols are
    /// underscore-prefixed by convention.
    pub name: String,
    /// Symbol vmaddr (matches the address dyld will resolve
    /// `dlsym` to).
    pub vaddr: u64,
}

/// Inputs to the Mach-O append-segment writer.
pub struct AppendedSegment {
    /// Virtual address where the new segment loads. Must be
    /// page-aligned and not overlap any existing segment.
    pub vaddr: u64,
    /// Bytes that fill the new segment's mapped range. The
    /// writer places these at a page-aligned file offset past
    /// the input's existing content.
    pub bytes: Vec<u8>,
}

impl AppendedSegment {
    pub fn new(vaddr: u64, bytes: Vec<u8>) -> Self {
        Self { vaddr, bytes }
    }
}

/// Build a Mach-O byte stream that round-trips the input,
/// applying any pending section-byte overrides at their
/// original file offsets, then re-signing the result ad-hoc
/// via `codesign`.
pub fn write(container: &Container) -> Result<Vec<u8>, ContainerWriteError> {
    let image = container
        .macho_image
        .as_ref()
        .ok_or(ContainerWriteError::ElfImageMissing)?;
    let mut bytes = image.raw_bytes.clone();

    // Apply section-byte overrides. For Phase 1 we honour only
    // sections whose neutral bytes differ from the original
    // file content — i.e. anything the caller modified via
    // `with_section_bytes` or the rewriter pipeline. The match
    // criterion is "the section has a non-zero file offset
    // and the new bytes are the same length as the captured
    // ones" — a length mismatch indicates a length-changing
    // edit which Phase 1 doesn't support.
    //
    // We discover original file offsets from the section's
    // `.address` plus the segment's vmaddr→fileoff mapping —
    // but the neutral container model doesn't carry that
    // mapping yet. As a pragmatic stand-in for Phase 1, find
    // the section's bytes inside the captured raw file by
    // scanning the load commands for matching vmaddrs.
    apply_section_overrides(container, image, &mut bytes)?;

    // Re-sign via `codesign -s -`. The command operates on a
    // file path, so we round-trip through a tempfile in the
    // OS temp dir.
    sign_ad_hoc(&mut bytes)?;

    Ok(bytes)
}

/// For each container section whose bytes differ from the
/// captured file content, overwrite the relevant bytes in
/// `out` at the original file offset.
///
/// Phase 1 supports only same-length overrides — length-
/// changing edits are deferred to the Phase-3 append-segment
/// path. We use the parsed [`MachOLayout::sections`] to map
/// vaddrs to file offsets.
fn apply_section_overrides(
    container: &Container,
    image: &crate::container::macho_image::MachOImage,
    out: &mut [u8],
) -> Result<(), ContainerWriteError> {
    let layout = &image.layout;

    for section in &container.sections {
        if section.bytes.is_empty() {
            continue;
        }
        // Skip zerofill sections (e.g. `__common`, `__bss`):
        // they have file_offset = 0 and no on-disk bytes.
        let Some(entry) = layout
            .sections
            .iter()
            .find(|s| s.vaddr == section.address && s.file_offset > 0)
        else {
            continue;
        };
        if section.bytes.len() as u64 != entry.size {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: section {:?} bytes ({} bytes) don't match \
                 captured size ({} bytes); length-changing edits aren't \
                 supported in Phase 1",
                section.name,
                section.bytes.len(),
                entry.size,
            )));
        }
        let start = entry.file_offset as usize;
        let end = start + section.bytes.len();
        if end > out.len() {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: section {:?} extends past end of file",
                section.name,
            )));
        }
        out[start..end].copy_from_slice(&section.bytes);
    }
    Ok(())
}

/// Re-sign the bytes ad-hoc via `codesign -s - --force`. We
/// write to a tempfile, run codesign, and read back.
fn sign_ad_hoc(bytes: &mut Vec<u8>) -> Result<(), ContainerWriteError> {
    use std::process::Command;

    // tempfile path. We pick a name in the system temp dir; the
    // file is unlinked at the end of this function regardless of
    // success.
    let mut path = std::env::temp_dir();
    let pid = std::process::id();
    // A simple atomic counter would be nicer than nanos, but
    // nanos within a single process is good enough for the
    // single-threaded test harness this targets. Two
    // concurrent calls in the same nanosecond would collide;
    // unlikely in practice.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.push(format!("armv8-encode-macho-{pid}-{nanos}.dylib"));

    {
        let mut f = std::fs::File::create(&path).map_err(|err| {
            ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: create tempfile {}: {err}",
                path.display(),
            ))
        })?;
        f.write_all(bytes).map_err(|err| {
            ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: write tempfile {}: {err}",
                path.display(),
            ))
        })?;
    }

    // `--no-strict` tells codesign not to apply additional
    // post-Catalina layout invariants (e.g. "the signature
    // blob is the last byte of __LINKEDIT") which our
    // append-segment writer doesn't preserve. The resulting
    // signature is still valid; dyld accepts it and
    // `codesign --verify` passes.
    let out = Command::new("codesign")
        .args(["--sign", "-", "--force", "--no-strict"])
        .arg(&path)
        .output();
    // Always try to clean up the tempfile regardless of the
    // outcome.
    let signed_bytes = match out {
        Ok(out) if out.status.success() => std::fs::read(&path).map_err(|err| {
            ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: read signed tempfile {}: {err}",
                path.display(),
            ))
        }),
        Ok(out) => Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O writer: codesign exit {}:\nstdout: {}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        ))),
        Err(err) => Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O writer: spawn codesign: {err}",
        ))),
    };
    let _ = std::fs::remove_file(&path);
    *bytes = signed_bytes?;
    Ok(())
}

/// Build a Mach-O byte stream that copies the input verbatim,
/// applies any pending section-byte overrides, then adds a new
/// `LC_SEGMENT_64` covering the bytes in `appended`. Re-signs at
/// the end.
///
/// Mechanics:
///
/// 1. Splice a new 152-byte load command (one
///    `segment_command_64` + one `section_64`) into the
///    load-command list, right before any
///    `LC_CODE_SIGNATURE` entry (so signing remains the last
///    load command).
/// 2. Patch `mach_header_64.ncmds` (+= 1) and `sizeofcmds`
///    (+= 152).
/// 3. Write the appended segment bytes at a page-aligned file
///    offset past the input's last byte of content.
/// 4. Run `codesign --force` to invalidate-and-replace the
///    existing signature.
///
/// The new segment's permissions are R-X-W (matches the ELF
/// append-segment permissions and supports both code and
/// initialiser-style writable data; tighter permissions are a
/// later optimisation).
///
/// Errors when:
///
/// - `image.layout` isn't populated (caller's input wasn't a
///   well-formed Mach-O the reader could parse);
/// - the input's headerpad room is too small for the new load
///   command (typically means the source was built without
///   `-Wl,-headerpad,0x1000`; the error message tells the
///   caller how to rebuild);
/// - `appended.vaddr` collides with an existing segment.
pub fn write_with_appended_segment(
    container: &Container,
    appended: AppendedSegment,
    section_overrides: &[(usize, Vec<u8>)],
    exports: &[MachOExportRequest],
    library_deps: &[String],
) -> Result<Vec<u8>, ContainerWriteError> {
    let image = container
        .macho_image
        .as_ref()
        .ok_or(ContainerWriteError::ElfImageMissing)?;
    let layout = &image.layout;

    // Validate vaddr doesn't collide with anything. The
    // caller is expected to pick a vaddr past
    // layout.max_vaddr_end(); be defensive.
    let vaddr_end = appended
        .vaddr
        .checked_add(appended.bytes.len() as u64)
        .ok_or_else(|| {
            ContainerWriteError::ObjectWrite(
                "Mach-O append: appended segment vaddr+size overflows u64".into(),
            )
        })?;
    for seg in &layout.segments {
        let seg_end = seg.vmaddr.saturating_add(seg.vmsize);
        let overlaps = appended.vaddr < seg_end && seg.vmaddr < vaddr_end;
        if overlaps {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O append: appended segment [0x{:x}, 0x{:x}) overlaps \
                 existing segment {:?} [0x{:x}, 0x{:x})",
                appended.vaddr, vaddr_end, seg.name, seg.vmaddr, seg_end,
            )));
        }
    }

    // Headerpad check: we need 152 bytes (segment_command_64 +
    // one section_64) for the new LC_SEGMENT_64 entry, plus
    // dylib_command bytes (24 + path + NUL + pad-to-8) for
    // each library dependency. If there isn't room, fail
    // with actionable guidance — the user must rebuild with
    // bigger headerpad.
    const NEW_LC_SIZE: u64 = 152;
    let dep_lc_total: u64 = library_deps
        .iter()
        .map(|name| dylib_command_size(name))
        .sum();
    let total_lc_growth = NEW_LC_SIZE + dep_lc_total;
    if layout.headerpad() < total_lc_growth {
        return Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O append: not enough headerpad room ({} bytes, need {total_lc_growth}). \
             Rebuild the input with `-Wl,-headerpad,0x1000` (or larger) so the \
             writer has space to grow the load-command list in place.",
            layout.headerpad(),
        )));
    }

    let mut bytes = image.raw_bytes.clone();

    // Apply section-byte overrides at original file offsets.
    // Same constraint as the round-trip path: same-length only.
    for (idx, override_bytes) in section_overrides {
        let section = &container.sections[*idx];
        let Some(entry) = layout
            .sections
            .iter()
            .find(|s| s.vaddr == section.address && s.file_offset > 0)
        else {
            continue;
        };
        if override_bytes.len() as u64 != entry.size {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O append: override for {:?} ({} bytes) doesn't match \
                 captured size ({} bytes); length-changing in-place edits aren't \
                 supported yet",
                section.name,
                override_bytes.len(),
                entry.size,
            )));
        }
        let start = entry.file_offset as usize;
        let end = start + override_bytes.len();
        bytes[start..end].copy_from_slice(override_bytes);
    }

    // Strategy for placing __APPENDED's file bytes:
    //
    // codesign extends `__LINKEDIT.filesize` to cover any
    // file content past __LINKEDIT.fileoff. If we put
    // __APPENDED's bytes after __LINKEDIT in the file, that
    // extension swallows them and dyld rejects the result
    // ("file range of __LINKEDIT overlaps __APPENDED").
    //
    // Solution: shift __LINKEDIT by APPEND_PAGE_ALIGN so
    // __APPENDED can occupy the freed slot. Updating
    // __LINKEDIT.fileoff in the load command isn't enough —
    // every load command pointing at content inside
    // __LINKEDIT (LC_DYLD_CHAINED_FIXUPS, LC_DYLD_EXPORTS_TRIE,
    // LC_SYMTAB, LC_DYSYMTAB, LC_FUNCTION_STARTS,
    // LC_DATA_IN_CODE) carries its own absolute file offset
    // that needs the same shift.
    let linkedit_old_fileoff = layout
        .segments
        .iter()
        .find(|s| s.name == "__LINKEDIT")
        .map(|s| s.fileoff)
        .ok_or_else(|| {
            ContainerWriteError::ObjectWrite(
                "Mach-O append: input has no __LINKEDIT segment".into(),
            )
        })?;
    let linkedit_old_filesize = layout
        .segments
        .iter()
        .find(|s| s.name == "__LINKEDIT")
        .map(|s| s.filesize)
        .unwrap();

    // Place __APPENDED at the original __LINKEDIT.fileoff,
    // and shift __LINKEDIT to the next page-aligned slot
    // past __APPENDED's end.
    let appended_file_offset = linkedit_old_fileoff;
    let appended_file_end = appended_file_offset + appended.bytes.len() as u64;
    let new_linkedit_fileoff =
        (appended_file_end + APPEND_PAGE_ALIGN - 1) & !(APPEND_PAGE_ALIGN - 1);
    let linkedit_shift = new_linkedit_fileoff - linkedit_old_fileoff;

    // Grow `bytes` to fit. We're about to:
    //   1. shift the original __LINKEDIT bytes from
    //      [old_fileoff, old_fileoff+filesize) to
    //      [old_fileoff+shift, old_fileoff+shift+filesize);
    //   2. write the appended bytes at old_fileoff;
    //   3. zero-pad anything between the appended bytes and
    //      the new __LINKEDIT location.
    let new_total = new_linkedit_fileoff + linkedit_old_filesize;
    let mut new_bytes = vec![0u8; new_total as usize];
    // Copy header + load commands (everything before __LINKEDIT
    // file region). We do this from the original `bytes` which
    // is already a clone of image.raw_bytes; later we'll splice
    // the new LC into this header region.
    new_bytes[..linkedit_old_fileoff as usize]
        .copy_from_slice(&bytes[..linkedit_old_fileoff as usize]);
    // Place the appended bytes at the original __LINKEDIT slot.
    new_bytes[appended_file_offset as usize..appended_file_end as usize]
        .copy_from_slice(&appended.bytes);
    // Copy __LINKEDIT to its new (shifted) location.
    let old_linkedit_end =
        (linkedit_old_fileoff + linkedit_old_filesize) as usize;
    new_bytes
        [new_linkedit_fileoff as usize..(new_linkedit_fileoff + linkedit_old_filesize) as usize]
        .copy_from_slice(&bytes[linkedit_old_fileoff as usize..old_linkedit_end]);
    bytes = new_bytes;

    // Patch every load command that carries a file offset
    // referencing data inside __LINKEDIT, shifting each by
    // linkedit_shift.
    shift_linkedit_offsets(&mut bytes, layout, linkedit_old_fileoff, linkedit_shift)?;

    // Build the new load command (segment_command_64 + one
    // section_64).
    let new_lc =
        build_segment_command_64(&appended, appended_file_offset, NEW_LC_SIZE)?;
    debug_assert_eq!(new_lc.len(), NEW_LC_SIZE as usize);

    // Splice the new LC into the existing load-command list.
    // Insert before LC_CODE_SIGNATURE if present (so the
    // signature remains the last load command); otherwise
    // append at the end of the LC list.
    let lc_start = layout.load_commands_offset as usize;
    let lc_end = lc_start + layout.sizeofcmds as usize;
    let insert_at = find_insert_offset_for_lc(&bytes, lc_start, lc_end)?;

    // Splice. The splice inserts `NEW_LC_SIZE` bytes at
    // `insert_at`, shifting everything after that point
    // forward by NEW_LC_SIZE. To keep section file offsets
    // stable, we then drain NEW_LC_SIZE zero bytes from the
    // headerpad region — between the (now shifted) end of
    // the load-command list and the start of section
    // content. The headerpad lives at offsets
    // [lc_end_post_splice, content_start_post_splice) where
    // `lc_end_post_splice` = original lc_end + NEW_LC_SIZE
    // (since we just inserted NEW_LC_SIZE bytes inside the
    // LC list).
    //
    // We checked headerpad() >= NEW_LC_SIZE above, so the
    // bytes between `lc_end` and the next content are zeros
    // we can overwrite.
    bytes.splice(insert_at..insert_at, new_lc.iter().copied());
    let post_splice_lc_end = lc_end + NEW_LC_SIZE as usize;
    bytes.drain(post_splice_lc_end..post_splice_lc_end + NEW_LC_SIZE as usize);

    // Patch mach_header_64.ncmds and sizeofcmds.
    let new_ncmds = layout.ncmds + 1;
    let new_sizeofcmds = layout.sizeofcmds + NEW_LC_SIZE as u32;
    bytes[16..20].copy_from_slice(&new_ncmds.to_le_bytes());
    bytes[20..24].copy_from_slice(&new_sizeofcmds.to_le_bytes());

    // Phase 7: splice LC_LOAD_DYLIB entries for each library
    // dependency. Same headerpad-eating splice pattern as the
    // LC_SEGMENT_64 above; consumes `dep_lc_total` bytes
    // we already accounted for in the headerpad check.
    if !library_deps.is_empty() {
        inject_lc_load_dylibs(&mut bytes, library_deps)?;
    }

    // Strip the existing LC_CODE_SIGNATURE entry. codesign
    // --force adds a fresh one at the right position (past
    // the appended segment) when re-signing, and our prior
    // signature blob's bytes inside __LINKEDIT are now stale
    // anyway. Removing the LC keeps codesign from getting
    // confused about strict-validation invariants ("the
    // signature lives at __LINKEDIT.end" is no longer true).
    strip_lc_code_signature(&mut bytes)?;

    // Phase 5: extend the export trie + symtab + strtab to
    // include the requested exports. New blobs are appended
    // to the file end (which becomes the trailing edge of
    // __LINKEDIT), and the relevant LC_*-pointed offsets are
    // patched to follow.
    if !exports.is_empty() {
        extend_exports_in_place(&mut bytes, layout, exports, linkedit_shift)?;
    }

    // Re-sign. With LC_CODE_SIGNATURE stripped above and
    // __LINKEDIT shifted past __APPENDED, codesign --force
    // creates a fresh signature blob at the end of the new
    // __LINKEDIT and re-emits LC_CODE_SIGNATURE pointing at
    // it. `--no-strict` is required: macOS 10.15+ codesign
    // applies post-sign layout invariants we can't satisfy
    // (e.g. "appended segments must come from the linker"),
    // and the resulting signature is still cryptographically
    // valid; dyld accepts it and `codesign --verify` passes.
    sign_ad_hoc(&mut bytes)?;

    Ok(bytes)
}

/// Patch every load command whose file-offset field points
/// at data inside the original `__LINKEDIT` range, shifting
/// each by `shift`. Call this after physically moving
/// __LINKEDIT's bytes in the file but before re-signing.
fn shift_linkedit_offsets(
    bytes: &mut [u8],
    layout: &crate::container::macho_image::MachOLayout,
    linkedit_old_fileoff: u64,
    shift: u64,
) -> Result<(), ContainerWriteError> {
    use object::macho;

    // Helper: read u32 at offset, add shift, write back.
    // Returns true if the value was non-zero (so we don't
    // accidentally promote zero to shift, which is meaningful
    // — zero means "no data").
    let shift_u32 = |bytes: &mut [u8], offset: usize| {
        let v = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if v != 0 {
            let new_v = (v as u64) + shift;
            bytes[offset..offset + 4]
                .copy_from_slice(&(new_v as u32).to_le_bytes());
        }
    };
    // Helper: u64 variant for segment fileoff.
    let shift_u64 = |bytes: &mut [u8], offset: usize| {
        let v = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if v != 0 {
            let new_v = v + shift;
            bytes[offset..offset + 8].copy_from_slice(&new_v.to_le_bytes());
        }
    };

    let lc_start = layout.load_commands_offset as usize;
    let sizeofcmds = layout.sizeofcmds as usize;
    let lc_end = lc_start + sizeofcmds;

    let mut cursor = lc_start;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: invalid cmdsize {cmdsize} at offset {cursor}",
            )));
        }
        match cmd {
            macho::LC_SEGMENT_64 => {
                // Patch fileoff iff this segment's fileoff
                // matches __LINKEDIT.
                let fileoff_off = cursor + 40;
                let fileoff = u64::from_le_bytes(
                    bytes[fileoff_off..fileoff_off + 8].try_into().unwrap(),
                );
                if fileoff == linkedit_old_fileoff {
                    shift_u64(bytes, fileoff_off);
                }
            }
            macho::LC_DYLD_INFO | macho::LC_DYLD_INFO_ONLY => {
                // dyld_info_command:
                //   u32 cmd, cmdsize,
                //   u32 rebase_off, rebase_size,
                //   u32 bind_off, bind_size,
                //   u32 weak_bind_off, weak_bind_size,
                //   u32 lazy_bind_off, lazy_bind_size,
                //   u32 export_off, export_size
                if cmdsize >= 48 {
                    shift_u32(bytes, cursor + 8);  // rebase_off
                    shift_u32(bytes, cursor + 16); // bind_off
                    shift_u32(bytes, cursor + 24); // weak_bind_off
                    shift_u32(bytes, cursor + 32); // lazy_bind_off
                    shift_u32(bytes, cursor + 40); // export_off
                }
            }
            macho::LC_DYLD_CHAINED_FIXUPS
            | macho::LC_DYLD_EXPORTS_TRIE
            | macho::LC_FUNCTION_STARTS
            | macho::LC_DATA_IN_CODE
            | macho::LC_CODE_SIGNATURE => {
                // linkedit_data_command: u32 cmd, cmdsize,
                // u32 dataoff, datasize.
                if cmdsize >= 16 {
                    shift_u32(bytes, cursor + 8); // dataoff
                }
            }
            macho::LC_SYMTAB => {
                // symtab_command: u32 cmd, cmdsize, u32 symoff,
                // nsyms, stroff, strsize.
                if cmdsize >= 24 {
                    shift_u32(bytes, cursor + 8);  // symoff
                    shift_u32(bytes, cursor + 16); // stroff
                }
            }
            macho::LC_DYSYMTAB => {
                // dysymtab_command has many fields; the
                // file-offset ones we need to patch:
                //   tocoff (offset 32),
                //   modtaboff (40),
                //   extrefsymoff (48),
                //   indirectsymoff (56),
                //   extreloff (64),
                //   locreloff (72)
                if cmdsize >= 76 {
                    shift_u32(bytes, cursor + 32);
                    shift_u32(bytes, cursor + 40);
                    shift_u32(bytes, cursor + 48);
                    shift_u32(bytes, cursor + 56);
                    shift_u32(bytes, cursor + 64);
                    shift_u32(bytes, cursor + 72);
                }
            }
            _ => {}
        }
        cursor += cmdsize;
    }
    Ok(())
}

/// Splice one LC_LOAD_DYLIB entry per requested library
/// dependency into the load-command list. Each entry carries
/// the library path inline (with NUL terminator + pad-to-8).
/// Patches `mach_header_64.ncmds` (+= deps.len()) and
/// `sizeofcmds` (+= total dylib_command bytes), and consumes
/// the same byte count of headerpad zeros after the LC list.
fn inject_lc_load_dylibs(
    bytes: &mut Vec<u8>,
    deps: &[String],
) -> Result<(), ContainerWriteError> {
    if bytes.len() < 24 {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O dep: input too short for mach_header_64".into(),
        ));
    }
    let mut ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let mut sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let lc_start = 32usize;
    let lc_end = lc_start + sizeofcmds as usize;

    // Build all dep load commands up front so we know the
    // total splice size.
    let lcs: Vec<Vec<u8>> = deps.iter().map(|d| build_dylib_command(d)).collect();
    let total_size: usize = lcs.iter().map(|lc| lc.len()).sum();

    // Find splice point: just before LC_CODE_SIGNATURE if
    // present, else at lc_end. We use the same helper as
    // LC_SEGMENT_64 splicing.
    let insert_at = find_insert_offset_for_lc(bytes, lc_start, lc_end)?;

    // Splice all bytes at once.
    let mut splice: Vec<u8> = Vec::with_capacity(total_size);
    for lc in &lcs {
        splice.extend_from_slice(lc);
    }
    bytes.splice(insert_at..insert_at, splice.iter().copied());

    // Drain matching headerpad zeros after the (now
    // shifted) lc_end.
    let post_splice_lc_end = lc_end + total_size;
    bytes.drain(post_splice_lc_end..post_splice_lc_end + total_size);

    // Patch ncmds + sizeofcmds.
    ncmds += deps.len() as u32;
    sizeofcmds += total_size as u32;
    bytes[16..20].copy_from_slice(&ncmds.to_le_bytes());
    bytes[20..24].copy_from_slice(&sizeofcmds.to_le_bytes());
    let _ = lc_end;
    Ok(())
}

/// Encoded size of a `dylib_command` for the given path.
/// 24-byte fixed header + path + NUL, padded to 8-byte
/// alignment.
fn dylib_command_size(path: &str) -> u64 {
    let raw = 24 + path.len() + 1;
    ((raw + 7) & !7) as u64
}

/// Build the bytes of a dylib_command (LC_LOAD_DYLIB with the
/// inline path string).
fn build_dylib_command(path: &str) -> Vec<u8> {
    use object::macho;

    let cmdsize = dylib_command_size(path) as usize;
    let mut out = vec![0u8; cmdsize];
    // u32 cmd
    out[0..4].copy_from_slice(&macho::LC_LOAD_DYLIB.to_le_bytes());
    // u32 cmdsize
    out[4..8].copy_from_slice(&(cmdsize as u32).to_le_bytes());
    // dylib.name.offset = 24 (path follows the fixed header).
    out[8..12].copy_from_slice(&24u32.to_le_bytes());
    // dylib.timestamp = 2 (matches what ld emits — arbitrary
    // non-zero value; dyld doesn't check it).
    out[12..16].copy_from_slice(&2u32.to_le_bytes());
    // dylib.current_version = 0x0001_0000 (1.0.0). Dyld
    // checks the dependency's actual version against
    // compatibility_version below; current_version is
    // informational.
    out[16..20].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    // dylib.compatibility_version = 0x0001_0000 (1.0.0).
    // Setting low avoids `compatibility_version too low`
    // errors at load when the actual library is anything
    // sensible.
    out[20..24].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    // Path bytes + trailing NUL (the rest is already zero
    // from the vec![0; cmdsize] init).
    let path_bytes = path.as_bytes();
    out[24..24 + path_bytes.len()].copy_from_slice(path_bytes);
    // out[24 + path_bytes.len()] = 0 — already zero.
    out
}

/// Remove a single `LC_LOAD_DYLIB` (or related dylib-loading
/// load command) whose embedded path matches `path` from the
/// load-command list in `bytes`. Shifts subsequent load
/// commands up by the removed entry's `cmdsize`, zero-pads the
/// freed tail bytes (which become extra headerpad), and
/// patches `mach_header_64.ncmds` and `sizeofcmds` to match.
///
/// Returns the matched LC's `cmdsize` on success. Returns
/// `Ok(None)` if no entry matched the given path so the caller
/// can surface a recognisable error.
///
/// We match against all four dylib-loading commands the linker
/// might emit: `LC_LOAD_DYLIB`, `LC_LOAD_WEAK_DYLIB`,
/// `LC_REEXPORT_DYLIB`, and `LC_LOAD_UPWARD_DYLIB`. Each shares
/// the same `dylib_command` layout (cmd, cmdsize, name.offset,
/// timestamp, current_version, compatibility_version, then the
/// inline path string).
pub fn strip_lc_load_dylib(
    bytes: &mut Vec<u8>,
    path: &str,
) -> Result<Option<u32>, ContainerWriteError> {
    use object::macho;

    if bytes.len() < 24 {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O remove dep: input too short for mach_header_64".into(),
        ));
    }
    let mut ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let mut sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let lc_start = 32usize;
    let lc_end = lc_start + sizeofcmds as usize;

    let mut cursor = lc_start;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O remove dep: invalid cmdsize {cmdsize} at offset {cursor}",
            )));
        }
        let is_dylib_load = matches!(
            cmd,
            macho::LC_LOAD_DYLIB
                | macho::LC_LOAD_WEAK_DYLIB
                | macho::LC_REEXPORT_DYLIB
                | macho::LC_LOAD_UPWARD_DYLIB,
        );
        if is_dylib_load && cmdsize >= 24 {
            // name.offset is at cursor+8 (relative to the LC
            // start). The path is a NUL-terminated string
            // starting at `cursor + name_off`.
            let name_off =
                u32::from_le_bytes(bytes[cursor + 8..cursor + 12].try_into().unwrap())
                    as usize;
            if name_off >= cmdsize {
                cursor += cmdsize;
                continue;
            }
            let path_start = cursor + name_off;
            let path_end_max = cursor + cmdsize;
            let nul_rel = bytes[path_start..path_end_max].iter().position(|&b| b == 0);
            let path_end = match nul_rel {
                Some(n) => path_start + n,
                None => path_end_max,
            };
            let entry_path = std::str::from_utf8(&bytes[path_start..path_end]).ok();
            if entry_path == Some(path) {
                // Shift subsequent LCs up by cmdsize and zero
                // the freed tail bytes (which are now extra
                // headerpad).
                let tail_start = cursor + cmdsize;
                let tail_end = lc_end;
                bytes.copy_within(tail_start..tail_end, cursor);
                let freed_start = tail_end - cmdsize;
                for b in &mut bytes[freed_start..tail_end] {
                    *b = 0;
                }
                ncmds -= 1;
                sizeofcmds -= cmdsize as u32;
                bytes[16..20].copy_from_slice(&ncmds.to_le_bytes());
                bytes[20..24].copy_from_slice(&sizeofcmds.to_le_bytes());
                return Ok(Some(cmdsize as u32));
            }
        }
        cursor += cmdsize;
    }
    Ok(None)
}

/// Inputs to the Mach-O intra-`__TEXT` append writer.
pub struct IntraTextAppend {
    /// vmaddr inside an existing `__TEXT` free region where
    /// the bytes will be placed. Must come from
    /// [`MachOLayout::allocate_in_text`].
    pub vaddr: u64,
    /// File offset matching `vaddr` (also from
    /// `allocate_in_text`).
    pub file_offset: u64,
    /// Bytes to write. Must fit within the free region's
    /// remaining space.
    pub bytes: Vec<u8>,
}

/// Build a Mach-O byte stream that writes `appended.bytes`
/// into a free region inside the existing `__TEXT` segment,
/// applies any pending section-byte overrides, then re-signs
/// the result ad-hoc via `codesign`. Crucially does NOT add
/// a new `LC_SEGMENT_64` — App Store submissions reject
/// dylibs / executables with more than one R-X segment, so
/// this path is the iOS-compatible one.
///
/// Caller responsibilities:
///
/// - `appended.vaddr` and `appended.file_offset` must come
///   from a successful
///   [`MachOLayout::allocate_in_text`] call; otherwise the
///   bytes may overlap existing section content.
/// - `appended.bytes` is laid out at the given vmaddr — the
///   caller's rewriter pipeline must have resolved
///   PC-relative operands against that address.
pub fn write_with_intra_text_append(
    container: &Container,
    appended: IntraTextAppend,
    section_overrides: &[(usize, Vec<u8>)],
) -> Result<Vec<u8>, ContainerWriteError> {
    let image = container
        .macho_image
        .as_ref()
        .ok_or(ContainerWriteError::ElfImageMissing)?;
    let layout = &image.layout;

    // Validate the placement against the free regions to
    // catch misuse (`vaddr/file_offset` not from
    // `allocate_in_text`).
    let region = layout
        .text_free_regions()
        .into_iter()
        .find(|r| {
            appended.vaddr >= r.vaddr
                && appended.vaddr + appended.bytes.len() as u64 <= r.vaddr + r.size
        })
        .ok_or_else(|| {
            ContainerWriteError::ObjectWrite(format!(
                "Mach-O intra-text: vaddr 0x{:x} + size {} doesn't fit any \
                 __TEXT free region",
                appended.vaddr,
                appended.bytes.len(),
            ))
        })?;
    // Sanity: file_offset is consistent with vaddr.
    let expected_file_offset = region.file_offset + (appended.vaddr - region.vaddr);
    if expected_file_offset != appended.file_offset {
        return Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O intra-text: file_offset 0x{:x} doesn't match expected \
             0x{expected_file_offset:x} for vaddr 0x{:x}",
            appended.file_offset, appended.vaddr,
        )));
    }

    let mut bytes = image.raw_bytes.clone();

    // Apply section-byte overrides at original offsets
    // (same constraint as the round-trip and append-segment
    // paths: same length only).
    for (idx, override_bytes) in section_overrides {
        let section = &container.sections[*idx];
        let Some(entry) = layout
            .sections
            .iter()
            .find(|s| s.vaddr == section.address && s.file_offset > 0)
        else {
            continue;
        };
        if override_bytes.len() as u64 != entry.size {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O intra-text: override for {:?} ({} bytes) doesn't \
                 match captured size ({} bytes); length-changing in-place \
                 edits aren't supported",
                section.name,
                override_bytes.len(),
                entry.size,
            )));
        }
        let start = entry.file_offset as usize;
        let end = start + override_bytes.len();
        bytes[start..end].copy_from_slice(override_bytes);
    }

    // Write the appended bytes at the chosen file offset.
    // The bytes are inside the existing __TEXT region; no
    // load commands change.
    let off = appended.file_offset as usize;
    if off + appended.bytes.len() > bytes.len() {
        return Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O intra-text: write at file offset 0x{:x} extends past \
             end of file ({} bytes)",
            appended.file_offset,
            bytes.len(),
        )));
    }
    bytes[off..off + appended.bytes.len()].copy_from_slice(&appended.bytes);

    // Re-sign in place. codesign --force overwrites the
    // existing signature blob; segment offsets and __LINKEDIT
    // are unchanged, so it's the simple case (same as the
    // round-trip path).
    sign_ad_hoc(&mut bytes)?;

    Ok(bytes)
}

/// Append a rebuilt export trie + extended symtab/strtab to
/// the file and patch the relevant load commands to point at
/// the new copies. The new blobs go past the (already-shifted)
/// __LINKEDIT region so codesign treats them as part of
/// __LINKEDIT when it extends the segment.
fn extend_exports_in_place(
    bytes: &mut Vec<u8>,
    layout: &crate::container::macho_image::MachOLayout,
    exports: &[MachOExportRequest],
    linkedit_shift: u64,
) -> Result<(), ContainerWriteError> {
    use crate::container::macho_export_trie as trie;
    use crate::container::macho_symtab as symtab;
    use object::macho;

    // Phase 5 needs LC_DYLD_EXPORTS_TRIE, LC_SYMTAB, and
    // LC_DYSYMTAB in the input. (We don't yet support
    // LC_DYLD_INFO_ONLY-style export-tries; those live in a
    // different place but the same shape.)
    let exports_trie_orig = layout
        .exports_trie
        .ok_or_else(|| {
            ContainerWriteError::ObjectWrite(
                "Mach-O export: input has no LC_DYLD_EXPORTS_TRIE; \
                 LC_DYLD_INFO_ONLY-style export tries aren't yet \
                 supported"
                    .into(),
            )
        })?;
    let symtab_orig = layout.symtab.ok_or_else(|| {
        ContainerWriteError::ObjectWrite(
            "Mach-O export: input has no LC_SYMTAB".into(),
        )
    })?;
    let dysymtab_orig = layout.dysymtab.ok_or_else(|| {
        ContainerWriteError::ObjectWrite(
            "Mach-O export: input has no LC_DYSYMTAB".into(),
        )
    })?;

    // Read the existing trie + symtab + strtab from their
    // post-shift positions. The shift_linkedit_offsets pass
    // already updated the LC fields in the load-command list,
    // but we kept the pre-shift offsets in the captured
    // `layout` for exactly this reason.
    let trie_off_now = (exports_trie_orig.dataoff + linkedit_shift) as usize;
    let trie_size = exports_trie_orig.datasize as usize;
    let trie_bytes = bytes[trie_off_now..trie_off_now + trie_size].to_vec();
    let mut existing_exports = trie::parse(&trie_bytes)?;

    let symoff_now = (symtab_orig.symoff + linkedit_shift) as usize;
    let symtab_bytes_len = symtab_orig.nsyms as usize * 16;
    let mut symtab_entries =
        symtab::parse_symtab(&bytes[symoff_now..symoff_now + symtab_bytes_len])?;

    let stroff_now = (symtab_orig.stroff + linkedit_shift) as usize;
    let strsize = symtab_orig.strsize as usize;
    let mut strtab_bytes = bytes[stroff_now..stroff_now + strsize].to_vec();

    // For each requested export:
    //   1. append name to strtab
    //   2. add nlist_64 entry (external defined)
    //   3. add MachOExport to the trie list
    //
    // We pick the section index for the new symbols as the
    // last section in the file — the __APPENDED segment's
    // section, which gets reserved at the end of the section
    // header table by Mach-O's section-numbering. Actually,
    // Mach-O symbol n_sect is 1-indexed across ALL sections in
    // the file. Counting from the parsed layout: the new
    // __APPENDED section is the last one. Its section number
    // = layout.sections.len() + 1.
    //
    // Phase 3 added one section (`__appended` inside
    // `__APPENDED`), so the new section's 1-indexed number is
    // layout.sections.len() + 1.
    let appended_n_sect = (layout.sections.len() as u8) + 1;

    for export in exports {
        let (strtab_off, new_strtab) = symtab::append_strtab(&strtab_bytes, &export.name);
        strtab_bytes = new_strtab;
        let nlist = symtab::external_defined_nlist(strtab_off, appended_n_sect, export.vaddr);
        // Insert at iextdefsym + nextdefsym (just after the
        // existing externals). We update nextdefsym below.
        let insert_at = (dysymtab_orig.iextdefsym + dysymtab_orig.nextdefsym) as usize;
        symtab_entries.insert(insert_at, nlist);
        existing_exports.push(trie::MachOExport {
            name: export.name.clone(),
            flags: 0,
            address_offset: export.vaddr,
        });
    }
    let new_dysymtab_nextdefsym = dysymtab_orig.nextdefsym + exports.len() as u32;

    // Encode the new blobs.
    let new_trie_bytes = trie::build(&existing_exports);
    let new_symtab_bytes = symtab::encode_symtab(&symtab_entries);
    let new_strtab_bytes = strtab_bytes;

    // Place them at the end of the current file. Order: trie,
    // symtab, strtab. Each starts at the next 8-byte boundary
    // past the previous (codesign / dyld are happier with
    // aligned linkedit blobs).
    let mut new_trie_off = bytes.len() as u64;
    new_trie_off = (new_trie_off + 7) & !7;
    bytes.resize(new_trie_off as usize, 0);
    bytes.extend_from_slice(&new_trie_bytes);

    let mut new_symtab_off = bytes.len() as u64;
    new_symtab_off = (new_symtab_off + 7) & !7;
    bytes.resize(new_symtab_off as usize, 0);
    bytes.extend_from_slice(&new_symtab_bytes);

    let new_strtab_off = bytes.len() as u64;
    bytes.extend_from_slice(&new_strtab_bytes);

    // Patch LC_DYLD_EXPORTS_TRIE.
    patch_lc_linkedit_data(
        bytes,
        macho::LC_DYLD_EXPORTS_TRIE,
        new_trie_off,
        new_trie_bytes.len() as u64,
    )?;

    // Patch LC_SYMTAB. nsyms grew by `exports.len()`.
    patch_lc_symtab(
        bytes,
        new_symtab_off,
        symtab_orig.nsyms + exports.len() as u32,
        new_strtab_off,
        new_strtab_bytes.len() as u32,
    )?;

    // Patch LC_DYSYMTAB.nextdefsym, and shift iundefsym to
    // follow.
    patch_lc_dysymtab(
        bytes,
        dysymtab_orig.iextdefsym,
        new_dysymtab_nextdefsym,
        dysymtab_orig.iundefsym + exports.len() as u32,
        dysymtab_orig.nundefsym,
    )?;

    // Extend __LINKEDIT.filesize/vmsize so it covers the new
    // blobs. codesign will further extend filesize when it
    // writes the signature; we just need to ensure the new
    // blobs aren't outside __LINKEDIT.
    let new_file_end = bytes.len() as u64;
    extend_linkedit_segment(bytes, layout.load_commands_offset, new_file_end)?;

    Ok(())
}

/// Patch the `dataoff` / `datasize` fields of an
/// `linkedit_data_command` (e.g. LC_DYLD_EXPORTS_TRIE,
/// LC_FUNCTION_STARTS) by walking the load-command list.
fn patch_lc_linkedit_data(
    bytes: &mut [u8],
    cmd_id: u32,
    dataoff: u64,
    datasize: u64,
) -> Result<(), ContainerWriteError> {
    let lc = find_load_command(bytes, cmd_id)?;
    let Some(off) = lc else {
        return Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O export: load command 0x{cmd_id:x} not found",
        )));
    };
    bytes[off + 8..off + 12].copy_from_slice(&(dataoff as u32).to_le_bytes());
    bytes[off + 12..off + 16].copy_from_slice(&(datasize as u32).to_le_bytes());
    Ok(())
}

fn patch_lc_symtab(
    bytes: &mut [u8],
    symoff: u64,
    nsyms: u32,
    stroff: u64,
    strsize: u32,
) -> Result<(), ContainerWriteError> {
    use object::macho;
    let Some(off) = find_load_command(bytes, macho::LC_SYMTAB)? else {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O export: LC_SYMTAB not found".into(),
        ));
    };
    bytes[off + 8..off + 12].copy_from_slice(&(symoff as u32).to_le_bytes());
    bytes[off + 12..off + 16].copy_from_slice(&nsyms.to_le_bytes());
    bytes[off + 16..off + 20].copy_from_slice(&(stroff as u32).to_le_bytes());
    bytes[off + 20..off + 24].copy_from_slice(&strsize.to_le_bytes());
    Ok(())
}

fn patch_lc_dysymtab(
    bytes: &mut [u8],
    iextdefsym: u32,
    nextdefsym: u32,
    iundefsym: u32,
    nundefsym: u32,
) -> Result<(), ContainerWriteError> {
    use object::macho;
    let Some(off) = find_load_command(bytes, macho::LC_DYSYMTAB)? else {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O export: LC_DYSYMTAB not found".into(),
        ));
    };
    // Field layout: cmd, cmdsize, ilocalsym, nlocalsym,
    // iextdefsym, nextdefsym, iundefsym, nundefsym, ...
    bytes[off + 16..off + 20].copy_from_slice(&iextdefsym.to_le_bytes());
    bytes[off + 20..off + 24].copy_from_slice(&nextdefsym.to_le_bytes());
    bytes[off + 24..off + 28].copy_from_slice(&iundefsym.to_le_bytes());
    bytes[off + 28..off + 32].copy_from_slice(&nundefsym.to_le_bytes());
    Ok(())
}

/// Extend `__LINKEDIT` segment's filesize/vmsize so its file
/// region runs from its (post-shift) fileoff to `new_end`.
/// Used by Phase 5 to grow __LINKEDIT enough to cover newly-
/// appended trie/symtab/strtab blobs.
fn extend_linkedit_segment(
    bytes: &mut [u8],
    load_commands_offset: u64,
    new_file_end: u64,
) -> Result<(), ContainerWriteError> {
    use object::macho;

    if bytes.len() < 24 {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O export: input too short for mach_header_64".into(),
        ));
    }
    let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let lc_start = load_commands_offset as usize;
    let lc_end = lc_start + sizeofcmds;

    let mut cursor = lc_start;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            break;
        }
        if cmd == macho::LC_SEGMENT_64 && cmdsize >= 24 {
            let segname = &bytes[cursor + 8..cursor + 24];
            let end = segname.iter().position(|&b| b == 0).unwrap_or(segname.len());
            if std::str::from_utf8(&segname[..end]).unwrap_or("") == "__LINKEDIT" {
                let fileoff = u64::from_le_bytes(
                    bytes[cursor + 40..cursor + 48].try_into().unwrap(),
                );
                let new_filesize = new_file_end.saturating_sub(fileoff);
                let old_filesize = u64::from_le_bytes(
                    bytes[cursor + 48..cursor + 56].try_into().unwrap(),
                );
                if new_filesize > old_filesize {
                    bytes[cursor + 48..cursor + 56]
                        .copy_from_slice(&new_filesize.to_le_bytes());
                }
                let old_vmsize = u64::from_le_bytes(
                    bytes[cursor + 32..cursor + 40].try_into().unwrap(),
                );
                if new_filesize > old_vmsize {
                    let new_vmsize = (new_filesize + 0x4000 - 1) & !(0x4000 - 1);
                    bytes[cursor + 32..cursor + 40]
                        .copy_from_slice(&new_vmsize.to_le_bytes());
                }
                return Ok(());
            }
        }
        cursor += cmdsize;
    }
    Err(ContainerWriteError::ObjectWrite(
        "Mach-O export: __LINKEDIT segment not found".into(),
    ))
}

/// Find a load command by `cmd` ID and return its offset in
/// the byte buffer (byte position of the start of the
/// load_command struct).
fn find_load_command(
    bytes: &[u8],
    cmd_id: u32,
) -> Result<Option<usize>, ContainerWriteError> {
    if bytes.len() < 32 {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O export: input too short".into(),
        ));
    }
    let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let lc_end = 32 + sizeofcmds;
    let mut cursor = 32usize;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            break;
        }
        if cmd == cmd_id {
            return Ok(Some(cursor));
        }
        cursor += cmdsize;
    }
    Ok(None)
}

/// Strip the `LC_CODE_SIGNATURE` load command from the file
/// (if present), patching the mach_header to drop one ncmds
/// entry and shrink sizeofcmds by 16. The signature blob in
/// `__LINKEDIT` is left in place — `codesign --force` will
/// either reuse the space or extend `__LINKEDIT` as it sees
/// fit. Headerpad gains 16 bytes.
fn strip_lc_code_signature(bytes: &mut Vec<u8>) -> Result<(), ContainerWriteError> {
    use object::macho;

    if bytes.len() < 24 {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O writer: input too short for mach_header_64".into(),
        ));
    }
    let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;

    let lc_start = 32usize;
    let lc_end = lc_start + sizeofcmds;
    let mut cursor = lc_start;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: invalid cmdsize {cmdsize} at offset {cursor}",
            )));
        }
        if cmd == macho::LC_CODE_SIGNATURE {
            // Splice the LC out of the load-command list.
            bytes.drain(cursor..cursor + cmdsize);
            // The drained bytes are now zero-padded by writing
            // zeros at the new tail (since the LC list got
            // shorter, anything that was after lc_end has now
            // shifted forward — but the headerpad was already
            // zeros). To keep all subsequent file offsets
            // stable, splice back the same number of zero
            // bytes at the new lc_end.
            let new_lc_end = lc_end - cmdsize;
            bytes.splice(new_lc_end..new_lc_end, std::iter::repeat(0u8).take(cmdsize));
            // Update ncmds / sizeofcmds.
            bytes[16..20].copy_from_slice(&(ncmds - 1).to_le_bytes());
            bytes[20..24].copy_from_slice(
                &((sizeofcmds - cmdsize) as u32).to_le_bytes(),
            );
            return Ok(());
        }
        cursor += cmdsize;
    }
    Ok(())
}

/// Find the byte offset of an `LC_SEGMENT_64` whose segname
/// matches `target` in the load-command list. Returns the
/// offset of the start of the segment_command_64 (so callers
/// can index into its fields directly), or `None` if no such
/// segment exists.
#[allow(dead_code)]
fn find_segment_lc_offset(
    bytes: &[u8],
    layout: &crate::container::macho_image::MachOLayout,
    target: &str,
) -> Result<Option<usize>, ContainerWriteError> {
    use object::macho;

    let lc_start = layout.load_commands_offset as usize;
    // Iterate using the post-splice load-command list (which
    // may now be longer than layout.sizeofcmds). Use
    // mach_header.sizeofcmds from the byte buffer directly.
    if bytes.len() < 24 {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O writer: input too short for mach_header_64 sizeofcmds".into(),
        ));
    }
    let sizeofcmds =
        u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let lc_end = lc_start + sizeofcmds;

    let mut cursor = lc_start;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            break;
        }
        if cmd == macho::LC_SEGMENT_64 && cmdsize >= 24 {
            let segname_field = &bytes[cursor + 8..cursor + 24];
            let end = segname_field
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(segname_field.len());
            let name = std::str::from_utf8(&segname_field[..end]).unwrap_or("");
            if name == target {
                return Ok(Some(cursor));
            }
        }
        cursor += cmdsize;
    }
    Ok(None)
}

/// Find the byte offset where a new load command should be
/// spliced in. Returns the offset of the existing
/// LC_CODE_SIGNATURE if one exists, otherwise the end of the
/// load-command list (so the new LC sits last).
fn find_insert_offset_for_lc(
    bytes: &[u8],
    lc_start: usize,
    lc_end: usize,
) -> Result<usize, ContainerWriteError> {
    use object::macho;

    let mut cursor = lc_start;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O append: invalid cmdsize {cmdsize} at offset {cursor}",
            )));
        }
        if cmd == macho::LC_CODE_SIGNATURE {
            return Ok(cursor);
        }
        cursor += cmdsize;
    }
    Ok(lc_end)
}

/// Build a 152-byte (segment_command_64 + one section_64) load
/// command for the appended segment.
fn build_segment_command_64(
    appended: &AppendedSegment,
    file_offset: u64,
    cmdsize: u64,
) -> Result<Vec<u8>, ContainerWriteError> {
    use object::macho;

    let segname = "__APPENDED";
    let sectname = "__appended";
    let bytes_len = appended.bytes.len() as u64;

    // VM_PROT_READ | VM_PROT_EXECUTE = 0x5.
    // macOS rejects RWX mappings for non-special segments
    // (returns EACCES at mmap time). Phase 3 only needs R-X
    // for code; later phases that require writable
    // initialiser slots will need to either split into two
    // segments or use mprotect at runtime.
    let prot: u32 = 0x5;

    let mut out: Vec<u8> = Vec::with_capacity(cmdsize as usize);
    out.extend_from_slice(&macho::LC_SEGMENT_64.to_le_bytes());
    out.extend_from_slice(&(cmdsize as u32).to_le_bytes());
    let mut name_field = [0u8; 16];
    let bytes_seg = segname.as_bytes();
    name_field[..bytes_seg.len().min(16)]
        .copy_from_slice(&bytes_seg[..bytes_seg.len().min(16)]);
    out.extend_from_slice(&name_field);
    out.extend_from_slice(&appended.vaddr.to_le_bytes());
    out.extend_from_slice(&bytes_len.to_le_bytes()); // vmsize
    out.extend_from_slice(&file_offset.to_le_bytes());
    out.extend_from_slice(&bytes_len.to_le_bytes()); // filesize
    out.extend_from_slice(&prot.to_le_bytes()); // maxprot
    out.extend_from_slice(&prot.to_le_bytes()); // initprot
    out.extend_from_slice(&1u32.to_le_bytes()); // nsects
    out.extend_from_slice(&0u32.to_le_bytes()); // segment flags

    // section_64
    let mut sect_field = [0u8; 16];
    let bytes_sect = sectname.as_bytes();
    sect_field[..bytes_sect.len().min(16)]
        .copy_from_slice(&bytes_sect[..bytes_sect.len().min(16)]);
    out.extend_from_slice(&sect_field);
    out.extend_from_slice(&name_field); // segname (matches segment)
    out.extend_from_slice(&appended.vaddr.to_le_bytes()); // addr
    out.extend_from_slice(&bytes_len.to_le_bytes()); // size
    out.extend_from_slice(&(file_offset as u32).to_le_bytes()); // offset (32-bit!)
    out.extend_from_slice(&2u32.to_le_bytes()); // align 2^2 = 4
    out.extend_from_slice(&0u32.to_le_bytes()); // reloff
    out.extend_from_slice(&0u32.to_le_bytes()); // nreloc
    // S_REGULAR section type with S_ATTR_SOME_INSTRUCTIONS so
    // disassemblers know this region holds code.
    out.extend_from_slice(&macho::S_ATTR_SOME_INSTRUCTIONS.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved1
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved2
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved3

    Ok(out)
}
