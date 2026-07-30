//! End-to-end demonstration of *decorating* an aarch64 ELF shared
//! library: appending a new function in a fresh PT_LOAD R-X
//! segment, then redirecting an existing function to invoke it.
//!
//! Pattern:
//!
//! 1. Open the library through [`Container`] and a [`TextEditor`].
//! 2. Build the new function as a list of [`RewriteInstruction`]s.
//! 3. Register it via [`TextEditor::add_function`]; receive a
//!    [`SymbolId`].
//! 4. Patch an existing call site to point at the new function.
//! 5. Commit and write.
//!
//! The library's ABI to its callers stays exactly the same — every
//! existing exported symbol still has its original virtual address.
//! What changes is what those symbols *do*. This is the natural
//! shape for "decorate every public function with a logging
//! wrapper", "redirect a hot path to a fast variant", or "patch
//! a vulnerable function to delegate to a sandboxed replacement."
//!
//! ## What this example does
//!
//! Reads `tests/elf_runtime/fixtures/lib_demo/libgreet.so` (which
//! exports `greet_double(n) = n * 2`), appends a new function
//! `greet_quintuple(n) = n * 5`, then patches `greet_double`'s
//! first instruction to be `b greet_quintuple`. The host program
//! that previously printed `double=42` now prints `double=105`.
//!
//! Run with:
//!
//! ```sh
//! ./tests/elf_runtime/setup.sh   # one-time
//! docker run --rm --platform=linux/arm64 \
//!     -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work -w /work \
//!     armv8-encode-runtime sh build.sh
//!
//! cargo run --example decorate_so
//! ```
//!
//! Then to verify the rewritten library actually works at runtime:
//!
//! ```sh
//! cargo test --test elf_runtime \
//!     et_dyn_appended_function_changes_observable_output \
//!     -- --ignored --nocapture
//! ```
//!
//! ## What this example *doesn't* show
//!
//! - Multiple appended functions — `add_function` can be called
//!   repeatedly and each function packs into the same segment in
//!   call order, but this walkthrough sticks to one for clarity.
//! - PLT calls from the new function — calling a libc extern
//!   like `printf` from the new code needs PLT-aware emit support
//!   that doesn't yet exist. The new function here is pure
//!   arithmetic, calling no externs.
//! - PIE main executables. The `add_function` path runs against
//!   ET_DYN/ET_EXEC inputs; for `.so` libraries it's tested
//!   end-to-end. Executables work the same way structurally
//!   but haven't yet had a runtime acceptance test.

use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{
    Aarch64Mnemonic, DecodedOperand, Register, RegisterClass, Shift, ShiftKind, ShiftedRegister,
};
use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand, Target};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let lib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/elf_runtime/fixtures/lib_demo/libgreet.so");

    if !lib_path.exists() {
        eprintln!(
            "error: fixture not found at {lib_path:?}\n\n\
             Build it via the runtime harness first:\n  \
             ./tests/elf_runtime/setup.sh\n  \
             docker run --rm --platform=linux/arm64 \\\n    \
                 -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\\n    \
                 -w /work armv8-encode-runtime sh build.sh",
        );
        return ExitCode::from(1);
    }

    let bytes = std::fs::read(&lib_path).expect("read libgreet.so");
    println!("read {} bytes from {}", bytes.len(), lib_path.display());

    let container = Container::from_bytes(&bytes).expect("parse libgreet.so");
    let mut editor = BinaryEditor::for_section(&container, ".text").expect("open editor");

    // ---------------------------------------------------------------
    // Step 1: build the new function as a list of RewriteInstructions.
    //
    // greet_quintuple(n) = n * 5, computed as
    //   w8 = n << 2     (n*4)
    //   w0 = w8 + n     (n*4 + n = n*5)
    //   ret
    //
    // PC-relative operands inside the function (none here, but if
    // there were — branches, adrp, etc.) get resolved at the new
    // function's assigned virtual address by the rewriter's normal
    // emit pipeline, so they encode correctly without any caller
    // intervention.
    // ---------------------------------------------------------------
    let w0 = Register { class: RegisterClass::W, index: 0 };
    let w8 = Register { class: RegisterClass::W, index: 8 };
    let x30 = Register { class: RegisterClass::X, index: 30 };
    let new_function = vec![
        // lsl w8, w0, #2
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
        // add w0, w8, w0
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Add,
            operands: vec![
                RewriteOperand::Decoded(DecodedOperand::Register(w0.clone())),
                RewriteOperand::Decoded(DecodedOperand::Register(w8.clone())),
                RewriteOperand::Decoded(DecodedOperand::ShiftedRegister(ShiftedRegister {
                    register: w0.clone(),
                    shift: Shift { kind: ShiftKind::Lsl, amount: 0 },
                })),
            ],
            original_address: None,
            source_size: None,
        },
        // ret
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Ret,
            operands: vec![RewriteOperand::Decoded(DecodedOperand::Register(x30))],
            original_address: None,
            source_size: None,
        },
    ];

    // ---------------------------------------------------------------
    // Step 2: register the new function. add_function returns a
    // SymbolId we can use as a Target::Symbol elsewhere.
    // ---------------------------------------------------------------
    let quintuple_id = editor
        .binary
        .add_function("greet_quintuple", new_function)
        .expect("add_function greet_quintuple");
    println!(
        "added greet_quintuple as SymbolId({})",
        quintuple_id.0,
    );

    // ---------------------------------------------------------------
    // Step 3: redirect greet_double's first instruction to be a
    // tail-call (`b greet_quintuple`). Existing callers that
    // resolve `greet_double` find its original entry point, hit
    // the new branch, and land in greet_quintuple's body.
    // ---------------------------------------------------------------
    let greet_double_addr = editor
        .binary
        .function_address("greet_double")
        .expect("greet_double symbol present");
    let tail_call = RewriteInstruction {
        mnemonic: Aarch64Mnemonic::B,
        operands: vec![RewriteOperand::Branch(Target::Symbol(quintuple_id))],
        original_address: Some(greet_double_addr),
        source_size: None,
    };
    editor
        .text
        .as_mut()
        .unwrap()
        .aarch64_mut()
        .unwrap()
        .replace_instruction_at(greet_double_addr, tail_call)
        .expect("replace_instruction_at");
    println!(
        "patched greet_double[0] (0x{greet_double_addr:x}) = b greet_quintuple",
    );

    // ---------------------------------------------------------------
    // Step 4: commit and write.
    //
    // commit_to_bytes notices that we appended a function and
    // routes through the writer's append-PT_LOAD path, which
    // emits a fresh executable segment past the input's mapped
    // range and links the new function's symbol to its assigned
    // virtual address.
    // ---------------------------------------------------------------
    let rewritten = editor.commit_to_bytes().expect("commit_to_bytes");
    let out_path = PathBuf::from("/tmp/libgreet_decorated.so");
    std::fs::write(&out_path, &rewritten).expect("write rewritten .so");
    println!(
        "wrote {} bytes to {} (was {} bytes)",
        rewritten.len(),
        out_path.display(),
        bytes.len(),
    );

    // Sanity: re-parse and confirm the new section landed.
    let reparsed = Container::from_bytes(&rewritten).expect("re-parse");
    let appended = reparsed
        .sections
        .iter()
        .find(|s| s.name == ".text.armv8_encode_appended");
    match appended {
        Some(s) => println!(
            "  ✓ appended section present at vaddr 0x{:x}, size {} bytes",
            s.address, s.size,
        ),
        None => println!("  ✗ appended section missing (writer bug?)"),
    }

    println!();
    println!("Now run:");
    println!("  cp /tmp/libgreet_decorated.so \\");
    println!("    tests/elf_runtime/fixtures/lib_demo/libgreet.so");
    println!("  docker run --rm --platform=linux/arm64 \\");
    println!("    -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\");
    println!("    -w /work armv8-encode-runtime ./host");
    println!("  # expected: double=105 offset=107");

    ExitCode::SUCCESS
}
