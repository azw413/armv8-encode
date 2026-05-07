#!/bin/sh
# Build aarch64 ELF object fixtures inside the runtime container.
#
# Run via the harness, not by hand — the harness sets the working
# directory and bind mount so paths line up.
set -eu

cd "$(dirname "$0")"

# `-c` produces an ET_REL .o that the harness will round-trip through
# the Container layer. `-fno-pie -fno-stack-protector` keeps the symbol
# table small and free of GOT/PLT relocations we don't need yet.
clang --target=aarch64-linux-gnu \
    -c -O0 -g \
    -fno-pie -fno-stack-protector \
    -o hello.o hello.c
