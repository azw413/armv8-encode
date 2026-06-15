//! Emit linked PE images (`.exe` / `.dll`) from a [`Container`] +
//! [`PeImage`] pair.
//!
//! Mirrors the Mach-O writer's round-trip-plus-overrides approach: the
//! captured [`PeImage::raw_bytes`] are copied verbatim, then any edited
//! section's bytes are spliced in at that section's original file
//! offset. This preserves the section table, data directories, and the
//! optional header exactly, so the image still loads.
//!
//! ## What this deliberately doesn't do
//!
//! - **Length-changing edits.** An override whose byte length differs
//!   from the captured section size would shift everything after it and
//!   invalidate `SizeOfImage`, the section table, and the data
//!   directories. Those are rejected with
//!   [`ContainerWriteError::ObjectWrite`] rather than silently
//!   producing a broken image. Growing a PE (new sections, import
//!   injection) is a later stage.
//! - **Checksum recompute.** The optional header's `CheckSum` is left
//!   as-is; Windows only enforces it for drivers and some signed
//!   images. In-place `.text`/`.data` edits to ordinary executables
//!   load without a valid checksum.

use crate::container::pe_image::PeImage;
use crate::container::{Container, ContainerWriteError};

/// Serialize a PE container back to bytes, applying in-place section
/// overrides on top of the captured raw image.
pub fn write(container: &Container) -> Result<Vec<u8>, ContainerWriteError> {
    let image = container
        .pe_image
        .as_ref()
        .ok_or(ContainerWriteError::PeImageMissing)?;

    let mut bytes = image.raw_bytes.clone();
    apply_section_overrides(container, image, &mut bytes)?;
    Ok(bytes)
}

fn apply_section_overrides(
    container: &Container,
    image: &PeImage,
    out: &mut [u8],
) -> Result<(), ContainerWriteError> {
    for section in &container.sections {
        if section.bytes.is_empty() {
            continue;
        }
        // Match the neutral section back to its on-disk placement by
        // name. Sections without a file presence (e.g. uninitialized
        // `.bss`-style sections) won't be found and are skipped.
        let Some(entry) = image.section_by_name(&section.name) else {
            continue;
        };
        if entry.file_offset == 0 || entry.file_size == 0 {
            continue;
        }
        // Only the bytes physically present on disk can be overridden.
        // A section's virtual size can exceed its raw size (tail-zero
        // padding); clamp the override to the raw size.
        let writable = section.bytes.len().min(entry.file_size as usize);
        if section.bytes.len() as u64 > entry.file_size {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "PE writer: section {:?} override ({} bytes) is larger than \
                 the captured on-disk size ({} bytes); length-growing edits \
                 aren't supported",
                section.name,
                section.bytes.len(),
                entry.file_size,
            )));
        }
        let start = entry.file_offset as usize;
        let end = start + writable;
        if end > out.len() {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "PE writer: section {:?} extends past end of file",
                section.name,
            )));
        }
        out[start..end].copy_from_slice(&section.bytes[..writable]);
    }
    Ok(())
}
