//! Tests for the [`TextEditor`] convenience API. The editor is a
//! thin wrapper over the rewrite-layer primitives; these tests
//! cover the wrapping itself (error mapping, lookups, the
//! commit-pipeline plumbing) rather than rewrite semantics, which
//! [`crate::tests::rewrite`] covers.

use crate::container::{
    Architecture, BinaryFormat, Container, ContainerKind, Section, SectionId, SectionKind,
    Symbol, SymbolBinding, SymbolId, SymbolKind,
};
use crate::isa::aarch64::{self, Aarch64Mnemonic};
use crate::rewrite::{Target, TextEditor, TextEditorError};

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
        dwarf: None,
    }
}

#[test]
fn for_section_finds_named_text_section_and_lifts_it() {
    let container = fixture_container();
    let editor = TextEditor::for_section(&container, ".text").expect("for_section");
    assert_eq!(editor.base_address(), 0x1000);
    assert_eq!(editor.instructions().len(), 2);
    assert_eq!(editor.instructions()[0].mnemonic, Aarch64Mnemonic::Bl);
    assert_eq!(editor.instructions()[1].mnemonic, Aarch64Mnemonic::Ret);
}

#[test]
fn for_section_reports_missing_section_cleanly() {
    let container = fixture_container();
    match TextEditor::for_section(&container, ".no_such_section") {
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
    match TextEditor::for_section(&container, ".rodata") {
        Err(TextEditorError::SectionNotText { name }) => assert_eq!(name, ".rodata"),
        other => panic!("expected SectionNotText, got {other:?}"),
    }
}

#[test]
fn symbol_by_name_resolves_known_symbols() {
    let container = fixture_container();
    let editor = TextEditor::for_section(&container, ".text").unwrap();
    assert_eq!(editor.symbol_by_name("main").unwrap(), SymbolId(0));
    assert_eq!(editor.symbol_by_name("printf").unwrap(), SymbolId(1));
    match editor.symbol_by_name("missing") {
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
    let editor = TextEditor::for_section(&container, ".text").unwrap();
    assert_eq!(editor.function_by_name("main").unwrap(), SymbolId(0));
    match editor.function_by_name("data_named_main") {
        Err(TextEditorError::SymbolNotFound(_)) => {}
        other => panic!("expected SymbolNotFound for OBJECT symbol, got {other:?}"),
    }
}

#[test]
fn function_address_returns_address_when_in_section() {
    let container = fixture_container();
    let editor = TextEditor::for_section(&container, ".text").unwrap();
    assert_eq!(editor.function_address("main"), Some(0x1000));
    // printf is undefined and has section=None, so it isn't "in" the section.
    assert_eq!(editor.function_address("printf"), None);
    assert_eq!(editor.function_address("nope"), None);
}

#[test]
fn symbols_in_section_lists_only_defined_symbols_in_target_section() {
    let container = fixture_container();
    let editor = TextEditor::for_section(&container, ".text").unwrap();
    let names: Vec<_> = editor.symbols_in_section().map(|s| s.name.as_str()).collect();
    // Only `main` is defined in `.text`. `printf` is undefined,
    // so it's excluded.
    assert_eq!(names, vec!["main"]);
}

#[test]
fn redirect_branch_at_proxies_through_to_plan() {
    let container = fixture_container();
    let mut editor = TextEditor::for_section(&container, ".text").unwrap();
    let printf = editor.symbol_by_name("printf").unwrap();

    // The bl at 0x1000 originally targets 0x1004. Redirect it to
    // the printf extern.
    editor
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
    let mut editor = TextEditor::for_section(&container, ".text").unwrap();
    match editor.redirect_branch_at(0xdeadbeef, Target::Absolute(0)) {
        Err(TextEditorError::Edit(_)) => {}
        other => panic!("expected Edit error, got {other:?}"),
    }
}

#[test]
fn commit_returns_container_with_rewritten_text_section() {
    let container = fixture_container();
    let editor = TextEditor::for_section(&container, ".text").unwrap();
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
    let mut editor = TextEditor::for_section(&container, ".text").unwrap();
    // Touch the plan via the escape hatch; the editor's commit
    // pipeline should still work.
    let plan = editor.plan_mut();
    assert!(!plan.blocks.is_empty(), "plan should have at least one block");
    let _bytes = editor.commit_to_bytes().expect("commit_to_bytes after plan_mut");
}
