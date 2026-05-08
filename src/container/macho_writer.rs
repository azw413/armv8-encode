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
    // one section_64) for the new LC_SEGMENT_64 entry. If
    // there isn't room, fail with actionable guidance — the
    // user must rebuild with bigger headerpad.
    const NEW_LC_SIZE: u64 = 152;
    if layout.headerpad() < NEW_LC_SIZE {
        return Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O append: not enough headerpad room ({} bytes, need {NEW_LC_SIZE}). \
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

    // Strip the existing LC_CODE_SIGNATURE entry. codesign
    // --force adds a fresh one at the right position (past
    // the appended segment) when re-signing, and our prior
    // signature blob's bytes inside __LINKEDIT are now stale
    // anyway. Removing the LC keeps codesign from getting
    // confused about strict-validation invariants ("the
    // signature lives at __LINKEDIT.end" is no longer true).
    strip_lc_code_signature(&mut bytes)?;

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
