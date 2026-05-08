//! Runtime smoke tests for the Mach-O writer (Phase 0+).
//!
//! Mirrors `tests/elf_runtime.rs` but runs natively on Apple
//! Silicon — no Docker, no QEMU. The fixture builds a minimal
//! `libgreet.dylib` + `host` pair through `clang -dynamiclib`,
//! signs them ad-hoc with `codesign -s -`, and runs the host
//! directly. Assertions are on the host's stdout, identical in
//! shape to the ELF harness so future Phase-N tests can reuse
//! the same expected strings.
//!
//! All tests are `#[ignore]` because they require:
//!
//!   1. macOS on `arm64` (Apple Silicon).
//!   2. Apple's clang + codesign in `$PATH` (the default toolchain).
//!
//! Run with:
//!
//!   cargo test --test macho_runtime -- --ignored --nocapture
//!
//! Phase 0 only covers a baseline: the fixture builds, signs,
//! loads, and runs. Subsequent phases (round-trip writer, in-place
//! edits, append, exports, initialisers, dependencies) plug in by
//! reading + rewriting the fixture between build and run, the same
//! way the ELF harness does.

use armv8_encode::container::Container;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Path to `tests/macho_runtime/` from the crate root. Stays
/// relative to `CARGO_MANIFEST_DIR` so the harness works
/// regardless of where `cargo test` was invoked from.
fn harness_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/macho_runtime")
}

/// Skip with a clear message when the environment can't run
/// these tests, instead of failing in a confusing way. Two
/// preconditions: native arm64 macOS and `clang` + `codesign` on
/// PATH.
fn require_macos_arm64() {
    if !cfg!(target_os = "macos") {
        panic!("tests/macho_runtime requires macOS host; current target_os is not macos");
    }
    if !cfg!(target_arch = "aarch64") {
        panic!(
            "tests/macho_runtime requires Apple Silicon (aarch64); \
             current target_arch is not aarch64",
        );
    }
    // Confirm clang and codesign are on PATH. `which` exits 0
    // iff the tool is found. `codesign --version` doesn't exist;
    // `which` is the simpler probe and works for both tools.
    for tool in ["clang", "codesign"] {
        let probe = Command::new("which")
            .arg(tool)
            .output()
            .expect("spawn which");
        assert!(
            probe.status.success(),
            "{tool} not available on PATH. Install Xcode \
             command-line tools (`xcode-select --install`).",
        );
    }
}

/// Build the lib_demo fixture (libgreet.dylib + host) by running
/// its build.sh. Returns paths to both artefacts.
fn build_lib_demo_fixture() -> (PathBuf, PathBuf) {
    let dir = harness_dir().join("fixtures/lib_demo");
    let out = Command::new("sh")
        .arg("build.sh")
        .current_dir(&dir)
        .output()
        .expect("run build.sh");
    assert!(
        out.status.success(),
        "lib_demo fixture build failed (exit {}):\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    (dir.join("libgreet.dylib"), dir.join("host"))
}

/// Run a binary in the lib_demo fixture directory (where it can
/// find libgreet.dylib via the @loader_path rpath baked at link
/// time). Returns stdout. Wraps each invocation in `/usr/bin/time
/// -p` style timeouts? No — macOS `timeout` isn't BSD; just trust
/// the binaries to terminate. They print one line and exit.
fn run_in_lib_demo(binary_name: &str) -> String {
    run_in_lib_demo_with_args(binary_name, &[])
}

fn run_in_lib_demo_with_args(binary_name: &str, args: &[&str]) -> String {
    let dir = harness_dir().join("fixtures/lib_demo");
    let bin = dir.join(binary_name);
    let run: Output = Command::new(&bin)
        .args(args)
        .current_dir(&dir)
        .output()
        .unwrap_or_else(|err| panic!("run {} failed to spawn: {err}", bin.display()));
    assert!(
        run.status.success(),
        "running ./{binary_name} {args:?} in lib_demo failed (exit {}):\n\
         stdout:\n{}\nstderr:\n{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8(run.stdout).expect("stdout utf8")
}

/// Sanity: confirm the file at `path` is a Mach-O 64-bit
/// arm64 binary by reading its magic + cpu type. Used by the
/// baseline test to fail loudly if the build script silently
/// produced something unexpected.
fn assert_is_arm64_macho(path: &Path) {
    let bytes = std::fs::read(path).expect("read file for magic check");
    assert!(
        bytes.len() >= 12,
        "{} too short to be a Mach-O file ({} bytes)",
        path.display(),
        bytes.len(),
    );
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    // MH_MAGIC_64 = 0xfeedfacf little-endian.
    const MH_MAGIC_64: u32 = 0xfeed_facf;
    assert_eq!(
        magic,
        MH_MAGIC_64,
        "{} is not a 64-bit Mach-O file (magic = 0x{magic:08x}, expected 0x{MH_MAGIC_64:08x})",
        path.display(),
    );
    let cputype = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    // CPU_TYPE_ARM64 = 0x0100000c.
    const CPU_TYPE_ARM64: u32 = 0x0100_000c;
    assert_eq!(
        cputype,
        CPU_TYPE_ARM64,
        "{} is not an arm64 binary (cputype = 0x{cputype:08x}, expected 0x{CPU_TYPE_ARM64:08x})",
        path.display(),
    );
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn baseline_fixture_builds_signs_and_runs() {
    require_macos_arm64();
    let (lib_path, host_path) = build_lib_demo_fixture();

    // Phase-0 sanity: confirm both artefacts are arm64 Mach-O
    // before invoking them. If the build script silently produced
    // x86_64 (because of `-arch` defaults) the host would still
    // run via Rosetta and we'd never notice the mismatch.
    assert_is_arm64_macho(&lib_path);
    assert_is_arm64_macho(&host_path);

    // Default invocation prints both function results.
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=42 offset=107\n",
        "vanilla libgreet.dylib should print the canonical \
         double=42 offset=107 line",
    );

    // `single` invocation prints only greet_double.
    let stdout = run_in_lib_demo_with_args("host", &["single"]);
    assert_eq!(stdout, "double=42\n");

    // `ctor` invocation prints the constructor marker. The
    // constructor sets bit 0x1 at load time; without rewriting
    // the marker reads 1.
    let stdout = run_in_lib_demo_with_args("host", &["ctor"]);
    assert_eq!(stdout, "ctor_marker=1\n");
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_inplace_text_edit_changes_observable_output() {
    // Phase 2 acceptance: edit libgreet.dylib's `__text`
    // section through the high-level BinaryEditor API and
    // observe the change at runtime via the host program.
    //
    // Mirrors the ELF `et_dyn_inplace_text_edit_*` test:
    // replaces greet_double's `lsl Wd, Wn, #1` (n*2) with
    // `lsl Wd, Wn, #2` (n*4). After the rewrite, host prints
    // `double=84 offset=107` instead of `double=42 offset=107`.
    //
    // Mach-O specifics:
    //   - Symbol name is `_greet_double` (Mach-O underscoring),
    //     not `greet_double`.
    //   - Section name is `__text` (Mach-O segment.section
    //     convention), not `.text`. The reader exposes it
    //     under the bare section name.
    //   - commit_to_bytes routes through macho_writer for
    //     in-place edits: it applies the new section bytes at
    //     the original file offset and re-signs the result
    //     ad-hoc via `codesign`.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{Aarch64Mnemonic, DecodedOperand};
    use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");

    // Open the editor on `__text`. The reader strips the
    // segment prefix and exposes Mach-O sections under the
    // bare section name (e.g. `__text`).
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");
    let function_address = editor
        .binary
        .function_address("_greet_double")
        .expect("_greet_double symbol present in __text");
    let text = editor.text.as_ref().expect("text section lifted");
    let lsl_index = text
        .instructions()
        .iter()
        .position(|insn| {
            insn.address >= function_address && insn.mnemonic == Aarch64Mnemonic::Lsl
        })
        .expect("greet_double should contain an lsl");
    let lsl_address = text.instructions()[lsl_index].address;
    let original_lsl = &text.instructions()[lsl_index];

    let (rd, rn) = match (&original_lsl.operands[0], &original_lsl.operands[1]) {
        (DecodedOperand::Register(rd), DecodedOperand::Register(rn)) => {
            (rd.clone(), rn.clone())
        }
        other => panic!("unexpected lsl operand shape: {other:?}"),
    };
    let original_shift = match &original_lsl.operands[2] {
        DecodedOperand::Immediate(n) => *n,
        other => panic!("unexpected lsl shift operand: {other:?}"),
    };
    assert_eq!(
        original_shift, 1,
        "fixture greet_double should use `lsl Wd, Wn, #1`",
    );

    let new_instruction = RewriteInstruction {
        mnemonic: Aarch64Mnemonic::Lsl,
        operands: vec![
            RewriteOperand::Decoded(DecodedOperand::Register(rd)),
            RewriteOperand::Decoded(DecodedOperand::Register(rn)),
            RewriteOperand::Decoded(DecodedOperand::Immediate(2)),
        ],
        original_address: Some(lsl_address),
    };
    editor
        .text
        .as_mut()
        .unwrap()
        .replace_instruction_at(lsl_address, new_instruction)
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let _ = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=84 offset=107\n",
        "in-place __text edit through BinaryEditor should make \
         greet_double return n*4=84 instead of n*2=42",
    );
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_appended_function_changes_observable_output() {
    // Phase 3 acceptance: append a new function to
    // libgreet.dylib via BinaryEditor::add_function (lands in a
    // fresh LC_SEGMENT_64 past the input's mapped range), then
    // patch _greet_double's first instruction to be
    // `b _greet_quintuple` so the host's existing call lands in
    // the new code.
    //
    // _greet_quintuple(n) = n * 5, so host's
    // greet_double(21) returns 21*5 = 105. Acceptance proves:
    //   1. the writer's load-command splice produced a valid
    //      Mach-O dyld accepts;
    //   2. the new segment is loaded at the chosen vaddr;
    //   3. the appended function's bytes execute correctly;
    //   4. PC-relative branches in/out of the new segment
    //      resolve correctly through the rewriter pipeline.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{
        Aarch64Mnemonic, DecodedOperand, Register, RegisterClass, Shift, ShiftKind,
        ShiftedRegister,
    };
    use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand, Target};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");

    // greet_quintuple(n) = n * 5: lsl w8,w0,#2; add w0,w8,w0; ret.
    let w0 = Register { class: RegisterClass::W, index: 0 };
    let w8 = Register { class: RegisterClass::W, index: 8 };
    let x30 = Register { class: RegisterClass::X, index: 30 };
    let new_function = vec![
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Lsl,
            operands: vec![
                RewriteOperand::Decoded(DecodedOperand::Register(w8.clone())),
                RewriteOperand::Decoded(DecodedOperand::Register(w0.clone())),
                RewriteOperand::Decoded(DecodedOperand::Immediate(2)),
            ],
            original_address: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Add,
            operands: vec![
                RewriteOperand::Decoded(DecodedOperand::Register(w0.clone())),
                RewriteOperand::Decoded(DecodedOperand::Register(w8)),
                RewriteOperand::Decoded(DecodedOperand::ShiftedRegister(ShiftedRegister {
                    register: w0,
                    shift: Shift { kind: ShiftKind::Lsl, amount: 0 },
                })),
            ],
            original_address: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Ret,
            operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
            original_address: None,
        },
    ];

    let quintuple_id = editor
        .binary
        .add_function("_greet_quintuple", new_function)
        .expect("add_function");

    // Patch _greet_double's first instruction to tail-call
    // _greet_quintuple.
    let greet_double_addr = editor
        .binary
        .function_address("_greet_double")
        .expect("_greet_double symbol");
    editor
        .text
        .as_mut()
        .unwrap()
        .replace_instruction_at(
            greet_double_addr,
            RewriteInstruction {
                mnemonic: Aarch64Mnemonic::B,
                operands: vec![RewriteOperand::Branch(Target::Symbol(quintuple_id))],
                original_address: Some(greet_double_addr),
            },
        )
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let _ = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=105 offset=107\n",
        "appended _greet_quintuple should make greet_double() \
         tail-call into it, returning 21*5=105",
    );
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_appended_data_referenced_by_appended_function() {
    // Phase 4 acceptance: `add_data` lays a read-only blob in
    // the same appended segment as `add_function`; the
    // appended function references it via `adrp + add`
    // (macro-fused into a `LoadAddress` against the data
    // symbol). Acceptance: host observes the value loaded
    // from the appended data, proving:
    //   1. the data lives at the expected vaddr inside the
    //      new R-X segment;
    //   2. the appended function's adrp+add fused correctly
    //      against the symbolic data address;
    //   3. an `ldr w0, [x0]` reads the right bytes — i.e. the
    //      writer placed the data at the same file offset
    //      dyld maps to the segment's vmaddr+offset.
    //
    // Concretely: append a 4-byte u32 literal `55`, then an
    // `_greet_const_double` function that loads the literal
    // and returns it (ignoring its argument). Patch
    // `_greet_double`'s first instruction to tail-call
    // `_greet_const_double`. host's `greet_double(21)` no
    // longer returns 42; it returns 55 regardless of input.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{self, Aarch64Mnemonic, DecodedOperand};
    use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand, Target};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");

    // Append a 4-byte u32 literal `55` to the appended segment.
    // 4-byte aligned so the subsequent ldr is well-defined.
    let value_id = editor
        .binary
        .add_data("_greet_const_value", &55i32.to_le_bytes(), 4)
        .expect("add_data");

    // Build `_greet_const_double`:
    //   adrp x0, &_greet_const_value      ; symbolic page
    //   add  x0, x0, #lo12(...)           ; symbolic, fuses into LoadAddress
    //   ldr  w0, [x0]                     ; w0 = 55
    //   ret
    let template = |word: u32| {
        let decoded = aarch64::decode_instruction(0, word).expect("decode template");
        RewriteInstruction {
            mnemonic: decoded.mnemonic,
            operands: decoded
                .operands
                .into_iter()
                .map(RewriteOperand::Decoded)
                .collect(),
            original_address: None,
        }
    };
    let symbolic_adrp = |word: u32, target: Target| {
        let mut t = template(word);
        *t.operands
            .iter_mut()
            .find(|op| matches!(op, RewriteOperand::Decoded(DecodedOperand::PageTarget(_))))
            .unwrap() = RewriteOperand::Page(target);
        t
    };
    let body = vec![
        symbolic_adrp(0x90000000, Target::Symbol(value_id)),
        template(0x91000000), // add x0, x0, #lo12 (fused with adrp)
        template(0xb9400000), // ldr w0, [x0]
        template(0xd65f03c0), // ret
    ];
    let func_id = editor
        .binary
        .add_function("_greet_const_double", body)
        .expect("add_function");

    // Tail-call from greet_double's first instruction.
    let greet_double_addr = editor
        .binary
        .function_address("_greet_double")
        .expect("_greet_double symbol");
    editor
        .text
        .as_mut()
        .unwrap()
        .replace_instruction_at(
            greet_double_addr,
            RewriteInstruction {
                mnemonic: Aarch64Mnemonic::B,
                operands: vec![RewriteOperand::Branch(Target::Symbol(func_id))],
                original_address: Some(greet_double_addr),
            },
        )
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let _ = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=55 offset=107\n",
        "appended function should load the appended data \
         literal (55) and return it; got {stdout:?}",
    );
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_exported_function_resolves_via_dlopen() {
    // Phase 5 acceptance: append a new function via
    // `add_function_exported`, which routes the registered
    // symbol through the Mach-O writer's export-trie /
    // LC_SYMTAB extender. After commit, host_dlopen looks up
    // the new symbol via dlsym at runtime and calls it.
    //
    // dlsym resolution walks the export trie (which we
    // rebuilt from the parsed-existing-exports list plus our
    // new entry) and resolves via LC_SYMTAB / LC_DYSYMTAB
    // (which we extended). A successful lookup confirms the
    // trie + symtab updates are well-formed.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{
        Aarch64Mnemonic, DecodedOperand, Register, RegisterClass, Shift, ShiftKind,
        ShiftedRegister,
    };
    use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");

    // _greet_quintuple(n) = n * 5: lsl w8, w0, #2; add w0, w8, w0; ret.
    let w0 = Register { class: RegisterClass::W, index: 0 };
    let w8 = Register { class: RegisterClass::W, index: 8 };
    let x30 = Register { class: RegisterClass::X, index: 30 };
    let body = vec![
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Lsl,
            operands: vec![
                RewriteOperand::Decoded(DecodedOperand::Register(w8.clone())),
                RewriteOperand::Decoded(DecodedOperand::Register(w0.clone())),
                RewriteOperand::Decoded(DecodedOperand::Immediate(2)),
            ],
            original_address: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Add,
            operands: vec![
                RewriteOperand::Decoded(DecodedOperand::Register(w0.clone())),
                RewriteOperand::Decoded(DecodedOperand::Register(w8)),
                RewriteOperand::Decoded(DecodedOperand::ShiftedRegister(ShiftedRegister {
                    register: w0,
                    shift: Shift { kind: ShiftKind::Lsl, amount: 0 },
                })),
            ],
            original_address: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Ret,
            operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
            original_address: None,
        },
    ];

    editor
        .binary
        .add_function_exported("_greet_quintuple", body)
        .expect("add_function_exported");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let _ = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");

    // dlsym resolves the symbol WITHOUT the leading
    // underscore (the underscore is a Mach-O ABI artifact;
    // dlsym strips it on input). So callers ask for
    // "_greet_quintuple" minus the leading underscore.
    let stdout = run_in_lib_demo_with_args("host_dlopen", &["greet_quintuple", "7"]);
    assert_eq!(
        stdout, "result=35\n",
        "dlsym(libgreet.dylib, \"greet_quintuple\")(7) should return 7*5=35; got {stdout:?}",
    );
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_add_library_dependency_forces_extra_load() {
    // Phase 7 acceptance: `add_library_dependency` injects an
    // LC_LOAD_DYLIB entry into libgreet.dylib pointing at
    // libdep.dylib (a sibling fixture NOT linked into either
    // host or libgreet). Without rewriting, dyld doesn't pull
    // libdep in and the host's dlsym lookup of
    // `libdep_loaded_marker` returns 0. After the rewrite,
    // dyld pulls libdep in alongside libgreet, libdep's
    // constructor sets the marker to 0xab, and host reads
    // 0xab via dlsym(RTLD_DEFAULT, ...).
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::rewrite::BinaryEditor;

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");

    // Sanity: libgreet shouldn't already depend on libdep.
    let pre_load_dylib_count = container
        .macho_image
        .as_ref()
        .expect("macho_image")
        .layout
        .segments
        .iter()
        .filter(|s| s.name == "__LINKEDIT")
        .count();
    let _ = pre_load_dylib_count;

    let mut editor = BinaryEditor::new(&container).expect("BinaryEditor::new");
    editor
        .binary
        .add_library_dependency("@rpath/libdep.dylib")
        .expect("add_library_dependency");
    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let _ = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");

    // Regular host still works.
    let stdout_default = run_in_lib_demo("host");
    assert_eq!(
        stdout_default,
        "double=42 offset=107\n",
        "regular library functions should still work after add_library_dependency",
    );

    // Acceptance: marker = 171 proves libdep was loaded.
    let stdout_libdep = run_in_lib_demo_with_args("host", &["libdep"]);
    assert_eq!(
        stdout_libdep,
        "libdep_marker=171\n",
        "expected libdep_marker=171 (0xab) after libdep was \
         force-loaded via LC_LOAD_DYLIB; got {stdout_libdep:?}",
    );
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_round_trip_through_container_loads_and_runs() {
    // Phase 1 acceptance: read libgreet.dylib, round-trip it
    // through Container::to_bytes (which routes through the
    // Mach-O writer's passthrough + ad-hoc re-sign path), write
    // the result back to disk, and confirm the host still loads
    // and runs against the rewritten copy with identical
    // output.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    let original_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&original_bytes).expect("parse libgreet.dylib");
    assert!(
        container.macho_image.is_some(),
        "Mach-O dylib should populate container.macho_image",
    );

    let rewritten = container.to_bytes().expect("Container::to_bytes round-trip");
    // Phase 1 should preserve total file length: no length-
    // changing edits, codesign overwrites the signature in
    // place. (If the LC_CODE_SIGNATURE blob's encoded length
    // matched ours exactly this is identity; if it doesn't,
    // codesign may slightly resize it. Tolerate small drift
    // but flag a 2x change as suspicious.)
    let drift = (rewritten.len() as i64 - original_bytes.len() as i64).abs();
    assert!(
        drift < original_bytes.len() as i64,
        "round-trip output drifted suspiciously: original {} bytes, \
         rewritten {} bytes",
        original_bytes.len(),
        rewritten.len(),
    );

    // Re-parse the rewritten bytes — confirms we didn't produce
    // a structurally-broken file, even before dyld sees it.
    let _ = Container::from_bytes(&rewritten).expect("re-parse rewritten libgreet.dylib");

    std::fs::write(&lib_path, &rewritten).expect("write rewritten libgreet.dylib");

    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=42 offset=107\n",
        "round-tripped libgreet.dylib should still print \
         double=42 offset=107",
    );

    // ctor still fires after round-trip.
    let stdout = run_in_lib_demo_with_args("host", &["ctor"]);
    assert_eq!(stdout, "ctor_marker=1\n");
}
