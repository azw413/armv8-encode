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
#include <stdint.h>

int32_t greet_base = 100;

int32_t greet_double(int32_t n) {
    return n * 2;
}

int32_t greet_offset(int32_t n) {
    return n + greet_base;
}
