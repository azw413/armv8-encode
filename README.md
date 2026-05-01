# armv8-encode

`armv8-encode` is a Rust project for machine-code analysis, decoding, encoding,
and rewriting.

The immediate target is AArch64/ARMv8, but the crate is structured so other
architectures can be added later. The long-term goal is a reliable
encoder/decoder plus a lightweight machine-code abstraction layer that can
identify basic blocks, build a control-flow graph, and emit relocation-aware
code modifications.

## Architecture

The crate is split into four layers, bottom-up: container → ISA → mc →
rewrite. Each layer only knows about the ones below it.

### Container Layer

Path: `src/container`

Reads Mach-O and ELF object files into a neutral, format-agnostic model:
sections, symbols, relocations, optional DWARF debug info, and `Function`
views derived from both. The `object` crate handles format parsing and
`gimli` handles DWARF; the container layer hides both so the rest of the
crate sees one shape regardless of source.

The layer's input is `&[u8]` (callers handle file I/O / mmap themselves);
the output is a [`Container`](src/container/types.rs) ready to feed into
disassembly. AArch64-relevant relocations (`Branch26`, `Branch19`,
`Branch14`, `AdrpPage21`, `PageOffset12`, `Absolute`) are mapped onto a
neutral enum; unrecognized relocation types pass through as
`RelocationKind::Other(raw_code)` so callers can still see them.

When a binary carries `.debug_info` / `__debug_info`, the container also
exposes a `DwarfInfo` with one `DwarfFunction` per `DW_TAG_subprogram`
(name, address range, optional source line). `Container::functions()`
merges symbol-derived and DWARF-derived entries — symbols take precedence
when both exist, DWARF fills in the gap when the binary is stripped.

The container is also writable: `Container::to_bytes()` produces a fresh
Mach-O / ELF byte stream from the in-memory model, and
`Container::with_section_bytes()` lets the rewriter splice new text bytes
back in before writing. Round-trip is "compatible," not byte-identical:
sections, symbols, and relocations are preserved structurally, but
format-specific header details are reconstructed by `object::write` from
scratch.

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
imported opcode table as the matching foundation, decodes table operands into
typed Rust values (registers, immediates, memory operands, branch targets,
vector registers, vector elements, system operands), implements
[`InstructionInfo`](src/mc/control_flow.rs) for control-flow classification,
and exposes table-driven encoding alongside helpers used by the rewrite
layer (`pcrel_range_bytes`, `invert_conditional_branch`).

### Machine-Code Layer

Path: `src/mc`

The machine-code layer is architecture-neutral. It models decoded code in
terms useful for analysis and rewriting:

- the [`InstructionInfo`](src/mc/control_flow.rs) trait that ISA crates
  implement so analysis stays generic
- the [`ControlFlow`](src/mc/control_flow.rs) classification (`Fall`,
  `Jump`, `ConditionalJump`, `Call`, `Return`, `IndirectJump`,
  `IndirectCall`, `Trap`)
- basic blocks and the [`ControlFlowGraph`](src/mc/cfg.rs) built by
  `mc::build_cfg`
- (planned) symbols, relocations, sections, functions

This layer preserves enough information to re-emit code exactly when no
changes are made and to emit correct relocation-aware code when changes are
made.

### Rewrite Layer

Path: `src/rewrite`

The rewrite layer turns a decoded code region into an editable IR whose
PC-relative operands are *symbolic*. Instead of carrying a hard-coded
address, each branch target carries a [`Target`](src/rewrite/ir.rs) — a
reference to a basic block, an extern symbol, a constant pool entry, or a
literal address. This is what lets the layout pass move things around freely
without invalidating displacements.

The pipeline is:

```text
  bytes ──► sweep ──► instructions ──► CFG
                                        │
                                        ▼
                                RewritePlan::lift
                                        │
                                        ▼  edit operations
                                (mutate operands, blocks, terminators)
                                        │
                                        ▼
                                lay_out(plan, base)
                                        │
                                        ▼
                                emit(plan, layout) ──► bytes
```

Layout iterates to a fixed point: if an edit pushes a conditional branch
past its `pcrel19` (`b.cond`, `cbz`, `cbnz`) or `pcrel14` (`tbz`, `tbnz`)
range, it widens the branch into `<inverted_cond> .Lskip ; b far_target ;
.Lskip:`, which can in turn push other branches out of range — repeat until
stable.

Operations supported today:

- `RewritePlan::lift(cfg, instructions)` — convert decoded instructions to
  symbolic IR.
- `RewritePlan::lift_with_container(cfg, instructions, container)` — same,
  but cross-function call/branch targets that match a container symbol
  become `Target::Symbol` instead of `Target::Absolute`, so they survive
  layout no matter where the symbol is.
- `redirect_branch(address, new_target)` — change the destination of a
  branch.
- `replace_terminator(block, new_instruction)` — swap a block's exit (e.g.
  `b.eq` ↔ `b.ne`).
- `insert_after_address(address, new_instructions)` — splice instructions
  into a block.
- `remove_at_address(address)` — drop an instruction.
- `lay_out(&plan, base, container) → Layout` — assign final addresses,
  widen out-of-range conditionals, resolve `Target::Symbol` against the
  container when supplied.
- `emit(&plan, &layout, container) → Vec<u8>` — produce final bytes.

## Current Status

The decoder is useful for real comparison work, the encoder covers the
operand kinds referenced by the imported table, and the analysis +
rewriting pipeline is end-to-end testable on synthetic streams.

Currently implemented:

- top-level crate structure for `container`, `isa`, `mc`, and `rewrite`
- read and write Mach-O and ELF AArch64 object files (sections, symbols,
  relocations); read → write → re-read round-trip preserves structural
  content
- DWARF lifting via `gimli`: function boundaries from
  `DW_TAG_subprogram`, source-line attributes when present, merged with
  symbol-derived functions in `Container::functions()`
- AArch64 namespace under `src/isa/aarch64`
- table-driven AArch64 opcode matching and decoding
- typed AArch64 operand decoding for every operand kind currently referenced
  by the imported opcode table
- formatted AArch64 disassembly for the covered table/operand surface
- table-driven AArch64 instruction encoding
- linear-sweep disassembler (`aarch64::disassemble_bytes`)
- architecture-neutral control-flow classification (`mc::ControlFlow`)
- basic-block discovery and CFG construction (`mc::build_cfg`)
- symbolic rewrite IR with lift, edit, layout, and emit passes
- container-aware lift: cross-function call targets that match a container
  symbol become `Target::Symbol` and resolve at layout time
- conditional-branch widening at layout time (fixed-point iteration)
- fixture-based comparisons against Apple `otool`
- ignored real-binary comparison tests for Mach-O text sections

Not yet implemented:

- DWARF line tables (file/line lookup for arbitrary addresses) and
  inlined-callsite metadata — only `DW_TAG_subprogram` is lifted today
- emission of new relocation records when the rewriter edits a
  `Target::Symbol(undefined)` reference (currently `lay_out` errors;
  needs a follow-up to thread emitted relocations through)
- PE/COFF container support
- stub, literal-pool, and section-aware annotation
- stable high-level disassembler API over whole object files
- branch islands for `b` / `bl` displacements beyond ±128 MiB (needs
  literal-pool support)
- emission of new relocation records for undefined-symbol targets
  (`lay_out` still errors on `Target::Symbol(undefined)` — the rewriter
  needs to learn to produce a `(bytes, Vec<Relocation>)` pair instead)
- resolution of `Target::Constant` (literal-pool layer not yet wired up)
- recursive-descent disassembly that tolerates literal pools and jump
  tables in the middle of `.text`

The distinction matters: the ISA layer can decode and encode raw AArch64
instruction words at known addresses, the analysis layer can build a CFG
from a contiguous instruction stream, and the rewrite layer can emit a
modified version of that stream. None of these layers parse binaries yet —
that belongs in a future binary/object-file layer above the ISA decoder.

## API Sketch

### Decode and inspect

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
            let instruction = aarch64::decode_instruction(address, *word).ok()?;

            let has_target = instruction.operands.iter().any(|operand| {
                matches!(operand, DecodedOperand::BranchTarget(value) if *value == target)
            });

            has_target.then_some(address)
        })
        .collect()
}
```

Formatted disassembly is available on each decoded instruction:

```rust
use armv8_encode::isa::aarch64;

let address = 0x1000;
let word = 0x5400_0081; // b.ne 0x1010
let instruction = aarch64::decode_instruction(address, word).unwrap();

assert_eq!(instruction.format_mnemonic(), "b.ne");
assert_eq!(instruction.format_operands(), "0x1010");
```

### Open an object file

```rust
use armv8_encode::container::Container;
use armv8_encode::isa::aarch64;

let bytes = std::fs::read("hello.o")?;
let container = Container::from_bytes(&bytes)?;

for section in container.text_sections() {
    let (base, code) = section.for_disassembly().unwrap();
    let instructions = aarch64::disassemble_bytes(base, code)?;
    println!(
        "{}: {} instructions at {:#x}",
        section.name,
        instructions.len(),
        base
    );
}

for function in container.functions() {
    println!("fn {} @ {:#x} ({} bytes)", function.name, function.address, function.size);
}
```

### Sweep + CFG + classify

```rust
use armv8_encode::isa::aarch64;
use armv8_encode::mc::{build_cfg, ControlFlow, InstructionInfo};

let bytes: &[u8] = /* ... */;
let instructions = aarch64::disassemble_bytes(0x1000, bytes).unwrap();
let cfg = build_cfg(&instructions);

for block in &cfg.blocks {
    println!(
        "block #{} at {:#x}..{:#x}, terminator: {:?}",
        block.id.0, block.start, block.end, block.terminator
    );
}
```

### Rewrite

```rust
use armv8_encode::container::Container;
use armv8_encode::isa::aarch64;
use armv8_encode::mc::build_cfg;
use armv8_encode::rewrite::{emit, lay_out, RewritePlan, Target};

let object_bytes = std::fs::read("hello.o")?;
let container = Container::from_bytes(&object_bytes)?;

// Pull the text section we want to edit out of the container.
let text = container.text_sections().next().unwrap();
let (base, code) = text.for_disassembly().unwrap();
let instructions = aarch64::disassemble_bytes(base, code).unwrap();
let cfg = build_cfg(&instructions);

// Container-aware lift: `bl other_function` becomes Target::Symbol.
let mut plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

// Redirect a call: pick a different function symbol from the container.
let new_target = container
    .functions()
    .iter()
    .find(|f| f.name == "replacement_target")
    .unwrap();
let new_target_id = container
    .symbols
    .iter()
    .find(|s| s.address == new_target.address)
    .unwrap()
    .id;
plan.redirect_branch(0x1004, Target::Symbol(new_target_id))?;

let layout = lay_out(&plan, base, Some(&container))?;
let new_bytes = emit(&plan, &layout, Some(&container))?;

// Splice the rewritten text back into the container and write a fresh
// object file.
let edited = container.with_section_bytes(text.id, new_bytes);
std::fs::write("hello.rewritten.o", edited.to_bytes()?)?;
```

## Validation

AArch64 comparison fixtures live under `tests/fixtures/aarch64`.

Each checked fixture contains:

- source assembly, such as `basic.s`
- encoded instruction words plus `otool` mnemonics and operands, such as
  `basic.otool.txt`

The unit tests parse those fixtures, decode the raw instruction words, format
the decoded instructions, and compare the result with `otool`. They also
exercise the linear sweep, CFG construction, and rewrite pipelines on the
same inputs to catch regressions end-to-end.

The AArch64 tests track operand-kind coverage; the current snapshot expects
every table operand kind to have a decoder implementation.

For wider local comparison against real Mach-O binaries, run the ignored
`otool` harness:

```sh
cargo test --test otool_compare -- --ignored --nocapture
ARMV8_COMPARE_BINARY=/path/to/binary cargo test --test otool_compare -- --ignored --nocapture
ARMV8_COMPARE_STRICT=1 cargo test --test otool_compare -- --ignored --nocapture
```

The real-binary comparison strips disassembler comments such as symbol-stub
and literal-pool annotations. Those annotations require Mach-O/ELF parsing
and are outside the ISA decoder's scope.

## Development Direction

The next milestones are:

1. Rewriter relocation emission: thread `(bytes, Vec<Relocation>)` out of
   `emit` so `Target::Symbol(undefined)` references produce a placeholder
   instruction word plus a fresh relocation record. Lets the writer
   round-trip rewrites that touch extern calls.
2. DWARF line tables for address-to-source mapping and inlined-callsite
   discovery.
3. Recursive-descent disassembly that uses symbol and section context to
   skip literal pools and jump tables.
4. Branch islands and literal-pool layout for rewrites that exceed the
   ±128 MiB pcrel26 range.
5. PE/COFF container support and a second ISA, in either order.

Correctness matters more than surface area. The project prefers generated
or externally validated ISA data over hand-maintained instruction semantics
where possible.

## Examples

`examples/dump.rs` is a small CLI that exercises the whole stack against
a real binary — Mach-O, ELF, `.so`, `.dylib`, or `.o`. It prints the
container header, section table, defined and undefined symbol tables,
derived functions, DWARF subprograms, a relocation summary, and a
symbol-resolved disassembly of every text section. Universal Mach-O
inputs auto-select the arm64 slice.

```sh
cargo run --example dump -- /bin/ls
cargo run --example dump -- /usr/lib/system/libsystem_pthread.dylib
cargo run --example dump -- --no-disasm hello.o
cargo run --example dump -- --max-listing 10 path/to/lib.so
```

`--cfg NAME` draws the control-flow graph of a single function as boxed
instruction blocks linked by labeled edges, with back-edges (loops)
flagged. It also runs against the whole stack: container symbol lookup
locates the function, the linear sweep produces the instruction stream,
`mc::build_cfg` produces the graph, and the renderer walks blocks and
edges. Linker-prefixed names (`_main`) are auto-tolerated.

```sh
cargo run --example dump -- --cfg pthread_self /usr/lib/system/libsystem_pthread.dylib
cargo run --example dump -- --cfg main hello.o
```

Sample output for a function with a probe loop:

```text
┌─ [block 7] 0x19e8..0x19fc  (cond jump)────
│        19e8  sub        x10, x10, #0x1000 │
│        19ec  ldr        x11, [x10]        │
│        19f0  sub        x9, x9, #0x1000   │
│        19f4  cmp        x9, #0x1000       │
│        19f8  b.hi       0x19e8            │
└───────────────────────────────────────────┘
    │  taken        ↑ back-edge block 7 @ 0x19e8
    │  fallthrough  ↓ next block 8 @ 0x19fc
```

## Verification

Run:

```sh
cargo check
cargo test
```
