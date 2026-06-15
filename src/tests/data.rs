//! Tests for the data-section rewrite layer (`rewrite::data`).

use crate::container::{
    Architecture, BinaryFormat, Container, ContainerKind, Relocation, RelocationId,
    RelocationKind, Section, SectionId, SectionKind, Symbol, SymbolBinding, SymbolId, SymbolKind,
};
use crate::rewrite::{
    commit_to_data_container, emit_data_section, DataPayload, DataSection, Target,
};

/// Build a vtable-shaped fixture: a `.rodata` section with two 8-byte
/// pointer slots (`fn_a` then `fn_b`) flanked by an `R_AARCH64_ABS64`
/// relocation each, plus a defined symbol `vtable` labelling offset 0.
/// Returns the assembled container.
fn vtable_container() -> Container {
    // 16 bytes of zeros — the linker patches in the actual addresses
    // when applying the relocations. Source bytes are conventionally
    // zero for ABS64 slots awaiting relocation.
    let bytes = vec![0u8; 16];

    Container {
        format: BinaryFormat::Elf,
        architecture: Architecture::Aarch64,
        kind: ContainerKind::Relocatable,
        sections: vec![
            Section {
                id: SectionId(0),
                name: ".text".to_string(),
                address: 0,
                size: 0,
                bytes: vec![],
                kind: SectionKind::Text,
                align: 4,
                flags: None,
                raw_sh_type: None,
            },
            Section {
                id: SectionId(1),
                name: ".rodata.vtable".to_string(),
                address: 0,
                size: bytes.len() as u64,
                bytes,
                kind: SectionKind::Rodata,
                align: 8,
                flags: None,
                raw_sh_type: None,
            },
        ],
        symbols: vec![
            Symbol {
                id: SymbolId(0),
                name: "fn_a".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: Some(SectionId(0)),
                is_undefined: false,
                flags: None,
            },
            Symbol {
                id: SymbolId(1),
                name: "fn_b".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: Some(SectionId(0)),
                is_undefined: false,
                flags: None,
            },
            Symbol {
                id: SymbolId(2),
                name: "vtable".to_string(),
                address: 0,
                size: 16,
                kind: SymbolKind::Object,
                binding: SymbolBinding::Global,
                section: Some(SectionId(1)),
                is_undefined: false,
                flags: None,
            },
        ],
        relocations: vec![
            Relocation {
                id: RelocationId(0),
                section: SectionId(1),
                offset: 0,
                kind: RelocationKind::Absolute,
                size: 64,
                addend: 0,
                symbol: Some(SymbolId(0)),
            },
            Relocation {
                id: RelocationId(1),
                section: SectionId(1),
                offset: 8,
                kind: RelocationKind::Absolute,
                size: 64,
                addend: 0,
                symbol: Some(SymbolId(1)),
            },
        ],
        file_flags: None,
        elf_image: None,
        macho_image: None,
        pe_image: None,
        dwarf: None,
    }
}

#[test]
fn lift_splits_section_at_each_absolute_relocation() {
    let container = vtable_container();
    let lifted = DataSection::lift(&container, SectionId(1)).expect("lift");

    // Both slots are pointer items.
    assert_eq!(lifted.plan.items.len(), 2);
    let item0 = &lifted.plan.items[0];
    let item1 = &lifted.plan.items[1];

    assert_eq!(
        item0.label,
        Some(SymbolId(2)),
        "vtable symbol labels the first item",
    );
    assert!(matches!(
        item0.payload,
        DataPayload::Pointer {
            target: Target::Symbol(SymbolId(0)),
            addend: 0,
            width_bytes: 8,
        }
    ));
    assert!(matches!(
        item1.payload,
        DataPayload::Pointer {
            target: Target::Symbol(SymbolId(1)),
            addend: 0,
            width_bytes: 8,
        }
    ));

    // No data relocations were unhandled.
    assert!(lifted.unhandled_relocations.is_empty());
}

#[test]
fn lift_emits_bytes_around_pointers() {
    // 4 leading bytes, then an ABS64 pointer, then 4 trailing bytes.
    let mut bytes = vec![0xaa, 0xbb, 0xcc, 0xdd];
    bytes.extend_from_slice(&[0u8; 8]); // pointer slot
    bytes.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);

    let container = Container {
        format: BinaryFormat::Elf,
        architecture: Architecture::Aarch64,
        kind: ContainerKind::Relocatable,
        sections: vec![Section {
            id: SectionId(0),
            name: ".rodata.mixed".to_string(),
            address: 0,
            size: bytes.len() as u64,
            bytes,
            kind: SectionKind::Rodata,
            align: 4,
            flags: None,
            raw_sh_type: None,
        }],
        symbols: vec![Symbol {
            id: SymbolId(0),
            name: "extern_target".to_string(),
            address: 0,
            size: 0,
            kind: SymbolKind::Function,
            binding: SymbolBinding::Global,
            section: None,
            is_undefined: true,
            flags: None,
        }],
        relocations: vec![Relocation {
            id: RelocationId(0),
            section: SectionId(0),
            offset: 4,
            kind: RelocationKind::Absolute,
            size: 64,
            addend: 0,
            symbol: Some(SymbolId(0)),
        }],
        file_flags: None,
        elf_image: None,
        macho_image: None,
        pe_image: None,
        dwarf: None,
    };

    let lifted = DataSection::lift(&container, SectionId(0)).unwrap();
    assert_eq!(lifted.plan.items.len(), 3, "leading bytes / ptr / trailing bytes");

    match &lifted.plan.items[0].payload {
        DataPayload::Bytes(b) => assert_eq!(b, &[0xaa, 0xbb, 0xcc, 0xdd]),
        other => panic!("expected leading Bytes, got {other:?}"),
    }
    assert!(matches!(
        lifted.plan.items[1].payload,
        DataPayload::Pointer { width_bytes: 8, .. }
    ));
    match &lifted.plan.items[2].payload {
        DataPayload::Bytes(b) => assert_eq!(b, &[0x11, 0x22, 0x33, 0x44]),
        other => panic!("expected trailing Bytes, got {other:?}"),
    }
}

#[test]
fn lift_passes_unhandled_relocation_kinds_through() {
    // A data section that carries an unexpected relocation kind. Lift
    // should not panic and should report it via unhandled_relocations
    // so commit can re-attach it structurally.
    let bytes = vec![0u8; 8];

    let container = Container {
        format: BinaryFormat::Elf,
        architecture: Architecture::Aarch64,
        kind: ContainerKind::Relocatable,
        sections: vec![Section {
            id: SectionId(0),
            name: ".rodata.weird".to_string(),
            address: 0,
            size: bytes.len() as u64,
            bytes,
            kind: SectionKind::Rodata,
            align: 8,
            flags: None,
            raw_sh_type: None,
        }],
        symbols: vec![Symbol {
            id: SymbolId(0),
            name: "weird_target".to_string(),
            address: 0,
            size: 0,
            kind: SymbolKind::Function,
            binding: SymbolBinding::Global,
            section: None,
            is_undefined: true,
            flags: None,
        }],
        // Use Other(0xdead) — definitely not a recognised pointer
        // shape. Whether or not the linker would accept it isn't the
        // point; the lift's job is to not silently lose it.
        relocations: vec![Relocation {
            id: RelocationId(0),
            section: SectionId(0),
            offset: 0,
            kind: RelocationKind::Other(0xdead),
            size: 32,
            addend: 0,
            symbol: Some(SymbolId(0)),
        }],
        file_flags: None,
        elf_image: None,
        macho_image: None,
        pe_image: None,
        dwarf: None,
    };

    let lifted = DataSection::lift(&container, SectionId(0)).unwrap();
    // No data items are pointers — the whole section came through as
    // one Bytes item.
    assert_eq!(lifted.plan.items.len(), 1);
    assert!(matches!(lifted.plan.items[0].payload, DataPayload::Bytes(_)));
    // The unhandled relocation is preserved for the commit step.
    assert_eq!(lifted.unhandled_relocations.len(), 1);
    assert_eq!(
        lifted.unhandled_relocations[0].kind,
        RelocationKind::Other(0xdead),
    );
}

#[test]
fn redirect_pointer_swaps_target() {
    let container = vtable_container();
    let mut lifted = DataSection::lift(&container, SectionId(1)).unwrap();

    // Swap slot 0 (currently → fn_a) to point at fn_b instead.
    let old = lifted
        .plan
        .redirect_pointer_at(0, Target::Symbol(SymbolId(1)))
        .expect("redirect");
    assert_eq!(old, Target::Symbol(SymbolId(0)));

    let DataPayload::Pointer { target, .. } = &lifted.plan.items[0].payload else {
        panic!("first item should still be a pointer");
    };
    assert_eq!(*target, Target::Symbol(SymbolId(1)));
}

#[test]
fn emit_produces_zero_bytes_and_one_relocation_per_pointer() {
    let container = vtable_container();
    let lifted = DataSection::lift(&container, SectionId(1)).unwrap();
    let output = emit_data_section(&lifted.plan);

    // 16 bytes total, all zero (placeholder for the linker).
    assert_eq!(output.bytes.len(), 16);
    assert!(output.bytes.iter().all(|&b| b == 0));

    // One relocation per pointer slot, in source order.
    assert_eq!(output.relocations.len(), 2);
    assert_eq!(output.relocations[0].offset, 0);
    assert_eq!(output.relocations[0].symbol, SymbolId(0));
    assert_eq!(output.relocations[0].kind, RelocationKind::Absolute);
    assert_eq!(output.relocations[1].offset, 8);
    assert_eq!(output.relocations[1].symbol, SymbolId(1));
}

#[test]
fn emit_preserves_addends() {
    let container = vtable_container();
    let mut lifted = DataSection::lift(&container, SectionId(1)).unwrap();

    // Synthesize a pointer with a non-zero addend (e.g. `&array[3]`).
    let DataPayload::Pointer { addend, .. } = &mut lifted.plan.items[0].payload else {
        panic!("first item should be pointer");
    };
    *addend = 24;

    let output = emit_data_section(&lifted.plan);
    assert_eq!(output.relocations[0].addend, 24);
}

#[test]
fn emit_inserts_alignment_padding_between_items() {
    // An item with align=8 placed after a 1-byte item must get 7 bytes
    // of padding.
    let plan = DataSection {
        source_section: None,
        items: vec![
            crate::rewrite::DataItem {
                label: None,
                align: 1,
                payload: DataPayload::Bytes(vec![0xff]),
            },
            crate::rewrite::DataItem {
                label: None,
                align: 8,
                payload: DataPayload::Pointer {
                    target: Target::Symbol(SymbolId(0)),
                    addend: 0,
                    width_bytes: 8,
                },
            },
        ],
    };

    let output = emit_data_section(&plan);
    // 1 byte content + 7 bytes padding + 8 bytes pointer = 16 bytes.
    assert_eq!(output.bytes.len(), 16);
    assert_eq!(output.bytes[0], 0xff);
    assert!(output.bytes[1..8].iter().all(|&b| b == 0));
    // The relocation lands at offset 8 (after padding), not offset 1.
    assert_eq!(output.relocations[0].offset, 8);
}

#[test]
fn commit_to_data_container_replaces_bytes_and_relocations() {
    let container = vtable_container();
    let mut lifted = DataSection::lift(&container, SectionId(1)).unwrap();

    // Edit: redirect both pointers to the same symbol.
    lifted
        .plan
        .redirect_pointer_at(0, Target::Symbol(SymbolId(1)))
        .unwrap();
    let output = emit_data_section(&lifted.plan);

    let edited = commit_to_data_container(
        &container,
        SectionId(1),
        output,
        lifted.unhandled_relocations,
    );

    // Bytes replaced.
    let section = edited.section(SectionId(1));
    assert_eq!(section.bytes.len(), 16);

    // Relocations on the edited section: both now point at SymbolId(1).
    let relocations: Vec<_> = edited.relocations_for(SectionId(1)).collect();
    assert_eq!(relocations.len(), 2);
    assert_eq!(relocations[0].symbol, Some(SymbolId(1)));
    assert_eq!(relocations[1].symbol, Some(SymbolId(1)));

    // Other-section relocations untouched (here there are none, but
    // verify the slot is empty rather than corrupted).
    let text_relocations: Vec<_> = edited.relocations_for(SectionId(0)).collect();
    assert!(text_relocations.is_empty());
}

#[test]
fn commit_to_data_container_preserves_unhandled_relocations() {
    // Simulate a section with one Absolute (handled) and one
    // Other(...) (passed through). Lift recognises the Absolute, the
    // Other survives via DataLift::unhandled_relocations and reaches
    // the committed container.
    let mut bytes = vec![0u8; 16];
    bytes[8] = 0xff; // distinguish post-pointer area visually

    let container = Container {
        format: BinaryFormat::Elf,
        architecture: Architecture::Aarch64,
        kind: ContainerKind::Relocatable,
        sections: vec![Section {
            id: SectionId(0),
            name: ".rodata.mixed".to_string(),
            address: 0,
            size: bytes.len() as u64,
            bytes,
            kind: SectionKind::Rodata,
            align: 8,
            flags: None,
            raw_sh_type: None,
        }],
        symbols: vec![Symbol {
            id: SymbolId(0),
            name: "fn_a".to_string(),
            address: 0,
            size: 0,
            kind: SymbolKind::Function,
            binding: SymbolBinding::Global,
            section: None,
            is_undefined: true,
            flags: None,
        }],
        relocations: vec![
            Relocation {
                id: RelocationId(0),
                section: SectionId(0),
                offset: 0,
                kind: RelocationKind::Absolute,
                size: 64,
                addend: 0,
                symbol: Some(SymbolId(0)),
            },
            Relocation {
                id: RelocationId(1),
                section: SectionId(0),
                offset: 8,
                kind: RelocationKind::Other(0xdead),
                size: 32,
                addend: 0,
                symbol: Some(SymbolId(0)),
            },
        ],
        file_flags: None,
        elf_image: None,
        macho_image: None,
        pe_image: None,
        dwarf: None,
    };

    let lifted = DataSection::lift(&container, SectionId(0)).unwrap();
    assert_eq!(
        lifted.unhandled_relocations.len(),
        1,
        "Other(0xdead) should be reported as unhandled",
    );

    let output = emit_data_section(&lifted.plan);
    let edited = commit_to_data_container(
        &container,
        SectionId(0),
        output,
        lifted.unhandled_relocations,
    );

    let kinds: Vec<_> = edited
        .relocations_for(SectionId(0))
        .map(|r| r.kind)
        .collect();
    assert!(kinds.contains(&RelocationKind::Absolute));
    assert!(kinds.contains(&RelocationKind::Other(0xdead)));
}

#[test]
fn round_trip_preserves_byte_content_for_pointer_only_sections() {
    // No edits: emitting a freshly-lifted section should produce
    // identical bytes (16 zeros) and the same relocations.
    let container = vtable_container();
    let lifted = DataSection::lift(&container, SectionId(1)).unwrap();
    let output = emit_data_section(&lifted.plan);

    assert_eq!(output.bytes, container.section(SectionId(1)).bytes);
}
