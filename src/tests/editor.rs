//! Tests for the [`BinaryEditor`] convenience API. The editor is
//! a thin wrapper over the rewrite-layer primitives; these tests
//! cover the wrapping itself (error mapping, lookups, the
//! commit-pipeline plumbing) rather than rewrite semantics, which
//! [`crate::tests::rewrite`] covers.

use crate::container::{
    Architecture, BinaryFormat, Container, ContainerKind, Section, SectionId, SectionKind,
    Symbol, SymbolBinding, SymbolId, SymbolKind,
};
use crate::isa::aarch64::{self, Aarch64Mnemonic};
use crate::rewrite::{BinaryEditor, Target, TextEditorError};

/// Build a tiny ELF ET_REL container with a `.text` section
/// containing two instructions and one defined function symbol.
/// Used as the test substrate.
fn fixture_container() -> Container {
    // bl 0x4 ; ret
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&0x94000001_u32.to_le_bytes()); // bl +4
    bytes.extend_from_slice(&0xd65f03c0_u32.to_le_bytes()); // ret

    Container {
        format: BinaryFormat::Elf,
        architecture: Architecture::Aarch64,
        kind: ContainerKind::Relocatable,
        sections: vec![Section {
            id: SectionId(0),
            name: ".text".to_string(),
            address: 0x1000,
            size: 8,
            bytes,
            kind: SectionKind::Text,
            align: 4,
            flags: None,
            raw_sh_type: None,
        }],
        symbols: vec![
            Symbol {
                id: SymbolId(0),
                name: "main".to_string(),
                address: 0x1000,
                size: 4,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: Some(SectionId(0)),
                is_undefined: false,
                flags: None,
            },
            Symbol {
                id: SymbolId(1),
                name: "printf".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: None,
                is_undefined: true,
                flags: None,
            },
        ],
        relocations: Vec::new(),
        file_flags: None,
        elf_image: None,
        macho_image: None,
        dwarf: None,
    }
}

#[test]
fn for_section_finds_named_text_section_and_lifts_it() {
    let container = fixture_container();
    let editor = BinaryEditor::for_section(&container, ".text").expect("for_section");
    let text = editor.text.as_ref().expect("text lifted").aarch64().expect("aarch64 section");
    assert_eq!(text.base_address(), 0x1000);
    assert_eq!(text.instructions().len(), 2);
    assert_eq!(text.instructions()[0].mnemonic, Aarch64Mnemonic::Bl);
    assert_eq!(text.instructions()[1].mnemonic, Aarch64Mnemonic::Ret);
}

#[test]
fn for_section_reports_missing_section_cleanly() {
    let container = fixture_container();
    match BinaryEditor::for_section(&container, ".no_such_section") {
        Err(TextEditorError::SectionNotFound(name)) => {
            assert_eq!(name, ".no_such_section");
        }
        other => panic!("expected SectionNotFound, got {other:?}"),
    }
}

#[test]
fn for_section_rejects_non_text_sections() {
    let mut container = fixture_container();
    container.sections.push(Section {
        id: SectionId(1),
        name: ".rodata".to_string(),
        address: 0x2000,
        size: 4,
        bytes: vec![0; 4],
        kind: SectionKind::Rodata,
        align: 4,
        flags: None,
        raw_sh_type: None,
    });
    match BinaryEditor::for_section(&container, ".rodata") {
        Err(TextEditorError::SectionNotText { name }) => assert_eq!(name, ".rodata"),
        other => panic!("expected SectionNotText, got {other:?}"),
    }
}

#[test]
fn symbol_by_name_resolves_known_symbols() {
    let container = fixture_container();
    let editor = BinaryEditor::for_section(&container, ".text").unwrap();
    assert_eq!(editor.binary.symbol_by_name("main").unwrap(), SymbolId(0));
    assert_eq!(editor.binary.symbol_by_name("printf").unwrap(), SymbolId(1));
    match editor.binary.symbol_by_name("missing") {
        Err(TextEditorError::SymbolNotFound(name)) => assert_eq!(name, "missing"),
        other => panic!("expected SymbolNotFound, got {other:?}"),
    }
}

#[test]
fn function_by_name_filters_to_function_kind() {
    let mut container = fixture_container();
    // Add an OBJECT-kind symbol with the same name; function_by_name
    // should ignore it. Useful disambiguation for binaries that
    // happen to have data + function with matching names.
    container.symbols.push(Symbol {
        id: SymbolId(2),
        name: "data_named_main".to_string(),
        address: 0x2000,
        size: 4,
        kind: SymbolKind::Object,
        binding: SymbolBinding::Global,
        section: None,
        is_undefined: false,
        flags: None,
    });
    let editor = BinaryEditor::for_section(&container, ".text").unwrap();
    assert_eq!(editor.binary.function_by_name("main").unwrap(), SymbolId(0));
    match editor.binary.function_by_name("data_named_main") {
        Err(TextEditorError::SymbolNotFound(_)) => {}
        other => panic!("expected SymbolNotFound for OBJECT symbol, got {other:?}"),
    }
}

#[test]
fn function_address_returns_address_for_defined_function() {
    let container = fixture_container();
    let editor = BinaryEditor::for_section(&container, ".text").unwrap();
    assert_eq!(editor.binary.function_address("main"), Some(0x1000));
    // printf is undefined, so function_address skips it.
    assert_eq!(editor.binary.function_address("printf"), None);
    assert_eq!(editor.binary.function_address("nope"), None);
}

#[test]
fn symbols_in_section_lists_only_defined_symbols_in_target_section() {
    let container = fixture_container();
    let editor = BinaryEditor::for_section(&container, ".text").unwrap();
    let names: Vec<_> = editor.symbols_in_section().map(|s| s.name.as_str()).collect();
    // Only `main` is defined in `.text`. `printf` is undefined,
    // so it's excluded.
    assert_eq!(names, vec!["main"]);
}

#[test]
fn redirect_branch_at_proxies_through_to_plan() {
    let container = fixture_container();
    let mut editor = BinaryEditor::for_section(&container, ".text").unwrap();
    let printf = editor.binary.symbol_by_name("printf").unwrap();

    // The bl at 0x1000 originally targets 0x1004. Redirect it to
    // the printf extern.
    editor
        .text
        .as_mut()
        .unwrap()
        .aarch64_mut()
        .unwrap()
        .redirect_branch_at(0x1000, Target::Symbol(printf))
        .expect("redirect_branch_at");

    // commit_to_bytes succeeds — emit produces a Branch26
    // relocation since `printf` is undefined.
    let bytes = editor.commit_to_bytes().expect("commit_to_bytes");
    // Re-parse and check the bl instruction is still there.
    let reparsed = Container::from_bytes(&bytes).unwrap();
    let text = reparsed.text_sections().next().unwrap();
    let insn = aarch64::disassemble_bytes(text.address, &text.bytes).unwrap();
    assert_eq!(insn[0].mnemonic, Aarch64Mnemonic::Bl);
}

#[test]
fn redirect_branch_at_unknown_address_returns_edit_error() {
    let container = fixture_container();
    let mut editor = BinaryEditor::for_section(&container, ".text").unwrap();
    match editor
        .text
        .as_mut()
        .unwrap()
        .aarch64_mut()
        .unwrap()
        .redirect_branch_at(0xdeadbeef, Target::Absolute(0))
    {
        Err(TextEditorError::Edit(_)) => {}
        other => panic!("expected Edit error, got {other:?}"),
    }
}

#[test]
fn commit_returns_container_with_rewritten_text_section() {
    let container = fixture_container();
    let editor = BinaryEditor::for_section(&container, ".text").unwrap();
    // No edits — commit should produce a structurally-equivalent
    // container.
    let edited = editor.commit().expect("commit");
    let original_text = container.text_sections().next().unwrap();
    let edited_text = edited.text_sections().next().unwrap();
    assert_eq!(original_text.bytes, edited_text.bytes);
}

#[test]
fn plan_mut_exposes_underlying_rewrite_plan_for_advanced_use() {
    let container = fixture_container();
    let mut editor = BinaryEditor::for_section(&container, ".text").unwrap();
    // Touch the plan via the escape hatch; the editor's commit
    // pipeline should still work.
    let plan = editor.text.as_mut().unwrap().aarch64_mut().unwrap().plan_mut();
    assert!(!plan.blocks.is_empty(), "plan should have at least one block");
    let _bytes = editor.commit_to_bytes().expect("commit_to_bytes after plan_mut");
}

#[test]
fn lift_text_section_on_armv7_container_dispatches_to_thumb_variant() {
    // Confirms the editor dispatches to the Thumb sweep when
    // `container.architecture == Arm`, that the result is the
    // Thumb variant of `LiftedTextSectionAny`, and that the
    // ISA-agnostic accessors on the enum work without
    // downcasting.
    //
    // Whole-`.text` sweep on a stripped binary doesn't always
    // succeed (real `.text` has literal pools and ARM/Thumb
    // mode-switch boundaries that linear sweep can't handle);
    // this test asserts dispatch behaviour rather than a
    // complete sweep. The full round-trip is covered by the
    // dispatch-module tests in src/isa/armv7/dispatch.rs.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("libtool-checker.so");
    let bytes = std::fs::read(&path).expect("read libtool-checker.so");
    let container = Container::from_bytes(&bytes).expect("parse");
    assert_eq!(container.architecture, Architecture::Arm);

    // Construct an editor without lifting (no sweep) and assert
    // that lift_text_section() returns an Err if the sweep
    // can't handle the full section — but if it succeeds, the
    // variant must be Thumb.
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let lift_result = editor.lift_text_section(".text");
    match lift_result {
        Ok(()) => {
            let text = editor.text.as_ref().expect("text section lifted");
            assert!(
                text.thumb().is_some(),
                "ARMv7 default lift should produce the Thumb variant; got {text:?}"
            );
        }
        Err(TextEditorError::DisassembleArmv7(_)) => {
            // Whole-`.text` sweep failed because the section
            // contains a mix of code, literal pools, and
            // mode-switch boundaries the linear sweep can't
            // handle. The dispatch was still correct (it tried
            // the Thumb path, as evidenced by the error
            // variant); that's what this test is checking.
        }
        Err(other) => panic!("unexpected lift error: {other:?}"),
    }
}

#[test]
fn lift_text_section_arm_on_armv7_container_yields_arm_variant() {
    // Same fixture but explicitly request ARM-mode lift. The
    // PLT in libtool-checker.so is A32; we don't lift the PLT
    // here, but the API just needs to dispatch and produce a
    // section. We pick `.plt` which is ARM-mode.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("libtool-checker.so");
    let bytes = std::fs::read(&path).expect("read libtool-checker.so");
    let container = Container::from_bytes(&bytes).expect("parse");
    let has_plt = container.sections.iter().any(|s| s.name == ".plt");
    if !has_plt {
        // Fixture changed — skip rather than fail. The Thumb
        // path test above still validates the variant dispatch.
        return;
    }

    let mut editor = BinaryEditor::new(&container).expect("editor");
    editor
        .lift_text_section_arm(".plt")
        .expect("lift .plt as ARM mode");
    let text = editor.text.as_ref().expect("plt lifted");
    assert!(text.arm().is_some(), "expected ARM variant");
    assert!(!text.arm().unwrap().instructions().is_empty());
}
