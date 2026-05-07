//! Splice rewriter output back into a [`Container`].
//!
//! After `lay_out` + `emit`, you have an [`EmitOutput`] holding the new
//! bytes for a section plus any relocations the linker needs to apply.
//! `commit_to_container` merges both back into a [`Container`] clone:
//! - The targeted section's bytes are replaced with `output.bytes`.
//! - Existing relocations for that section are dropped (their offsets
//!   refer to the *old* bytes, which are now gone) and replaced with the
//!   freshly-emitted ones.
//!
//! Relocations on other sections are untouched. If the rewriter and the
//! original binary together produce a relocation list that needs more
//! sophisticated merging (e.g., partial-section rewrites that preserve
//! some original relocations), do it by hand against
//! [`Container::with_section_bytes`] plus the public `relocations` vec.

use crate::container::{Container, Relocation, RelocationId, RelocationKind, SectionId};
use crate::rewrite::emit::{EmitOutput, EmittedRelocation};

/// Take the result of `emit` and produce a new [`Container`] with the
/// targeted section's bytes and relocations replaced.
pub fn commit_to_container(
    container: &Container,
    section: SectionId,
    output: EmitOutput,
) -> Container {
    let mut edited = container.with_section_bytes(section, output.bytes);

    // The section's existing relocations referenced offsets in the old
    // bytes, which are no longer there. Drop them and rebuild from the
    // emit output.
    edited
        .relocations
        .retain(|relocation| relocation.section != section);

    for emitted in output.relocations {
        edited.relocations.push(into_relocation(section, emitted, &edited));
    }

    edited
}

fn into_relocation(
    section: SectionId,
    emitted: EmittedRelocation,
    container: &Container,
) -> Relocation {
    Relocation {
        id: RelocationId(container.relocations.len()),
        section,
        offset: emitted.offset,
        kind: emitted.kind,
        size: relocation_size_bits(emitted.kind),
        addend: emitted.addend,
        symbol: Some(emitted.symbol),
    }
}

/// Width of the relocated field in bits, as reported by both ELF and
/// Mach-O for AArch64. `Other(_)` falls back to a full instruction word.
fn relocation_size_bits(kind: RelocationKind) -> u8 {
    match kind {
        RelocationKind::Branch26 => 26,
        RelocationKind::Branch19 => 19,
        RelocationKind::Branch14 => 14,
        RelocationKind::AdrpPage21 => 21,
        RelocationKind::AddPageOffset12 => 12,
        RelocationKind::LoadStorePageOffset12 { .. } => 12,
        RelocationKind::Absolute => 64,
        RelocationKind::Other(_) => 32,
    }
}
