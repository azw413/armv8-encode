// Mach-O variant of the ELF runtime fixture's libgreet.so.
// Same observable API so tests can mirror their assertions.
//
// Differences vs. the ELF fixture:
//   - No dlsym anchor needed: macOS exports dlsym from libSystem
//     by default, so any imported symbol from libSystem (e.g. the
//     compiler's automatic dependency on libSystem) is enough.
//   - Built as a Mach-O dynamic library (`-dynamiclib`) and signed
//     ad-hoc via `codesign -s -` so dyld will load it.
//
// Functions:
//   greet_double(n) = n * 2
//   greet_offset(n) = n + greet_base   (greet_base mutable)
//
// Constructor:
//   greet_ctor sets `greet_ctor_marker |= 0x1` at load time. Used
//   by future Phase-6 (add_initialiser) acceptance tests.
#include <stdint.h>

int32_t greet_base = 100;
int32_t greet_ctor_marker = 0;

int32_t greet_double(int32_t n) {
    return n * 2;
}

int32_t greet_offset(int32_t n) {
    return n + greet_base;
}

__attribute__((constructor))
static void greet_ctor(void) {
    greet_ctor_marker |= 0x1;
}
