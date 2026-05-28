# ARMv7 ELF32 runtime smoke test

Minimal end-to-end harness for the editor's ARMv7 ELF32 code
paths. Mirrors the structure of `tests/elf_runtime/` but at a
much smaller scope: one fixture, one smoke test.

## What it covers

Builds a tiny ARMv7 `hello` binary under
`linux/arm/v7` Docker, runs it (unmodified) under qemu-user to
record the baseline output, then loads it through the editor,
commits to bytes, runs the rewritten binary, and asserts the
output is byte-identical.

Catches the "rewriter produces an ELF that no longer loads"
class of bug — the kind unit tests can't see because they
don't actually execute the rewritten binary.

## What it does NOT cover

- `add_library_dependency` / `remove_library_dependency` runtime
  behaviour. Those are exercised at the unit-test level
  against `tests/libtool-checker.so` (see
  `src/tests/editor.rs`). A full runtime suite mirroring
  `tests/elf_runtime/`'s ~20 tests is future work.
- ARM-mode (A32) lift/commit. The default Thumb-2 codegen in
  this fixture exercises only the Thumb path.

## Setup

```sh
tests/arm_elf32_runtime/setup.sh
```

Installs QEMU armhf binfmt handlers if needed (no-op on macOS
and arm64 hosts) and builds the `armv7-encode-runtime` image.

## Running

```sh
cargo test --test arm_elf32_runtime -- --ignored --nocapture
```

All tests are `#[ignore]`d; they require Docker + the image
built by `setup.sh`.
