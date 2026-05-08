#!/bin/sh
# Build the libgreet.dylib + host fixtures for the Mach-O runtime
# harness. Runs natively on Apple Silicon (arm64) — no Docker /
# qemu needed.
#
# Three artefacts:
#
#   libgreet.dylib   ARM64 dynamic library exporting greet_double,
#                    greet_offset, and the mutable globals
#                    greet_base / greet_ctor_marker. Built with
#                    `-dynamiclib`, ad-hoc signed via codesign.
#                    `-Wl,-headerpad,0x1000` reserves spare load-
#                    command space so future load-command growth
#                    (LC_LOAD_DYLIB additions, LC_SEGMENT_64
#                    appends) doesn't require a relink.
#
#   host             Executable that links libgreet.dylib via
#                    install-path `@rpath/libgreet.dylib` and
#                    `-Wl,-rpath,@loader_path` so dyld finds the
#                    library next to the binary.
set -eu

cd "$(dirname "$0")"

clang -arch arm64 \
    -dynamiclib \
    -O0 -g \
    -fno-stack-protector \
    -Wl,-headerpad,0x1000 \
    -install_name '@rpath/libgreet.dylib' \
    -o libgreet.dylib libgreet.c

# Ad-hoc sign so dyld loads the library on macOS 10.15+. The `-`
# identity means "ad-hoc" — no developer cert required.
codesign --sign - --force libgreet.dylib

clang -arch arm64 \
    -O0 -g \
    -L. -lgreet \
    -Wl,-rpath,@loader_path \
    -o host host.c

codesign --sign - --force host
