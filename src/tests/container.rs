//! Tests for the binary-container layer.
//!
//! Fixtures are synthesized in-memory via `object::write` so the test suite
//! stays hermetic — no external assembler required. Real-binary coverage
//! lands once we have round-trip support (PR 4) and can compare against
//! `otool` / `objdump` output.

use crate::container::{
    Architecture, BinaryFormat, Container, ContainerError, FunctionProvenance, RelocationKind,
    SectionKind, SymbolBinding, SymbolKind,
};
use crate::isa::aarch64;
use object::write::{
    Object as WriteObject, Relocation as WriteRelocation, StandardSection,
    Symbol as WriteSymbol, SymbolSection as WriteSymbolSection,
};
use object::{
    Architecture as ObjArch, BinaryFormat as ObjFormat, Endianness, RelocationEncoding,
    RelocationFlags, SymbolFlags, SymbolKind as ObjSymbolKind, SymbolScope,
};

/// Two valid AArch64 instructions: `nop ; ret` (8 bytes).
fn nop_ret_bytes() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&0xd503201fu32.to_le_bytes()); // nop
    bytes.extend_from_slice(&0xd65f03c0u32.to_le_bytes()); // ret
    bytes
}

fn fresh_object(format: ObjFormat) -> WriteObject<'static> {
    WriteObject::new(format, ObjArch::Aarch64, Endianness::Little)
}

/// Build a tiny object file with a `.text` section containing `nop_ret_bytes`
/// and a `main` symbol pointing at it. Returns the byte stream.
fn build_minimal(format: ObjFormat) -> Vec<u8> {
    let mut obj = fresh_object(format);
    let text_id = obj.section_id(StandardSection::Text);
    let offset = obj.append_section_data(text_id, &nop_ret_bytes(), 4);
    let _symbol_id = obj.add_symbol(WriteSymbol {
        name: b"main".to_vec(),
        value: offset,
        size: 8,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Section(text_id),
        flags: SymbolFlags::None,
    });
    obj.write().expect("write object")
}

#[test]
fn parses_minimal_elf_aarch64() {
    let bytes = build_minimal(ObjFormat::Elf);
    let container = Container::from_bytes(&bytes).expect("parse elf");

    assert_eq!(container.format, BinaryFormat::Elf);
    assert_eq!(container.architecture, Architecture::Aarch64);

    // A `.text` section must be present and contain our 8 bytes of code.
    let text = container
        .text_sections()
        .next()
        .expect("text section present");
    assert_eq!(text.bytes, nop_ret_bytes());
    assert_eq!(text.kind, SectionKind::Text);
}

#[test]
fn parses_minimal_macho_aarch64() {
    let bytes = build_minimal(ObjFormat::MachO);
    let container = Container::from_bytes(&bytes).expect("parse macho");

    assert_eq!(container.format, BinaryFormat::Macho);
    assert_eq!(container.architecture, Architecture::Aarch64);

    let text = container
        .text_sections()
        .next()
        .expect("text section present");
    assert_eq!(text.bytes, nop_ret_bytes());
}

#[test]
fn parses_function_symbol() {
    for format in [ObjFormat::Elf, ObjFormat::MachO] {
        let bytes = build_minimal(format);
        let container = Container::from_bytes(&bytes).unwrap();

        let main = container
            .defined_symbols()
            .find(|symbol| symbol.name.contains("main") || symbol.name == "_main")
            .unwrap_or_else(|| panic!("missing main symbol in {format:?}"));

        assert_eq!(main.kind, SymbolKind::Function);
        assert_eq!(main.binding, SymbolBinding::Global);
        // Symbol sizes are format-quirky: Mach-O's standard symbol table
        // has no size field; the `object` write+read round-trip can
        // produce 0 even when set on the way in. Don't pin an exact value.
        assert!(!main.is_undefined);
        assert!(main.section.is_some());
    }
}

#[test]
fn functions_view_lists_defined_function_symbols() {
    for format in [ObjFormat::Elf, ObjFormat::MachO] {
        let bytes = build_minimal(format);
        let container = Container::from_bytes(&bytes).unwrap();

        let functions = container.functions();
        assert_eq!(functions.len(), 1, "expected one function in {format:?}");
        assert!(functions[0].name.ends_with("main"));
    }
}

#[test]
fn text_section_can_be_disassembled() {
    // End-to-end: parse a container, hand its text bytes to the linear
    // sweep, get back two valid AArch64 instructions.
    let bytes = build_minimal(ObjFormat::Elf);
    let container = Container::from_bytes(&bytes).unwrap();

    let text = container.text_sections().next().unwrap();
    let (base, code) = text.for_disassembly().unwrap();
    let instructions = aarch64::disassemble_bytes(base, code).unwrap();

    assert_eq!(instructions.len(), 2);
    assert_eq!(instructions[0].mnemonic, aarch64::Aarch64Mnemonic::Nop);
    assert_eq!(instructions[1].mnemonic, aarch64::Aarch64Mnemonic::Ret);
}

#[test]
fn parses_branch26_relocation_in_elf() {
    let mut obj = fresh_object(ObjFormat::Elf);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &nop_ret_bytes(), 4);

    let target_symbol = obj.add_symbol(WriteSymbol {
        name: b"target_fn".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });

    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 0,
            symbol: target_symbol,
            addend: 0,
            flags: RelocationFlags::Elf {
                r_type: object::elf::R_AARCH64_CALL26,
            },
        },
    )
    .expect("add relocation");

    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).unwrap();

    let mut relocations: Vec<_> = container.relocations.iter().collect();
    relocations.retain(|r| r.kind == RelocationKind::Branch26);
    assert_eq!(relocations.len(), 1, "exactly one branch26 relocation");
    let reloc = relocations[0];
    assert_eq!(reloc.offset, 0);
    assert!(reloc.symbol.is_some());

    let target = container.symbol(reloc.symbol.unwrap());
    assert!(target.is_undefined);
    assert!(target.name.ends_with("target_fn"));
}

#[test]
fn parses_adrp_pageoff_pair_in_elf() {
    let mut obj = fresh_object(ObjFormat::Elf);
    let text_id = obj.section_id(StandardSection::Text);
    // Two instructions: adrp x0, _data ; add x0, x0, :lo12:_data. Bytes
    // don't matter — the relocations are what we test.
    obj.append_section_data(text_id, &[0; 8], 4);

    let data_symbol = obj.add_symbol(WriteSymbol {
        name: b"_data".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Data,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });

    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 0,
            symbol: data_symbol,
            addend: 0,
            flags: RelocationFlags::Elf {
                r_type: object::elf::R_AARCH64_ADR_PREL_PG_HI21,
            },
        },
    )
    .unwrap();
    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 4,
            symbol: data_symbol,
            addend: 0,
            flags: RelocationFlags::Elf {
                r_type: object::elf::R_AARCH64_ADD_ABS_LO12_NC,
            },
        },
    )
    .unwrap();

    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).unwrap();

    let kinds: Vec<RelocationKind> =
        container.relocations.iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&RelocationKind::AdrpPage21));
    assert!(kinds.contains(&RelocationKind::AddPageOffset12));
}

#[test]
fn parses_branch26_relocation_in_macho() {
    let mut obj = fresh_object(ObjFormat::MachO);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &nop_ret_bytes(), 4);

    let target_symbol = obj.add_symbol(WriteSymbol {
        name: b"_target_fn".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });

    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 0,
            symbol: target_symbol,
            addend: 0,
            flags: RelocationFlags::MachO {
                r_type: object::macho::ARM64_RELOC_BRANCH26,
                r_pcrel: true,
                r_length: 2,
            },
        },
    )
    .expect("add relocation");

    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).unwrap();

    let branches: Vec<_> = container
        .relocations
        .iter()
        .filter(|r| r.kind == RelocationKind::Branch26)
        .collect();
    assert_eq!(branches.len(), 1);
}

#[test]
fn unknown_relocation_falls_through_to_other() {
    // R_AARCH64_PREL64 (260) isn't in our explicit map; must show up as
    // RelocationKind::Other(260).
    let mut obj = fresh_object(ObjFormat::Elf);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &[0; 8], 4);

    let symbol = obj.add_symbol(WriteSymbol {
        name: b"x".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Data,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });

    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 0,
            symbol,
            addend: 0,
            flags: RelocationFlags::Elf {
                r_type: object::elf::R_AARCH64_PREL64,
            },
        },
    )
    .unwrap();

    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).unwrap();

    let other_kinds: Vec<_> = container
        .relocations
        .iter()
        .filter_map(|r| match r.kind {
            RelocationKind::Other(code) => Some(code),
            _ => None,
        })
        .collect();
    assert!(other_kinds.contains(&object::elf::R_AARCH64_PREL64));
}

#[test]
fn unsupported_input_errors_cleanly() {
    let result = Container::from_bytes(b"not a real object file");
    assert!(matches!(result, Err(ContainerError::Parse(_))));
}

#[test]
fn defined_vs_undefined_symbols_partition_correctly() {
    let mut obj = fresh_object(ObjFormat::Elf);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &nop_ret_bytes(), 4);

    obj.add_symbol(WriteSymbol {
        name: b"defined".to_vec(),
        value: 0,
        size: 8,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Section(text_id),
        flags: SymbolFlags::None,
    });
    obj.add_symbol(WriteSymbol {
        name: b"imported".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });

    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).unwrap();

    let defined: Vec<_> = container.defined_symbols().map(|s| s.name.clone()).collect();
    let undefined: Vec<_> = container
        .symbols
        .iter()
        .filter(|s| s.is_undefined)
        .map(|s| s.name.clone())
        .collect();

    assert!(defined.iter().any(|n| n.contains("defined")));
    assert!(undefined.iter().any(|n| n.contains("imported")));
}

#[test]
fn function_symbol_at_resolves_lookups() {
    let bytes = build_minimal(ObjFormat::Elf);
    let container = Container::from_bytes(&bytes).unwrap();
    let main = container
        .defined_symbols()
        .find(|s| s.name.ends_with("main"))
        .unwrap();
    assert_eq!(
        container.function_symbol_at(main.address).map(|s| s.id),
        Some(main.id)
    );
    assert_eq!(container.function_symbol_at(0xdead_beef), None);
}

// Silence the unused-import warning for `RelocationEncoding` — keeping it
// available for the next batch of relocation tests.
#[allow(dead_code)]
const _ENC: RelocationEncoding = RelocationEncoding::AArch64Call;

// ---- Writer round-trip tests --------------------------------------------

mod writer {
    use super::*;
    use crate::container::ContainerWriteError;

    /// Read → write → re-read: the structural content of the second
    /// container should match the first.
    fn round_trip(format: ObjFormat) -> Container {
        let bytes = build_minimal(format);
        let parsed = Container::from_bytes(&bytes).expect("initial parse");
        let written = parsed.to_bytes().expect("to_bytes");
        Container::from_bytes(&written).expect("re-parse written bytes")
    }

    #[test]
    fn elf_round_trip_preserves_text_bytes() {
        let original = Container::from_bytes(&build_minimal(ObjFormat::Elf)).unwrap();
        let reparsed = round_trip(ObjFormat::Elf);

        let original_text = original.text_sections().next().unwrap();
        let reparsed_text = reparsed.text_sections().next().unwrap();
        assert_eq!(original_text.bytes, reparsed_text.bytes);
    }

    #[test]
    fn macho_round_trip_preserves_text_bytes() {
        let original = Container::from_bytes(&build_minimal(ObjFormat::MachO)).unwrap();
        let reparsed = round_trip(ObjFormat::MachO);

        let original_text = original.text_sections().next().unwrap();
        let reparsed_text = reparsed.text_sections().next().unwrap();
        assert_eq!(original_text.bytes, reparsed_text.bytes);
    }

    #[test]
    fn elf_round_trip_preserves_function_symbol() {
        let reparsed = round_trip(ObjFormat::Elf);
        let main = reparsed
            .defined_symbols()
            .find(|symbol| symbol.name.ends_with("main"));
        let main = main.expect("main survived round-trip");
        assert_eq!(main.kind, SymbolKind::Function);
        assert_eq!(main.binding, SymbolBinding::Global);
    }

    #[test]
    fn macho_round_trip_preserves_function_symbol() {
        let reparsed = round_trip(ObjFormat::MachO);
        let main = reparsed
            .defined_symbols()
            .find(|symbol| symbol.name.ends_with("main"));
        let main = main.expect("main survived round-trip");
        assert_eq!(main.kind, SymbolKind::Function);
        assert_eq!(main.binding, SymbolBinding::Global);
    }

    #[test]
    fn elf_round_trip_preserves_branch26_relocation() {
        // Build a stream with a branch26 relocation, write it, re-read.
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &nop_ret_bytes(), 4);
        let target = obj.add_symbol(WriteSymbol {
            name: b"target_fn".to_vec(),
            value: 0,
            size: 0,
            kind: ObjSymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: WriteSymbolSection::Undefined,
            flags: SymbolFlags::None,
        });
        obj.add_relocation(
            text_id,
            object::write::Relocation {
                offset: 0,
                symbol: target,
                addend: 0,
                flags: object::RelocationFlags::Elf {
                    r_type: object::elf::R_AARCH64_CALL26,
                },
            },
        )
        .unwrap();

        let bytes = obj.write().unwrap();
        let parsed = Container::from_bytes(&bytes).unwrap();
        let written = parsed.to_bytes().unwrap();
        let reparsed = Container::from_bytes(&written).unwrap();

        let branch26: Vec<_> = reparsed
            .relocations
            .iter()
            .filter(|r| r.kind == RelocationKind::Branch26)
            .collect();
        assert_eq!(branch26.len(), 1, "branch26 relocation should survive");
        let symbol_id = branch26[0].symbol.expect("relocation must point at a symbol");
        let target_symbol = reparsed.symbol(symbol_id);
        assert!(target_symbol.name.ends_with("target_fn"));
        assert!(target_symbol.is_undefined);
    }

    #[test]
    fn with_section_bytes_replaces_only_targeted_section() {
        let parsed = Container::from_bytes(&build_minimal(ObjFormat::Elf)).unwrap();
        let text_section_id = parsed.text_sections().next().unwrap().id;

        let new_text = vec![0xde, 0xad, 0xbe, 0xef];
        let edited = parsed.with_section_bytes(text_section_id, new_text.clone());
        assert_eq!(edited.section(text_section_id).bytes, new_text);
        assert_eq!(edited.section(text_section_id).size, 4);

        // Other sections untouched.
        for (original, modified) in parsed.sections.iter().zip(edited.sections.iter()) {
            if original.id == text_section_id {
                continue;
            }
            assert_eq!(original.bytes, modified.bytes);
            assert_eq!(original.size, modified.size);
        }
    }

    #[test]
    fn edit_text_then_write_then_reparse_sees_the_edit() {
        // End-to-end: read fixture, replace text bytes (as if rewriter
        // produced them), write, parse the result, verify the new text
        // shows up.
        let parsed = Container::from_bytes(&build_minimal(ObjFormat::Elf)).unwrap();
        let text_id = parsed.text_sections().next().unwrap().id;

        // Replace `nop ; ret` with `mov x0, #0 ; ret`.
        let new_text = {
            let mut bytes = Vec::with_capacity(8);
            bytes.extend_from_slice(&0xd2800000u32.to_le_bytes()); // mov x0, #0
            bytes.extend_from_slice(&0xd65f03c0u32.to_le_bytes()); // ret
            bytes
        };
        let edited = parsed.with_section_bytes(text_id, new_text.clone());
        let written = edited.to_bytes().expect("to_bytes");
        let reparsed = Container::from_bytes(&written).expect("reparse");

        let reparsed_text = reparsed.text_sections().next().unwrap();
        assert_eq!(reparsed_text.bytes, new_text, "edit survived the write");
    }

    #[test]
    fn macho_branch19_relocation_is_rejected_by_writer() {
        // Hand-craft a container with a Branch19 + Mach-O. Mach-O has no
        // ARM64 standard relocation type for this; the writer must
        // surface that explicitly.
        let mut container = Container::from_bytes(&build_minimal(ObjFormat::MachO)).unwrap();
        // Inject a Branch19 relocation pointing at the existing main symbol.
        let main_id = container
            .defined_symbols()
            .next()
            .expect("main symbol present")
            .id;
        let text_id = container.text_sections().next().unwrap().id;
        container.relocations.push(crate::container::Relocation {
            id: crate::container::RelocationId(container.relocations.len()),
            section: text_id,
            offset: 0,
            kind: RelocationKind::Branch19,
            size: 32,
            addend: 0,
            symbol: Some(main_id),
        });

        match container.to_bytes() {
            Err(ContainerWriteError::UnsupportedRelocation { format, kind }) => {
                assert_eq!(format, BinaryFormat::Macho);
                assert_eq!(kind, RelocationKind::Branch19);
            }
            other => panic!("expected UnsupportedRelocation, got {other:?}"),
        }
    }

    #[test]
    fn other_architecture_is_rejected_by_writer() {
        let mut container = Container::from_bytes(&build_minimal(ObjFormat::Elf)).unwrap();
        container.architecture = crate::container::Architecture::Other;
        assert_eq!(
            container.to_bytes(),
            Err(ContainerWriteError::UnsupportedArchitecture)
        );
    }

    #[test]
    fn shared_object_kind_is_rejected_by_writer_until_stage_5() {
        // The writer is hard-wired to ET_REL output via
        // `object::write::Object`. Round-tripping a SharedObject /
        // Executable-shaped container would silently re-emit it as
        // ET_REL, breaking dynamic linking. Surface that explicitly.
        use crate::container::ContainerKind;
        let mut container = Container::from_bytes(&build_minimal(ObjFormat::Elf)).unwrap();
        container.kind = ContainerKind::SharedObject;
        match container.to_bytes() {
            Err(ContainerWriteError::UnsupportedKind { kind }) => {
                assert_eq!(kind, ContainerKind::SharedObject);
            }
            other => panic!("expected UnsupportedKind, got {other:?}"),
        }
    }

    #[test]
    fn executable_kind_is_rejected_by_writer_until_stage_5() {
        use crate::container::ContainerKind;
        let mut container = Container::from_bytes(&build_minimal(ObjFormat::Elf)).unwrap();
        container.kind = ContainerKind::Executable;
        match container.to_bytes() {
            Err(ContainerWriteError::UnsupportedKind { kind }) => {
                assert_eq!(kind, ContainerKind::Executable);
            }
            other => panic!("expected UnsupportedKind, got {other:?}"),
        }
    }

    #[test]
    fn elf_relocatable_input_is_classified_as_relocatable() {
        // Reader sanity check: the synthetic build_minimal fixture is
        // an ET_REL .o, so its `kind` should be Relocatable. Read this
        // alongside the SharedObject / Executable rejection tests
        // above so future shifts in classifier behaviour can't pass
        // the rejection tests by accident.
        use crate::container::ContainerKind;
        let container = Container::from_bytes(&build_minimal(ObjFormat::Elf)).unwrap();
        assert_eq!(container.kind, ContainerKind::Relocatable);
    }

    #[test]
    fn macho_object_input_is_classified_as_relocatable() {
        use crate::container::ContainerKind;
        let container = Container::from_bytes(&build_minimal(ObjFormat::MachO)).unwrap();
        assert_eq!(container.kind, ContainerKind::Relocatable);
    }

    #[test]
    fn read_rewrite_write_pipeline_changes_text_bytes() {
        // Full pipeline: read an ELF, lift the text section to a rewrite
        // plan, redirect a branch, emit, splice the new bytes back into
        // the container, and write a fresh ELF whose text section
        // contains the redirected branch encoding.
        use crate::isa::aarch64;
        use crate::mc::build_cfg;
        use crate::rewrite::{emit, lay_out, RewritePlan, Target};

        // Source: `b 0x4 ; nop ; ret` — three instructions.
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        let mut text_bytes = Vec::with_capacity(12);
        text_bytes.extend_from_slice(&0x14000001u32.to_le_bytes()); // b +4
        text_bytes.extend_from_slice(&0xd503201fu32.to_le_bytes()); // nop
        text_bytes.extend_from_slice(&0xd65f03c0u32.to_le_bytes()); // ret
        obj.append_section_data(text_id, &text_bytes, 4);
        obj.add_symbol(WriteSymbol {
            name: b"main".to_vec(),
            value: 0,
            size: 12,
            kind: ObjSymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: WriteSymbolSection::Section(text_id),
            flags: SymbolFlags::None,
        });
        let initial_bytes = obj.write().unwrap();
        let container = Container::from_bytes(&initial_bytes).unwrap();

        let text_section_id = container.text_sections().next().unwrap().id;
        let text_section = container.section(text_section_id);
        let (base, code) = text_section.for_disassembly().unwrap();
        let instructions = aarch64::disassemble_bytes(base, code).unwrap();
        let cfg = build_cfg(&instructions);
        let mut plan = RewritePlan::lift(&cfg, &instructions);

        // Redirect the leading `b +4` to point to the `ret` (offset 8)
        // instead — non-trivial change to the encoded word.
        plan.redirect_branch(0, Target::Absolute(8)).unwrap();

        let layout = lay_out(&plan, base, None).unwrap();
        let new_text = emit(&plan, &layout, None).unwrap().bytes;
        assert_ne!(new_text, text_bytes, "rewriter must change the encoding");

        // Splice new text into the container and write a fresh ELF.
        let edited = container.with_section_bytes(text_section_id, new_text.clone());
        let written = edited.to_bytes().unwrap();

        // Re-read and verify the text section carries the new bytes
        // verbatim.
        let reparsed = Container::from_bytes(&written).unwrap();
        let final_text = reparsed.text_sections().next().unwrap();
        assert_eq!(final_text.bytes, new_text);
    }
}

// ---- DWARF fixtures and tests --------------------------------------------

mod dwarf {
    use super::*;
    use gimli::write::{
        Address, AttributeValue as WriteAttrValue, Dwarf as WriteDwarf, EndianVec, LineProgram,
        LineString, Sections, Unit,
    };
    use gimli::{constants, Encoding, Format, LineEncoding, RunTimeEndian};

    /// Build a minimal DWARF v4 description of a single CU with one
    /// subprogram and serialise it into ELF debug-section byte arrays.
    /// Returns `(section_name, bytes)` pairs ready to attach to an
    /// `object::write::Object`.
    fn build_minimal_dwarf(
        function_name: &[u8],
        function_address: u64,
        function_size: u64,
    ) -> Vec<(String, Vec<u8>)> {
        let encoding = Encoding {
            address_size: 8,
            format: Format::Dwarf32,
            version: 4,
        };

        let comp_dir = LineString::String(b".".to_vec());
        let comp_name = LineString::String(b"hello.s".to_vec());
        let line_program = LineProgram::new(
            encoding,
            LineEncoding::default(),
            comp_dir,
            comp_name,
            None,
        );

        let mut unit = Unit::new(encoding, line_program);
        let root_id = unit.root();
        {
            let root = unit.get_mut(root_id);
            root.set(
                constants::DW_AT_name,
                WriteAttrValue::String(b"hello.s".to_vec()),
            );
            root.set(
                constants::DW_AT_comp_dir,
                WriteAttrValue::String(b".".to_vec()),
            );
            root.set(
                constants::DW_AT_low_pc,
                WriteAttrValue::Address(Address::Constant(function_address)),
            );
        }

        let func_id = unit.add(root_id, constants::DW_TAG_subprogram);
        {
            let func = unit.get_mut(func_id);
            func.set(
                constants::DW_AT_name,
                WriteAttrValue::String(function_name.to_vec()),
            );
            func.set(
                constants::DW_AT_low_pc,
                WriteAttrValue::Address(Address::Constant(function_address)),
            );
            func.set(
                constants::DW_AT_high_pc,
                WriteAttrValue::Udata(function_size),
            );
            func.set(constants::DW_AT_decl_line, WriteAttrValue::Udata(7));
        }

        let mut dwarf = WriteDwarf::new();
        dwarf.units.add(unit);

        let mut sections = Sections::new(EndianVec::new(RunTimeEndian::Little));
        dwarf.write(&mut sections).expect("dwarf write");

        let mut out: Vec<(String, Vec<u8>)> = Vec::new();
        sections
            .for_each(|id, data| {
                let bytes = data.clone().into_vec();
                if !bytes.is_empty() {
                    out.push((id.name().to_string(), bytes));
                }
                Ok::<(), gimli::write::Error>(())
            })
            .expect("collect dwarf sections");
        out
    }

    /// Build an ELF object with our standard `nop ; ret` `.text` plus the
    /// minimal DWARF described above.
    fn build_elf_with_dwarf(function_address: u64, function_size: u64) -> Vec<u8> {
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &nop_ret_bytes(), 4);
        obj.add_symbol(WriteSymbol {
            name: b"main".to_vec(),
            value: function_address,
            size: function_size,
            kind: ObjSymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: WriteSymbolSection::Section(text_id),
            flags: SymbolFlags::None,
        });

        for (name, bytes) in build_minimal_dwarf(b"main", function_address, function_size) {
            let id = obj.add_section(
                Vec::new(),
                name.into_bytes(),
                object::SectionKind::Debug,
            );
            obj.append_section_data(id, &bytes, 1);
        }

        obj.write().expect("write elf with dwarf")
    }

    #[test]
    fn parses_subprogram_from_minimal_elf() {
        let bytes = build_elf_with_dwarf(0x0, 8);
        let container = Container::from_bytes(&bytes).expect("parse");

        let info = container.dwarf.as_ref().expect("dwarf info populated");
        assert_eq!(info.functions.len(), 1);
        let func = &info.functions[0];
        assert_eq!(func.name, "main");
        assert_eq!(func.address, 0);
        assert_eq!(func.size, 8);
        assert_eq!(func.source_line, Some(7));
    }

    #[test]
    fn no_dwarf_yields_none() {
        // Re-use the basic fixture from the outer module — it has no
        // debug sections at all.
        let bytes = build_minimal(ObjFormat::Elf);
        let container = Container::from_bytes(&bytes).expect("parse");
        assert!(container.dwarf.is_none());
    }

    #[test]
    fn dwarf_supplements_missing_symbol_for_functions_view() {
        // Build an ELF with DWARF but *no* function symbol — this is the
        // stripped-binary case.
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &nop_ret_bytes(), 4);

        for (name, bytes) in build_minimal_dwarf(b"hidden_main", 0x0, 8) {
            let id = obj.add_section(
                Vec::new(),
                name.into_bytes(),
                object::SectionKind::Debug,
            );
            obj.append_section_data(id, &bytes, 1);
        }

        let bytes = obj.write().expect("write");
        let container = Container::from_bytes(&bytes).unwrap();

        let functions = container.functions();
        assert_eq!(functions.len(), 1, "DWARF should fill in the function");
        assert_eq!(functions[0].name, "hidden_main");
        assert_eq!(functions[0].provenance, FunctionProvenance::Dwarf);
    }

    #[test]
    fn symbol_takes_precedence_over_dwarf_at_same_address() {
        // Both symbol and DWARF describe a function at 0x0. The merged
        // view should list one entry, sourced from the symbol.
        let bytes = build_elf_with_dwarf(0x0, 8);
        let container = Container::from_bytes(&bytes).unwrap();

        let functions = container.functions();
        let at_zero: Vec<_> = functions.iter().filter(|f| f.address == 0).collect();
        assert_eq!(
            at_zero.len(),
            1,
            "expected a single merged entry at 0x0, got {at_zero:?}"
        );
        assert_eq!(
            at_zero[0].provenance,
            FunctionProvenance::Symbol,
            "symbol provenance should win over DWARF"
        );
    }
}

// ---- Faithful-round-trip tests for ELF (Stage 1) -----------------------
//
// These cover the fields beyond the structural set: file `e_flags`,
// section `sh_flags` / alignment, symbol `st_other` (visibility),
// section-relative relocations (no symbol), and addend-bearing
// `RelocationKind::Other` passthrough.

mod elf_faithful {
    use super::*;
    use crate::container::FileFlags;

    /// Read the raw ELF symbol entries from a freshly written byte stream.
    /// Returns `(name, st_info, st_other)` for each non-null entry.
    fn raw_symtab(bytes: &[u8]) -> Vec<(String, u8, u8)> {
        use object::read::elf::{ElfFile64, Sym};
        let elf = ElfFile64::<Endianness>::parse(bytes).unwrap();
        let endian = elf.endian();
        let symtab = elf.elf_symbol_table();
        symtab
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 0)
            .map(|(_, sym)| {
                let name = std::str::from_utf8(sym.name(endian, symtab.strings()).unwrap_or(b""))
                    .unwrap_or("?")
                    .to_string();
                (name, sym.st_info(), sym.st_other())
            })
            .collect()
    }

    #[test]
    fn e_flags_round_trip_through_container() {
        // AArch64 ELF carries BTI/PAC feature flags in `e_flags`. Set a
        // recognisable non-zero value and confirm it survives.
        const FLAGS: u32 = 0x0000_0001; // arbitrary marker bit

        let mut obj = fresh_object(ObjFormat::Elf);
        obj.flags = object::FileFlags::Elf {
            os_abi: 0,
            abi_version: 0,
            e_flags: FLAGS,
        };
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &nop_ret_bytes(), 4);
        let bytes = obj.write().unwrap();

        let parsed = Container::from_bytes(&bytes).unwrap();
        assert_eq!(
            parsed.file_flags,
            Some(FileFlags::Elf {
                os_abi: 0,
                abi_version: 0,
                e_flags: FLAGS,
            }),
        );

        let written = parsed.to_bytes().unwrap();
        let reparsed = Container::from_bytes(&written).unwrap();
        assert_eq!(
            reparsed.file_flags,
            Some(FileFlags::Elf {
                os_abi: 0,
                abi_version: 0,
                e_flags: FLAGS,
            }),
            "e_flags must survive a round-trip",
        );
    }

    #[test]
    fn hidden_visibility_round_trips() {
        // STV_HIDDEN must round-trip via SymbolFlags::Elf passthrough.
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &nop_ret_bytes(), 4);
        // STB_GLOBAL << 4 | STT_FUNC = 0x12; STV_HIDDEN = 2.
        obj.add_symbol(WriteSymbol {
            name: b"hidden_fn".to_vec(),
            value: 0,
            size: 8,
            kind: ObjSymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: WriteSymbolSection::Section(text_id),
            flags: SymbolFlags::Elf {
                st_info: 0x12,
                st_other: 2,
            },
        });
        let bytes = obj.write().unwrap();
        let parsed = Container::from_bytes(&bytes).unwrap();
        let written = parsed.to_bytes().unwrap();

        let entries = raw_symtab(&written);
        let entry = entries
            .iter()
            .find(|(name, _, _)| name == "hidden_fn")
            .expect("hidden_fn must appear in re-emitted symtab");
        assert_eq!(entry.1, 0x12, "st_info must round-trip");
        assert_eq!(entry.2, 2, "st_other (STV_HIDDEN) must round-trip");
    }

    #[test]
    fn unknown_relocation_round_trip_preserves_addend() {
        // R_AARCH64_PREL64 (260) isn't in our explicit relocation enum, so
        // it lifts to `RelocationKind::Other(260)`. The non-zero addend
        // must survive read → write → re-read.
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &[0; 8], 4);
        let symbol = obj.add_symbol(WriteSymbol {
            name: b"target".to_vec(),
            value: 0,
            size: 0,
            kind: ObjSymbolKind::Data,
            scope: SymbolScope::Linkage,
            weak: false,
            section: WriteSymbolSection::Undefined,
            flags: SymbolFlags::None,
        });
        obj.add_relocation(
            text_id,
            WriteRelocation {
                offset: 0,
                symbol,
                addend: 0x1234_5678,
                flags: RelocationFlags::Elf {
                    r_type: object::elf::R_AARCH64_PREL64,
                },
            },
        )
        .unwrap();

        let bytes = obj.write().unwrap();
        let parsed = Container::from_bytes(&bytes).unwrap();
        let written = parsed.to_bytes().unwrap();
        let reparsed = Container::from_bytes(&written).unwrap();

        let reloc = reparsed
            .relocations
            .iter()
            .find(|r| matches!(r.kind, RelocationKind::Other(260)))
            .expect("PREL64 relocation must survive");
        assert_eq!(reloc.addend, 0x1234_5678);
    }

    #[test]
    fn section_alignment_round_trips() {
        // Set a non-default alignment (16) on .text and confirm the writer
        // preserves it through the container's `align` field.
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &nop_ret_bytes(), 16);
        let bytes = obj.write().unwrap();

        let parsed = Container::from_bytes(&bytes).unwrap();
        let text = parsed
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .expect("text present");
        assert_eq!(text.align, 16, "alignment captured on read");

        let written = parsed.to_bytes().unwrap();
        let reparsed = Container::from_bytes(&written).unwrap();
        let text2 = reparsed
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .unwrap();
        assert_eq!(text2.align, 16, "alignment survives round-trip");
    }

    #[test]
    fn section_sh_flags_round_trip() {
        // Capture sh_flags on a stock section. .text has SHF_ALLOC|SHF_EXECINSTR
        // = 0x6, which the writer should reproduce after we read+write it.
        let bytes = build_minimal(ObjFormat::Elf);
        let parsed = Container::from_bytes(&bytes).unwrap();
        let text = parsed
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .unwrap();
        let original_flags = text.flags.expect("text section had ELF flags on read");

        let written = parsed.to_bytes().unwrap();
        let reparsed = Container::from_bytes(&written).unwrap();
        let reparsed_text = reparsed
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .unwrap();
        assert_eq!(
            reparsed_text.flags,
            Some(original_flags),
            "sh_flags must survive a round-trip",
        );
    }

    #[test]
    fn section_relative_relocation_survives_round_trip() {
        // A relocation whose target is a section, not a named symbol. The
        // writer must synthesize a section symbol on the fly so the reloc
        // survives.
        //
        // Compilers emit these as R_AARCH64_ABS64 with a section-symbol
        // target after section subsumption — common in `.eh_frame` and
        // debug info pointing into `.text`.
        let mut obj = fresh_object(ObjFormat::Elf);
        let text_id = obj.section_id(StandardSection::Text);
        obj.append_section_data(text_id, &[0; 16], 4);
        // `object::write` exposes section_symbol() to materialize an
        // STT_SECTION symbol that points at the section itself.
        let text_section_symbol = obj.section_symbol(text_id);
        // Relocate offset 8 of .text to point at .text+0 (a self-reference,
        // structurally valid).
        obj.add_relocation(
            text_id,
            WriteRelocation {
                offset: 8,
                symbol: text_section_symbol,
                addend: 0,
                flags: RelocationFlags::Elf {
                    r_type: object::elf::R_AARCH64_ABS64,
                },
            },
        )
        .unwrap();
        let bytes = obj.write().unwrap();

        let parsed = Container::from_bytes(&bytes).unwrap();
        // The reloc may show up either with a section-kind symbol or no
        // symbol at all depending on how `object::read` lifts it. The
        // important thing is that round-trip preserves it.
        let initial_count = parsed.relocations.len();
        assert!(initial_count >= 1);

        let written = parsed.to_bytes().unwrap();
        let reparsed = Container::from_bytes(&written).unwrap();
        assert_eq!(
            reparsed.relocations.len(),
            initial_count,
            "section-relative relocation must survive (initial={initial_count})",
        );
    }
}
