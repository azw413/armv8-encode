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

    let run = docker_run("scratch", &[&format!("./{binary_name}")]);
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

    // Quick sanity diff: the rewritten file must still parse back to the
    // same neutral container structure (sections, symbols, relocations).
    // This is what the lib unit tests already cover, but having the same
    // assertion here means a runtime failure can be triaged quickly:
    // if structural round-trip is broken, fix that first.
    let reparsed = Container::from_bytes(&written).expect("reparse");
    assert_eq!(
        container.sections.iter().map(|s| (&s.name, s.size)).collect::<Vec<_>>(),
        reparsed.sections.iter().map(|s| (&s.name, s.size)).collect::<Vec<_>>(),
        "section list changed during round-trip",
    );

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
