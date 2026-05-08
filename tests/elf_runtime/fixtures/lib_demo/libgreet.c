// Tiny aarch64 shared library used by the ELF runtime harness.
//
// Exports two functions plus one global. The host program in
// `host.c` resolves all three at link time and calls them, so any
// rewrite that changes the addresses (function bodies move, globals
// get replaced) is observable in the host's stdout.
//
// Both functions take a small input and return a deterministic
// transformation, making it easy to assert on observed behaviour:
//
//   - `greet_double(n)` returns `n * 2`.
//   - `greet_offset(n)` returns `n + greet_base` (default base = 100).
//
// `greet_base` is mutable so a rewriter can tweak the constant
// directly via `with_section_bytes` and observe the change.
//
// `_greet_unused_puts_anchor` exists so the linker emits a .dynsym
// entry and .plt stub for `puts`, even though no compiled-in code
// path calls it. The Stage 8 demo uses that PLT entry to call
// `puts` from a function it appends to the library at rewrite
// time — without this anchor the linker would resolve `puts`
// statically (or omit it entirely) and we'd have no PLT stub to
// target.
#include <stdint.h>
#include <stdio.h>

int32_t greet_base = 100;

int32_t greet_double(int32_t n) {
    return n * 2;
}

int32_t greet_offset(int32_t n) {
    return n + greet_base;
}

// Force `puts` into .dynsym/.plt by taking its address through a
// volatile sink. The linker can't tell whether the sink will be
// read at runtime, so it preserves the dynamic relocation. This
// function is unreachable in normal use; it exists purely for its
// side effect on link-time symbol resolution.
typedef int (*puts_fn)(const char *);
volatile puts_fn _greet_unused_puts_anchor = puts;
