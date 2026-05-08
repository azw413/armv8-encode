#!/bin/sh
# Build the libgreet.so + host fixtures for the ELF runtime harness.
#
# Run from inside the runtime container — paths assume the
# tests/elf_runtime directory is bind-mounted at /work.
#
# Three artefacts:
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
#
#   host_dlopen     Executable that uses dlopen + dlsym to look up a
#                   function in libgreet.so by name at runtime. Used
#                   by the harness test that exercises
#                   TextEditor::add_function_exported (Stage 9): we
#                   rewrite libgreet.so to add a new dynamic export,
#                   then run host_dlopen to confirm the dynamic
#                   linker resolves the new symbol via the
#                   regenerated `.gnu.hash`.
set -eu

cd "$(dirname "$0")"

clang --target=aarch64-linux-gnu \
    -fPIC -shared -O0 -g \
    -fno-stack-protector \
    -Wl,-rpath,'$ORIGIN' -Wl,--disable-new-dtags \
    -o libgreet.so libgreet.c -ldl

# libdep.so: a tiny library NOT linked into the host or
# libgreet.so. Used only by the add_library_dependency
# acceptance test, which rewrites libgreet.so to add a
# DT_NEEDED entry pointing here so the loader pulls libdep
# in alongside libgreet.
clang --target=aarch64-linux-gnu \
    -fPIC -shared -O0 -g \
    -fno-stack-protector \
    -o libdep.so libdep.c

# `-Wl,-rpath,$ORIGIN` makes the loader look next to the host
# executable for `libgreet.so`. Without it the test would need
# `LD_LIBRARY_PATH=. ./host`, which is fragile across shells.
clang --target=aarch64-linux-gnu \
    -fuse-ld=lld \
    -O0 -g \
    -L. -Wl,-rpath,'$ORIGIN' \
    -o host host.c -lgreet

# host_dlopen does NOT link against libgreet.so directly — the
# Stage 9 acceptance test needs `dlsym` to be the only path to
# the new export, isolating .gnu.hash regeneration from any
# static-link-time symbol resolution. Linking with `-ldl` so
# dlopen/dlsym resolve at runtime via libdl (or libc on glibc).
clang --target=aarch64-linux-gnu \
    -fuse-ld=lld \
    -O0 -g \
    -o host_dlopen host_dlopen.c -ldl
