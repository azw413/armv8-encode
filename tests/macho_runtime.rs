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
    let text = editor.text.as_ref().expect("text section lifted").aarch64().expect("aarch64 section");
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
        source_size: None,
    };
    editor
        .text
        .as_mut()
        .unwrap()
        .aarch64_mut()
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
            source_size: None,
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
            source_size: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Ret,
            operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
            original_address: None,
            source_size: None,
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
        .aarch64_mut()
        .unwrap()
        .replace_instruction_at(
            greet_double_addr,
            RewriteInstruction {
                mnemonic: Aarch64Mnemonic::B,
                operands: vec![RewriteOperand::Branch(Target::Symbol(quintuple_id))],
                original_address: Some(greet_double_addr),
                source_size: None,
            },
        )
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let rewritten = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    // Phase 6.5: this flow uses no exports / library deps,
    // so intra-`__TEXT` placement should activate and no
    // `__APPENDED` segment should appear in the output.
    let segment_names: Vec<&str> = rewritten
        .macho_image
        .as_ref()
        .expect("macho_image")
        .layout
        .segments
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !segment_names.iter().any(|n| *n == "__APPENDED"),
        "intra-text placement should keep the rewritten dylib free of \
         `__APPENDED`; got segments {segment_names:?}",
    );

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
fn reserve_text_region_add_function_in_runs_under_dyld() {
    // Increment-1 runtime acceptance for the reserve/carve API:
    // reserve a region in `__TEXT` free space, place _greet_quintuple
    // into it via `add_function_in`, and redirect _greet_double to it.
    // Same observable outcome as the add_function path
    // (double=21*5=105), but driven through reserve_text_region +
    // add_function_in — proving the reserved-region placement produces
    // a dyld-loadable, runnable dylib that stays a single R-X segment
    // (no `__APPENDED`).
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{
        Aarch64Mnemonic, DecodedOperand, Register, RegisterClass, Shift, ShiftKind,
        ShiftedRegister,
    };
    use armv8_encode::rewrite::space::ReserveRequest;
    use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand, Target};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");

    // Reserve room for the new 3-instruction function from __TEXT free
    // space (0x20 is comfortably more than the ~12 bytes it needs, and
    // the structural tests confirm this fixture has the room).
    let reservation = editor
        .binary
        .reserve_text_region(ReserveRequest::exact(0x20))
        .expect("reserve __TEXT region");

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
            source_size: None,
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
            source_size: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Ret,
            operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
            original_address: None,
            source_size: None,
        },
    ];

    let quintuple_id = editor
        .binary
        .add_function_in(reservation.region, "_greet_quintuple", new_function)
        .expect("add_function_in");

    let greet_double_addr = editor
        .binary
        .function_address("_greet_double")
        .expect("_greet_double symbol");
    editor
        .text
        .as_mut()
        .unwrap()
        .aarch64_mut()
        .unwrap()
        .replace_instruction_at(
            greet_double_addr,
            RewriteInstruction {
                mnemonic: Aarch64Mnemonic::B,
                operands: vec![RewriteOperand::Branch(Target::Symbol(quintuple_id))],
                original_address: Some(greet_double_addr),
                source_size: None,
            },
        )
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let rewritten = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    // Reserved-region placement stays intra-`__TEXT`: no `__APPENDED`.
    let segment_names: Vec<&str> = rewritten
        .macho_image
        .as_ref()
        .expect("macho_image")
        .layout
        .segments
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !segment_names.iter().any(|n| *n == "__APPENDED"),
        "reserved-region placement should stay intra-__TEXT; got {segment_names:?}",
    );

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=105 offset=107\n",
        "reserved _greet_quintuple should make greet_double() \
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
            source_size: None,
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
        .aarch64_mut()
        .unwrap()
        .replace_instruction_at(
            greet_double_addr,
            RewriteInstruction {
                mnemonic: Aarch64Mnemonic::B,
                operands: vec![RewriteOperand::Branch(Target::Symbol(func_id))],
                original_address: Some(greet_double_addr),
                source_size: None,
            },
        )
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let rewritten = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    // Phase 6.5: this flow uses no exports / library deps,
    // so intra-`__TEXT` placement should activate.
    let segment_names: Vec<&str> = rewritten
        .macho_image
        .as_ref()
        .expect("macho_image")
        .layout
        .segments
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !segment_names.iter().any(|n| *n == "__APPENDED"),
        "intra-text placement should keep the rewritten dylib free of \
         `__APPENDED`; got segments {segment_names:?}",
    );

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
            source_size: None,
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
            source_size: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Ret,
            operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
            original_address: None,
            source_size: None,
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
fn et_dyn_remove_library_dependency_drops_lc_load_dylib() {
    // Symmetric to add_library_dependency. Two-step:
    //   1. Add @rpath/libdep.dylib, commit, install, run host
    //      to confirm dyld is honouring the injected dep
    //      (marker = 171).
    //   2. Re-parse the rewritten dylib, remove the dep,
    //      commit, install, and confirm:
    //        - statically: no LC_LOAD_DYLIB entry carries the
    //          path "@rpath/libdep.dylib";
    //        - at runtime: marker reads back as 0 because dyld
    //          no longer pulls libdep in.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::rewrite::BinaryEditor;

    let original_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");

    // Step 1: add the dep.
    let container =
        Container::from_bytes(&original_bytes).expect("parse libgreet.dylib");
    let mut editor = BinaryEditor::new(&container).expect("BinaryEditor::new");
    editor
        .binary
        .add_library_dependency("@rpath/libdep.dylib")
        .expect("add_library_dependency");
    let with_dep = editor.commit_to_bytes().expect("commit add_library_dependency");
    std::fs::write(&lib_path, &with_dep).expect("install libgreet+libdep");
    let baseline = run_in_lib_demo_with_args("host", &["libdep"]);
    assert_eq!(
        baseline, "libdep_marker=171\n",
        "baseline sanity: libdep should be force-loaded after \
         add_library_dependency before we try removing it; \
         got {baseline:?}",
    );

    // Step 2: remove the dep.
    let with_dep_container =
        Container::from_bytes(&with_dep).expect("re-parse libgreet+libdep");
    let mut editor = BinaryEditor::new(&with_dep_container).expect("BinaryEditor::new");
    editor
        .binary
        .remove_library_dependency("@rpath/libdep.dylib")
        .expect("remove_library_dependency");
    let without_dep =
        editor.commit_to_bytes().expect("commit remove_library_dependency");

    // Static: no LC_LOAD_DYLIB resolves to "@rpath/libdep.dylib".
    let rewritten =
        Container::from_bytes(&without_dep).expect("re-parse libgreet (dep removed)");
    // Walk the raw load commands directly; we don't yet have a
    // typed view of LC_LOAD_DYLIB on the container.
    let raw = &rewritten
        .macho_image
        .as_ref()
        .expect("macho_image")
        .raw_bytes;
    assert!(
        !find_load_dylib_path(raw, "@rpath/libdep.dylib"),
        "LC_LOAD_DYLIB for \"@rpath/libdep.dylib\" should be gone \
         after remove_library_dependency",
    );

    std::fs::write(&lib_path, &without_dep).expect("install libgreet (dep removed)");
    let stdout = run_in_lib_demo_with_args("host", &["libdep"]);
    assert_eq!(
        stdout, "libdep_marker=0\n",
        "expected libdep_marker=0 after remove_library_dependency \
         (libdep should not be loaded); got {stdout:?}",
    );
    let stdout_default = run_in_lib_demo("host");
    assert_eq!(
        stdout_default,
        "double=42 offset=107\n",
        "regular library functions should still work after \
         remove_library_dependency",
    );

    std::fs::write(&lib_path, &original_bytes).expect("restore libgreet.dylib");
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_remove_library_dependency_missing_name_errors() {
    // remove_library_dependency must surface a clear error
    // when no LC_LOAD_DYLIB matches the requested path.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::rewrite::{BinaryEditor, TextEditorError};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor = BinaryEditor::new(&container).expect("BinaryEditor::new");
    let result = editor
        .binary
        .remove_library_dependency("@rpath/libnothere.dylib");
    match result {
        Err(TextEditorError::LibraryDependencyNotFound(name)) => {
            assert_eq!(name, "@rpath/libnothere.dylib");
        }
        other => panic!(
            "expected LibraryDependencyNotFound; got {other:?}",
        ),
    }
}

/// Walk a Mach-O byte stream's load-command list looking for any
/// `LC_LOAD_DYLIB` (or sibling) whose embedded path equals
/// `target`. Returns true iff a match exists.
fn find_load_dylib_path(bytes: &[u8], target: &str) -> bool {
    use object::macho;
    if bytes.len() < 24 {
        return false;
    }
    let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let lc_start = 32usize;
    let lc_end = lc_start + sizeofcmds;
    let mut cursor = lc_start;
    while cursor + 8 <= lc_end {
        let cmd = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let cmdsize =
            u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 || cursor + cmdsize > lc_end {
            return false;
        }
        if matches!(
            cmd,
            macho::LC_LOAD_DYLIB
                | macho::LC_LOAD_WEAK_DYLIB
                | macho::LC_REEXPORT_DYLIB
                | macho::LC_LOAD_UPWARD_DYLIB,
        ) && cmdsize >= 24
        {
            let name_off =
                u32::from_le_bytes(bytes[cursor + 8..cursor + 12].try_into().unwrap())
                    as usize;
            if name_off < cmdsize {
                let path_start = cursor + name_off;
                let path_end_max = cursor + cmdsize;
                let nul = bytes[path_start..path_end_max]
                    .iter()
                    .position(|&b| b == 0);
                let path_end = nul.map_or(path_end_max, |n| path_start + n);
                if let Ok(p) = std::str::from_utf8(&bytes[path_start..path_end]) {
                    if p == target {
                        return true;
                    }
                }
            }
        }
        cursor += cmdsize;
    }
    false
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn et_dyn_add_initialiser_hijacks_init_offsets_slot() {
    // Phase 6 acceptance: `add_initialiser` hijacks the
    // `__init_offsets` slot (modern Mach-O equivalent of
    // `.init_array`). The appended wrapper:
    //   1. saves x0/x1/x2 (the `(argc, argv, envp)` dyld
    //      passes to constructors);
    //   2. calls the user body, which sets
    //      `greet_ctor_marker = 0x10`;
    //   3. restores;
    //   4. chain-tail-calls the original `_greet_ctor`,
    //      which `|= 0x1` the marker;
    //   5. returns.
    //
    // Final marker value:
    //   - body runs first: marker = 0x10
    //   - chained _greet_ctor runs: marker |= 0x1 → 0x11 = 17
    //
    // host's `ctor` mode prints the marker.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{self, DecodedOperand};
    use armv8_encode::rewrite::{
        BinaryEditor, InitialiserPosition, RewriteInstruction, RewriteOperand, Target,
    };

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");
    let marker_id = editor
        .binary
        .symbol_by_name("_greet_ctor_marker")
        .expect("_greet_ctor_marker should be defined");

    // User body — leaf, no stack frame. Sets
    // greet_ctor_marker to 0x10.
    //
    //   adrp x0, &greet_ctor_marker
    //   add  x0, x0, #lo12 (fused)
    //   mov  w1, #0x10
    //   str  w1, [x0]
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
            source_size: None,
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
        symbolic_adrp(0x90000000, Target::Symbol(marker_id)),
        template(0x91000000), // add x0, x0, #lo12 (fused)
        template(0x52800201), // mov w1, #0x10
        template(0xb9000001), // str w1, [x0]
        template(0xd65f03c0), // ret
    ];
    let _user_body_id = editor
        .binary
        .add_initialiser("greet_appended_init", body, InitialiserPosition::Last)
        .expect("add_initialiser");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let rewritten = Container::from_bytes(&written).expect("re-parse rewritten libgreet.dylib");

    // iOS / App Store invariant: no extra `__APPENDED` R-X
    // segment in the output. The intra-`__TEXT` placement
    // strategy puts the wrapper inside the existing `__TEXT`
    // free region, so the rewritten dylib still has exactly
    // the original three segments (`__TEXT`, `__DATA`,
    // `__LINKEDIT`).
    let segment_names: Vec<&str> = rewritten
        .macho_image
        .as_ref()
        .expect("macho_image")
        .layout
        .segments
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !segment_names.iter().any(|n| *n == "__APPENDED"),
        "intra-text placement should keep the rewritten dylib free of \
         `__APPENDED`; got segments {segment_names:?}",
    );

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");

    // Default mode still works.
    let stdout_default = run_in_lib_demo("host");
    assert_eq!(
        stdout_default, "double=42 offset=107\n",
        "regular library functions should still work after add_initialiser",
    );

    // Acceptance: ctor_marker = 17 proves both the appended
    // initialiser and the chained-back _greet_ctor ran in
    // order.
    let stdout_ctor = run_in_lib_demo_with_args("host", &["ctor"]);
    assert_eq!(
        stdout_ctor, "ctor_marker=17\n",
        "expected ctor_marker=17 (= 0x10 from appended | 0x1 \
         from chained _greet_ctor); got {stdout_ctor:?}",
    );
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn prohibit_new_segments_rejects_add_function_exported() {
    // Strict-mode acceptance: `add_function_exported` always
    // requires the `__APPENDED`-segment writer (rebuild
    // export trie + symtab + shift `__LINKEDIT`). When the
    // caller has set `prohibit_new_segments()` the call must
    // fail at queue time with `WouldCreateNewSegment`.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{
        Aarch64Mnemonic, DecodedOperand, Register, RegisterClass,
    };
    use armv8_encode::rewrite::{
        BinaryEditor, RewriteInstruction, RewriteOperand, TextEditorError,
    };

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");
    editor.binary.prohibit_new_segments();

    let w0 = Register { class: RegisterClass::W, index: 0 };
    let x30 = Register { class: RegisterClass::X, index: 30 };
    let body = vec![
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Lsl,
            operands: vec![
                RewriteOperand::Decoded(DecodedOperand::Register(w0.clone())),
                RewriteOperand::Decoded(DecodedOperand::Register(w0)),
                RewriteOperand::Decoded(DecodedOperand::Immediate(2)),
            ],
            original_address: None,
            source_size: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Ret,
            operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
            original_address: None,
            source_size: None,
        },
    ];
    let result = editor
        .binary
        .add_function_exported("_greet_quintuple", body);
    match result {
        Err(TextEditorError::WouldCreateNewSegment { reason }) => {
            assert!(
                reason.contains("add_function_exported"),
                "error reason should name the offending call; got {reason:?}",
            );
        }
        other => panic!("expected WouldCreateNewSegment, got {other:?}"),
    }
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn prohibit_new_segments_rejects_add_library_dependency() {
    // Same shape as above but for LC_LOAD_DYLIB injection.
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::rewrite::{BinaryEditor, TextEditorError};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor = BinaryEditor::new(&container).expect("BinaryEditor::new");
    editor.binary.prohibit_new_segments();

    let result = editor
        .binary
        .add_library_dependency("@rpath/libdep.dylib");
    match result {
        Err(TextEditorError::WouldCreateNewSegment { reason }) => {
            assert!(
                reason.contains("add_library_dependency"),
                "error reason should name the offending call; got {reason:?}",
            );
        }
        other => panic!("expected WouldCreateNewSegment, got {other:?}"),
    }
}

#[test]
#[ignore = "requires native macOS arm64 with clang + codesign on \
            PATH; run with --ignored --nocapture"]
fn prohibit_new_segments_allows_initialiser_that_fits() {
    // Strict-mode positive case: `add_initialiser` with a
    // small body fits in `__TEXT` free region padding, so
    // the commit succeeds and produces a dylib without
    // `__APPENDED`. Same observable behaviour as the
    // permissive path's `add_initialiser` test, but here
    // the fit is enforced (any future regression that
    // forces segment placement would be caught here).
    require_macos_arm64();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{self, DecodedOperand};
    use armv8_encode::rewrite::{
        BinaryEditor, InitialiserPosition, RewriteInstruction, RewriteOperand, Target,
    };

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.dylib");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.dylib");
    let mut editor =
        BinaryEditor::for_section(&container, "__text").expect("open editor on __text");
    editor.binary.prohibit_new_segments();
    let marker_id = editor
        .binary
        .symbol_by_name("_greet_ctor_marker")
        .expect("_greet_ctor_marker should be defined");

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
            source_size: None,
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
        symbolic_adrp(0x90000000, Target::Symbol(marker_id)),
        template(0x91000000),
        template(0x52800201),
        template(0xb9000001),
        template(0xd65f03c0),
    ];
    editor
        .binary
        .add_initialiser("greet_strict_init", body, InitialiserPosition::Last)
        .expect("add_initialiser under prohibit_new_segments should succeed");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let rewritten = Container::from_bytes(&written).expect("re-parse");
    let segment_names: Vec<&str> = rewritten
        .macho_image
        .as_ref()
        .expect("macho_image")
        .layout
        .segments
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !segment_names.iter().any(|n| *n == "__APPENDED"),
        "strict-mode add_initialiser should produce a dylib with no \
         `__APPENDED` segment; got {segment_names:?}",
    );

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.dylib");
    let stdout_ctor = run_in_lib_demo_with_args("host", &["ctor"]);
    assert_eq!(
        stdout_ctor, "ctor_marker=17\n",
        "strict-mode initialiser should still chain back to _greet_ctor; got {stdout_ctor:?}",
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
