//! Append a function that runs at library load time, before
//! any code in the host can reach into libgreet.so.
//!
//! Stage-A `add_initialiser` semantics:
//!
//! - The library must already have a non-empty `.init_array`.
//!   libgreet.c registers one with
//!   `__attribute__((constructor)) greet_ctor`.
//! - `add_initialiser` redirects the *last* `.init_array` slot
//!   to a freshly-appended wrapper. The wrapper preserves the
//!   loader's `(argc, argv, envp)` arguments across the
//!   user-supplied body and then tail-calls the *original*
//!   constructor that the slot was pointing at.
//! - Net effect: appended code runs first, original ctor runs
//!   second, every other ctor (CRT helpers like `frame_dummy`)
//!   runs as before.
//!
//! In libgreet, the original `greet_ctor` does
//! `greet_ctor_marker |= 0x1`. This example appends an
//! initialiser that writes `0x10` to the same marker, so after
//! both run the marker reads `0x11 = 17`. Run host with
//! `./host ctor` to inspect:
//!
//!   ctor_marker=17
//!
//! Without the rewrite (or with a broken chain-back) the value
//! would be 1 (only original) or 16 (only appended).
//!
//! Build the lib_demo fixture first; then run with
//! `cargo run --example add_initialiser`.

use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{self, DecodedOperand};
use armv8_encode::rewrite::{
    BinaryEditor, InitialiserPosition, RewriteInstruction, RewriteOperand, Target,
};
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

    let marker_id = editor
        .binary
        .symbol_by_name("greet_ctor_marker")
        .expect("greet_ctor_marker should be defined in libgreet.so");
    println!(
        "found greet_ctor_marker as SymbolId({}) at vaddr 0x{:x}",
        marker_id.0,
        container.symbol(marker_id).address,
    );

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

    // Body — leaf function, no stack frame:
    //
    //   adrp x0, &greet_ctor_marker
    //   add  x0, x0, #lo12(&greet_ctor_marker)   ; fused
    //   mov  w1, #0x10
    //   str  w1, [x0]
    //   ret
    let body = vec![
        symbolic_adrp(0x90000000, Target::Symbol(marker_id)),
        template(0x91000000),    // add x0, x0, #0   (fused with adrp)
        template(0x52800201),    // mov w1, #0x10
        template(0xb9000001),    // str w1, [x0]
        template(0xd65f03c0),    // ret
    ];

    // InitialiserPosition::First runs us before every other
    // ctor in the library (ahead of frame_dummy as well as
    // greet_ctor). Switch to ::Last to insert just before the
    // final ctor only.
    let user_body_id = editor
        .binary
        .add_initialiser(
            "greet_appended_init",
            body,
            InitialiserPosition::First,
        )
        .expect("add_initialiser");
    println!("added user body greet_appended_init__body as SymbolId({})", user_body_id.0);

    let rewritten = editor.commit_to_bytes().expect("commit_to_bytes");
    let out_path = PathBuf::from("/tmp/libgreet_with_init.so");
    std::fs::write(&out_path, &rewritten).expect("write");
    println!("wrote {} bytes to {}", rewritten.len(), out_path.display());

    println!();
    println!("Run the host against the rewritten library:");
    println!("  cp /tmp/libgreet_with_init.so \\");
    println!("    tests/elf_runtime/fixtures/lib_demo/libgreet.so");
    println!("  docker run --rm --platform=linux/arm64 \\");
    println!("    -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\");
    println!("    -w /work armv8-encode-runtime ./host ctor");
    println!("  # expected: ctor_marker=17");
    println!();
    println!("  docker run --rm --platform=linux/arm64 \\");
    println!("    -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work \\");
    println!("    -w /work armv8-encode-runtime ./host");
    println!("  # expected: double=42 offset=107  (other functionality intact)");

    ExitCode::SUCCESS
}
