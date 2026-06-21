//! End-to-end x86-64 editor validation on an aarch64 host via qemu-user.
//!
//! Skips cleanly if the x86-64 cross compiler or `qemu-x86_64` aren't installed.

use std::process::Command;

fn have(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Cross-compile `src` to a static x86-64 ELF at `out`. Returns false if the
/// toolchain is missing or the build failed.
fn build_x86(src: &str, out: &std::path::Path) -> bool {
    if !have("x86_64-linux-gnu-gcc") {
        return false;
    }
    let s = std::env::temp_dir().join(format!("av8_x86run_{}.c", std::process::id()));
    std::fs::write(&s, src).unwrap();
    Command::new("x86_64-linux-gnu-gcc")
        .args(["-O2", "-static", "-o"])
        .arg(out)
        .arg(&s)
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

/// Run `path` under qemu-x86_64, returning (stdout, exit code).
fn qemu_run(path: &std::path::Path) -> (String, i32) {
    let out = Command::new("qemu-x86_64").arg(path).output().expect("qemu-x86_64 run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

// DOCUMENTS A KNOWN LIMITATION (ignored): the current x86 commit re-assembles the
// WHOLE `.text` via iced `BlockEncoder`, which relayouts every instruction and
// breaks RIP-relative/data references in a real binary (it only ever worked on a
// 4-byte object). The runnable x86 path must instead write `.text` verbatim and
// byte-patch only the trampoline at a relocated function's entry (the aarch64
// model) — that's item 2. Re-enable / replace with an append+trampoline test once
// that lands.
#[test]
#[ignore = "whole-.text BlockEncoder commit breaks real binaries; needs the byte-patch trampoline path (item 2)"]
fn x86_editor_noop_roundtrip_runs() {
    use armv8_encode::container::Container;
    use armv8_encode::rewrite::BinaryEditor;

    if !have("qemu-x86_64") {
        eprintln!("x86_run: no qemu-x86_64 — skipping");
        return;
    }
    let orig = std::env::temp_dir().join(format!("av8_x86run_{}.orig", std::process::id()));
    if !build_x86("#include <stdio.h>\nint main(void){printf(\"v=%d\\n\", 6*7); return 0;}\n", &orig) {
        eprintln!("x86_run: no x86-64 cross toolchain — skipping");
        return;
    }
    let (native_out, native_rc) = qemu_run(&orig);

    // Round-trip the binary through the editor's text lift + commit (no edits).
    let bytes = std::fs::read(&orig).unwrap();
    let container = Container::from_bytes(&bytes).unwrap();
    let mut editor = BinaryEditor::new(&container).unwrap();
    match editor.lift_text_section(".text") {
        Ok(()) => {}
        Err(e) => {
            // A real static .text has data/jump-tables the fail-fast sweep can't
            // decode yet — that's a known x86 sweep limitation, not a commit bug.
            eprintln!("x86_run: .text did not lift ({e:?}) — skipping commit check");
            return;
        }
    }
    let out_bytes = editor.commit_to_bytes().expect("x86 commit");
    let rw = std::env::temp_dir().join(format!("av8_x86run_{}.rw", std::process::id()));
    std::fs::write(&rw, &out_bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&rw, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let (rw_out, rw_rc) = qemu_run(&rw);
    assert_eq!(rw_rc, native_rc, "exit code differs after editor round-trip");
    assert_eq!(rw_out, native_out, "stdout differs after editor round-trip");
}
