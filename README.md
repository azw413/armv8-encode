# armv8-encode

`armv8-encode` is a Rust project for machine-code analysis, decoding, encoding,
and rewriting on AArch64.

The crate started as a decoder/encoder pair, then grew an analysis layer
(basic blocks, control-flow graphs), then a symbolic rewriter, then full
ELF read+write support including ET_DYN. Today it can take an unmodified
aarch64 `.so`, edit its `.text` or `.data`, append new functions in a
fresh PT_LOAD segment that call existing PLT-bound externs (and, via a
single `dlsym` anchor, any dynamic symbol the process can see), and
produce a runnable byte stream the dynamic linker accepts.

The primary architectural target is AArch64; the rest of the layering
(`container`, `mc`, `rewrite`) stays format- and architecture-neutral
so other ISAs can plug in later.

## What you can do today

- **Decode** any AArch64 instruction word the imported opcode table covers.
- **Encode** new instructions from typed templates.
- **Linear-sweep** or **recursive-descent** disassemble a code region.
- **Build a CFG** and reason about basic blocks and control flow.
- **Read** ELF (`.o`, `.so`, executables) and Mach-O (`.o`) into a neutral
  container model. ET_DYN/ET_EXEC inputs preserve the full ELF surface
  needed to round-trip them.
- **Write** ELF ET_REL (`.o`) and ET_DYN (`.so`/PIE executable) byte
  streams that the system linker / dynamic linker accept.
- **Rewrite a section symbolically**: lift instructions to an editable
  IR with `Target::Symbol` / `Target::Block` operands, mutate, lay
  out (with conditional-branch widening), emit, splice back.
- **Edit `.rodata`/`.data`** as a sequence of pointer + bytes items
  (e.g. swap a function-pointer slot in a vtable).
- **Edit `.text` of a real `.so`** in place via the high-level
  [`BinaryEditor`] API.
- **Append a new function** to an `.so` in a fresh executable
  segment, complete with new read-only data, and have the new code
  call existing extern functions through the PLT.
- **Reach any libc symbol via a single `dlsym` anchor.** If the
  source library imports `dlsym` (one PLT entry is enough), appended
  code can call `dlsym(RTLD_DEFAULT, "name")` to resolve any symbol
  the dynamic loader can find — `printf`, `strlen`, `getenv`,
  anything — without needing it to be pre-anchored in the source.
- **Force-load another library at this one's load time.**
  `add_library_dependency("libfoo.so")` appends the name to a
  rebuilt `.dynstr` (in the appended segment) and inserts a
  `DT_NEEDED` tag in `.dynamic`. The dynamic linker resolves
  every `DT_NEEDED` before firing this library's constructors,
  so the named library is guaranteed to be present and its
  symbols reachable through `dlsym(RTLD_DEFAULT, ...)` by the
  time any code in this library runs. Pairs naturally with
  `add_initialiser` for "load this side library, then run my
  init code that uses it."
- **Run code at library load time.**
  `add_initialiser(name, body, position)` registers a
  load-time constructor. Three insertion strategies:
  - `InitialiserPosition::First` — hijack `.init_array[0]` so
    the appended code runs ahead of *every* other ctor
    (including CRT helpers like `frame_dummy`); a wrapper
    chain-tails to the original ctor so it still runs.
  - `InitialiserPosition::Last` — hijack the final
    `.init_array` slot; same chain-back semantics, but the
    appended code runs after every other ctor and before only
    the last one.
  - `InitialiserPosition::Append` — *add a brand-new slot*
    rather than hijacking. The appended code runs after all
    original ctors as a separate constructor. The new slot
    plus the extended `.rela.dyn` (with one new
    `R_AARCH64_RELATIVE` per slot) live in the appended
    segment; `.dynamic` is patched to point at the rebuilt
    sections via `DT_INIT_ARRAY`/`DT_INIT_ARRAYSZ`/`DT_RELA`/
    `DT_RELASZ`/`DT_RELACOUNT`. Works on inputs that have
    no `.init_array` at all, provided the input has an
    existing `.rela.dyn` and at least two unused `DT_NULL`
    slots in `.dynamic`.
- **Export the new function** as a `.dynsym` entry resolvable via
  `dlopen` + `dlsym` from any caller. The writer rebuilds
  `.dynsym` / `.dynstr` / `.gnu.version` and regenerates
  `.gnu.hash` from scratch, then points the `.dynamic` tags at the
  new copies in the appended segment.

End-to-end runtime tests confirm rewritten libraries load and run
correctly under aarch64 Linux (via QEMU on macOS host).

## Architecture

The crate is split into four layers, bottom-up: container → ISA → mc →
rewrite. Each layer only knows about the ones below it.

### Container layer

Path: `src/container`

Reads Mach-O and ELF object files into a neutral, format-agnostic
model: sections, symbols, relocations, optional DWARF debug info, and
`Function` views derived from both. The `object` crate handles format
parsing and `gimli` handles DWARF; the container layer hides both so
the rest of the crate sees one shape regardless of source.

The layer's input is `&[u8]`; the output is a
[`Container`](src/container/types.rs) ready to feed into disassembly
or rewriting. AArch64-relevant relocations are mapped onto a neutral
enum:

- `Branch26`, `Branch19`, `Branch14` (PC-relative branches)
- `AdrpPage21` (adrp page reference)
- `AddPageOffset12` (add immediate companion to adrp)
- `LoadStorePageOffset12 { access_width_bytes }` (ldr/str companion)
- `Absolute` (data references / GOT)
- `Other(raw_code)` (unrecognized — preserved structurally)

A [`ContainerKind`](src/container/types.rs) classifies inputs as
`Relocatable` / `SharedObject` / `Executable` / `Other`.
`Container::to_bytes()` dispatches on this:

- **Relocatable** ELF / Mach-O `.o` → emitted via
  `object::write::Object`. Round-trip is structurally compatible.
- **SharedObject / Executable** ELF (ET_DYN / ET_EXEC) → emitted via
  `object::write::elf::Writer` driven through reserve/write phases
  by `src/container/elf_writer.rs`. The companion
  [`ElfImage`](src/container/elf_image.rs) struct captures
  everything the neutral types deliberately don't model (program
  headers, `.dynamic` tag list, `.gnu.hash`, `.gnu.version*`,
  `.eh_frame_hdr`, build-ID, `.interp`, per-section
  `sh_offset`/`sh_size`/`sh_link`/`sh_info`, PLT stub addresses for
  every dynsym extern). The writer reproduces the input's layout
  faithfully — file offsets and section header positions may shift,
  but program-header virtual addresses, `.dynamic` tags, and
  dynsym/PLT-resolved call sites stay valid.
- Anything else → `UnsupportedKind` error.

When a binary carries `.debug_info` / `__debug_info`, the container
also exposes a `DwarfInfo` with one `DwarfFunction` per
`DW_TAG_subprogram`. `Container::functions()` merges symbol-derived
and DWARF-derived entries — symbols take precedence, DWARF fills in
the gap when the binary is stripped.

### ISA layer

Path: `src/isa`

The ISA layer owns architecture-specific instruction knowledge: raw
instruction encodings, opcode tables, operand schemas, operand
extraction and insertion, validation rules, aliases and canonical
forms, architecture feature/version constraints.

The current AArch64 implementation lives under `src/isa/aarch64`. It
uses an imported opcode table as the matching foundation, decodes
table operands into typed Rust values (registers, immediates, memory
operands, branch/page targets, vector registers, vector elements,
system operands), implements
[`InstructionInfo`](src/mc/control_flow.rs) for control-flow
classification, and exposes table-driven encoding alongside helpers
used by the rewrite layer.

Two disassembler entry points live here:
[`disassemble_bytes`](src/isa/aarch64/sweep.rs) does fail-fast linear
sweep (every word must decode), and
[`disassemble_recursive`](src/isa/aarch64/recursive.rs) walks control
flow from a set of entry points and classifies anything it doesn't
reach as data. The latter is what works on real shipped binaries.

### Machine-code layer

Path: `src/mc`

The machine-code layer is architecture-neutral. It models decoded
code in terms useful for analysis and rewriting:

- the [`InstructionInfo`](src/mc/control_flow.rs) trait that ISA
  crates implement so analysis stays generic
- the `ControlFlow` classification (`Fall`, `Jump`,
  `ConditionalJump`, `Call`, `Return`, `IndirectJump`,
  `IndirectCall`, `Trap`)
- basic blocks and the [`ControlFlowGraph`](src/mc/cfg.rs) built by
  `mc::build_cfg`

### Rewrite layer

Path: `src/rewrite`

The rewrite layer turns a decoded code region into an editable IR
whose PC-relative operands are *symbolic*: instead of carrying a
hard-coded address, each branch target carries a
[`Target`](src/rewrite/ir.rs) — a reference to a basic block, an
extern symbol, a constant pool entry, or a literal address. This is
what lets the layout pass move things around freely without
invalidating displacements.

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
                                        │
                                        ▼
                              commit_to_container(...) ──► Container
                                        │
                                        ▼
                                Container::to_bytes() ──► byte stream
```

Layout iterates to a fixed point: if an edit pushes a conditional
branch past its `pcrel19` (`b.cond`, `cbz`, `cbnz`) or `pcrel14`
(`tbz`, `tbnz`) range, it widens the branch into
`<inverted_cond> .Lskip ; b far_target ; .Lskip:`, which can in turn
push other branches out of range — repeat until stable.

Each block stores `Vec<RewriteOp>` where `RewriteOp` is either a
single `RewriteInstruction` or a `MacroOp` (a fused multi-instruction
idiom). Two macros are recognised today:

- `MacroKind::LoadAddress` — `adrp Rd, page; add Rd, Rd, #lo12` for
  computing a symbol's absolute address.
- `MacroKind::AccessValue` — `adrp Rd, page; ldr/str Rt, [Rd, #lo12]`
  for loading from / storing to a symbol's address.

Macros are recognised at lift time, edited as a unit, and expanded
back to their component instructions on emit.

#### High-level `BinaryEditor` API

The [`BinaryEditor`](src/rewrite/editor.rs) wraps the lift → edit →
layout → emit → commit pipeline behind a smaller, typed surface.
It splits its operations across two scoped sub-views:

- `editor.binary` ([`BinaryState`]) — whole-binary methods: append
  functions, declare dependencies, register exports, etc.
- `editor.text` ([`LiftedTextSection`], optional) — section-scoped
  methods: redirect branches, replace instructions, etc. Populated
  by [`BinaryEditor::lift_text_section`].

```rust
use armv8_encode::container::Container;
use armv8_encode::rewrite::{BinaryEditor, Target};

let bytes = std::fs::read("libgreet.so")?;
let container = Container::from_bytes(&bytes)?;

let mut editor = BinaryEditor::for_section(&container, ".text")?;
let printf = editor.binary.symbol_by_name("printf")?;
editor
    .text
    .as_mut()
    .unwrap()
    .redirect_branch_at(0x1234, Target::Symbol(printf))?;
let new_bytes = editor.commit_to_bytes()?;
std::fs::write("libgreet.rewritten.so", new_bytes)?;
```

When you need to interleave whole-binary and section-scoped edits,
destructure once so both `&mut` references coexist:

```rust
let BinaryEditor { binary, text, .. } = &mut editor;
let text = text.as_mut().unwrap();

let log = binary.add_function("hello_log", body)?;
let target = binary.function_address("greet_double").unwrap();
text.replace_instruction_at(target, /* b log */)?;
```

The editor proxies the rewrite primitives:

- `redirect_branch_at(address, target)` — change a branch's
  destination.
- `redirect_macro_target_at(address, target)` — change a macro's
  target.
- `replace_instruction_at(address, instruction)` — overwrite a
  singleton instruction.
- `insert_after_address(address, instructions)` — splice in new
  instructions.
- `remove_at_address(address)` — drop an op.
- `add_function(name, instructions) -> SymbolId` — append a new
  function in a fresh PT_LOAD R-X segment past the input's mapped
  range. Returns a `SymbolId` callers can pass back as
  `Target::Symbol` to redirect existing branches at the new code.
  The function is registered in the static `.symtab` only — it
  isn't visible to `dlsym` callers.
- `add_function_exported(name, instructions) -> SymbolId` —
  same as `add_function`, plus promotes the new symbol to
  `.dynsym` so `dlopen`/`dlsym` callers can resolve it by name.
  The writer rebuilds `.dynsym`, `.dynstr`, `.gnu.version`, and
  regenerates `.gnu.hash` (with `nbuckets = 1` for layout
  simplicity), then updates the captured `.dynamic` tags so the
  loader follows the new copies (placed in the appended segment).
  The original sections stay in the file but are ignored at
  runtime.
- `add_data(name, bytes, align) -> SymbolId` — append read-only
  data alongside the new functions in the same segment. The new
  function can compute the blob's address via the standard
  `adrp + add` pair against `Target::Symbol(blob_id)`; the
  rewriter's macro-fusion pass folds the pair into a
  `LoadAddress` macro that resolves at the appended segment's
  vaddr.
- `editor.binary.add_library_dependency(library_name) -> ()` — force the
  dynamic linker to load another shared library when this one
  is loaded. Appends the name to `.dynstr` (rebuilt in the
  appended segment) and inserts a `DT_NEEDED` tag in
  `.dynamic`. Returns `DynamicTooFull` if `.dynamic` lacks the
  trailing `DT_NULL` slot needed to absorb the new tag.
- `add_initialiser(name, body, position) -> SymbolId` — register
  a function that runs at library load time. Three positions:
  - `First` / `Last` — hijack an existing `.init_array` slot
    via a wrapper that chain-tails to the displaced original
    ctor. Requires the input to have a non-empty
    `.init_array`; returns `NoExistingInitArray` otherwise.
  - `Append` — add a brand-new `.init_array` slot without
    hijacking. The new slot, plus a rebuilt `.rela.dyn` with
    one extra `R_AARCH64_RELATIVE`, lives in the appended
    segment, and `.dynamic` is patched to point at the new
    copies via `DT_INIT_ARRAY` / `DT_INIT_ARRAYSZ` /
    `DT_RELA` / `DT_RELASZ` / `DT_RELACOUNT`. Returns
    `NoExistingRelaDyn` if the input has no `.rela.dyn`, or
    `DynamicTooFull` if `.dynamic` lacks the trailing
    `DT_NULL` padding to absorb the new tags.
- `commit() -> Container` and `commit_to_bytes() -> Vec<u8>` —
  drive the layout/emit/commit pipeline.

`commit_to_bytes` automatically routes through the right writer
path: in-place when no functions were appended (existing `.text`
edits stay within the source extent), or the append-PT_LOAD path
when new functions live in a fresh segment.

#### PLT-aware extern calls

For ET_DYN inputs, the reader populates `ElfImage.plt_stubs` with a
`SymbolId → plt_stub_vaddr` map by walking `.rela.plt`.
[`Container::callable_address_of_symbol`](src/container/types.rs)
returns the PLT stub address for any extern that has one. Emit folds
`Target::Symbol(extern_id)` into a direct `bl <stub>` at write time,
so appended code can call existing PLT-bound externs (`puts`,
`printf`, etc.) without adding new dynsym entries.

#### Lower-level rewrite operations

Direct access to the underlying primitives is still available for
callers who need to bypass the editor:

- `RewritePlan::lift(cfg, instructions)`
- `RewritePlan::lift_with_container(cfg, instructions, container)`
- `RewritePlan::from_instructions(instructions, container)` —
  build a plan from a raw instruction list (used by `add_function`
  to fuse `adrp + add` macros in user-supplied bodies).
- `lay_out(&plan, base, container) -> Layout`
- `emit(&plan, &layout, container) -> EmitOutput`
- `commit_to_container(&container, section, output) -> Container`

For data sections there's a parallel API:

- `DataSection::lift(container, section_id) -> DataLift`
- `DataSection::redirect_pointer_at(index, new_target)`
- `emit_data_section(plan) -> DataEmitOutput`
- `commit_to_data_container(container, section, output, unhandled)`

## Examples

The `examples/` directory contains runnable demonstrations of each
capability:

- **`examples/dump.rs`** — inspect any Mach-O / ELF binary. Prints
  the container header, section table, symbol tables, derived
  functions, DWARF subprograms, relocation summary, and a
  symbol-resolved disassembly of every text section. `--cfg NAME`
  draws the control-flow graph of a single function as boxed
  instruction blocks linked by labelled edges.
- **`examples/elf_inspect.rs`** — deep ELF surface inventory:
  program headers, section headers, `.dynamic` tags, `.dynsym`,
  `.gnu.version*`, `.gnu.hash`, build-ID, `.eh_frame_hdr`,
  `.interp`. Useful for understanding what an ET_DYN input
  carries before editing.
- **`examples/text_edit_so.rs`** — read `libgreet.so`, find
  `greet_double`'s `lsl` instruction, replace it (changing
  `n*2` to `n*4`), write the result, re-parse to confirm.
  Demonstrates the `BinaryEditor` API end-to-end on an ET_DYN.
- **`examples/decorate_so.rs`** — append a new function
  `greet_quintuple` to `libgreet.so` and patch `greet_double` to
  tail-call it. Demonstrates `add_function` + `replace_instruction_at`
  for the "decorator" pattern.
- **`examples/decorate_so_with_log.rs`** — the most ambitious
  static-decorator example. Appends two new strings via `add_data`
  (the symbol name `"puts"` and the message), appends a new function
  via `add_function` that:
  - resolves `puts` at runtime via
    `dlsym(RTLD_DEFAULT, "puts")` — the only PLT-bound extern the
    appended code needs is `dlsym` itself;
  - computes the message address via fused `adrp + add` against the
    appended symbol;
  - calls the resolved `puts` and returns the original `n*2` result.
  Patches `greet_double` to tail-call the new function so each
  call prints a line *and* returns its usual answer.
- **`examples/call_printf_via_dlsym.rs`** — proves the dlsym anchor
  is a *universal* resolver. libgreet.so does not import `printf`,
  yet appended code calls `printf` by going through
  `dlsym(RTLD_DEFAULT, "printf")` and invoking the resolved pointer.
  One PLT anchor (`dlsym`) subsumes the need for any number of
  imported externs.
- **`examples/add_initialiser.rs`** — appends a function that
  runs at library load time before the host's `main()` reaches
  any libgreet code. Hijacks the last `.init_array` slot
  (originally pointing at libgreet's own `__attribute__((constructor))`),
  redirects it to a freshly-appended wrapper, and chains the
  wrapper back to the original ctor — so both run, in
  user-first order. Verify the result with `./host ctor` (expected
  `ctor_marker=17`, proving both ran) and `./host` (still
  `double=42 offset=107`, proving normal functionality is intact).
- **`examples/export_function.rs`** — appends a new function
  `greet_quintuple` via `add_function_exported` so it appears in
  `.dynsym` and is resolvable via `dlopen`/`dlsym`. Demonstrates
  the `.gnu.hash` regeneration path; pair with the runtime
  fixture's `host_dlopen` binary to verify the export end-to-end.

Run any example with `cargo run --example NAME`. Examples that
target `libgreet.so` need the runtime fixture built first; see
`tests/elf_runtime/README.md` for setup.

## Validation

### Unit tests (no Docker)

236 unit tests cover the decoder, encoder, sweep, recursive descent,
container reader/writer, rewrite IR, data IR, the editor API, and
operand-kind coverage assertions. Fixtures live under
`tests/fixtures/aarch64`. Each fixture contains source assembly plus
encoded instruction words and `otool`-format mnemonics; the unit
tests parse those, decode the words, format the result, and compare
against `otool`. They also exercise the linear sweep, CFG
construction, and rewrite pipelines on the same inputs to catch
regressions end-to-end.

```sh
cargo test
```

### Real-binary `otool` comparison (optional, macOS)

For wider local comparison against real Mach-O binaries:

```sh
cargo test --test otool_compare -- --ignored --nocapture
ARMV8_COMPARE_BINARY=/path/to/binary cargo test --test otool_compare -- --ignored --nocapture
ARMV8_COMPARE_STRICT=1 cargo test --test otool_compare -- --ignored --nocapture
```

### ELF runtime harness (Docker, all platforms)

The `tests/elf_runtime/` directory builds a small aarch64 Linux
fixture (`libgreet.so` + `libdep.so` + `host` + `host_dlopen`) inside a Docker
image, exercises the full read → edit → write → load → run
pipeline, and asserts on host stdout. Fourteen tests cover:

- baseline (sanity: harness works, fixture runs).
- identity round-trip — `Container::to_bytes()` produces a
  loadable `.so` with no edits.
- no-op text rewrite — full lift → emit → commit pipeline with
  no edits, host still works.
- data-section edit — redirect a function-pointer slot in `.data`.
- ET_DYN round-trip — read `libgreet.so`, write it back, host
  loads and runs against the rewritten copy.
- in-place text edit — patch `greet_double`'s `lsl` constant via
  `BinaryEditor`, host observes new return value.
- appended function — add `greet_quintuple` via `add_function`,
  redirect `greet_double` to it, host observes new return value.
- appended function resolving extern via dlsym — add
  `greet_log_double` that calls `dlsym(RTLD_DEFAULT, "puts")` then
  invokes the resolved pointer to print a string before returning
  `n*2`. Host observes both behaviours.
- appended function calling unimported extern via dlsym — add
  `greet_printf_double` that calls `printf` (which libgreet.so does
  *not* import) by going through `dlsym(RTLD_DEFAULT, "printf")`.
  Demonstrates that a single PLT anchor (`dlsym`) is enough to reach
  any libc function from appended code.
- exported appended function — add `greet_quintuple` via
  `add_function_exported`, then `host_dlopen` resolves it by
  name through the regenerated `.gnu.hash` and calls it.
- appended initialiser hijacks `.init_array` (Last) — appends
  an init function that writes a marker, redirects the last
  `.init_array` slot to a wrapper around it, chains the wrapper
  back to the library's original constructor, and verifies via
  `host ctor` that both ran (final marker 17 = 0x10 appended |
  0x1 chained).
- appended initialiser hijacks `.init_array` (First) — same
  body, but redirects slot[0] (originally `frame_dummy`) so the
  appended code runs ahead of every other library ctor.
  Marker still ends at 17 because the original ctor still runs
  from slot[1].
- appended initialiser adds a *brand-new* `.init_array` slot
  (Append) — rebuilt `.init_array` and `.rela.dyn` live in the
  appended segment, `.dynamic` patched accordingly. Marker
  ends at 16 (greet_ctor sets bit 0x1, then the appended slot
  overwrites with 0x10 because it runs *after* the originals
  rather than via a hijack-and-chain-back).
- forced library load via `add_library_dependency` — fixture
  ships an unlinked `libdep.so` whose ctor sets a marker.
  Without rewriting, host's dlsym lookup of the marker
  returns 0 (libdep not loaded). After `add_library_dependency`
  injects a DT_NEEDED for libdep into libgreet's `.dynamic`,
  the loader pulls libdep in and the marker reads 0xab.

Setup (one-time):

```sh
tests/elf_runtime/setup.sh
```

(Probes for Docker, installs QEMU arm64 binfmt handlers if needed,
builds the runtime image. Idempotent.)

Run:

```sh
cargo test --test elf_runtime -- --ignored --nocapture
```

## Examples — quick API tour

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

### Edit a `.so` in place

```rust
use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{Aarch64Mnemonic, DecodedOperand};
use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand};

let bytes = std::fs::read("libgreet.so")?;
let container = Container::from_bytes(&bytes)?;
let mut editor = BinaryEditor::for_section(&container, ".text")?;

// Find an instruction by walking editor.text.instructions(),
// build a replacement, install it.
# let lsl_addr = 0u64;
# let rd = todo!(); let rn = todo!();
let new_lsl = RewriteInstruction {
    mnemonic: Aarch64Mnemonic::Lsl,
    operands: vec![
        RewriteOperand::Decoded(DecodedOperand::Register(rd)),
        RewriteOperand::Decoded(DecodedOperand::Register(rn)),
        RewriteOperand::Decoded(DecodedOperand::Immediate(2)),
    ],
    original_address: Some(lsl_addr),
};
editor
    .text
    .as_mut()
    .unwrap()
    .replace_instruction_at(lsl_addr, new_lsl)?;

let rewritten = editor.commit_to_bytes()?;
std::fs::write("libgreet.rewritten.so", rewritten)?;
```

### Append a new function and call an existing extern

```rust
use armv8_encode::container::Container;
use armv8_encode::isa::aarch64::{self, Aarch64Mnemonic, DecodedOperand};
use armv8_encode::rewrite::{BinaryEditor, RewriteInstruction, RewriteOperand, Target};

let bytes = std::fs::read("libgreet.so")?;
let container = Container::from_bytes(&bytes)?;
let mut editor = BinaryEditor::for_section(&container, ".text")?;

// Destructure once so both scopes are usable side-by-side.
let BinaryEditor { binary, text, .. } = &mut editor;
let text = text.as_mut().unwrap();

// Resolve an existing PLT-bound extern. The reader populated
// elf_image.plt_stubs at parse time; emit will fold a call to
// this symbol into `bl <plt_stub>`.
let puts = binary.symbol_by_name("puts@GLIBC_2.17")?;

// Append a string in the new segment.
let msg_id = binary.add_data("hello_msg", b"hello from new code\0", 1)?;

// Build a new function (instructions elided — see
// examples/decorate_so_with_log.rs for the full body). The adrp+add
// pair against Target::Symbol(msg_id) fuses into a LoadAddress
// macro; bl Target::Symbol(puts) folds to the existing PLT stub.
# let body: Vec<RewriteInstruction> = vec![];
let log_id = binary.add_function("hello_log", body)?;

// Redirect an existing function to call the new one.
let target = binary.function_address("greet_double").unwrap();
text.replace_instruction_at(
    target,
    RewriteInstruction {
        mnemonic: Aarch64Mnemonic::B,
        operands: vec![RewriteOperand::Branch(Target::Symbol(log_id))],
        original_address: Some(target),
    },
)?;

let rewritten = editor.commit_to_bytes()?;
std::fs::write("libgreet.rewritten.so", rewritten)?;
```

## Limits and future work

What's deliberately not yet implemented:

- **Length-growing in-place text edits.** Inserting instructions
  into an existing function past its source extent needs the
  rewrite layer to relocate the function to a new vaddr and update
  PC-relative addressing. Workaround today: use `add_function` to
  put the new code in a fresh segment and tail-call into it.
- **Adding new dynsym *imports* via fresh PLT stubs.** We don't
  synthesise new `.rela.plt` entries or PLT stubs for imports the
  source library didn't already carry. In practice this rarely
  matters: if the source imports `dlsym` (one PLT entry is enough),
  appended code can resolve any other libc symbol at runtime via
  `dlsym(RTLD_DEFAULT, "name")` — see
  `examples/call_printf_via_dlsym.rs`. The "grow the import table"
  path is still future work, but the dlsym pattern usually obviates
  the need for it.
- **Versioned exports.** `add_function_exported` emits the new
  symbol with versym = 1 (unversioned/base). Producing a
  versioned export with a `.gnu.version_d` definition isn't yet
  supported.
- **`.eh_frame_hdr` regeneration.** When existing functions
  move (Stage 6.3 grown-text path), the binary-search table inside
  `.eh_frame_hdr` becomes stale. Today we copy `.eh_frame_hdr`
  verbatim — fine for in-place edits and for appending new code
  (the new code has no FDEs).
- **PT_PHDR-bearing inputs.** The append-PT_LOAD writer relocates
  the program header table to file end, which is awkward when the
  input has a PT_PHDR segment describing the table's location.
  libgreet.so doesn't have one; many Linux binaries don't.
- **Mach-O ET_DYN rewriting.** `object::write::macho` is much
  weaker than `object::write::elf::Writer`; the Mach-O dylib
  writer is its own substantial project. Mach-O `.o` round-trip
  works.
- **PE/COFF.** Not started.
- **DWARF line tables** (file/line lookup for arbitrary
  addresses) and inlined-callsite metadata — only
  `DW_TAG_subprogram` is lifted today.
- **Jump-table / vtable / indirect-branch analysis** to recover
  targets recursive descent currently can't follow.
- **Branch islands** for `b` / `bl` displacements beyond ±128 MiB.
- **Resolution of `Target::Constant`** (literal-pool layer).

## Verification commands

```sh
cargo check            # fast type-check
cargo test             # 236 unit tests
cargo test -- --ignored # also runs the runtime harness (needs Docker)
```

Correctness matters more than surface area. The project prefers
generated or externally validated ISA data over hand-maintained
instruction semantics where possible.
