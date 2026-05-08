//! End-to-end demonstration of editing a `.text` section in an
//! aarch64 ELF shared library via the [`TextEditor`] high-level
//! API.
//!
//! This example reads the runtime-harness fixture
//! `tests/elf_runtime/fixtures/lib_demo/libgreet.so`, swaps
//! `greet_double`'s `lsl w0, w8, #1` (multiply by 2) for
//! `lsl w0, w8, #2` (multiply by 4), writes the rewritten library
//! back to disk, and re-parses it to confirm the edit landed.
//!
//! Run with:
//!
//! ```sh
//! # Build the fixture first (only needed once per fixture change):
//! ./tests/elf_runtime/setup.sh
//! docker run --rm --platform=linux/arm64 \
//!     -v $PWD/tests/elf_runtime/fixtures/lib_demo:/work -w /work \
//!     armv8-encode-runtime sh build.sh
//!
//! # Then run the example:
//! cargo run --example text_edit_so
//! ```
//!
//! The example prints what it's doing at each stage so callers can
//! follow the rewrite-then-write pipeline without reading the
//! library code.
//!
//! ## What this illustrates
//!
//! - Loading an ET_DYN aarch64 library through [`Container`].
//! - Constructing a [`TextEditor`] for the `.text` section.
//! - Finding a function by name via [`TextEditor::function_address`].
//! - Inspecting the lifted instructions to find the one to edit.
//! - Building a [`RewriteInstruction`] template by hand.
//! - Installing the new instruction with
//!   [`TextEditor::replace_instruction_at`].
//! - Committing through to bytes via [`TextEditor::commit_to_bytes`].
//!
//! ## What this *doesn't* illustrate
//!
//! - Edits that change `.text`'s length. The current writer
//!   handles same-length edits and shrinks; growth requires
//!   rewrite-layer cooperation that lives in a future stage.
//! - Editing `.rodata` / `.data`. Use the
//!   [`armv8_encode::rewrite::DataSection`] API for that.
//! - Mach-O. The container layer round-trips Mach-O `.o` files
//!   today, but `.dylib` editing depends on a Mach-O ET_DYN
//!   writer that doesn't exist yet.

use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{self, Aarch64Mnemonic, DecodedOperand, Register, RegisterClass};
use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand};
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

    // ---------------------------------------------------------------
    // Step 1: parse into a Container.
    // ---------------------------------------------------------------
    let container = Container::from_bytes(&bytes).expect("parse libgreet.so");
    println!(
        "parsed: kind={:?}, {} sections, {} symbols",
        container.kind,
        container.sections.len(),
        container.symbols.len(),
    );

    // ---------------------------------------------------------------
    // Step 2: open a TextEditor over the .text section.
    // ---------------------------------------------------------------
    let mut editor = BinaryEditor::for_section(&container, ".text").expect("open editor");
    let text = editor.text.as_ref().expect("text section lifted");
    println!(
        "editor opened: base=0x{:x}, {} instructions",
        text.base_address(),
        text.instructions().len(),
    );

    // ---------------------------------------------------------------
    // Step 3: locate greet_double and find its `lsl` instruction.
    //
    // greet_double's body is small (multiply-by-2 plus prologue/
    // epilogue). The actual `lsl Wd, Wn, #1` instruction is the
    // one we want to mutate. The high-level surface is "instructions()"
    // returning the lifted decoded instructions in source order; we
    // walk from the function's entry until we hit the lsl.
    // ---------------------------------------------------------------
    let function_address = editor
        .binary
        .function_address("greet_double")
        .expect("greet_double symbol present in .text");
    println!("greet_double starts at 0x{:x}", function_address);

    let lsl_index = text
        .instructions()
        .iter()
        .position(|insn| {
            insn.address >= function_address && insn.mnemonic == Aarch64Mnemonic::Lsl
        })
        .expect("greet_double should contain an `lsl` instruction");
    let lsl_address = text.instructions()[lsl_index].address;
    let original_lsl = &text.instructions()[lsl_index];
    println!(
        "  found `{}` at 0x{:x}: operands={:?}",
        original_lsl.mnemonic.as_str(),
        lsl_address,
        original_lsl.operands,
    );

    // ---------------------------------------------------------------
    // Step 4: build a replacement instruction.
    //
    // The decoded `lsl Wd, Wn, #shift` carries three operands:
    // the destination register, the source register, and the
    // shift amount as an immediate. We construct a new shift
    // amount of 2 (multiply by 4) while preserving Wd and Wn.
    // ---------------------------------------------------------------
    let (rd, rn) = match (
        &original_lsl.operands[0],
        &original_lsl.operands[1],
    ) {
        (DecodedOperand::Register(rd), DecodedOperand::Register(rn)) => (rd.clone(), rn.clone()),
        other => panic!("unexpected lsl operand shape: {other:?}"),
    };
    // Sanity: confirm the instruction we found really is `n*2`,
    // i.e. shift==1. Otherwise the fixture has shifted and the
    // example would silently do something else.
    let original_shift = match &original_lsl.operands[2] {
        DecodedOperand::Immediate(n) => *n,
        other => panic!("unexpected lsl shift operand: {other:?}"),
    };
    assert_eq!(
        original_shift, 1,
        "fixture's greet_double should use `lsl Wd, Wn, #1`",
    );

    let new_instruction = RewriteInstruction {
        mnemonic: Aarch64Mnemonic::Lsl,
        operands: vec![
            RewriteOperand::Decoded(DecodedOperand::Register(rd.clone())),
            RewriteOperand::Decoded(DecodedOperand::Register(rn.clone())),
            RewriteOperand::Decoded(DecodedOperand::Immediate(2)),
        ],
        original_address: Some(lsl_address),
    };

    // ---------------------------------------------------------------
    // Step 5: install the new instruction.
    // ---------------------------------------------------------------
    editor
        .text
        .as_mut()
        .unwrap()
        .replace_instruction_at(lsl_address, new_instruction)
        .expect("replace_instruction_at");
    println!("installed `lsl {wd}, {wn}, #2` (n*4) at 0x{:x}", lsl_address,
             wd = format_register(&rd), wn = format_register(&rn));

    // ---------------------------------------------------------------
    // Step 6: commit through to bytes and write.
    //
    // `commit_to_bytes` runs the layout + emit + commit_to_container
    // + Container::to_bytes pipeline. The output is a runnable .so.
    //
    // Note: commit_to_bytes consumes the editor. Capture any state
    // we'll need afterwards (here, the base address) first.
    // ---------------------------------------------------------------
    let base_address = editor.text.as_ref().unwrap().base_address();
    let rewritten = editor.commit_to_bytes().expect("commit_to_bytes");
    let out_path = PathBuf::from("/tmp/libgreet_text_edit_example.so");
    std::fs::write(&out_path, &rewritten).expect("write rewritten .so");
    println!(
        "wrote {} bytes to {} (was {} bytes)",
        rewritten.len(),
        out_path.display(),
        bytes.len(),
    );

    // ---------------------------------------------------------------
    // Step 7: re-parse and confirm the edit landed at the expected
    // file offset. Run-time verification (loading via the dynamic
    // linker) lives in the runtime harness; here we just sanity-
    // check the byte stream.
    // ---------------------------------------------------------------
    let reparsed = Container::from_bytes(&rewritten).expect("reparse");
    let reparsed_text = reparsed
        .sections
        .iter()
        .find(|s| s.name == ".text")
        .expect("re-parsed .text present");
    let lsl_offset_in_section = (lsl_address - base_address) as usize;
    let new_word = u32::from_le_bytes(
        reparsed_text.bytes[lsl_offset_in_section..lsl_offset_in_section + 4]
            .try_into()
            .unwrap(),
    );
    let decoded = aarch64::decode_instruction(lsl_address, new_word).unwrap();
    println!(
        "re-parsed `{}` at 0x{:x}: word={:#x}, operands={:?}",
        decoded.mnemonic.as_str(),
        lsl_address,
        new_word,
        decoded.operands,
    );
    assert_eq!(decoded.mnemonic, Aarch64Mnemonic::Lsl);
    assert!(
        matches!(decoded.operands[2], DecodedOperand::Immediate(2)),
        "re-parsed shift should be 2; got {:?}",
        decoded.operands[2],
    );

    println!();
    println!("Edit landed cleanly. To run the rewritten library through");
    println!("the dynamic linker (and verify greet_double now returns");
    println!("n*4 instead of n*2), run:");
    println!();
    println!("  cargo test --test elf_runtime \\");
    println!("    et_dyn_inplace_text_edit_changes_observable_output \\");
    println!("    -- --ignored --nocapture");

    ExitCode::SUCCESS
}

/// Pretty-print a register the way the disassembler would. Reuses
/// the same conventions: `w0..w30/wzr/wsp` for 32-bit, `x0..x30/xzr/sp`
/// for 64-bit. Used here only for the example's stdout output.
fn format_register(r: &Register) -> String {
    let prefix = match r.class {
        RegisterClass::W | RegisterClass::WOrSp => 'w',
        RegisterClass::X | RegisterClass::XOrSp => 'x',
        // Other classes (S/D/Q/B/H, vector) don't appear in lsl
        // operands; fall through to a debug repr.
        _ => return format!("{:?}", r),
    };
    match (r.class, r.index) {
        (RegisterClass::W | RegisterClass::X, 31) => format!("{prefix}zr"),
        (RegisterClass::WOrSp | RegisterClass::XOrSp, 31) => {
            if prefix == 'w' { "wsp".into() } else { "sp".into() }
        }
        _ => format!("{prefix}{}", r.index),
    }
}
