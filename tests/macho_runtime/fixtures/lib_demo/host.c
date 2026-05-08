// Mach-O variant of the ELF runtime fixture's host.c.
//
// Three invocation modes (matching the ELF host so tests can
// assert on the same stdout strings):
//
//   ./host              prints both function results.
//   ./host single       prints only `greet_double(21)`.
//   ./host ctor         prints `greet_ctor_marker`.
//
// Linked at build time against libgreet.dylib via
// `-Wl,-rpath,@loader_path` so the loader finds it next to the
// host binary at runtime, no DYLD_LIBRARY_PATH needed.
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int32_t greet_double(int32_t n);
int32_t greet_offset(int32_t n);
extern int32_t greet_ctor_marker;

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "ctor") == 0) {
        printf("ctor_marker=%d\n", greet_ctor_marker);
        return 0;
    }
    int32_t doubled = greet_double(21);
    if (argc > 1) {
        printf("double=%d\n", doubled);
        return 0;
    }
    int32_t offset = greet_offset(7);
    printf("double=%d offset=%d\n", doubled, offset);
    return 0;
}
