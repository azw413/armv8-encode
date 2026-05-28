//! ARMv7 ELF32 runtime smoke test.
//!
//! Mirrors the smallest useful slice of `tests/elf_runtime.rs`:
//! builds a tiny ARMv7 `hello` binary under qemu-user, records
//! its output, runs it through `BinaryEditor::commit_to_bytes`
//! with no edits, runs the rewritten binary, and asserts the
//! output is byte-identical.
//!
//! All tests are `#[ignore]` and require:
//!   1. Docker reachable (qemu-user via Docker Desktop or
//!      tonistiigi/binfmt).
//!   2. The `armv7-encode-runtime` image built by
//!      `tests/arm_elf32_runtime/setup.sh`.
//!
//! Run with:
//!   cargo test --test arm_elf32_runtime -- --ignored --nocapture
//!
//! Catches the "rewriter produces an ELF32 that no longer
//! loads" class of bug — the kind unit tests can't see because
//! they don't actually execute the rewritten binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use armv8_encode::container::Container;
use armv8_encode::rewrite::BinaryEditor;

fn image_tag() -> String {
    std::env::var("ARMV7_RUNTIME_IMAGE").unwrap_or_else(|_| "armv7-encode-runtime".to_string())
}

fn harness_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/arm_elf32_runtime")
}

fn require_docker() {
    let inspect = Command::new("docker")
        .args(["image", "inspect", &image_tag()])
        .output()
        .expect("docker image inspect");
    assert!(
        inspect.status.success(),
        "image `{}` not found. Build it first:\n  \
         tests/arm_elf32_runtime/setup.sh",
        image_tag(),
    );
}

fn docker_run(workdir_subpath: &str, argv: &[&str]) -> Output {
    let mount = format!("{}:/work", harness_dir().display());
    let workdir = format!("/work/{workdir_subpath}");
    let mut cmd = Command::new("docker");
    cmd.args([
        "run",
        "--rm",
        "--platform=linux/arm/v7",
        "-v",
        &mount,
        "-w",
        &workdir,
        &image_tag(),
    ])
    .args(argv);
    cmd.output().expect("docker run")
}

/// Build the hello fixture inside the runtime container.
/// Returns the host path to the resulting binary.
fn build_hello_fixture() -> PathBuf {
    let out = docker_run("fixtures", &["sh", "build.sh"]);
    assert!(
        out.status.success(),
        "fixture build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    harness_dir().join("fixtures/hello")
}

/// Run a binary under qemu-arm via Docker. Returns stdout.
/// `bin_dir_subpath` is the workdir relative to the harness
/// dir; `binary_name` is the executable in that dir.
fn run_binary(bin_dir_subpath: &str, binary_name: &str) -> String {
    let run = docker_run(
        bin_dir_subpath,
        &["timeout", "10", &format!("./{binary_name}")],
    );
    assert!(
        run.status.success(),
        "running ./{binary_name} failed (exit {}):\nstdout:\n{}\nstderr:\n{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8(run.stdout).expect("stdout utf8")
}

fn ensure_scratch_dir() -> PathBuf {
    let scratch = harness_dir().join("scratch");
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    scratch
}

/// Make `path` executable. Required after `std::fs::write`
/// (which sets mode 0644) so qemu-arm can `./hello_rewritten`.
fn chmod_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .expect("stat for chmod")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod +x");
}

const EXPECTED_STDOUT: &str = "hello arm\n";

#[test]
#[ignore = "requires Docker with linux/arm/v7 (qemu-user) and the \
            armv7-encode-runtime image; run with --ignored --nocapture"]
fn hello_round_trips_through_container_and_runs() {
    // Build the fixture (or reuse a previous build), record
    // the baseline output, then read → parse → re-emit via
    // Container::to_bytes and confirm the resulting binary
    // still runs and prints the same line.
    require_docker();
    let bin_path = build_hello_fixture();
    let baseline = run_binary("fixtures", "hello");
    assert_eq!(baseline, EXPECTED_STDOUT, "baseline output unexpected");

    let bytes = std::fs::read(&bin_path).expect("read hello");
    let container = Container::from_bytes(&bytes).expect("parse hello");
    // Round-trip through the container layer with no edits.
    let written = container.to_bytes().expect("write hello");

    let scratch = ensure_scratch_dir();
    let out_path = scratch.join("hello_identity");
    std::fs::write(&out_path, &written).expect("write identity");
    chmod_executable(&out_path);

    let after = run_binary("scratch", "hello_identity");
    assert_eq!(
        after, EXPECTED_STDOUT,
        "identity round-trip changed program behaviour"
    );
}

#[test]
#[ignore = "requires Docker with linux/arm/v7 (qemu-user) and the \
            armv7-encode-runtime image; run with --ignored --nocapture"]
fn hello_round_trips_through_binary_editor_and_runs() {
    // Same as above but goes through BinaryEditor (the higher-
    // level API). With no edits applied, commit_to_bytes must
    // produce a runnable binary.
    require_docker();
    let bin_path = build_hello_fixture();
    let baseline = run_binary("fixtures", "hello");
    assert_eq!(baseline, EXPECTED_STDOUT, "baseline output unexpected");

    let bytes = std::fs::read(&bin_path).expect("read hello");
    let container = Container::from_bytes(&bytes).expect("parse hello");
    let editor = BinaryEditor::new(&container).expect("editor");
    let written = editor.commit_to_bytes().expect("commit_to_bytes");

    let scratch = ensure_scratch_dir();
    let out_path = scratch.join("hello_editor");
    std::fs::write(&out_path, &written).expect("write");
    chmod_executable(&out_path);

    let after = run_binary("scratch", "hello_editor");
    assert_eq!(
        after, EXPECTED_STDOUT,
        "no-op BinaryEditor commit changed program behaviour"
    );
}
