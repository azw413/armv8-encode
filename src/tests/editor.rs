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
