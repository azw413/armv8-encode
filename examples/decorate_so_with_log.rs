//! End-to-end demonstration of:
//!
//!   1. Appending a *new* function to libgreet.so that calls
//!      `puts` via the existing PLT stub.
//!   2. Appending a *new* string blob alongside the function (in
//!      the same R-X segment) and computing its address from the
//!      new code via `adrp + add`.
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
//!   greet_double called via appended decorator
//!   double=42 offset=107
//!
//! The new function lives in a fresh PT_LOAD R-X segment past the
//! input's mapped range. Its body looks like a normal ABI-respecting
//! function:
//!
//!   stp  x29, x30, [sp, #-32]!  ; save fp + lr
//!   mov  x29, sp
//!   str  w0, [sp, #16]          ; spill the input n
//!   adrp x0, msg_page           ; arg = &msg
//!   add  x0, x0, #lo12(msg)
//!   bl   puts                   ; folded to the existing .plt stub
//!   ldr  w0, [sp, #16]          ; reload n
//!   lsl  w0, w0, #1             ; w0 = n * 2 (the original
//!                               ;             greet_double behaviour)
//!   ldp  x29, x30, [sp], #32   ; restore + dealloc
//!   ret
//!
//! Build the lib_demo fixture first; then run with
//! `cargo run --example decorate_so_with_log`.

use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{self, Aarch64Mnemonic};
use armv8_encode::rewrite::{RewriteInstruction, RewriteOperand, Target, TextEditor};
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
    let mut editor = TextEditor::for_section(&container, ".text").expect("open editor");

    // Sanity: confirm libgreet.so already imports puts (the
    // anchor in libgreet.c forces a .dynsym/.plt entry). If this
    // assertion fails the fixture has drifted.
    let puts_id = editor
        .symbol_by_name("puts@GLIBC_2.17")
        .or_else(|_| editor.symbol_by_name("puts"))
        .expect("libgreet.so must import puts (see _greet_unused_puts_anchor)");
    println!("found puts as SymbolId({})", puts_id.0);

    // ---------------------------------------------------------------
    // Step 1: append the message string. Returns a SymbolId we can
    // use as Target::Symbol for adrp/add address computation.
    // Trailing NUL because puts wants a C string.
    // ---------------------------------------------------------------
    let message = b"greet_double called via appended decorator\0";
    let msg_id = editor
        .add_data("greet_log_msg", message, /*align=*/ 1)
        .expect("add_data");
    println!("added greet_log_msg as SymbolId({})", msg_id.0);

    // ---------------------------------------------------------------
    // Step 2: build the new function. We construct each instruction
    // via the decoder ("decode this 32-bit word") to avoid
    // hand-rolling the operand structs. The decoded form already
    // exposes the right operand shape; we just need to substitute
    // symbolic targets for the adrp/add pair and the bl.
    // ---------------------------------------------------------------
    let template = |word: u32| {
        let decoded = aarch64::decode_instruction(0, word).expect("decode template word");
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

    // Source of the template words: emit each via clang manually
    // would be the gold standard, but for clarity we use known
    // AArch64 encodings. Every word here re-encodes to itself
    // (verified during development via the symbol_probe
    // scratchpad).

    // stp x29, x30, [sp, #-32]!
    let stp_save = template(0xa9be7bfd);
    // mov x29, sp
    let mov_x29_sp = template(0x910003fd);
    // str w0, [sp, #16]   (offset 16 → imm12=4 since size=2)
    let str_w0 = template(0xb90013e0);
    // ldr w0, [sp, #16]
    let ldr_w0 = template(0xb94013e0);
    // lsl w0, w0, #1
    let lsl_w0 = template(0x531f7800);
    // ldp x29, x30, [sp], #32
    let ldp_restore = template(0xa8c27bfd);
    // ret
    let ret_insn = template(0xd65f03c0);

    // adrp x0, &msg — the page operand is symbolic. Pair it
    // with `add x0, x0, #0` and the rewriter's macro-fusion pass
    // (run inside `add_function`) collapses the pair into a
    // LoadAddress macro that resolves at the appended function's
    // final vaddr.
    let mut adrp_msg = template(0x90000000); // adrp x0, +0
    *adrp_msg
        .operands
        .iter_mut()
        .find(|op| {
            matches!(op, RewriteOperand::Decoded(aarch64::DecodedOperand::PageTarget(_)))
        })
        .expect("adrp template missing PageTarget operand") =
        RewriteOperand::Page(Target::Symbol(msg_id));

    // add x0, x0, #0 — placeholder offset. The fusion pass takes
    // this and the adrp above as a unit; emit computes the lo12
    // from msg_id's vaddr.
    let add_msg = template(0x91000000);

    // bl puts (decoded as bl <some_target>; replace the
    // BranchTarget with Target::Symbol(puts_id) so the rewriter
    // folds the call to the existing .plt stub).
    let mut bl_puts = template(0x94000000); // bl +0 placeholder
    *bl_puts
        .operands
        .iter_mut()
        .find(|op| {
            matches!(op, RewriteOperand::Decoded(aarch64::DecodedOperand::BranchTarget(_)))
        })
        .expect("bl template missing BranchTarget operand") =
        RewriteOperand::Branch(Target::Symbol(puts_id));

    let new_function_body = vec![
        stp_save,
        mov_x29_sp,
        str_w0,
        adrp_msg,
        add_msg,
        bl_puts,
        ldr_w0,
        lsl_w0,
        ldp_restore,
        ret_insn,
    ];

    let log_id = editor
        .add_function("greet_log_double", new_function_body)
        .expect("add_function greet_log_double");
    println!("added greet_log_double as SymbolId({})", log_id.0);

    // ---------------------------------------------------------------
    // Step 3: patch greet_double's first instruction to be `b
    // greet_log_double`. The new function does the print *and*
    // computes n*2, so it's a tail-replacement.
    // ---------------------------------------------------------------
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
    println!(
        "patched greet_double[0] (0x{greet_double_addr:x}) -> b greet_log_double",
    );

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
    println!("  #   greet_double called via appended decorator");
    println!("  #   double=42 offset=107");

    ExitCode::SUCCESS
}

