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

/// Walk the captured load commands to find each section's file
/// offset, then overwrite the bytes in `out` with the
/// container's current `.bytes` for that section if it differs
/// from the original.
fn apply_section_overrides(
    container: &Container,
    image: &crate::container::macho_image::MachOImage,
    out: &mut [u8],
) -> Result<(), ContainerWriteError> {
    // Parse the load commands once to build a vaddr→file-offset
    // map for each (segment, section).
    let layout = parse_section_layout(&image.raw_bytes)?;

    // For each container section, check if its current bytes
    // differ from what's in the file at the discovered file
    // offset. Apply the override iff:
    //   - we found a matching entry in the layout (by vaddr);
    //   - the override length matches the original section
    //     size.
    for section in &container.sections {
        if section.bytes.is_empty() {
            continue;
        }
        let Some(entry) = layout.find_by_vaddr(section.address) else {
            // Section not in the file (e.g. zerofill / __common).
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

/// One section's location in the file as captured from the
/// load commands. The vaddr is the dyld virtual address; the
/// file_offset is where to read/write the section's bytes in
/// the on-disk image.
#[derive(Debug, Clone, Copy)]
struct SectionLayoutEntry {
    vaddr: u64,
    file_offset: u64,
    size: u64,
}

#[derive(Debug, Clone, Default)]
struct SectionLayout {
    entries: Vec<SectionLayoutEntry>,
}

impl SectionLayout {
    fn find_by_vaddr(&self, vaddr: u64) -> Option<SectionLayoutEntry> {
        self.entries
            .iter()
            .find(|e| e.vaddr == vaddr)
            .copied()
    }
}

/// Walk the LC_SEGMENT_64 commands and collect each section's
/// (vaddr, file_offset, size). We only handle the 64-bit Mach-O
/// shape — the harness fixture is arm64 and that's all Phase 1
/// targets.
fn parse_section_layout(bytes: &[u8]) -> Result<SectionLayout, ContainerWriteError> {
    use object::macho;

    // Parse the mach_header_64 manually so we don't have to
    // pull in `object`'s read API for this. The header is 32
    // bytes; the load commands follow immediately.
    if bytes.len() < 32 {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O writer: input too short for mach_header_64".into(),
        ));
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    if magic != macho::MH_MAGIC_64 {
        return Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O writer: unsupported magic 0x{magic:08x} (only MH_MAGIC_64 supported)",
        )));
    }
    let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;

    let mut layout = SectionLayout::default();
    let mut cursor = 32usize;
    let cmds_end = cursor + sizeofcmds;
    if cmds_end > bytes.len() {
        return Err(ContainerWriteError::ObjectWrite(
            "Mach-O writer: load commands extend past file end".into(),
        ));
    }

    for _ in 0..ncmds {
        if cursor + 8 > cmds_end {
            return Err(ContainerWriteError::ObjectWrite(
                "Mach-O writer: truncated load command header".into(),
            ));
        }
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > cmds_end {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "Mach-O writer: invalid cmdsize {cmdsize} at offset {cursor}",
            )));
        }

        if cmd == macho::LC_SEGMENT_64 {
            // segment_command_64 is 72 bytes:
            //   u32 cmd, u32 cmdsize,
            //   char segname[16],
            //   u64 vmaddr, u64 vmsize,
            //   u64 fileoff, u64 filesize,
            //   u32 maxprot, u32 initprot,
            //   u32 nsects, u32 flags
            const SEG_HEADER: usize = 72;
            if cmdsize < SEG_HEADER {
                return Err(ContainerWriteError::ObjectWrite(
                    "Mach-O writer: LC_SEGMENT_64 cmdsize < 72".into(),
                ));
            }
            let nsects = u32::from_le_bytes(
                bytes[cursor + 64..cursor + 68].try_into().unwrap(),
            ) as usize;
            // section_64 is 80 bytes:
            //   char sectname[16],
            //   char segname[16],
            //   u64 addr, u64 size,
            //   u32 offset, u32 align,
            //   u32 reloff, u32 nreloc,
            //   u32 flags,
            //   u32 reserved1, u32 reserved2, u32 reserved3
            const SECT_SIZE: usize = 80;
            if cmdsize < SEG_HEADER + nsects * SECT_SIZE {
                return Err(ContainerWriteError::ObjectWrite(
                    "Mach-O writer: LC_SEGMENT_64 cmdsize doesn't fit nsects".into(),
                ));
            }
            for i in 0..nsects {
                let s = cursor + SEG_HEADER + i * SECT_SIZE;
                let addr = u64::from_le_bytes(bytes[s + 32..s + 40].try_into().unwrap());
                let size = u64::from_le_bytes(bytes[s + 40..s + 48].try_into().unwrap());
                let file_offset =
                    u32::from_le_bytes(bytes[s + 48..s + 52].try_into().unwrap()) as u64;
                // file_offset = 0 means zerofill (e.g. __common,
                // __bss). Skip — they have no on-disk bytes.
                if file_offset == 0 {
                    continue;
                }
                layout.entries.push(SectionLayoutEntry {
                    vaddr: addr,
                    file_offset,
                    size,
                });
            }
        }
        cursor += cmdsize;
    }
    Ok(layout)
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

    let out = Command::new("codesign")
        .args(["--sign", "-", "--force"])
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
