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
        pe_image: None,
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
fn unmodified_instructions_are_copied_verbatim_not_re_encoded() {
    // `ldrh w0, [x0, w20, uxtw #1]` round-trips lossily: the decoder drops the
    // `uxtw` extend (it surfaces as `lsl`), so re-encoding from operands would
    // flip the option bits and change the instruction. Committing a section that
    // wasn't edited must reproduce the original bytes exactly — the emitter
    // copies unmodified instructions verbatim instead of re-encoding them.
    let ldrh: u32 = 0x7874_5800; // ldrh w0, [x0, w20, uxtw #1]
    let ret: u32 = 0xd65f_03c0;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&ldrh.to_le_bytes());
    bytes.extend_from_slice(&ret.to_le_bytes());

    let mut container = fixture_container();
    container.sections[0].bytes = bytes.clone();
    container.symbols[0].size = 8;

    let editor = BinaryEditor::for_section(&container, ".text").expect("for_section");
    let edited = editor.commit().expect("commit");
    let text = edited.sections.iter().find(|s| s.name == ".text").expect(".text");
    assert_eq!(&text.bytes[..8], &bytes[..], "unmodified .text must round-trip byte-exact");
}

#[test]
fn overlay_bytes_lands_in_the_committed_section() {
    // Overlaying raw bytes at a VA in the lifted section must overwrite whatever
    // the plan emitted there (used to drop hole-placed code/data into .text).
    let container = fixture_container();
    let mut editor = BinaryEditor::for_section(&container, ".text").expect("for_section");
    let poke = [0xde, 0xad, 0xbe, 0xef];
    editor
        .text
        .as_mut()
        .and_then(|t| t.aarch64_mut())
        .expect("aarch64")
        .overlay_bytes(0x1004, poke.to_vec()); // over the second instruction
    let edited = editor.commit().expect("commit");
    let text = edited.sections.iter().find(|s| s.name == ".text").expect(".text");
    assert_eq!(&text.bytes[4..8], &poke, "overlay did not land");
    assert_eq!(&text.bytes[0..4], &0x9400_0001u32.to_le_bytes(), "first insn clobbered");
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

// ------------------------------------------------------------
// Tier 3: ARMv7 ELF32 verification of the editor's format-keyed
// methods. The `add_library_dependency` / `remove_library_dependency`
// implementations are ISA-agnostic — they dispatch on
// `container.format` (ELF vs Mach-O), not on architecture. These
// tests exercise the ELF code paths against a real 32-bit ARMv7
// fixture (libtool-checker.so) to catch any ELF32-specific bugs
// that the aarch64 ELF64 integration tests would miss.

fn libtool_checker_bytes() -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("libtool-checker.so");
    std::fs::read(&path).expect("read libtool-checker.so")
}

/// Resolve DT_NEEDED entries against the *current* DT_STRTAB
/// vaddr rather than the section-named `.dynstr`. After
/// `add_library_dependency`, DT_STRTAB points at a new dynstr
/// copy in the appended segment; the original `.dynstr` section
/// stays in place but is orphaned. Tests need to follow the
/// loader's resolution path (DT_STRTAB → segment vaddr) to see
/// the real DT_NEEDED names.
fn dt_needed_names_via_strtab(container: &Container, bytes: &[u8]) -> Vec<String> {
    const DT_NEEDED: u64 = 1;
    const DT_STRTAB: u64 = 5;
    const DT_STRSZ: u64 = 10;
    let img = container.elf_image.as_ref().expect("ElfImage");
    let strtab_vaddr = img
        .dynamic
        .iter()
        .find(|e| e.tag == DT_STRTAB)
        .map(|e| e.value)
        .expect("DT_STRTAB");
    let strsz = img
        .dynamic
        .iter()
        .find(|e| e.tag == DT_STRSZ)
        .map(|e| e.value as usize)
        .expect("DT_STRSZ");

    // Find the PT_LOAD segment containing strtab_vaddr and
    // extract the dynstr bytes from the file's loaded image.
    let dynstr_bytes: Vec<u8> = {
        let mut found: Option<Vec<u8>> = None;
        for ph in &img.program_headers {
            if ph.p_type != 1 /* PT_LOAD */ {
                continue;
            }
            if strtab_vaddr >= ph.p_vaddr && strtab_vaddr < ph.p_vaddr + ph.p_filesz {
                let off_in_seg = (strtab_vaddr - ph.p_vaddr) as usize;
                let file_off = ph.p_offset as usize + off_in_seg;
                let end = (file_off + strsz).min(bytes.len());
                found = Some(bytes[file_off..end].to_vec());
                break;
            }
        }
        found.unwrap_or_else(|| img.dynstr.as_ref().expect("dynstr").bytes.clone())
    };

    img.dynamic
        .iter()
        .filter(|e| e.tag == DT_NEEDED)
        .map(|e| {
            let off = e.value as usize;
            let end = dynstr_bytes[off..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| off + p)
                .unwrap_or(dynstr_bytes.len());
            std::str::from_utf8(&dynstr_bytes[off..end]).unwrap_or("?").to_string()
        })
        .collect()
}

#[test]
fn add_library_dependency_on_elf32_armv7_writes_new_dt_needed() {
    let bytes = libtool_checker_bytes();
    let container = Container::from_bytes(&bytes).expect("parse");
    assert_eq!(container.architecture, Architecture::Arm);

    let mut editor = BinaryEditor::new(&container).expect("editor");
    editor
        .binary
        .add_library_dependency("libnew.so")
        .expect("add_library_dependency");
    let written = editor.commit_to_bytes().expect("commit_to_bytes");

    let reparsed = Container::from_bytes(&written).expect("re-parse");
    // Resolve via DT_STRTAB: the rewritten dynstr lives in the
    // appended segment, not in the original .dynstr section.
    let names = dt_needed_names_via_strtab(&reparsed, &written);
    assert!(
        names.iter().any(|n| n == "libnew.so"),
        "expected libnew.so in DT_NEEDED list after add_library_dependency, got {names:?}"
    );
    // Existing entries must still be present.
    let originals = dt_needed_names_via_strtab(&container, &bytes);
    for original in &originals {
        assert!(
            names.iter().any(|n| n == original),
            "original DT_NEEDED entry {original:?} dropped; got {names:?}"
        );
    }
}

#[test]
fn remove_library_dependency_on_elf32_armv7_drops_dt_needed() {
    let bytes = libtool_checker_bytes();
    let container = Container::from_bytes(&bytes).expect("parse");
    assert_eq!(container.architecture, Architecture::Arm);

    let originals = dt_needed_names_via_strtab(&container, &bytes);
    // Pick an existing DT_NEEDED to drop. `libm.so` is the
    // least-likely-to-matter entry for the round-trip parse.
    let target = originals
        .iter()
        .find(|n| n.as_str() == "libm.so")
        .cloned()
        .unwrap_or_else(|| originals.first().expect("at least one DT_NEEDED").clone());

    let mut editor = BinaryEditor::new(&container).expect("editor");
    editor
        .binary
        .remove_library_dependency(&target)
        .expect("remove_library_dependency");
    let written = editor.commit_to_bytes().expect("commit_to_bytes");

    let reparsed = Container::from_bytes(&written).expect("re-parse after remove");
    let names = dt_needed_names_via_strtab(&reparsed, &written);
    assert!(
        !names.iter().any(|n| n == &target),
        "DT_NEEDED for {target:?} should have been dropped; got {names:?}"
    );
    // Every other original entry must still be there.
    for original in &originals {
        if original == &target {
            continue;
        }
        assert!(
            names.iter().any(|n| n == original),
            "unrelated DT_NEEDED {original:?} disappeared; got {names:?}"
        );
    }
}

#[test]
fn add_function_generic_thumb_appends_to_armv7_elf32() {
    // Stage-C/D end-to-end smoke: build a Thumb body (one
    // `bx lr` halfword), call add_function_generic::<ThumbIsa>
    // on the ARMv7 ELF32 fixture, commit, and re-parse. The
    // new function should appear as a global symbol whose
    // size matches the body bytes (2).
    use crate::isa::armv7::operand::{DecodedOperand, Register, RegisterClass};
    use crate::isa::armv7::table_generated::ThumbMnemonicGenerated;
    use crate::isa::armv7::ThumbIsa;
    use crate::rewrite::ir::{RewriteInstruction as RewriteInstructionGeneric, RewriteOperand};

    let bytes = libtool_checker_bytes();
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");

    let body: Vec<RewriteInstructionGeneric<ThumbIsa>> = vec![RewriteInstructionGeneric {
        mnemonic: ThumbMnemonicGenerated::Bx,
        operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(Register {
            class: RegisterClass::R,
            index: 14, // lr
        }))],
        original_address: None,
        source_size: None,
    }];

    let symbol_id = editor
        .binary
        .add_function_generic::<ThumbIsa>("my_thumb_fn", body)
        .expect("add_function_generic");

    // The symbol must be discoverable by name via the editor's
    // public lookup helper, confirming add_function_generic
    // registered it in the container.
    let addr = editor
        .binary
        .function_address("my_thumb_fn")
        .expect("my_thumb_fn registered");
    assert!(addr > 0, "function vaddr should be non-zero");
    let _ = symbol_id;

    // Commit produces a valid ELF32 that re-parses. The body
    // bytes (`bx lr` = 0x4770 in Thumb halfword form) must
    // appear at the assigned vaddr in the appended segment.
    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let reparsed = Container::from_bytes(&written).expect("re-parse");
    assert_eq!(reparsed.architecture, Architecture::Arm);

    // Locate the PT_LOAD that contains the function vaddr and
    // read the two bytes at that offset.
    let img = reparsed.elf_image.as_ref().expect("ElfImage");
    let body_bytes = img
        .program_headers
        .iter()
        .find_map(|ph| {
            if ph.p_type == 1 /* PT_LOAD */
                && addr >= ph.p_vaddr
                && addr < ph.p_vaddr + ph.p_filesz
            {
                let off = (addr - ph.p_vaddr) as usize;
                let file_off = ph.p_offset as usize + off;
                Some(written[file_off..file_off + 2].to_vec())
            } else {
                None
            }
        })
        .expect("PT_LOAD containing function vaddr");
    assert_eq!(
        body_bytes,
        vec![0x70, 0x47],
        "function body should be the Thumb `bx lr` halfword (0x4770 LE)"
    );
}

#[test]
fn remove_library_dependency_missing_name_errors_on_elf32() {
    let bytes = libtool_checker_bytes();
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let result = editor
        .binary
        .remove_library_dependency("libnothere.so");
    assert!(matches!(
        result,
        Err(TextEditorError::LibraryDependencyNotFound(_))
    ));
}

// ---------------------------------------------------------------------------
// Mach-O chained-fixups commit path
// ---------------------------------------------------------------------------

fn macho_objc_fixture_bytes() -> Option<Vec<u8>> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/macho_objc_fixture/libgreet_objc.dylib");
    std::fs::read(path).ok()
}

#[test]
fn commit_to_bytes_unsigned_round_trips_macho() {
    let Some(bytes) = macho_objc_fixture_bytes() else {
        eprintln!("skip: fixture not present");
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let editor = BinaryEditor::new(&container).expect("editor");
    let out = editor
        .commit_to_bytes_unsigned()
        .expect("commit_to_bytes_unsigned");
    // The unsigned output must still parse as a Mach-O. dyld
    // will refuse to load it (no valid signature) but we only
    // care about structural validity here.
    let _reparsed = Container::from_bytes(&out).expect("re-parse unsigned");
}

#[test]
fn commit_chained_fixups_rewrites_objc_import_in_place() {
    use crate::container::ChainedFixups;

    let Some(bytes) = macho_objc_fixture_bytes() else {
        eprintln!("skip: fixture not present");
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    // 1. Read the chained-fixups blob into the editable view.
    let macho = container.macho_image.as_ref().expect("MachOImage present");
    let mut cf = ChainedFixups::read(macho).expect("read fixups");

    // 2. Rename every NSObject reference in the imports table.
    let mut hits = 0usize;
    for imp in cf.imports.iter_mut() {
        if imp.symbol == "_OBJC_CLASS_$_NSObject" {
            imp.symbol = "_OBJC_CLASS_$_MyNSObject".to_string();
            hits += 1;
        }
    }
    assert!(hits > 0, "fixture must reference NSObject");

    // 3. Serialise back and stage the blob into the editor.
    let ser = cf
        .serialize(&macho.layout.segments)
        .expect("serialize blob");
    // Sanity-check the blob fits in the existing data range —
    // string mutation of equal-length names should produce an
    // equal-or-smaller blob.
    let cf_lc = macho.layout.chained_fixups.expect("LC present");
    assert!(
        (ser.bytes.len() as u64) <= cf_lc.datasize,
        "rewritten blob ({} bytes) should fit in existing datasize ({})",
        ser.bytes.len(),
        cf_lc.datasize,
    );

    let mut editor = BinaryEditor::new(&container).expect("editor");
    editor
        .binary
        .commit_chained_fixups(&ser.bytes)
        .expect("stage chained-fixups blob");

    // 4. Commit unsigned (we're not codesigning in a unit test)
    //    and re-parse to confirm the rename made it into the
    //    final byte stream.
    let out = editor
        .commit_to_bytes_unsigned()
        .expect("commit_to_bytes_unsigned");
    let reparsed = Container::from_bytes(&out).expect("re-parse");
    let macho2 = reparsed.macho_image.as_ref().expect("MachOImage present");
    let cf2 = ChainedFixups::read(macho2).expect("read fixups from rewritten image");
    let renamed = cf2
        .imports
        .iter()
        .filter(|i| i.symbol == "_OBJC_CLASS_$_MyNSObject")
        .count();
    assert_eq!(
        renamed, hits,
        "all NSObject bind imports should be renamed in the rewritten image",
    );
    // Sanity: no leftover unrenamed entries.
    let leftover = cf2
        .imports
        .iter()
        .filter(|i| i.symbol == "_OBJC_CLASS_$_NSObject")
        .count();
    assert_eq!(leftover, 0, "no original NSObject entries should remain");
}

#[test]
fn append_macho_segment_padding_writes_into_data_const_tail() {
    let Some(bytes) = macho_objc_fixture_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    // Append a short marker — small enough to fit in any
    // segment's trailing padding. The exact segment depends on
    // the linker; try __DATA_CONST first, fall back to __DATA.
    let marker = b"_OBJC_CLASS_$_RenameMe\0";
    let vaddr = editor
        .binary
        .append_macho_segment_padding("__DATA_CONST", marker)
        .or_else(|_| editor.binary.append_macho_segment_padding("__DATA", marker))
        .expect("at least one __DATA* segment has tail padding");
    // Commit unsigned and re-parse; the marker must land at the
    // returned vaddr in the rewritten image.
    let out = editor
        .commit_to_bytes_unsigned()
        .expect("commit_to_bytes_unsigned");
    let reparsed = Container::from_bytes(&out).expect("re-parse");
    let macho = reparsed.macho_image.as_ref().unwrap();
    // Locate the file offset via the containing segment — the
    // marker is in segment padding, not inside any section, so
    // file_offset_for_vaddr (which only resolves into section
    // ranges) wouldn't find it.
    let seg = macho
        .layout
        .segments
        .iter()
        .find(|s| vaddr >= s.vmaddr && vaddr < s.vmaddr + s.vmsize)
        .expect("vaddr inside a segment");
    let off = (seg.fileoff + (vaddr - seg.vmaddr)) as usize;
    assert_eq!(&out[off..off + marker.len()], marker);
}

#[test]
fn commit_chained_fixups_full_round_trips_repointed_rebase() {
    // End-to-end exercise of the grow-rename path using the
    // two new convenience methods:
    //
    //   1. Read the chained-fixups view.
    //   2. Pick an arbitrary rebase fixup and append a short
    //      marker into __DATA_CONST segment padding.
    //   3. Call `repoint_fixup(old, new_vaddr)` to redirect
    //      the rebase at the new marker.
    //   4. Call `commit_chained_fixups_full(&cf)` — the
    //      method handles serialise + blob stage + slot byte
    //      stage in one call.
    //   5. Re-parse and confirm the rebase now points at the
    //      marker vaddr.
    use crate::container::ChainedFixups;

    let Some(bytes) = macho_objc_fixture_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let macho = container.macho_image.as_ref().unwrap();
    let mut cf = ChainedFixups::read(macho).expect("read");

    // Pick the first rebase fixup as the test subject.
    let (orig_slot_vaddr, orig_target) = cf
        .segments
        .iter()
        .flat_map(|s| s.fixups.iter())
        .find_map(|fx| match fx.target {
            crate::container::FixupTarget::Rebase { target_vaddr } => {
                Some((fx.vaddr, target_vaddr))
            }
            _ => None,
        })
        .expect("at least one rebase fixup");

    let mut editor = BinaryEditor::new(&container).expect("editor");
    let marker = b"renamed_target\0";
    let new_vaddr = editor
        .binary
        .append_macho_segment_padding("__DATA_CONST", marker)
        .or_else(|_| editor.binary.append_macho_segment_padding("__DATA", marker))
        .expect("padding fits in __DATA_CONST or __DATA");

    let hits = cf.repoint_fixup(orig_target, new_vaddr);
    assert!(
        hits > 0,
        "repoint_fixup must touch at least the one rebase we picked"
    );

    editor
        .binary
        .commit_chained_fixups_full(&cf)
        .expect("commit_chained_fixups_full");

    let out = editor
        .commit_to_bytes_unsigned()
        .expect("commit_to_bytes_unsigned");
    let reparsed = Container::from_bytes(&out).expect("re-parse");
    let macho2 = reparsed.macho_image.as_ref().unwrap();
    let cf2 = ChainedFixups::read(macho2).expect("re-read fixups");
    // The slot that previously pointed at `orig_target` must
    // now point at `new_vaddr`.
    let new_target = cf2
        .segments
        .iter()
        .flat_map(|s| s.fixups.iter())
        .find(|fx| fx.vaddr == orig_slot_vaddr)
        .map(|fx| match fx.target {
            crate::container::FixupTarget::Rebase { target_vaddr } => target_vaddr,
            _ => panic!("slot turned into a bind, expected rebase"),
        })
        .expect("slot still present after round-trip");
    assert_eq!(
        new_target, new_vaddr,
        "repointed rebase must survive round-trip"
    );
}

#[test]
fn commit_chained_fixups_rejects_blob_too_large() {
    let Some(bytes) = macho_objc_fixture_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let macho = container.macho_image.as_ref().unwrap();
    let cf = macho.layout.chained_fixups.unwrap();
    // A blob one byte bigger than the existing datasize must
    // be rejected.
    let oversized = vec![0u8; (cf.datasize as usize) + 1];
    let err = editor
        .binary
        .commit_chained_fixups(&oversized)
        .expect_err("oversized blob must error");
    assert!(
        matches!(
            err,
            TextEditorError::ChainedFixupsBlobTooLarge { .. }
        ),
        "expected ChainedFixupsBlobTooLarge, got {err:?}",
    );
}

// ---------------------------------------------------------------------------
// Reserve / carve text-space API (Mach-O, free-space path)
// ---------------------------------------------------------------------------

fn macho_lib_demo_bytes() -> Option<Vec<u8>> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/macho_runtime/fixtures/lib_demo/libgreet.dylib");
    std::fs::read(path).ok()
}

/// A one-instruction `ret` body for placement round-trips.
fn ret_body() -> Vec<crate::rewrite::RewriteInstruction> {
    use crate::isa::aarch64::{DecodedOperand, Register, RegisterClass};
    use crate::rewrite::{RewriteInstruction, RewriteOperand};
    let x30 = Register {
        class: RegisterClass::X,
        index: 30,
    };
    vec![RewriteInstruction {
        mnemonic: Aarch64Mnemonic::Ret,
        operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
        original_address: None,
        source_size: None,
    }]
}

#[test]
fn reserve_text_region_lands_in_text_free_space() {
    use crate::rewrite::space::{ReserveRequest, SpaceSource};
    let Some(bytes) = macho_lib_demo_bytes() else {
        eprintln!("skip: macho fixture absent");
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let image = container.macho_image.as_ref().expect("macho image");
    let free = image.layout.text_free_regions();
    let total_free: u64 = free
        .iter()
        .filter(|r| r.segment_name == "__TEXT")
        .map(|r| r.size)
        .sum();
    assert!(
        total_free >= 0x40,
        "fixture should carry __TEXT free space; got {total_free}"
    );

    let mut editor = BinaryEditor::new(&container).expect("editor");
    let res = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x20))
        .expect("reserve");
    // The reserved span must sit fully inside a real __TEXT free region,
    // with the file offset matching that region's vaddr→fileoff skew.
    let containing = free
        .iter()
        .find(|r| {
            r.segment_name == "__TEXT"
                && res.base_address >= r.vaddr
                && res.base_address + res.capacity <= r.vaddr + r.size
        })
        .expect("reservation lies inside one free region");
    assert_eq!(
        res.base_file_offset,
        containing.file_offset + (res.base_address - containing.vaddr),
    );
    assert_eq!(res.capacity, 0x20);
    assert!(matches!(
        res.sources.as_slice(),
        [(SpaceSource::TailPad | SpaceSource::InterSectionHole, 0x20)]
    ));
    assert_eq!(editor.binary.region_remaining(res.region), Some(0x20));
}

#[test]
fn reserve_free_only_rejects_oversize_without_growth() {
    use crate::rewrite::space::ReserveRequest;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    // 4 GiB fits no free region; the Fail policy must error, not grow.
    let err = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x1_0000_0000))
        .unwrap_err();
    assert!(
        matches!(err, TextEditorError::InsufficientTextSpace { .. }),
        "got {err:?}"
    );
}

#[test]
fn reserve_grow_policy_reports_growth_required() {
    use crate::rewrite::space::{Exhaustion, ReserveRequest};
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let req = ReserveRequest {
        min_bytes: 0x1_0000_0000,
        headroom: 0,
        align: 4,
        allow_headerpad: false,
        on_exhaustion: Exhaustion::Grow,
    };
    let err = editor.binary.reserve_text_region(req).unwrap_err();
    assert!(
        matches!(err, TextEditorError::WouldRequireTextGrowth { .. }),
        "got {err:?}"
    );
}

#[test]
fn reserve_then_add_data_in_round_trips_macho() {
    use crate::rewrite::space::ReserveRequest;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let res = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x40))
        .expect("reserve");
    let blob = [0xDEu8, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];
    let sym = editor
        .binary
        .add_data_in(res.region, "reserved_blob", &blob, 4)
        .expect("add_data_in");
    // First 4-aligned carve lands at the region base.
    assert_eq!(editor.binary.symbol_address(sym), res.base_address);
    let written = editor.commit_to_bytes_unsigned().expect("commit");
    // Output is still a valid Mach-O.
    let _ = Container::from_bytes(&written).expect("re-parse");
    // The blob is physically at the reserved file offset.
    let off = res.base_file_offset as usize;
    assert_eq!(&written[off..off + blob.len()], &blob);
}

#[test]
fn reserve_then_add_function_in_round_trips_macho() {
    use crate::rewrite::space::ReserveRequest;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let res = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x40))
        .expect("reserve");
    let sym = editor
        .binary
        .add_function_in(res.region, "reserved_ret", ret_body())
        .expect("add_function_in");
    let addr = editor.binary.symbol_address(sym);
    assert_eq!(addr, res.base_address);
    let written = editor.commit_to_bytes_unsigned().expect("commit");
    let _ = Container::from_bytes(&written).expect("re-parse");
    // The emitted word decodes back to `ret` at its placed address.
    let off = res.base_file_offset as usize;
    let word = u32::from_le_bytes(written[off..off + 4].try_into().unwrap());
    let insn = aarch64::decode_instruction(addr, word).expect("decode placed word");
    assert_eq!(insn.mnemonic, Aarch64Mnemonic::Ret);
}

#[test]
fn next_address_then_add_raw_code_round_trips_macho() {
    use crate::rewrite::space::ReserveRequest;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let res = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x40))
        .expect("reserve");
    // Peek the address, then hand it pre-encoded `ret` bytes.
    let addr = editor.binary.next_address_in(res.region, 4).expect("peek");
    assert_eq!(addr, res.base_address);
    let raw = 0xd65f03c0u32.to_le_bytes(); // ret
    let sym = editor
        .binary
        .add_raw_code_in(res.region, "raw_ret", &raw, 4)
        .expect("add_raw_code_in");
    assert_eq!(editor.binary.symbol_address(sym), addr);
    let written = editor.commit_to_bytes_unsigned().expect("commit");
    let _ = Container::from_bytes(&written).expect("re-parse");
    let off = res.base_file_offset as usize;
    assert_eq!(&written[off..off + 4], &raw, "raw code placed verbatim");
}

#[test]
fn region_full_is_a_clean_error() {
    use crate::rewrite::space::ReserveRequest;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let res = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x8))
        .expect("reserve");
    editor
        .binary
        .add_data_in(res.region, "a", &[0u8; 8], 1)
        .expect("first fills the region exactly");
    let err = editor
        .binary
        .add_data_in(res.region, "b", &[0u8; 8], 1)
        .unwrap_err();
    assert!(
        matches!(err, TextEditorError::RegionFull { .. }),
        "overflow must be a caught error, got {err:?}"
    );
}

#[test]
fn stale_region_handle_is_rejected() {
    use crate::rewrite::space::ReserveRequest;
    use crate::rewrite::RegionId;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let _res = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x20))
        .expect("reserve");
    let bogus = RegionId(999);
    assert!(matches!(
        editor.binary.add_data_in(bogus, "x", &[0u8; 4], 4),
        Err(TextEditorError::NoSuchRegion)
    ));
    assert!(matches!(
        editor.binary.next_address_in(bogus, 4),
        Err(TextEditorError::NoSuchRegion)
    ));
    assert_eq!(editor.binary.region_remaining(bogus), None);
}

#[test]
fn double_reserve_is_rejected() {
    use crate::rewrite::space::ReserveRequest;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x10))
        .expect("first reserve");
    let err = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x10))
        .unwrap_err();
    assert!(
        matches!(err, TextEditorError::TextRegionAlreadyReserved),
        "got {err:?}"
    );
}

#[test]
fn multiple_carves_are_contiguous_and_round_trip() {
    use crate::rewrite::space::ReserveRequest;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let res = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x20).with_headroom(0x20))
        .expect("reserve");
    // A function, then data, then raw code — packed back to back.
    let f = editor
        .binary
        .add_function_in(res.region, "f0", ret_body())
        .expect("f");
    let d = editor
        .binary
        .add_data_in(res.region, "d0", &[1u8, 2, 3, 4], 4)
        .expect("d");
    let r = editor
        .binary
        .add_raw_code_in(res.region, "r0", &0xd65f03c0u32.to_le_bytes(), 4)
        .expect("r");
    let (fa, da, ra) = (
        editor.binary.symbol_address(f),
        editor.binary.symbol_address(d),
        editor.binary.symbol_address(r),
    );
    // Strictly increasing, non-overlapping, all inside the reservation.
    assert!(fa < da && da < ra);
    assert!(fa >= res.base_address);
    assert!(ra + 4 <= res.base_address + res.capacity);
    let written = editor.commit_to_bytes_unsigned().expect("commit");
    let _ = Container::from_bytes(&written).expect("re-parse");
}

#[test]
fn reserve_on_non_macho_is_rejected() {
    use crate::rewrite::space::ReserveRequest;
    // `fixture_container` is an ELF ET_REL — not Mach-O.
    let container = fixture_container();
    let mut editor = BinaryEditor::new(&container).expect("editor");
    let err = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x10))
        .unwrap_err();
    assert!(
        matches!(err, TextEditorError::ReserveUnsupportedFormat(_)),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Growth geometry against a real Mach-O layout (Increment 2, geometry only)
// ---------------------------------------------------------------------------

#[test]
fn plan_text_growth_for_real_dylib_is_page_quantised() {
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let image = container.macho_image.as_ref().expect("macho image");
    let text = image
        .layout
        .segments
        .iter()
        .find(|s| s.name == "__TEXT")
        .expect("__TEXT segment");
    let after_text = image
        .layout
        .segments
        .iter()
        .filter(|s| s.vmaddr > text.vmaddr)
        .count();
    let (text_vmsize, text_filesize) = (text.vmsize, text.filesize);

    // Request far more than the fixture's few-KB of __TEXT slack so a
    // grow is genuinely required.
    let needed = 0x40_000u64; // 256 KiB
    let editor = BinaryEditor::new(&container).expect("editor");
    let plan = editor
        .binary
        .plan_text_growth_for(needed)
        .expect("growth plan");

    // Whole-page growth, enough capacity.
    assert!(plan.pages >= 1);
    assert_eq!(plan.delta, plan.pages * 0x4000);
    assert_eq!(plan.delta % 0x4000, 0);
    assert!(plan.region_capacity >= needed);
    // __TEXT grows by delta in both axes.
    assert_eq!(plan.new_vmsize, text_vmsize + plan.delta);
    assert_eq!(plan.new_filesize, text_filesize + plan.delta);
    // Every segment after __TEXT (at least __LINKEDIT) shifts by delta,
    // preserving its file↔vaddr skew.
    assert_eq!(plan.shifts.len(), after_text);
    assert!(after_text >= 1, "a dylib always has __LINKEDIT after __TEXT");
    for s in &plan.shifts {
        assert_eq!(s.new_vmaddr - s.old_vmaddr, plan.delta);
        assert_eq!(s.new_fileoff - s.old_fileoff, plan.delta);
    }
}

#[test]
fn plan_text_growth_for_non_macho_is_rejected() {
    let container = fixture_container(); // ELF ET_REL
    let editor = BinaryEditor::new(&container).expect("editor");
    let err = editor.binary.plan_text_growth_for(0x1000).unwrap_err();
    assert!(
        matches!(err, TextEditorError::ReserveUnsupportedFormat(_)),
        "got {err:?}"
    );
}

/// Read a segment's (vmaddr, vmsize, fileoff, filesize).
fn macho_seg(c: &Container, name: &str) -> Option<(u64, u64, u64, u64)> {
    c.macho_image
        .as_ref()?
        .layout
        .segments
        .iter()
        .find(|s| s.name == name)
        .map(|s| (s.vmaddr, s.vmsize, s.fileoff, s.filesize))
}

/// First `__TEXT` section's file offset — the uniform-shift insert point.
fn first_text_section_offset(c: &Container) -> u64 {
    c.macho_image
        .as_ref()
        .unwrap()
        .layout
        .sections
        .iter()
        .filter(|s| s.segname == "__TEXT" && s.file_offset > 0)
        .map(|s| s.file_offset)
        .min()
        .expect("a __TEXT code section")
}

#[test]
fn write_with_text_growth_applies_uniform_shift_structurally() {
    // The writer applies the uniform-shift geometry: insert delta before
    // __text and move the whole image after it up by delta, keeping the
    // header + __TEXT.vmaddr fixed. Checks the load-command bookkeeping
    // via a re-parse. It does NOT prove the output loads (absolute
    // pointers — chained-fixup targets, export trie — are unfixed here);
    // it proves the shift is structurally coherent and, crucially, that
    // __TEXT's own sections move too.
    use crate::container::macho_writer::{write_with_text_growth_opts, MachOWriteOptions};
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let old_text = macho_seg(&container, "__TEXT").expect("__TEXT");
    let old_linkedit = macho_seg(&container, "__LINKEDIT").expect("__LINKEDIT");
    // A __text section's address/offset before the grow.
    let text_sec = |c: &Container| {
        c.macho_image
            .as_ref()
            .unwrap()
            .layout
            .sections
            .iter()
            .find(|s| s.segname == "__TEXT" && s.sectname == "__text")
            .map(|s| (s.vaddr, s.file_offset))
            .expect("__text section")
    };
    let old_text_sec = text_sec(&container);

    let growth_point = first_text_section_offset(&container);
    let delta = 0x4000u64; // one arm64 page
    let payload = 0xd65f03c0u32.to_le_bytes(); // ret, into the inserted gap
    let opts = MachOWriteOptions {
        sign: false,
        raw_byte_overrides: vec![],
    };
    let out = write_with_text_growth_opts(
        &container,
        growth_point,
        delta,
        growth_point, // payload at the start of the inserted gap
        &payload,
        &[],
        &opts,
    )
    .expect("grow write");

    // Exactly delta bytes inserted; payload sits in the new gap.
    assert_eq!(out.len() as u64, bytes.len() as u64 + delta);
    let gp = growth_point as usize;
    assert_eq!(&out[gp..gp + 4], &payload);

    // Re-parses as a coherent Mach-O whose symbol table still reads.
    let reparsed = Container::from_bytes(&out).expect("re-parse grown dylib");
    assert!(!reparsed.symbols.is_empty(), "symtab must still parse");

    // __TEXT grew in place (base fixed); its __text section moved up.
    let new_text = macho_seg(&reparsed, "__TEXT").expect("__TEXT after");
    let new_text_sec = text_sec(&reparsed);
    assert_eq!(new_text.0, old_text.0, "__TEXT vmaddr unchanged");
    assert_eq!(new_text.2, old_text.2, "__TEXT fileoff unchanged");
    assert_eq!(new_text.1, old_text.1 + delta, "__TEXT vmsize += delta");
    assert_eq!(new_text.3, old_text.3 + delta, "__TEXT filesize += delta");
    assert_eq!(new_text_sec.0, old_text_sec.0 + delta, "__text addr += delta");
    assert_eq!(new_text_sec.1, old_text_sec.1 + delta, "__text offset += delta");

    // __LINKEDIT moved wholesale by delta, unchanged in size.
    let new_linkedit = macho_seg(&reparsed, "__LINKEDIT").expect("__LINKEDIT after");
    assert_eq!(new_linkedit.0, old_linkedit.0 + delta, "__LINKEDIT vmaddr");
    assert_eq!(new_linkedit.2, old_linkedit.2 + delta, "__LINKEDIT fileoff");
    assert_eq!(new_linkedit.1, old_linkedit.1, "__LINKEDIT vmsize unchanged");
    assert_eq!(new_linkedit.3, old_linkedit.3, "__LINKEDIT filesize unchanged");

    // File-backed segments stay ordered and non-overlapping.
    let segs = &reparsed.macho_image.as_ref().unwrap().layout.segments;
    let mut file_backed: Vec<_> = segs.iter().filter(|s| s.filesize > 0).collect();
    file_backed.sort_by_key(|s| s.fileoff);
    for w in file_backed.windows(2) {
        assert!(
            w[0].fileoff + w[0].filesize <= w[1].fileoff,
            "file overlap between {} and {}",
            w[0].name,
            w[1].name,
        );
    }
}

#[test]
fn write_with_text_growth_rejects_bad_params() {
    use crate::container::macho_writer::{write_with_text_growth_opts, MachOWriteOptions};
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let opts = MachOWriteOptions {
        sign: false,
        raw_byte_overrides: vec![],
    };
    let gp = first_text_section_offset(&container);
    // Zero delta is rejected.
    assert!(write_with_text_growth_opts(&container, gp, 0, gp, &[], &[], &opts).is_err());
    // A payload that overruns the grown region is rejected, not a panic.
    let too_big = vec![0u8; 0x5000]; // > one 0x4000 page
    assert!(
        write_with_text_growth_opts(&container, gp, 0x4000, gp, &too_big, &[], &opts).is_err()
    );
}

// ---------------------------------------------------------------------------
// Chained-fixup +delta shift (the load-critical uniform-shift fixup)
// ---------------------------------------------------------------------------

#[test]
fn chained_fixups_shift_by_moves_slots_and_rebase_targets() {
    use crate::container::chained_fixups::FixupTarget;
    use crate::container::ChainedFixups;
    let Some(bytes) = macho_objc_fixture_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let macho = container.macho_image.as_ref().expect("macho");
    let mut cf = ChainedFixups::read(macho).expect("read fixups");

    let snapshot = |cf: &ChainedFixups| -> Vec<(u64, Option<u64>)> {
        cf.segments
            .iter()
            .flat_map(|sf| {
                sf.fixups.iter().map(|fx| {
                    let t = match &fx.target {
                        FixupTarget::Rebase { target_vaddr } => Some(*target_vaddr),
                        _ => None,
                    };
                    (fx.vaddr, t)
                })
            })
            .collect()
    };
    let before = snapshot(&cf);
    if before.is_empty() {
        eprintln!("skip: fixture has no chained fixups");
        return;
    }

    let delta = 0x4000u64;
    cf.shift_by(delta);
    let after = snapshot(&cf);

    assert_eq!(before.len(), after.len());
    let mut rebases = 0;
    for (b, a) in before.iter().zip(&after) {
        assert_eq!(a.0, b.0 + delta, "slot vaddr shifts by delta");
        match (b.1, a.1) {
            (Some(bt), Some(at)) => {
                assert_eq!(at, bt + delta, "rebase target shifts by delta");
                rebases += 1;
            }
            (None, None) => {} // bind: target is an import index, unchanged
            _ => panic!("fixup target kind must not change"),
        }
    }
    assert!(rebases > 0, "objc fixture should carry rebases to exercise");
}

#[test]
fn chained_fixups_shift_by_serialises_against_shifted_segments() {
    use crate::container::ChainedFixups;
    let Some(bytes) = macho_objc_fixture_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let macho = container.macho_image.as_ref().expect("macho");
    let mut cf = ChainedFixups::read(macho).expect("read fixups");
    if cf.segments.iter().all(|s| s.fixups.is_empty()) {
        return;
    }

    let delta = 0x4000u64;
    // The uniform shift moves every segment's vmaddr up by delta; the
    // shifted fixups must still validate and serialise against them
    // (each fixup lands inside its segment because both moved by delta).
    let mut segs = macho.layout.segments.clone();
    for s in &mut segs {
        s.vmaddr = s.vmaddr.saturating_add(delta);
    }
    cf.shift_by(delta);
    cf.serialize(&segs)
        .expect("shifted fixups serialise against shifted segments");
}

#[test]
fn build_grown_macho_shifts_chained_fixups_in_output() {
    // End-to-end (structural): grow a fixup-carrying dylib and re-read
    // the chained fixups FROM THE GROWN OUTPUT — their rebase targets
    // are shifted by delta and the count is preserved, proving the
    // load-critical fixup splice landed correctly. (dyld applying them
    // is the macho_runtime harness's job.)
    use crate::container::chained_fixups::FixupTarget;
    use crate::container::ChainedFixups;
    let Some(bytes) = macho_objc_fixture_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let macho = container.macho_image.as_ref().unwrap();
    let orig_cf = ChainedFixups::read(macho).expect("fixture has chained fixups");
    let first_rebase = |cf: &ChainedFixups| -> Option<u64> {
        cf.segments
            .iter()
            .flat_map(|s| s.fixups.iter())
            .find_map(|fx| match &fx.target {
                FixupTarget::Rebase { target_vaddr } => Some(*target_vaddr),
                _ => None,
            })
    };
    let count = |cf: &ChainedFixups| cf.segments.iter().map(|s| s.fixups.len()).sum::<usize>();
    let orig_target = first_rebase(&orig_cf);
    let orig_count = count(&orig_cf);

    let editor = BinaryEditor::new(&container).expect("editor");
    let gp = first_text_section_offset(&container);
    let delta = 0x4000u64;
    let payload = 0xd65f03c0u32.to_le_bytes(); // ret
    let out = editor
        .binary
        .build_grown_macho(gp, delta, gp, &payload, false)
        .expect("grow + fixup splice");

    // File grew by delta; payload placed; still a valid Mach-O.
    assert_eq!(out.len() as u64, bytes.len() as u64 + delta);
    assert_eq!(&out[gp as usize..gp as usize + 4], &payload);
    let reparsed = Container::from_bytes(&out).expect("re-parse grown");

    // Re-read the fixups from the OUTPUT: targets shifted, count intact.
    let new_cf =
        ChainedFixups::read(reparsed.macho_image.as_ref().unwrap()).expect("read grown fixups");
    assert_eq!(count(&new_cf), orig_count, "fixup count preserved");
    if let (Some(ot), Some(nt)) = (orig_target, first_rebase(&new_cf)) {
        assert_eq!(nt, ot + delta, "rebase target in grown output shifted by delta");
    }
}

#[test]
fn build_grown_macho_shifts_export_trie_in_output() {
    // The exports-critical fixup: after a grow, each regular export's
    // trie offset must be +delta (the symbol moved but the image base
    // didn't). Re-read the trie from the grown OUTPUT to prove it.
    use crate::container::macho_export_trie;
    let Some(bytes) = macho_lib_demo_bytes() else {
        return;
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let macho = container.macho_image.as_ref().unwrap();
    let Some(trie) = macho.layout.exports_trie else {
        eprintln!("skip: fixture has no export trie");
        return;
    };
    let read_trie = |raw: &[u8], off: u64, size: u64| {
        macho_export_trie::parse(&raw[off as usize..(off + size) as usize])
            .expect("parse export trie")
    };
    let orig_exports = read_trie(&macho.raw_bytes, trie.dataoff, trie.datasize);
    let orig: std::collections::HashMap<String, (u64, u64)> = orig_exports
        .iter()
        .map(|e| (e.name.clone(), (e.flags, e.address_offset)))
        .collect();

    let editor = BinaryEditor::new(&container).expect("editor");
    let gp = first_text_section_offset(&container);
    let delta = 0x4000u64;
    let out = editor
        .binary
        .build_grown_macho(gp, delta, gp, &0xd65f03c0u32.to_le_bytes(), false)
        .expect("grow");
    let reparsed = Container::from_bytes(&out).expect("re-parse");
    let rtrie = reparsed
        .macho_image
        .as_ref()
        .unwrap()
        .layout
        .exports_trie
        .expect("output has export trie");
    let new_exports = read_trie(&out, rtrie.dataoff, rtrie.datasize);

    assert_eq!(new_exports.len(), orig_exports.len(), "export count preserved");
    const REEXPORT: u64 = 0x08;
    let mut regular_checked = 0;
    for e in &new_exports {
        let Some(&(oflags, oaddr)) = orig.get(&e.name) else {
            panic!("export {} appeared/renamed unexpectedly", e.name);
        };
        if oflags & REEXPORT == 0 {
            assert_eq!(
                e.address_offset,
                oaddr + delta,
                "regular export {} offset += delta",
                e.name
            );
            regular_checked += 1;
        }
    }
    assert!(regular_checked > 0, "expected at least one regular export");
}
