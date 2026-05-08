//! Demonstrates the universal-resolver power of the dlsym
//! anchor: this example calls `printf` from appended code, even
//! though `printf` is *not* in libgreet.so's `.dynsym` (only
//! `dlsym` is). With one PLT anchor we can reach any symbol the
//! dynamic linker can find — `printf`, `strlen`, `getenv`,
//! anything libc exports.
//!
//! What this example does:
//!
//! 1. Append three string blobs: the symbol name `"printf\0"`,
//!    a format string `"appended printf says n=%d\n"`, plus a
//!    formatted-output destination if we wanted one (we don't —
//!    the format string takes `n` directly).
//! 2. Append a function `greet_printf_double(n)` that calls
//!    `dlsym(RTLD_DEFAULT, "printf")` and then calls the
//!    resolved pointer with `(format, n)`.
//! 3. Patch `greet_double` to tail-call the new function.
//!
//! When the host runs `funcs[0](21)`:
//!
//!   appended printf says n=21
//!   double=42 offset=107
//!
//! The `appended printf says n=21` line proves the appended code
//! reached and called `printf`, despite libgreet.so never having
//! imported it directly.
//!
//! ## Why this works
//!
//! `dlsym(RTLD_DEFAULT, "printf")` walks the global symbol
//! resolution scope — every shared library the process has
//! loaded. Since the host program links libc (via crt1), `printf`
//! is reachable from the scope. dlsym returns its address and
//! the appended code calls through it.
//!
//! Build the lib_demo fixture first; then run with
//! `cargo run --example call_printf_via_dlsym`.

use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{self, Aarch64Mnemonic, DecodedOperand};
use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand, Target};
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
    let container = Container::from_bytes(&bytes).expect("parse libgreet.so");

    // Confirm libgreet.so does NOT import printf — that's the
    // whole point of this demo.
    let printf_already_present = container.symbols.iter().any(|s| {
        s.is_undefined && s.name.split_once('@').map(|(b, _)| b).unwrap_or(s.name.as_str()) == "printf"
    });
    if printf_already_present {
        eprintln!(
            "warning: libgreet.so already imports printf — the dlsym \
             trick still works, but the demo's narrative ('we can call \
             printf without importing it') doesn't apply"
        );
    } else {
        println!("confirmed: libgreet.so does NOT import printf");
    }

    let mut editor = BinaryEditor::for_section(&container, ".text").expect("open editor");
    let dlsym_id = editor
        .binary
        .symbol_by_name("dlsym@GLIBC_2.34")
        .or_else(|_| editor.binary.symbol_by_name("dlsym@GLIBC_2.17"))
        .or_else(|_| editor.binary.symbol_by_name("dlsym"))
        .expect("libgreet.so should import dlsym (see _greet_unused_dlsym_anchor)");

    let printf_name_id = editor
        .binary
        .add_data("greet_printf_name", b"printf\0", 1)
        .expect("add_data printf_name");
    let format_id = editor
        .binary
        .add_data(
            "greet_printf_format",
            b"appended printf says n=%d\n\0",
            1,
        )
        .expect("add_data format");

    // Helpers identical to the puts-via-dlsym example.
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
    let symbolic_bl = |word: u32, target: Target| {
        let mut t = template(word);
        *t.operands
            .iter_mut()
            .find(|op| matches!(op, RewriteOperand::Decoded(DecodedOperand::BranchTarget(_))))
            .unwrap() = RewriteOperand::Branch(target);
        t
    };

    // Body:
    //   stp x29, x30, [sp, #-32]!
    //   str x19, [sp, #16]
    //   mov x29, sp
    //   mov x19, x0          ; spill n
    //   mov x0, xzr          ; RTLD_DEFAULT
    //   adrp x1, "printf"
    //   add  x1, x1, #lo12("printf")
    //   bl   dlsym
    //   mov  x16, x0         ; resolved printf
    //   adrp x0, format
    //   add  x0, x0, #lo12(format)
    //   mov  w1, w19         ; arg = n
    //   blr  x16             ; printf(format, n)
    //   mov  w0, w19         ; restore n
    //   lsl  w0, w0, #1      ; n * 2
    //   ldr  x19, [sp, #16]
    //   ldp  x29, x30, [sp], #32
    //   ret
    let body = vec![
        template(0xa9be7bfd),    // stp x29, x30, [sp, #-32]!
        template(0xf9000bf3),    // str x19, [sp, #16]
        template(0x910003fd),    // mov x29, sp
        template(0xaa0003f3),    // mov x19, x0
        template(0xaa1f03e0),    // mov x0, xzr
        symbolic_adrp(0x90000001, Target::Symbol(printf_name_id)), // adrp x1, &"printf"
        template(0x91000021),    // add  x1, x1, #0
        symbolic_bl(0x94000000, Target::Symbol(dlsym_id)),         // bl dlsym
        template(0xaa0003f0),    // mov x16, x0
        symbolic_adrp(0x90000000, Target::Symbol(format_id)),      // adrp x0, &format
        template(0x91000000),    // add  x0, x0, #0
        // mov w1, w19  — printf's second arg = n. Encoded as
        // `orr w1, wzr, w19`.
        template(0x2a1303e1),    // mov w1, w19
        template(0xd63f0200),    // blr x16
        template(0x2a1303e0),    // mov w0, w19
        template(0x531f7800),    // lsl w0, w0, #1
        template(0xf9400bf3),    // ldr x19, [sp, #16]
        template(0xa8c27bfd),    // ldp x29, x30, [sp], #32
        template(0xd65f03c0),    // ret
    ];
    let log_id = editor
        .binary
        .add_function("greet_printf_double", body)
        .expect("add_function");

    let greet_double_addr = editor
        .binary
        .function_address("greet_double")
        .expect("greet_double symbol");
    editor
        .text
        .as_mut()
        .unwrap()
        .replace_instruction_at(
            greet_double_addr,
            RewriteInstruction {
                mnemonic: Aarch64Mnemonic::B,
                operands: vec![RewriteOperand::Branch(Target::Symbol(log_id))],
                original_address: Some(greet_double_addr),
            },
        )
        .expect("replace_instruction_at");

    let rewritten = editor.commit_to_bytes().expect("commit_to_bytes");
    let out_path = PathBuf::from("/tmp/libgreet_with_printf.so");
    std::fs::write(&out_path, &rewritten).expect("write");
    println!("wrote {} bytes to {}", rewritten.len(), out_path.display());

    println!();
    println!("Run the host against the rewritten library:");
    println!("  cp /tmp/libgreet_with_printf.so \\");
    println!("    tests/elf_runtime/fixtures/lib_demo/libgreet.so");
    println!("  docker run --rm --platform=linux/arm64 \\");
    println!("    -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\");
    println!("    -w /work armv8-encode-runtime ./host");
    println!("  # expected:");
    println!("  #   appended printf says n=21");
    println!("  #   double=42 offset=107");

    ExitCode::SUCCESS
}
