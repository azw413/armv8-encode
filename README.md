# armv8-encode

`armv8-encode` is a Rust project for machine-code analysis, decoding, encoding,
and rewriting.

The immediate target is AArch64/ARMv8, but the crate is structured so other
architectures can be added later. The long-term goal is a reliable
encoder/decoder plus a lightweight machine-code abstraction layer that can
identify basic blocks and make relocatable code modifications.

## Architecture

The crate is split into three layers.

### ISA Layer

Path: `src/isa`

The ISA layer owns architecture-specific instruction knowledge:

- raw instruction encodings
- opcode tables
- operand schemas
- operand extraction and insertion
- validation rules
- aliases and canonical instruction forms
- architecture feature/version constraints

The current AArch64 implementation lives under `src/isa/aarch64`. It uses an
imported opcode table as the matching foundation and decodes table operands
into typed Rust values such as registers, immediates, memory operands, branch
targets, vector registers, vector elements, and system operands.

### Machine-Code Layer

Path: `src/mc`

The machine-code layer is intended to be architecture-neutral. It will model
decoded machine code in terms useful for analysis and rewriting:

- instructions
- operands
- symbols
- relocations
- sections
- basic blocks
- functions

This layer should preserve enough information to re-emit code exactly when no
changes are made, and to emit correct relocation-aware code when changes are
made.

### Rewrite Layer

Path: `src/rewrite`

The rewrite layer will build on the ISA and machine-code layers to support
safe code transformation:

- control-flow graph construction
- basic-block splitting
- branch target resolution
- layout and re-encoding
- trampoline and branch-island generation
- relocation-aware patching

This layer should not need to know AArch64 encoding details directly. It should
operate on the machine-code abstraction and delegate architecture-specific
decisions to the ISA layer.

## Current Status

This is still an early-stage prototype, but the AArch64 decoder is now useful
for real comparison work.

Currently implemented:

- top-level crate structure for `isa`, `mc`, and `rewrite`
- AArch64 namespace under `src/isa/aarch64`
- table-driven AArch64 opcode matching
- typed AArch64 operand decoding for every operand kind currently referenced by
  the imported opcode table
- formatted AArch64 disassembly for the covered table/operand surface
- placeholder public encoding API
- fixture-based comparisons against Apple `otool`
- ignored real-binary comparison tests for Mach-O text sections
- placeholder machine-code and rewrite data structures

Not yet implemented:

- complete table validation against every ARMv8/AArch64 extension and reserved
  encoding
- table-driven instruction encoding
- binary container parsing for Mach-O, ELF, or PE
- symbol, relocation, stub, literal-pool, and section-aware annotation
- stable high-level disassembler API over whole object files
- real basic-block discovery
- relocation-aware rewriting

The distinction matters: the ISA layer can decode raw AArch64 instruction words
at known addresses. It does not yet parse binaries or resolve external symbols.
That belongs in a future binary/object-file layer above the ISA decoder.

## API Sketch

The public API is intentionally small while the internal model is still being
validated. The basic workflow today is:

1. Provide an instruction address and 32-bit instruction word.
2. Decode it with `aarch64::decode_instruction`.
3. Inspect the typed operands or format the instruction.

Example: find all decoded branches to a specific target address.

```rust
use armv8_encode::isa::aarch64::{self, DecodedOperand};

fn branches_to_target(
    base_address: u64,
    words: &[u32],
    target: u64,
) -> Vec<u64> {
    words
        .iter()
        .enumerate()
        .filter_map(|(index, word)| {
            let address = base_address + (index as u64 * 4);
            let instruction = aarch64::decode_instruction(address, *word)?;

            let has_target = instruction
                .operands
                .iter()
                .any(|operand| matches!(operand, DecodedOperand::BranchTarget(value) if *value == target));

            has_target.then_some(address)
        })
        .collect()
}
```

Formatted disassembly is also available on each decoded instruction:

```rust
use armv8_encode::isa::aarch64;

let address = 0x1000;
let word = 0x5400_0081; // b.ne 0x1010
let instruction = aarch64::decode_instruction(address, word).unwrap();

assert_eq!(instruction.format_mnemonic(), "b.ne");
assert_eq!(instruction.format_operands(), "0x1010");
```

Encoding has a public placeholder shape but is not implemented yet:

```rust
use armv8_encode::isa::aarch64::{self, InstructionTemplate};

let template = InstructionTemplate {
    mnemonic: "b",
    operands: Vec::new(),
};

assert!(aarch64::encode_instruction(&template).is_err());
```

## Validation

AArch64 comparison fixtures live under `tests/fixtures/aarch64`.

Each checked fixture contains:

- source assembly, such as `basic.s`
- encoded instruction words plus `otool` mnemonics and operands, such as
  `basic.otool.txt`

The unit tests parse those fixtures, decode the raw instruction words, format
the decoded instructions, and compare the result with `otool`.

The AArch64 tests also track operand-kind coverage. The current snapshot
expects all table operand kinds to have decoder implementations.

For wider local comparison against real Mach-O binaries, run the ignored
`otool` harness:

```sh
cargo test --test otool_compare -- --ignored --nocapture
ARMV8_COMPARE_BINARY=/path/to/binary cargo test --test otool_compare -- --ignored --nocapture
ARMV8_COMPARE_STRICT=1 cargo test --test otool_compare -- --ignored --nocapture
```

The real-binary comparison strips disassembler comments such as symbol-stub and
literal-pool annotations. Those annotations require Mach-O/ELF parsing and are
outside the ISA decoder's scope.

## Development Direction

The next milestones are:

1. Continue reducing real-binary `otool` mismatches that are pure ISA
   decode/formatting issues.
2. Add table-driven AArch64 encoding for the same operand model used by
   decoding.
3. Define a stable binary/object-file boundary for sections, symbols, and
   relocations.
4. Grow the machine-code layer into a real basic-block and relocation model.
5. Implement rewrite operations on top of the machine-code layer.

Correctness matters more than surface area. The project should prefer generated
or externally validated ISA data over hand-maintained instruction semantics
where possible.

## Verification

Run:

```sh
cargo check
cargo test
```
