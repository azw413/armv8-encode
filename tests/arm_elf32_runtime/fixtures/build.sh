#!/bin/sh
# Build the hello fixture for the ARMv7 runtime smoke test.
#
# Run from inside the runtime container — paths assume the
# tests/arm_elf32_runtime directory is bind-mounted at /work.
#
# Produces:
#   hello   ARMv7 ET_EXEC built with -no-pie so the rewriter's
#           in-place writer path can edit it without worrying
#           about PIE-only segment layout. Default Thumb-2
#           code generation (clang defaults).

set -eu

cd "$(dirname "$0")"

clang --target=arm-linux-gnueabihf \
    -fuse-ld=lld \
    -O0 -g \
    -no-pie \
    -o hello hello.c
