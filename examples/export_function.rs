//! Append a new function to libgreet.so and *export* it as a
//! dynamic symbol that other code can resolve by name (via
//! `dlopen` + `dlsym`, or by linking against the rewritten
//! library).
//!
//! Promotion to a dynamic export means rebuilding the dynamic
//! linker's accelerator data structures:
//!
//! - `.dynsym` (one new entry for the export)
//! - `.dynstr` (the new symbol's name)
//! - `.gnu.version` (a new versym entry — `1` for unversioned)
//! - `.gnu.hash` (regenerated from the extended dynsym)
//!
//! And updating the `.dynamic` `DT_SYMTAB` / `DT_STRTAB` /
//! `DT_GNU_HASH` / `DT_VERSYM` tags to point at the new copies.
//! The originals stay in the file but become inert; the loader
//! follows the `.dynamic` tags by virtual address.
//!
//! This example reads `libgreet.so`, appends a new
//! `greet_quintuple(n) = n * 5` function, exports it, and
//! writes the result. After running, you can verify the export
//! with `nm -D` (which reads the section table — note the
//! original `.dynsym` section header points at the *original*
//! dynsym, so `nm -D` won't see it; this is expected and
//! orthogonal to dlopen/dlsym working correctly) or by running
//! the dlopen host.
//!
//! Run with:
//!
//! ```sh
//! ./tests/elf_runtime/setup.sh   # one-time
//! docker run --rm --platform=linux/arm64 \
//!     -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work -w /work \
//!     armv8-encode-runtime sh build.sh
//!
//! cargo run --example export_function
//!
//! # Verify dlsym can find the new export:
//! cp /tmp/libgreet_with_export.so \
//!     tests/elf_runtime/fixtures/lib_demo/libgreet.so
//! docker run --rm --platform=linux/arm64 \
//!     -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work -w /work \
//!     armv8-encode-runtime ./host_dlopen greet_quintuple 7
//! # expected: result=35
//! ```

use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{
    Aarch64Mnemonic, DecodedOperand, Register, RegisterClass, Shift, ShiftKind, ShiftedRegister,
};
use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let lib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/elf_runtime/fixtures/lib_demo/libgreet.so");
    if !lib_path.exists() {
        eprintln!(
            "error: {lib_path:?} missing — run\n  \
             docker run --rm --platform=linux/arm64 \\\n    \
                 -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\\n    \
                 -w /work armv8-encode-runtime sh build.sh"
        );
        return ExitCode::from(1);
    }

    let bytes = std::fs::read(&lib_path).expect("read libgreet.so");
    println!("read {} bytes from {}", bytes.len(), lib_path.display());

    let container = Container::from_bytes(&bytes).expect("parse libgreet.so");
    let mut editor = BinaryEditor::for_section(&container, ".text").expect("open editor");

    // ---------------------------------------------------------------
    // Build greet_quintuple(n) = n * 5.
    //
    //   lsl  w8, w0, #2     ; w8 = n * 4
    //   add  w0, w8, w0     ; w0 = n * 4 + n = n * 5
    //   ret
    // ---------------------------------------------------------------
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

    // ---------------------------------------------------------------
    // Export it. The single API call:
    //   1. Lays the function out in the appended PT_LOAD segment.
    //   2. Records the symbol for promotion to dynsym.
    //   3. At commit_to_bytes time, rebuilds dynsym/dynstr/versym/
    //      gnu_hash and points .dynamic at the new copies.
    // ---------------------------------------------------------------
    let id = editor
        .binary
        .add_function_exported("greet_quintuple", body)
        .expect("add_function_exported");
    println!(
        "registered greet_quintuple as SymbolId({}) — will appear in .dynsym after commit",
        id.0,
    );

    let rewritten = editor.commit_to_bytes().expect("commit_to_bytes");
    let out_path = PathBuf::from("/tmp/libgreet_with_export.so");
    std::fs::write(&out_path, &rewritten).expect("write");
    println!("wrote {} bytes to {}", rewritten.len(), out_path.display());

    // Re-parse to confirm the file is structurally valid.
    let _reparsed = Container::from_bytes(&rewritten).expect("re-parse");

    println!();
    println!("To verify dlsym can resolve the new export:");
    println!("  cp /tmp/libgreet_with_export.so \\");
    println!("    tests/elf_runtime/fixtures/lib_demo/libgreet.so");
    println!("  docker run --rm --platform=linux/arm64 \\");
    println!("    -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\");
    println!("    -w /work armv8-encode-runtime ./host_dlopen greet_quintuple 7");
    println!("  # expected: result=35");

    ExitCode::SUCCESS
}
