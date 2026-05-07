# ELF runtime harness

Smoke tests that confirm `Container::to_bytes` produces ELF object files
the system linker accepts and the dynamic linker can run on aarch64
Linux. Complements the in-process round-trip tests in `tests/container.rs`
by checking real runtime behaviour, not just structural parity.

## Status

Stage 1 only: ET_REL `.o` files. We rewrite, link with `clang -fuse-ld=lld`,
and run. Real `.so` and executable rewriting will land with stage 5
(`object::write::elf::Writer`).

## Setup (one-time)

```sh
tests/elf_runtime/setup.sh
```

The script probes for Docker, installs QEMU arm64 binfmt handlers if
the host needs them (x86_64 Linux only — macOS Docker Desktop and
native aarch64 hosts skip this step), and builds the
`armv8-encode-runtime` image. Idempotent and safe to re-run.

Manual setup if you want to do it by hand:

```sh
# x86_64 Linux only (macOS / aarch64 already covered):
docker run --privileged --rm tonistiigi/binfmt --install arm64

docker buildx build --platform linux/arm64 \
    -t armv8-encode-runtime tests/elf_runtime
```

## Running

```sh
cargo test --test elf_runtime -- --ignored --nocapture
```

Pin a different image tag via `ARMV8_RUNTIME_IMAGE=tag` if your CI
maintains pinned digests.

## Layout

- `Dockerfile` — minimal aarch64 Ubuntu image with clang + lld + libc6-dev.
- `fixtures/` — source files for fixtures plus `build.sh`. Built artefacts
  (e.g. `hello.o`) are produced inside the container, written back to
  this directory via the bind mount, and are gitignored.
- `fixtures/lib_demo/` — second fixture set: a tiny shared library
  (`libgreet.so`) and a host executable that links against it and
  prints results from its exports. Used as the round-trip oracle for
  Stage 5 (`.so` rewriting). Built artefacts gitignored.
- `scratch/` — harness write area for rewritten objects and linked
  binaries. Gitignored. Never `rm -rf`'d by the harness; safe to inspect.

## What's tested

1. **Baseline.** Compile `hello.c` to `.o`, link, run. Confirms the
   harness itself works.
2. **Identity round-trip.** Read the `.o` into `Container`, write it
   back unchanged, link, run. Catches writer corruption that doesn't
   show up in structural re-parse.
3. **No-op rewrite.** Lift `.text` through the full sweep → CFG → lift
   → lay-out → emit pipeline with no edits, splice back, link, run.
   Catches bugs in the rewriter ↔ container seam.

Each test is `#[ignore]` so contributors without Docker still get a green
suite by default.
