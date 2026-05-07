#!/bin/sh
# Build the libgreet.so + host fixtures for the ELF runtime harness.
#
# Run from inside the runtime container — paths assume the
# tests/elf_runtime directory is bind-mounted at /work.
#
# Two artefacts:
#
#   libgreet.so     ET_DYN shared library exporting `greet_double`,
#                   `greet_offset`, and the mutable `greet_base`. Built
#                   with `-fPIC -shared` so its `.text` is position-
#                   independent and Stage-5-shaped.
#
#   host            Executable that links against `libgreet.so` and
#                   uses an `-rpath '$ORIGIN'` so the loader finds the
#                   library next to the binary at runtime, with no
#                   LD_LIBRARY_PATH plumbing required.
set -eu

cd "$(dirname "$0")"

clang --target=aarch64-linux-gnu \
    -fPIC -shared -O0 -g \
    -fno-stack-protector \
    -o libgreet.so libgreet.c

# `-Wl,-rpath,$ORIGIN` makes the loader look next to the host
# executable for `libgreet.so`. Without it the test would need
# `LD_LIBRARY_PATH=. ./host`, which is fragile across shells.
clang --target=aarch64-linux-gnu \
    -fuse-ld=lld \
    -O0 -g \
    -L. -Wl,-rpath,'$ORIGIN' \
    -o host host.c -lgreet
