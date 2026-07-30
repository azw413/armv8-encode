//! End-to-end demonstration of:
//!
//!   1. Appending a *new* function to libgreet.so that resolves
//!      `puts` at runtime via `dlsym(RTLD_DEFAULT, "puts")` and
//!      calls it.
//!   2. Appending two *new* string blobs alongside the function
//!      (the symbol name to look up, plus the message to print)
//!      and computing their addresses from the new code via
//!      `adrp + add`.
//!   3. Patching `greet_double` to delegate to the new function,
//!      so each call to greet_double prints a line *and* returns
//!      its usual `n * 2`.
//!
//! Concretely: the host program previously printed
//!
//!   double=42 offset=107
//!
//! After this example runs, the rewritten library prints
//!
//!   greet_double called via appended decorator (dlsym)
//!   double=42 offset=107
//!
//! ## Why dlsym?
//!
//! Earlier iterations of this example called `puts` *directly*
//! through the input library's PLT, which required `puts` itself
//! to be anchored as an extern in `libgreet.c`. That worked but
//! it didn't generalise — every extern we might want to call had
//! to be anchored ahead of time.
//!
//! `libgreet.c` now anchors only one extern: `dlsym`. Appended
//! code calls `dlsym(RTLD_DEFAULT, "name")` to resolve any
//! function the process has loaded, then calls through the
//! returned pointer. One anchor subsumes any number of future
//! externs — `puts`, `printf`, `getenv`, anything libc exports.
//!
//! ## What the new function does (AArch64 pseudo-assembly)
//!
//!   stp  x29, x30, [sp, #-32]!  ; save fp + lr
//!   str  x19, [sp, #16]         ; save callee-saved x19
//!   mov  x29, sp
//!   mov  x19, x0                ; spill the input n into x19
//!   mov  x0, xzr                ; arg 1 = RTLD_DEFAULT (= 0)
//!   adrp x1, puts_name          ; arg 2 = "puts"
//!   add  x1, x1, #lo12(puts_name)
//!   bl   dlsym                  ; → folded to the existing PLT stub
//!   mov  x16, x0                ; resolved puts function pointer
//!   adrp x0, msg                ; arg = &msg
//!   add  x0, x0, #lo12(msg)
//!   blr  x16                    ; call the resolved puts
//!   mov  w0, w19                ; restore n
//!   lsl  w0, w0, #1             ; w0 = n * 2
//!   ldr  x19, [sp, #16]
//!   ldp  x29, x30, [sp], #32
//!   ret
//!
//! Build the lib_demo fixture first; then run with
//! `cargo run --example decorate_so_with_log`.

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
    let mut editor = BinaryEditor::for_section(&container, ".text").expect("open editor");

    // The one anchor we rely on. libgreet.c forces a PLT entry
    // for dlsym via `_greet_unused_dlsym_anchor`; from there our
    // appended code can resolve any symbol visible to the
    // process at runtime.
    let dlsym_id = editor
        .binary
        .symbol_by_name("dlsym@GLIBC_2.34")
        .or_else(|_| editor.binary.symbol_by_name("dlsym@GLIBC_2.17"))
        .or_else(|_| editor.binary.symbol_by_name("dlsym"))
        .expect("libgreet.so should import dlsym (see _greet_unused_dlsym_anchor)");
    println!("found dlsym anchor as SymbolId({})", dlsym_id.0);

    // The two strings the new function references: the symbol
    // name to resolve, plus the message to print.
    let puts_name_id = editor
        .binary
        .add_data("greet_log_putsname", b"puts\0", /*align=*/ 1)
        .expect("add_data puts_name");
    let msg_id = editor
        .binary
        .add_data(
            "greet_log_msg",
            b"greet_double called via appended decorator (dlsym)\0",
            /*align=*/ 1,
        )
        .expect("add_data msg");
    println!(
        "appended strings: putsname=SymbolId({}), msg=SymbolId({})",
        puts_name_id.0, msg_id.0,
    );

    // ---------------------------------------------------------------
    // Build the new function. Each instruction is constructed by
    // decoding a known-good AArch64 word, then (where the
    // instruction references a symbol) substituting a symbolic
    // operand. The rewriter's macro-fusion pass then folds
    // adrp+add pairs against `Target::Symbol` into a
    // `LoadAddress` macro that resolves at the appended
    // function's final virtual address.
    // ---------------------------------------------------------------
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
    let symbolic_bl = |word: u32, target: Target| {
        let mut t = template(word);
        *t.operands
            .iter_mut()
            .find(|op| matches!(op, RewriteOperand::Decoded(DecodedOperand::BranchTarget(_))))
            .unwrap() = RewriteOperand::Branch(target);
        t
    };

    let body = vec![
        template(0xa9be7bfd),    // stp x29, x30, [sp, #-32]!
        template(0xf9000bf3),    // str x19, [sp, #16]
        template(0x910003fd),    // mov x29, sp
        template(0xaa0003f3),    // mov x19, x0   — spill n
        template(0xaa1f03e0),    // mov x0, xzr   — RTLD_DEFAULT
        symbolic_adrp(0x90000001, Target::Symbol(puts_name_id)), // adrp x1, &"puts"
        template(0x91000021),    // add  x1, x1, #lo12  (fused)
        symbolic_bl(0x94000000, Target::Symbol(dlsym_id)),       // bl dlsym
        template(0xaa0003f0),    // mov x16, x0   — resolved puts ptr
        symbolic_adrp(0x90000000, Target::Symbol(msg_id)),       // adrp x0, &msg
        template(0x91000000),    // add  x0, x0, #lo12  (fused)
        template(0xd63f0200),    // blr x16        — call puts(msg)
        template(0x2a1303e0),    // mov w0, w19    — restore n
        template(0x531f7800),    // lsl w0, w0, #1 — n * 2
        template(0xf9400bf3),    // ldr x19, [sp, #16]
        template(0xa8c27bfd),    // ldp x29, x30, [sp], #32
        template(0xd65f03c0),    // ret
    ];
    let log_id = editor
        .binary
        .add_function("greet_log_double", body)
        .expect("add_function");
    println!("added greet_log_double as SymbolId({})", log_id.0);

    // Patch greet_double to tail-call greet_log_double.
    let greet_double_addr = editor
        .binary
        .function_address("greet_double")
        .expect("greet_double symbol");
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
                operands: vec![RewriteOperand::Branch(Target::Symbol(log_id))],
                original_address: Some(greet_double_addr),
                source_size: None,
            },
        )
        .expect("replace_instruction_at");
    println!("patched greet_double[0] (0x{greet_double_addr:x}) -> b greet_log_double");

    let rewritten = editor.commit_to_bytes().expect("commit_to_bytes");
    let out_path = PathBuf::from("/tmp/libgreet_with_log.so");
    std::fs::write(&out_path, &rewritten).expect("write");
    println!("wrote {} bytes to {}", rewritten.len(), out_path.display());

    println!();
    println!("Run the host against the rewritten library:");
    println!("  cp /tmp/libgreet_with_log.so \\");
    println!("    tests/elf_runtime/fixtures/lib_demo/libgreet.so");
    println!("  docker run --rm --platform=linux/arm64 \\");
    println!("    -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\");
    println!("    -w /work armv8-encode-runtime ./host");
    println!("  # expected:");
    println!("  #   greet_double called via appended decorator (dlsym)");
    println!("  #   double=42 offset=107");

    ExitCode::SUCCESS
}
