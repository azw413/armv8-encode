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

/// The cross sysroot that ships the x86-64 dynamic loader + libc, so qemu-user
/// can resolve a PIE's `NEEDED` libs. The Debian/Ubuntu multiarch package lays
/// it out under `/usr/x86_64-linux-gnu`.
const SYSROOT: &str = "/usr/x86_64-linux-gnu";

/// Locate the x86-64 `ld-linux` inside [`SYSROOT`].
fn cross_ld() -> Option<std::path::PathBuf> {
    for p in [
        "/usr/x86_64-linux-gnu/lib64/ld-linux-x86-64.so.2",
        "/usr/x86_64-linux-gnu/lib/ld-linux-x86-64.so.2",
    ] {
        let p = std::path::PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Cross-compile `src` to a **PIE** x86-64 ELF at `out`. PIE is the editor's
/// well-trodden append path; running it under qemu needs the cross sysroot's
/// loader (see [`qemu_run`]). Returns false if the toolchain is missing/failed.
fn build_x86(src: &str, out: &std::path::Path) -> bool {
    if !have("x86_64-linux-gnu-gcc") {
        return false;
    }
    // Derive the source name from `out` so concurrently-running tests (each with
    // a distinct output path) never clobber each other's `.c` file.
    let stem = out.file_name().and_then(|n| n.to_str()).unwrap_or("av8_x86run");
    let s = std::env::temp_dir().join(format!("{stem}.c"));
    std::fs::write(&s, src).unwrap();
    Command::new("x86_64-linux-gnu-gcc")
        .args(["-O2", "-fPIE", "-pie", "-o"])
        .arg(out)
        .arg(&s)
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

/// Run `path` under qemu-x86_64, returning (stdout, exit code).
///
/// We invoke the dynamic loader explicitly (`qemu-x86_64 -L SYSROOT LD path`)
/// rather than running the binary directly: qemu-user's *direct-exec* ELF loader
/// mis-derives `AT_PHDR` when the program-header table has been relocated into an
/// appended segment (which the editor does when it adds a PT_LOAD), whereas the
/// real `ld.so` — the loader on an actual system — maps it correctly. Routing
/// through `ld.so` exercises the genuine load path.
fn qemu_run(path: &std::path::Path) -> (String, i32) {
    let ld = cross_ld().expect("x86-64 ld-linux in sysroot");
    let out = Command::new("qemu-x86_64")
        .args(["-L", SYSROOT])
        .arg(&ld)
        .arg(path)
        .output()
        .expect("qemu-x86_64 run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

/// Build `src` (which must define `fn_name` and a `main`), relocate `fn_name`
/// into the appended segment via the generic x86 pipeline, trampoline its entry,
/// commit, and run both the original and rewritten binaries under qemu/ld.so.
/// Returns `((native_out, native_rc), (rewritten_out, rewritten_rc))`, or `None`
/// if the toolchain/sysroot is unavailable (caller should skip).
fn relocate_fn_and_run(
    src: &str,
    fn_name: &str,
    tag: &str,
) -> Option<((String, i32), (String, i32))> {
    use armv8_encode::container::Container;
    use armv8_encode::isa::x86::{disassemble_bytes, Bitness, X86Isa};
    use armv8_encode::mc::build_cfg;
    use armv8_encode::rewrite::{plan::RewritePlan, BinaryEditor};

    if !have("qemu-x86_64") || cross_ld().is_none() {
        eprintln!("x86_run: no qemu-x86_64 / cross sysroot — skipping");
        return None;
    }
    let orig = std::env::temp_dir().join(format!("av8_{tag}_{}.orig", std::process::id()));
    if !build_x86(src, &orig) {
        eprintln!("x86_run: no x86-64 cross toolchain — skipping");
        return None;
    }
    let native = qemu_run(&orig);

    let bytes = std::fs::read(&orig).unwrap();
    let container = Container::from_bytes(&bytes).unwrap();

    // Locate the target function and its bytes.
    let sym = container
        .symbols
        .iter()
        .find(|s| s.name == fn_name && s.size > 0)
        .unwrap_or_else(|| panic!("{fn_name} symbol"));
    let (entry, size) = (sym.address, sym.size);
    let sid = container.section_for_address(entry).expect("section for fn");
    let section = container.section(sid);
    let off = (entry - section.address) as usize;
    let window = &section.bytes[off..(off + size as usize).min(section.bytes.len())];

    // Disassemble the function up to (and including) its first `ret`.
    let mut insns = Vec::new();
    for d in disassemble_bytes(entry, window, Bitness::Bits64).unwrap() {
        let is_ret = d.mnemonic() == iced_x86::Mnemonic::Ret;
        insns.push(d);
        if is_ret {
            break;
        }
    }
    assert!(matches!(insns.last().map(|d| d.mnemonic()), Some(iced_x86::Mnemonic::Ret)));

    // Lift → relocate a copy into the appended segment → trampoline the entry.
    let cfg = build_cfg(&insns);
    let plan = RewritePlan::<X86Isa>::lift_from_decoded_with_container(&cfg, &insns, &container);
    let mut editor = BinaryEditor::new(&container).unwrap();
    let copy = editor
        .binary
        .add_function_from_plan::<X86Isa>(&format!("{fn_name}_copy"), plan)
        .unwrap();
    let copy_addr = editor.binary.symbol_address(copy);
    editor.prepare_text_patch(".text").unwrap();
    editor
        .text
        .as_mut()
        .unwrap()
        .x86_mut()
        .unwrap()
        .add_trampoline(entry, copy_addr)
        .unwrap();
    let out_bytes = editor.commit_to_bytes().unwrap();

    let rw = std::env::temp_dir().join(format!("av8_{tag}_{}.rw", std::process::id()));
    std::fs::write(&rw, &out_bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&rw, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let rewritten = qemu_run(&rw);
    Some((native, rewritten))
}

#[test]
fn x86_relocate_function_and_trampoline_runs() {
    // A leaf function (no calls) + a main that prints it.
    let src = "#include <stdio.h>\n\
        __attribute__((noinline)) int compute(int x){ return x*3 + 7; }\n\
        int main(void){ printf(\"r=%d\\n\", compute(11)); return 0; }\n";
    let Some(((native_out, native_rc), (rw_out, rw_rc))) =
        relocate_fn_and_run(src, "compute", "x86tr")
    else {
        return;
    };
    assert_eq!(native_out, "r=40\n");
    assert_eq!(rw_rc, native_rc, "exit code differs");
    assert_eq!(rw_out, native_out, "output differs — relocated compute or trampoline is wrong");
}

#[test]
fn x86_relocate_function_with_plt_call_runs() {
    // `compute` makes a genuine external call (`strlen@plt`) that survives -O2
    // because `s` is opaque at compile time. When the function is relocated to
    // the appended segment, emit must re-fold its `call rel32` so it still
    // reaches the PLT stub (no dynamic relocation is needed — the stub already
    // exists). This is the realistic "reference that needs relocation-or-folding"
    // case for the x86 commit path.
    let src = "#include <stdio.h>\n#include <string.h>\n\
        __attribute__((noinline)) int compute(const char *s, int x){\n\
            return (int)strlen(s) + x*3 + 7;\n\
        }\n\
        int main(int argc, char **argv){\n\
            const char *s = (argc>1)? argv[1] : \"hello\";\n\
            printf(\"r=%d\\n\", compute(s, 11));\n\
            return 0;\n\
        }\n";
    let Some(((native_out, native_rc), (rw_out, rw_rc))) =
        relocate_fn_and_run(src, "compute", "x86plt")
    else {
        return;
    };
    assert_eq!(native_out, "r=45\n"); // strlen("hello")=5 + 33 + 7
    assert_eq!(rw_rc, native_rc, "exit code differs");
    assert_eq!(rw_out, native_out, "output differs — relocated PLT call not re-folded correctly");
}

// DOCUMENTS A KNOWN LIMITATION (ignored): lifting the WHOLE `.text`
// (`lift_text_section`) and committing it re-assembles every instruction via iced
// `BlockEncoder`, which relayouts the section and breaks RIP-relative/data
// references in a real binary (it only ever worked on a 4-byte object). The sound
// runnable path — write `.text` verbatim and byte-patch only a trampoline at a
// relocated function's entry — now exists via `prepare_text_patch` +
// `add_trampoline`, exercised by `x86_relocate_function_and_trampoline_runs`
// above. This test stays ignored to document why whole-.text re-assembly is NOT
// the commit model for x86.
#[test]
#[ignore = "whole-.text BlockEncoder commit breaks real binaries; use the byte-patch trampoline path (prepare_text_patch + add_trampoline)"]
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
