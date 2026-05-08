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
// `_greet_unused_dlsym_anchor` exists so the linker emits a
// .dynsym entry and .plt stub for `dlsym`, even though no
// compiled-in code path calls it. The Stage 8+ rewriter demos
// use that one PLT entry as a universal extern resolver:
// appended code calls `dlsym(RTLD_DEFAULT, "name")` to get a
// function pointer for any symbol the process has loaded, then
// calls through it. This generalises away from the
// previous-iteration "anchor every extern we might want" pattern
// — one anchor (`dlsym`) covers any future need.
//
// Why use `dlsym` rather than `puts` / `printf` / etc.: every
// standard library function we'd plausibly want is reachable
// through `dlsym` at runtime, but the inverse isn't true (we
// can't get to `dlsym` itself except through its own PLT entry).
// One anchor on `dlsym` therefore subsumes all other anchors.
#include <dlfcn.h>
#include <stdint.h>

int32_t greet_base = 100;

int32_t greet_double(int32_t n) {
    return n * 2;
}

int32_t greet_offset(int32_t n) {
    return n + greet_base;
}

// Force `dlsym` into .dynsym/.plt by taking its address through
// a volatile sink. The linker can't tell whether the sink will
// be read at runtime, so it preserves the dynamic relocation.
// Unreachable in normal use; exists purely for its side effect
// on link-time symbol resolution.
typedef void *(*dlsym_fn)(void *, const char *);
volatile dlsym_fn _greet_unused_dlsym_anchor = dlsym;

// Marker bumped by the original constructor so a test can prove
// it still ran after `add_initialiser` hijacked the .init_array
// slot. Default value 0; original ctor sets it to 1; appended
// initialiser sets it to 1 plus a high bit (see Stage-A
// `add_initialiser` test). Exported so the host can read it.
int32_t greet_ctor_marker = 0;

// Real library constructor — runs at .so load time. The
// Stage-A `add_initialiser` test hijacks the .init_array slot
// pointing here and chains back to this function, so
// post-rewrite both the appended initialiser AND this one run.
// We bump a dedicated marker rather than `greet_base` so all
// the existing runtime tests (which assert on `greet_base=100`)
// keep passing.
__attribute__((constructor))
static void greet_ctor(void) {
    greet_ctor_marker |= 0x1;
}
