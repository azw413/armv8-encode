//! Runtime smoke tests for the ELF writer.
//!
//! Confirms that `Container::to_bytes` produces an object file the system
//! linker accepts and the dynamic linker can run. Stage 1 only covers
//! ET_REL: read a `.o`, round-trip it through the container, link, run,
//! diff stdout. Real `.so` runtime tests land with stage 5 (the
//! `object::write::elf::Writer` path).
//!
//! All tests are `#[ignore]` and require Docker:
//!
//!   docker buildx build --platform linux/arm64 \
//!       -t armv8-encode-runtime tests/elf_runtime
//!   cargo test --test elf_runtime -- --ignored --nocapture
//!
//! Set `ARMV8_RUNTIME_IMAGE` to override the image tag if you've built
//! it under a different name.

use armv8_encode::container::Container;
use std::path::PathBuf;
use std::process::{Command, Output};

const DEFAULT_IMAGE: &str = "armv8-encode-runtime";
const EXPECTED_STDOUT: &str = "answer=42\n";

/// Resolve the runtime image tag. CI may pass a different one to pin a
/// digest.
fn image_tag() -> String {
    std::env::var("ARMV8_RUNTIME_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string())
}

/// Path to `tests/elf_runtime/` from the crate root. We stay relative to
/// `CARGO_MANIFEST_DIR` so the harness works regardless of where `cargo
/// test` was invoked from.
fn harness_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/elf_runtime")
}

/// Skip with a clear message when Docker isn't available, instead of
/// failing. Contributors without Docker should still be able to run the
/// rest of the suite.
fn require_docker() {
    let probe = Command::new("docker").arg("version").output();
    match probe {
        Ok(out) if out.status.success() => {}
        Ok(out) => panic!(
            "docker is installed but `docker version` failed (exit {}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        ),
        Err(err) => panic!(
            "docker not available: {err}. Install Docker Desktop / Engine \
             and ensure linux/arm64 emulation is enabled, then re-run with \
             --ignored",
        ),
    }

    // Confirm the image exists. We don't auto-build — image build pulls
    // a base image and packages, which is too slow for a per-test step
    // and surprising as a side effect.
    let inspect = Command::new("docker")
        .args(["image", "inspect", &image_tag()])
        .output()
        .expect("docker image inspect");
    assert!(
        inspect.status.success(),
        "image `{}` not found. Build it first:\n  \
         docker buildx build --platform linux/arm64 -t {} tests/elf_runtime",
        image_tag(),
        image_tag(),
    );
}

/// Run a command inside the runtime container with the harness directory
/// bind-mounted at /work. Stdin is closed; stdout/stderr captured.
fn docker_run(workdir_subpath: &str, argv: &[&str]) -> Output {
    let mount = format!("{}:/work", harness_dir().display());
    let workdir = format!("/work/{workdir_subpath}");

    let mut cmd = Command::new("docker");
    cmd.args([
        "run",
        "--rm",
        "--platform=linux/arm64",
        "-v",
        &mount,
        "-w",
        &workdir,
        &image_tag(),
    ])
    .args(argv);
    cmd.output().expect("docker run")
}

/// Compile `hello.c` to `hello.o` inside the container. Idempotent: the
/// container `build.sh` re-clobbers the artifact each time, so concurrent
/// test runs against the same checkout will race. Tests serialize via
/// the host filesystem layout — each scenario writes to a unique output
/// path under `tests/elf_runtime/scratch/`.
fn build_fixture_o() -> PathBuf {
    let out = docker_run("fixtures", &["sh", "build.sh"]);
    assert!(
        out.status.success(),
        "fixture build failed: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    harness_dir().join("fixtures/hello.o")
}

/// Link `object_path` (an `.o` already on disk under the harness dir)
/// into an executable named `binary_name`, run it, and return stdout.
/// Errors out the test on any non-zero exit.
fn link_and_run(object_relative_path: &str, binary_name: &str) -> String {
    // Use clang as the linker driver so we don't have to manage crt
    // objects by hand — clang knows where they live in the image.
    let link = docker_run(
        "scratch",
        &[
            "clang",
            "--target=aarch64-linux-gnu",
            "-fuse-ld=lld",
            "-no-pie",
            "-o",
            binary_name,
            &format!("../{object_relative_path}"),
        ],
    );
    assert!(
        link.status.success(),
        "link of {object_relative_path} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&link.stdout),
        String::from_utf8_lossy(&link.stderr),
    );

    // `timeout 10` inside the container so a runaway binary fails
    // loudly instead of stalling cargo test indefinitely. 10 s is far
    // longer than QEMU needs for a "print one line and exit" program.
    let run = docker_run("scratch", &["timeout", "10", &format!("./{binary_name}")]);
    assert!(
        run.status.success(),
        "running ./{binary_name} failed (exit {}):\nstdout:\n{}\nstderr:\n{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8(run.stdout).expect("stdout utf8")
}

/// Ensure `tests/elf_runtime/scratch/` exists and is empty enough for
/// our writes. We never `rm -rf` user paths; we just create the
/// directory and overwrite known filenames.
fn ensure_scratch_dir() -> PathBuf {
    let scratch = harness_dir().join("scratch");
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    scratch
}

/// Build the lib_demo fixture (libgreet.so + host) inside the
/// runtime container. Returns paths to both artefacts.
fn build_lib_demo_fixture() -> (PathBuf, PathBuf) {
    let out = docker_run("fixtures/lib_demo", &["sh", "build.sh"]);
    assert!(
        out.status.success(),
        "lib_demo fixture build failed: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    let dir = harness_dir().join("fixtures/lib_demo");
    (dir.join("libgreet.so"), dir.join("host"))
}

/// Run a binary in the lib_demo fixture directory (where it can find
/// `libgreet.so` via the `$ORIGIN` rpath baked at link time). Returns
/// stdout. The binary path is relative to the fixture directory.
fn run_in_lib_demo(binary_name: &str) -> String {
    let run = docker_run(
        "fixtures/lib_demo",
        &["timeout", "10", &format!("./{binary_name}")],
    );
    assert!(
        run.status.success(),
        "running ./{binary_name} in lib_demo failed (exit {}):\nstdout:\n{}\nstderr:\n{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8(run.stdout).expect("stdout utf8")
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user) and the \
            armv8-encode-runtime image; run with --ignored --nocapture"]
fn baseline_fixture_runs_and_prints_expected_output() {
    require_docker();
    let _ = build_fixture_o();
    ensure_scratch_dir();

    // Sanity check: the unmodified .o links and runs and prints what we
    // expect. If this fails, the harness or fixture is wrong, not the
    // rewriter.
    let stdout = link_and_run("fixtures/hello.o", "hello_baseline");
    assert_eq!(stdout, EXPECTED_STDOUT);
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user); run with \
            --ignored --nocapture"]
fn identity_round_trip_through_container_still_runs() {
    require_docker();
    let object_path = build_fixture_o();
    let scratch = ensure_scratch_dir();

    // Read the .o, write it back unchanged through the container.
    let bytes = std::fs::read(&object_path).expect("read hello.o");
    let container = Container::from_bytes(&bytes).expect("parse hello.o");
    let written = container.to_bytes().expect("write hello.o");

    let out_path = scratch.join("hello_identity.o");
    std::fs::write(&out_path, &written).expect("write identity .o");

    // Sanity check: re-parse must succeed. We don't assert section-list
    // equality because `object::write` reconstructs writer-managed
    // sections (`.strtab`, `.shstrtab`, `.symtab`) at different sizes
    // and positions than the input — that's expected and structurally
    // correct. The runtime check below is the authoritative oracle.
    let _reparsed = Container::from_bytes(&written).expect("reparse");

    let stdout = link_and_run("scratch/hello_identity.o", "hello_identity");
    assert_eq!(
        stdout, EXPECTED_STDOUT,
        "identity round-trip changed program behaviour",
    );
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user); run with \
            --ignored --nocapture"]
fn rewritten_text_section_still_runs() {
    require_docker();
    let object_path = build_fixture_o();
    let scratch = ensure_scratch_dir();

    // Lift the text section through the rewrite pipeline with no edits.
    // This exercises sweep → CFG → lift → lay_out → emit → splice → write,
    // which is the path stages 2–4 will lean on. A no-op rewrite must
    // produce a runnable program; if it doesn't, something in the
    // pipeline is mutating bytes it shouldn't.
    use armv8_encode::isa::aarch64;
    use armv8_encode::mc::build_cfg;
    use armv8_encode::rewrite::{commit_to_container, emit, lay_out, RewritePlan};

    let bytes = std::fs::read(&object_path).expect("read hello.o");
    let container = Container::from_bytes(&bytes).expect("parse hello.o");

    // Find .text. The fixture is small; one text section.
    let text = container
        .text_sections()
        .next()
        .expect("hello.o has a .text section");
    let text_id = text.id;
    let (base, code) = text.for_disassembly().expect("text disassembly handle");
    let instructions = aarch64::disassemble_bytes(base, code).expect("decode .text");
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

    let layout = lay_out(&plan, base, Some(&container)).expect("layout");
    let output = emit(&plan, &layout, Some(&container)).expect("emit");

    let edited = commit_to_container(&container, text_id, output);
    let written = edited.to_bytes().expect("write edited container");

    let out_path = scratch.join("hello_rewritten.o");
    std::fs::write(&out_path, &written).expect("write rewritten .o");

    let stdout = link_and_run("scratch/hello_rewritten.o", "hello_rewritten");
    assert_eq!(
        stdout, EXPECTED_STDOUT,
        "no-op rewrite changed program behaviour",
    );
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user); run with \
            --ignored --nocapture"]
fn data_edit_redirects_function_pointer_in_funcs_array() {
    // Stage 3 acceptance: edit `.data` to swap `funcs[0]` from `answer`
    // to `replacement`. After link+run, `funcs[0]()` invokes
    // `replacement`, so the program prints `answer=99` instead of
    // `answer=42`.
    //
    // Exercises the full data-section pipeline: lift `.data` →
    // redirect a pointer item → emit → commit → write → link → run.
    // The fixture's main calls `funcs[argc>1]()`; with no argv args
    // it always picks `funcs[0]`.
    require_docker();
    let object_path = build_fixture_o();
    let scratch = ensure_scratch_dir();

    use armv8_encode::container::SymbolKind;
    use armv8_encode::rewrite::{
        commit_to_data_container, emit_data_section, DataSection, Target,
    };

    let bytes = std::fs::read(&object_path).expect("read hello.o");
    let container = Container::from_bytes(&bytes).expect("parse hello.o");

    // Locate `.data` and the `replacement` function symbol.
    let data_section_id = container
        .sections
        .iter()
        .find(|s| s.name == ".data")
        .expect("`.data` section")
        .id;
    let replacement_symbol_id = container
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Function && s.name == "replacement")
        .expect("`replacement` function symbol")
        .id;
    let funcs_symbol_id = container
        .symbols
        .iter()
        .find(|s| s.name == "funcs")
        .expect("`funcs` array symbol")
        .id;

    // Lift `.data`, find the item labelled `funcs` (the array's
    // first element), redirect it.
    let mut lifted = DataSection::lift(&container, data_section_id).expect("lift .data");
    let funcs_index = lifted
        .plan
        .find_by_label(funcs_symbol_id)
        .expect("`funcs` symbol labels an item in .data");
    lifted
        .plan
        .redirect_pointer_at(funcs_index, Target::Symbol(replacement_symbol_id))
        .expect("redirect funcs[0]");

    let output = emit_data_section(&lifted.plan);
    let edited = commit_to_data_container(
        &container,
        data_section_id,
        output,
        lifted.unhandled_relocations,
    );
    let written = edited.to_bytes().expect("write edited container");

    std::fs::write(scratch.join("hello_data_edit.o"), &written).expect("write");

    let stdout = link_and_run("scratch/hello_data_edit.o", "hello_data_edit");
    assert_eq!(
        stdout, "answer=99\n",
        "redirecting funcs[0] should make funcs[0]() return 99",
    );
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user); run with \
            --ignored --nocapture"]
fn et_dyn_round_trip_through_container_loads_and_runs() {
    // Stage 5 acceptance: read libgreet.so (a real ET_DYN clang
    // shared library), write it back through Container::to_bytes()
    // unchanged, replace the original, then run the host program
    // that links against it. The dynamic linker must load the
    // rewritten library and the program must produce the same
    // observable output as the baseline.
    require_docker();
    let (lib_path, host_path) = build_lib_demo_fixture();

    // Sanity: baseline runs and prints what we expect.
    let baseline_stdout = run_in_lib_demo("host");
    assert_eq!(
        baseline_stdout, "double=42 offset=107\n",
        "baseline lib_demo run produces unexpected output",
    );

    // Read libgreet.so → Container → to_bytes() → re-parse →
    // assert structural equivalence to the source. (Re-parse is
    // first to make a quick "did we produce a valid ELF at all"
    // check before going to QEMU.)
    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.so");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.so");
    assert_eq!(
        container.kind,
        armv8_encode::container::ContainerKind::SharedObject,
        "libgreet.so should classify as SharedObject",
    );
    assert!(
        container.elf_image.is_some(),
        "libgreet.so should have an attached ElfImage",
    );

    let rewritten = container.to_bytes().expect("Container::to_bytes for ET_DYN");
    let _ = Container::from_bytes(&rewritten).expect("re-parse rewritten libgreet.so");

    // Structural-equivalence diff against the source. The diff list
    // surfaces any mismatches the dynamic linker would care about
    // (program-header shape, allocated section content, .dynamic
    // tags, dynsym exports, build-ID, .interp).
    let diffs = elf_equivalence::compare(&lib_bytes, &rewritten);
    assert!(
        diffs.is_empty(),
        "rewritten libgreet.so is not structurally equivalent to source: {:#?}",
        diffs,
    );

    // Replace libgreet.so with the rewritten version and run host.
    // The host's rpath is `$ORIGIN`, so it loads the libgreet.so
    // that lives next to it — i.e. our rewritten copy.
    let _ = host_path;
    std::fs::write(&lib_path, &rewritten).expect("write rewritten libgreet.so");

    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=42 offset=107\n",
        "host running against rewritten libgreet.so should produce \
         identical output to the baseline",
    );
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user); run with \
            --ignored --nocapture"]
fn et_dyn_inplace_text_edit_changes_observable_output() {
    // Stage 6 acceptance: edit libgreet.so's `.text` through the
    // high-level [`TextEditor`] API and observe the change at
    // runtime via the host program.
    //
    // The edit replaces greet_double's `lsl Wd, Wn, #1` (n*2)
    // with `lsl Wd, Wn, #2` (n*4). After the rewrite, host's
    // `funcs[0]()` returns 21*4 = 84 instead of 21*2 = 42.
    //
    // The same edit done via raw byte poking lives in
    // `examples/text_edit_so.rs`; this test is the runtime
    // counterpart that proves the high-level API drives the
    // ET_DYN writer end-to-end.
    require_docker();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{Aarch64Mnemonic, DecodedOperand};
    use armv8_encode::rewrite::{RewriteInstruction, RewriteOperand, TextEditor};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.so");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.so");

    // Open the editor and locate greet_double's `lsl`.
    let mut editor = TextEditor::for_section(&container, ".text").expect("open editor");
    let function_address = editor
        .function_address("greet_double")
        .expect("greet_double symbol");
    let lsl_index = editor
        .instructions()
        .iter()
        .position(|insn| {
            insn.address >= function_address && insn.mnemonic == Aarch64Mnemonic::Lsl
        })
        .expect("greet_double should contain an lsl");
    let lsl_address = editor.instructions()[lsl_index].address;
    let original_lsl = &editor.instructions()[lsl_index];

    // Confirm the fixture's compiled shape matches what we expect
    // before mutating. If the compiler changes shape the test
    // should fail loudly here.
    let (rd, rn) = match (&original_lsl.operands[0], &original_lsl.operands[1]) {
        (DecodedOperand::Register(rd), DecodedOperand::Register(rn)) => (rd.clone(), rn.clone()),
        other => panic!("unexpected lsl operands: {other:?}"),
    };
    let original_shift = match &original_lsl.operands[2] {
        DecodedOperand::Immediate(n) => *n,
        other => panic!("unexpected lsl shift: {other:?}"),
    };
    assert_eq!(
        original_shift, 1,
        "fixture greet_double should use `lsl Wd, Wn, #1`",
    );

    // Build and install a replacement instruction with shift=2.
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
        .replace_instruction_at(lsl_address, new_instruction)
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let _ = Container::from_bytes(&written).expect("re-parse rewritten libgreet.so");

    // Replace libgreet.so with the rewritten version and run host.
    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.so");
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=84 offset=107\n",
        "in-place .text edit through TextEditor should make \
         greet_double return n*4=84 instead of n*2=42",
    );
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user); run with \
            --ignored --nocapture"]
fn et_dyn_appended_function_changes_observable_output() {
    // Decorator pattern: append a new function to libgreet.so via
    // TextEditor::add_function (lands in a fresh PT_LOAD R-X
    // segment past the input's mapped range), then patch
    // greet_double's first instruction to be `b greet_quintuple`
    // so the host's existing call lands in the new code.
    //
    // greet_quintuple(n) = n * 5, so host's funcs[0](21) returns
    // 21*5 = 105 instead of 21*2 = 42. This is the observable
    // signal that the new segment loaded and its content executes.
    require_docker();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{
        Aarch64Mnemonic, DecodedOperand, Register, RegisterClass, Shift, ShiftKind,
        ShiftedRegister,
    };
    use armv8_encode::rewrite::{RewriteInstruction, RewriteOperand, Target, TextEditor};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.so");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.so");
    let mut editor = TextEditor::for_section(&container, ".text").expect("open editor");

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
        .add_function("greet_quintuple", new_function)
        .expect("add_function");

    // Patch greet_double's first instruction to tail-call
    // greet_quintuple.
    let greet_double_addr = editor
        .function_address("greet_double")
        .expect("greet_double symbol");
    editor
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
    let _ = Container::from_bytes(&written).expect("re-parse rewritten libgreet.so");

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.so");
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout, "double=105 offset=107\n",
        "appended greet_quintuple should make funcs[0]() = greet_double() \
         tail-call into greet_quintuple, returning 21*5=105",
    );
}

#[test]
#[ignore = "requires Docker with linux/arm64 (qemu-user); run with \
            --ignored --nocapture"]
fn et_dyn_appended_function_can_call_extern_via_plt() {
    // Stage 8 acceptance: append a new function that calls an
    // existing PLT-bound extern (`puts`), and prints a message
    // from a string blob also placed in the appended segment.
    //
    // libgreet.c imports `puts` via the `_greet_unused_puts_anchor`
    // hack so a .plt stub already exists. The rewriter's
    // `callable_address_of_symbol` resolves Target::Symbol(puts)
    // to that stub's address, and emit folds the call into a
    // direct `bl <stub>`.
    //
    // The new function body matches the C ABI for a leaf-ish
    // function that calls one extern: stp/mov/str + adrp/add +
    // bl puts + ldr/lsl + ldp + ret.
    require_docker();
    let (lib_path, _) = build_lib_demo_fixture();

    use armv8_encode::isa::aarch64::{self, Aarch64Mnemonic, DecodedOperand};
    use armv8_encode::rewrite::{RewriteInstruction, RewriteOperand, Target, TextEditor};

    let lib_bytes = std::fs::read(&lib_path).expect("read libgreet.so");
    let container = Container::from_bytes(&lib_bytes).expect("parse libgreet.so");
    let mut editor = TextEditor::for_section(&container, ".text").expect("open editor");

    // Resolve puts (versioned spelling on glibc).
    let puts_id = editor
        .symbol_by_name("puts@GLIBC_2.17")
        .or_else(|_| editor.symbol_by_name("puts"))
        .expect("libgreet.so should import puts via the anchor in libgreet.c");

    // Append the message before the function so the function's
    // adrp+add can target it.
    let message = b"greet_double called via appended decorator\0";
    let msg_id = editor
        .add_data("greet_log_msg", message, /*align=*/ 1)
        .expect("add_data");

    // Helper to lift a known-good encoded word into a
    // RewriteInstruction (skips hand-rolling operand vecs).
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

    // adrp x0, &msg ; replace numeric PageTarget with symbolic.
    let mut adrp_msg = template(0x90000000);
    *adrp_msg
        .operands
        .iter_mut()
        .find(|op| matches!(op, RewriteOperand::Decoded(DecodedOperand::PageTarget(_))))
        .unwrap() = RewriteOperand::Page(Target::Symbol(msg_id));
    // add x0, x0, #0 ; placeholder offset, fused with adrp by the
    // macro pass and resolved to lo12(msg).
    let add_msg = template(0x91000000);
    // bl puts ; replace numeric BranchTarget with symbolic.
    let mut bl_puts = template(0x94000000);
    *bl_puts
        .operands
        .iter_mut()
        .find(|op| matches!(op, RewriteOperand::Decoded(DecodedOperand::BranchTarget(_))))
        .unwrap() = RewriteOperand::Branch(Target::Symbol(puts_id));

    let body = vec![
        template(0xa9be7bfd),  // stp x29, x30, [sp, #-32]!
        template(0x910003fd),  // mov x29, sp
        template(0xb90013e0),  // str w0, [sp, #16]
        adrp_msg,              // adrp x0, &msg
        add_msg,               // add  x0, x0, #lo12(msg)
        bl_puts,               // bl   puts
        template(0xb94013e0),  // ldr w0, [sp, #16]
        template(0x531f7800),  // lsl w0, w0, #1
        template(0xa8c27bfd),  // ldp x29, x30, [sp], #32
        template(0xd65f03c0),  // ret
    ];
    let log_id = editor
        .add_function("greet_log_double", body)
        .expect("add_function");

    // Patch greet_double to tail-call the new function.
    let greet_double_addr = editor
        .function_address("greet_double")
        .expect("greet_double symbol");
    editor
        .replace_instruction_at(
            greet_double_addr,
            RewriteInstruction {
                mnemonic: Aarch64Mnemonic::B,
                operands: vec![RewriteOperand::Branch(Target::Symbol(log_id))],
                original_address: Some(greet_double_addr),
            },
        )
        .expect("replace_instruction_at");

    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    let _ = Container::from_bytes(&written).expect("re-parse");

    std::fs::write(&lib_path, &written).expect("write rewritten libgreet.so");
    let stdout = run_in_lib_demo("host");
    assert_eq!(
        stdout,
        "greet_double called via appended decorator\ndouble=42 offset=107\n",
        "appended function should call puts via PLT stub before \
         returning n*2 to the host",
    );
}

// ---- ELF structural-equivalence checker --------------------------------
//
// Used by Stage 5 round-trip tests as a stronger oracle than "did
// objdump segfault." Compares two ELF byte streams along the axes the
// dynamic linker cares about: file header constants, program-header
// shape, allocated section content, .dynamic tag list, .dynsym
// exports, build-ID, .interp.
//
// File offsets, .symtab/.strtab/.shstrtab content, and DWARF debug
// sections are deliberately *not* compared — `object::write::elf::Writer`
// rebuilds those internally and they don't affect runtime behaviour.

#[allow(dead_code)] // some helpers are reserved for future stage-5 tests
mod elf_equivalence {
    use object::elf;
    use object::read::elf::{Dyn, ElfFile64, ProgramHeader, SectionHeader, Sym};
    use object::read::{Object, ObjectSection};
    use object::Endianness;
    use std::collections::BTreeMap;

    /// Mismatch report. Carries enough context for a panicking
    /// assertion message to point at the first discrepancy.
    #[derive(Debug, Clone, Eq, PartialEq)]
    pub enum Mismatch {
        FileHeader { field: &'static str, lhs: u64, rhs: u64 },
        ProgramHeaderCount { lhs: usize, rhs: usize },
        ProgramHeader { index: usize, field: &'static str, lhs: u64, rhs: u64 },
        AllocatedSectionMissing { name: String, side: &'static str },
        AllocatedSectionSize { name: String, lhs: u64, rhs: u64 },
        AllocatedSectionContent { name: String, first_diff_byte: usize },
        DynamicTagMissing { tag: u64, side: &'static str },
        DynamicTagValue { tag: u64, lhs: u64, rhs: u64 },
        DynamicSymbolMissing { name: String, side: &'static str },
        DynamicSymbolValue { name: String, field: &'static str, lhs: u64, rhs: u64 },
        BuildId { lhs: Option<Vec<u8>>, rhs: Option<Vec<u8>> },
        Interp { lhs: Option<String>, rhs: Option<String> },
    }

    /// Compare two ELF64 byte streams structurally. Returns the list
    /// of mismatches; an empty vec means "behaviorally equivalent."
    pub fn compare(lhs: &[u8], rhs: &[u8]) -> Vec<Mismatch> {
        let lhs_file = match ElfFile64::<Endianness>::parse(lhs) {
            Ok(f) => f,
            Err(err) => panic!("lhs not a valid ELF64: {err}"),
        };
        let rhs_file = match ElfFile64::<Endianness>::parse(rhs) {
            Ok(f) => f,
            Err(err) => panic!("rhs not a valid ELF64: {err}"),
        };

        let mut diffs = Vec::new();
        diffs.extend(compare_file_header(&lhs_file, &rhs_file));
        diffs.extend(compare_program_headers(&lhs_file, &rhs_file));
        diffs.extend(compare_allocated_sections(&lhs_file, &rhs_file));
        diffs.extend(compare_dynamic(&lhs_file, &rhs_file, lhs, rhs));
        diffs.extend(compare_dynsym(&lhs_file, &rhs_file));
        diffs.extend(compare_build_id(&lhs_file, &rhs_file));
        diffs.extend(compare_interp(&lhs_file, &rhs_file));
        diffs
    }

    /// Convenience: panic with a helpful message if the two streams
    /// aren't equivalent.
    pub fn assert_equivalent(lhs: &[u8], rhs: &[u8]) {
        let diffs = compare(lhs, rhs);
        if diffs.is_empty() {
            return;
        }
        let mut msg = format!("ELF streams not structurally equivalent ({} mismatches):\n", diffs.len());
        for (idx, diff) in diffs.iter().enumerate().take(10) {
            msg.push_str(&format!("  [{idx}] {diff:?}\n"));
        }
        if diffs.len() > 10 {
            msg.push_str(&format!("  ... {} more\n", diffs.len() - 10));
        }
        panic!("{msg}");
    }

    fn compare_file_header(
        lhs: &ElfFile64<Endianness>,
        rhs: &ElfFile64<Endianness>,
    ) -> Vec<Mismatch> {
        let mut diffs = Vec::new();
        let lh = lhs.elf_header();
        let rh = rhs.elf_header();
        let le = lhs.endian();
        let re = rhs.endian();

        macro_rules! check {
            ($field:expr, $lvalue:expr, $rvalue:expr) => {
                if $lvalue != $rvalue {
                    diffs.push(Mismatch::FileHeader {
                        field: $field,
                        lhs: $lvalue as u64,
                        rhs: $rvalue as u64,
                    });
                }
            };
        }

        check!("e_ident.class", lh.e_ident.class, rh.e_ident.class);
        check!("e_ident.data", lh.e_ident.data, rh.e_ident.data);
        check!("e_ident.os_abi", lh.e_ident.os_abi, rh.e_ident.os_abi);
        check!("e_ident.abi_version", lh.e_ident.abi_version, rh.e_ident.abi_version);
        check!("e_type", lh.e_type.get(le), rh.e_type.get(re));
        check!("e_machine", lh.e_machine.get(le), rh.e_machine.get(re));
        check!("e_entry", lh.e_entry.get(le), rh.e_entry.get(re));
        check!("e_flags", lh.e_flags.get(le), rh.e_flags.get(re));
        diffs
    }

    fn compare_program_headers(
        lhs: &ElfFile64<Endianness>,
        rhs: &ElfFile64<Endianness>,
    ) -> Vec<Mismatch> {
        let mut diffs = Vec::new();
        let lp = lhs.elf_program_headers();
        let rp = rhs.elf_program_headers();
        if lp.len() != rp.len() {
            diffs.push(Mismatch::ProgramHeaderCount {
                lhs: lp.len(),
                rhs: rp.len(),
            });
            return diffs;
        }
        let le = lhs.endian();
        let re = rhs.endian();
        for (index, (l, r)) in lp.iter().zip(rp.iter()).enumerate() {
            macro_rules! ph_check {
                ($field:expr, $lvalue:expr, $rvalue:expr) => {
                    if $lvalue != $rvalue {
                        diffs.push(Mismatch::ProgramHeader {
                            index,
                            field: $field,
                            lhs: $lvalue,
                            rhs: $rvalue,
                        });
                    }
                };
            }
            ph_check!("p_type", l.p_type(le) as u64, r.p_type(re) as u64);
            ph_check!("p_flags", l.p_flags(le) as u64, r.p_flags(re) as u64);
            // p_offset deliberately excluded: file layout may shift.
            ph_check!("p_vaddr", l.p_vaddr(le), r.p_vaddr(re));
            ph_check!("p_filesz", l.p_filesz(le), r.p_filesz(re));
            ph_check!("p_memsz", l.p_memsz(le), r.p_memsz(re));
            ph_check!("p_align", l.p_align(le), r.p_align(re));
        }
        diffs
    }

    fn compare_allocated_sections(
        lhs: &ElfFile64<Endianness>,
        rhs: &ElfFile64<Endianness>,
    ) -> Vec<Mismatch> {
        let mut diffs = Vec::new();
        let lhs_table = collect_allocated_sections(lhs);
        let rhs_table = collect_allocated_sections(rhs);

        for (name, (size, content)) in &lhs_table {
            match rhs_table.get(name) {
                None => diffs.push(Mismatch::AllocatedSectionMissing {
                    name: name.clone(),
                    side: "rhs",
                }),
                Some((rsize, rcontent)) => {
                    if size != rsize {
                        diffs.push(Mismatch::AllocatedSectionSize {
                            name: name.clone(),
                            lhs: *size,
                            rhs: *rsize,
                        });
                        continue;
                    }
                    if let Some(diff_at) = first_byte_diff(content, rcontent) {
                        diffs.push(Mismatch::AllocatedSectionContent {
                            name: name.clone(),
                            first_diff_byte: diff_at,
                        });
                    }
                }
            }
        }
        for name in rhs_table.keys() {
            if !lhs_table.contains_key(name) {
                diffs.push(Mismatch::AllocatedSectionMissing {
                    name: name.clone(),
                    side: "lhs",
                });
            }
        }
        diffs
    }

    fn collect_allocated_sections(
        file: &ElfFile64<Endianness>,
    ) -> BTreeMap<String, (u64, Vec<u8>)> {
        let mut map = BTreeMap::new();
        let endian = file.endian();
        let table = file.elf_section_table();
        for header in table.iter().skip(1) {
            // SHF_ALLOC distinguishes "loaded into memory" sections from
            // metadata (.symtab, .shstrtab, .debug_*). Only the
            // allocated set affects runtime behaviour, so that's all
            // we compare.
            if header.sh_flags(endian) & u64::from(elf::SHF_ALLOC) == 0 {
                continue;
            }
            let Ok(name_bytes) = table.section_name(endian, header) else {
                continue;
            };
            let Ok(name) = std::str::from_utf8(name_bytes) else {
                continue;
            };
            // Skip the section-name table itself; we don't compare
            // metadata sections.
            if name == ".shstrtab" {
                continue;
            }
            let size = header.sh_size(endian);
            let content = header.data(endian, file.data()).map(|d| d.to_vec()).unwrap_or_default();
            map.insert(name.to_string(), (size, content));
        }
        map
    }

    fn first_byte_diff(a: &[u8], b: &[u8]) -> Option<usize> {
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            if x != y {
                return Some(i);
            }
        }
        if a.len() != b.len() {
            return Some(a.len().min(b.len()));
        }
        None
    }

    fn compare_dynamic(
        lhs: &ElfFile64<Endianness>,
        rhs: &ElfFile64<Endianness>,
        lhs_data: &[u8],
        rhs_data: &[u8],
    ) -> Vec<Mismatch> {
        let mut diffs = Vec::new();
        let lhs_dyn = collect_dynamic(lhs, lhs_data);
        let rhs_dyn = collect_dynamic(rhs, rhs_data);
        for (tag, lvals) in &lhs_dyn {
            match rhs_dyn.get(tag) {
                None => diffs.push(Mismatch::DynamicTagMissing { tag: *tag, side: "rhs" }),
                Some(rvals) => {
                    if lvals != rvals {
                        // Pick the first differing pair to report.
                        for (l, r) in lvals.iter().zip(rvals.iter()) {
                            if l != r {
                                diffs.push(Mismatch::DynamicTagValue {
                                    tag: *tag,
                                    lhs: *l,
                                    rhs: *r,
                                });
                                break;
                            }
                        }
                        if lvals.len() != rvals.len() {
                            diffs.push(Mismatch::DynamicTagValue {
                                tag: *tag,
                                lhs: lvals.len() as u64,
                                rhs: rvals.len() as u64,
                            });
                        }
                    }
                }
            }
        }
        for tag in rhs_dyn.keys() {
            if !lhs_dyn.contains_key(tag) {
                diffs.push(Mismatch::DynamicTagMissing { tag: *tag, side: "lhs" });
            }
        }
        diffs
    }

    fn collect_dynamic(
        file: &ElfFile64<Endianness>,
        data: &[u8],
    ) -> BTreeMap<u64, Vec<u64>> {
        let mut map: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        let endian = file.endian();
        let table = file.elf_section_table();
        let entries = match table.dynamic(endian, data) {
            Ok(Some((entries, _))) => entries,
            _ => return map,
        };
        for entry in entries {
            map.entry(entry.d_tag(endian))
                .or_default()
                .push(entry.d_val(endian));
        }
        map
    }

    fn compare_dynsym(
        lhs: &ElfFile64<Endianness>,
        rhs: &ElfFile64<Endianness>,
    ) -> Vec<Mismatch> {
        let mut diffs = Vec::new();
        let lhs_syms = collect_dynsym(lhs);
        let rhs_syms = collect_dynsym(rhs);
        for (name, lhs_meta) in &lhs_syms {
            match rhs_syms.get(name) {
                None => diffs.push(Mismatch::DynamicSymbolMissing {
                    name: name.clone(),
                    side: "rhs",
                }),
                Some(rhs_meta) => {
                    if lhs_meta.value != rhs_meta.value {
                        diffs.push(Mismatch::DynamicSymbolValue {
                            name: name.clone(),
                            field: "st_value",
                            lhs: lhs_meta.value,
                            rhs: rhs_meta.value,
                        });
                    }
                    if lhs_meta.size != rhs_meta.size {
                        diffs.push(Mismatch::DynamicSymbolValue {
                            name: name.clone(),
                            field: "st_size",
                            lhs: lhs_meta.size,
                            rhs: rhs_meta.size,
                        });
                    }
                }
            }
        }
        for name in rhs_syms.keys() {
            if !lhs_syms.contains_key(name) {
                diffs.push(Mismatch::DynamicSymbolMissing {
                    name: name.clone(),
                    side: "lhs",
                });
            }
        }
        diffs
    }

    struct DynsymMeta {
        value: u64,
        size: u64,
    }

    fn collect_dynsym(file: &ElfFile64<Endianness>) -> BTreeMap<String, DynsymMeta> {
        let mut map = BTreeMap::new();
        let endian = file.endian();
        let dynsym = file.elf_dynamic_symbol_table();
        let strings = dynsym.strings();
        for (idx, sym) in dynsym.symbols().iter().enumerate() {
            if idx == 0 {
                continue;
            }
            let Ok(name_bytes) = sym.name(endian, strings) else {
                continue;
            };
            let Ok(name) = std::str::from_utf8(name_bytes) else {
                continue;
            };
            // Only globally visible function/object symbols matter for
            // dynamic linking. Skip locals (the linker doesn't expose
            // them anyway, but be defensive).
            if sym.st_bind() == elf::STB_LOCAL {
                continue;
            }
            map.insert(
                name.to_string(),
                DynsymMeta {
                    value: sym.st_value(endian),
                    size: sym.st_size(endian),
                },
            );
        }
        map
    }

    fn compare_build_id(
        lhs: &ElfFile64<Endianness>,
        rhs: &ElfFile64<Endianness>,
    ) -> Vec<Mismatch> {
        let l = lhs.build_id().ok().flatten().map(|b| b.to_vec());
        let r = rhs.build_id().ok().flatten().map(|b| b.to_vec());
        if l != r {
            vec![Mismatch::BuildId { lhs: l, rhs: r }]
        } else {
            Vec::new()
        }
    }

    fn compare_interp(
        lhs: &ElfFile64<Endianness>,
        rhs: &ElfFile64<Endianness>,
    ) -> Vec<Mismatch> {
        let l = read_interp(lhs);
        let r = read_interp(rhs);
        if l != r {
            vec![Mismatch::Interp { lhs: l, rhs: r }]
        } else {
            Vec::new()
        }
    }

    fn read_interp(file: &ElfFile64<Endianness>) -> Option<String> {
        let section = file.section_by_name(".interp")?;
        let data = section.data().ok()?;
        let trimmed = data.split(|&b| b == 0).next()?;
        Some(String::from_utf8_lossy(trimmed).into_owned())
    }
}

// Regression sanity: comparing a file to itself reports zero
// mismatches. Cheap and runs without Docker, so it's *not* gated by
// `#[ignore]` — a contributor wrecking the comparator wouldn't need
// to opt in to discover it.
#[test]
fn equivalence_checker_treats_a_file_as_equivalent_to_itself() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/elf_runtime/fixtures/lib_demo/libgreet.so");
    if !path.exists() {
        // Fixture isn't built yet (the runtime image hasn't been
        // exercised on this checkout). The Stage 5 acceptance test
        // depends on it; just skip here so a fresh clone still gets
        // a green run.
        eprintln!(
            "note: skipping equivalence-checker self-test — fixture {path:?} \
             missing; run the runtime harness setup to build it",
        );
        return;
    }
    let bytes = std::fs::read(&path).expect("read fixture");
    let diffs = elf_equivalence::compare(&bytes, &bytes);
    assert!(
        diffs.is_empty(),
        "self-comparison must produce zero mismatches; got: {diffs:?}",
    );
}
