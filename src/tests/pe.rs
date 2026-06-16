//! Tests for the PE writer's round-trip + in-place section-override
//! logic.
//!
//! These build a `Container` + `PeImage` by hand (synthesizing a tiny
//! fake image) so the suite stays hermetic — no committed PE binaries.
//! The full read→write path was additionally verified by hand against a
//! real PE32+ EFI executable during development.

use crate::container::pe_image::{PeImage, PeSectionFile};
use crate::container::{
    Architecture, BinaryFormat, Container, ContainerKind, Section, SectionId, SectionKind,
};

/// Build a fake 16-byte PE image with a single `.text` section whose
/// raw data occupies file offsets 8..12, and a neutral container whose
/// `.text` bytes are `text`.
fn fake_pe(text: Vec<u8>) -> Container {
    let raw_bytes = vec![
        0, 1, 2, 3, 4, 5, 6, 7, 0xAA, 0xBB, 0xCC, 0xDD, 12, 13, 14, 15,
    ];
    let pe_image = PeImage {
        raw_bytes,
        sections: vec![PeSectionFile {
            name: ".text".to_string(),
            file_offset: 8,
            file_size: 4,
        }],
        image_base: 0x1_4000_0000,
        entry_point: 0,
        imports: Vec::new(),
        exports: Vec::new(),
        base_relocs: Vec::new(),
    };
    Container {
        format: BinaryFormat::Pe,
        architecture: Architecture::X86_64,
        kind: ContainerKind::Executable,
        sections: vec![Section {
            id: SectionId(0),
            name: ".text".to_string(),
            address: 0x1000,
            size: text.len() as u64,
            bytes: text,
            kind: SectionKind::Text,
            align: 16,
            flags: None,
            raw_sh_type: None,
        }],
        symbols: Vec::new(),
        relocations: Vec::new(),
        file_flags: None,
        elf_image: None,
        macho_image: None,
        pe_image: Some(pe_image),
        dwarf: None,
    }
}

#[test]
fn pe_no_edit_round_trip_is_byte_identical() {
    // The container's .text matches the captured raw bytes, so emitting
    // reproduces the input verbatim.
    let container = fake_pe(vec![0xAA, 0xBB, 0xCC, 0xDD]);
    let out = container.to_bytes().expect("pe write");
    assert_eq!(out, container.pe_image.as_ref().unwrap().raw_bytes);
}

#[test]
fn pe_in_place_edit_overrides_only_that_section() {
    // Replace .text bytes; only file offsets 8..12 should change.
    let container = fake_pe(vec![0x11, 0x22, 0x33, 0x44]);
    let out = container.to_bytes().expect("pe write");
    assert_eq!(&out[8..12], &[0x11, 0x22, 0x33, 0x44]);
    // Everything outside the section is untouched.
    assert_eq!(&out[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(&out[12..], &[12, 13, 14, 15]);
}

#[test]
fn pe_length_growing_edit_is_rejected() {
    // A .text override longer than the captured raw size can't be done
    // in place — the writer must refuse rather than corrupt the image.
    let container = fake_pe(vec![0x11, 0x22, 0x33, 0x44, 0x55]); // 5 > 4
    assert!(container.to_bytes().is_err());
}

#[test]
fn pe_shorter_edit_overrides_prefix_only() {
    // A shorter override writes its bytes and leaves the rest of the
    // section's raw bytes intact (tail not zeroed).
    let container = fake_pe(vec![0x11, 0x22]);
    let out = container.to_bytes().expect("pe write");
    assert_eq!(&out[8..12], &[0x11, 0x22, 0xCC, 0xDD]);
}
