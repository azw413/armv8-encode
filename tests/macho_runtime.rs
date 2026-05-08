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
