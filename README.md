# armv8-encode

`armv8-encode` is a Rust project for machine-code analysis, encoding,
decoding, and rewriting.

The immediate target is AArch64/ARMv8, but the crate is structured so other
architectures can be added later. The long-term goal is a reliable complete
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

The current AArch64 work lives under `src/isa/aarch64`. It contains a large
opcode table that is intended to become the foundation for AArch64 matching,
disassembly, and assembly. At the moment, that table can match instruction
descriptors, but it does not yet decode all operands or provide a complete
assembler/disassembler API.

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

This is an early-stage prototype.

Currently implemented:

- top-level crate structure for `isa`, `mc`, and `rewrite`
- AArch64 namespace under `src/isa/aarch64`
- imported AArch64 opcode table and basic matching scaffold
- small placeholder AArch64 encoder/decoder for a few arithmetic forms
- placeholder machine-code and rewrite data structures

Not yet implemented:

- complete AArch64 operand decoding
- complete AArch64 instruction encoding
- deterministic alias/canonicalization policy
- validation against trusted external assemblers/disassemblers
- complete public API
- real basic-block discovery
- relocation-aware rewriting

## Development Direction

The next milestones are:

1. Make the AArch64 table matcher deterministic and expose typed match results.
2. Validate table coverage against trusted tools such as LLVM or GNU binutils.
3. Add operand decoding and encoding for representative instruction classes.
4. Build golden round-trip tests for decode, encode, and disassembly.
5. Grow the machine-code layer into a real basic-block and relocation model.
6. Implement rewrite operations on top of the machine-code layer.

Correctness matters more than surface area. The project should prefer generated
or externally validated ISA data over hand-maintained instruction semantics
where possible.

## Verification

Run:

```sh
cargo check
cargo test
```

## Test Fixtures

AArch64 comparison fixtures live under `tests/fixtures/aarch64`.

The initial fixture records a small function as:

- source assembly: `basic.s`
- encoded instruction words plus `otool` mnemonics: `basic.otool.txt`

Additional focused fixtures, such as `integer.s`, widen coverage by operand
family. Each fixture should stay small enough that a mismatch points at a
specific instruction class or operand decoder.

The test harness parses the encoded words and checks that the ISA table matcher
selects the same mnemonic as `otool`. These tests currently validate opcode
matching only; operand decoding and full formatted disassembly are later
milestones.

The AArch64 tests also track operand-kind coverage. The table exposes every
operand kind used by opcode definitions, the operand codec layer declares which
kinds are implemented, and fixture words provide coverage for those
implemented kinds. Unsupported operand kinds decode to visible placeholders
such as `<unimplemented:ImmMov>` so disassembly can continue while missing
decoders remain easy to spot.
