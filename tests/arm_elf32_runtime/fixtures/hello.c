// Minimal ARMv7 ELF32 fixture for the runtime smoke test.
// Prints a single fixed line; the smoke test rewrites the
// resulting binary through the editor (no-op edit), runs it
// under qemu-arm, and asserts the output is unchanged.

#include <stdio.h>

int main(void) {
    puts("hello arm");
    return 0;
}
