// Host program for the ELF runtime harness.
//
// Calls both functions in libgreet.so and prints results in a
// deterministic format. The harness asserts on stdout — any rewrite
// the .so undergoes that changes observable behaviour shows up here.
//
// Three invocation modes selected by argv[1]:
//
//   ./host             prints both function results with the default
//                      `greet_base` value (100).
//
//   ./host single      prints only `greet_double(21)` — useful for
//                      isolating one function in a regression test.
//
//   ./host ctor        prints the value of `greet_ctor_marker` (set
//                      by the library's constructor and, in the
//                      Stage-A add_initialiser test, also by the
//                      appended initialiser). Used to prove that
//                      both the original ctor and the appended one
//                      ran.
//
//   ./host libdep      prints the value of `libdep_loaded_marker`
//                      via dlsym(RTLD_DEFAULT, ...). Returns 0 if
//                      libdep is not loaded, 0xab (171) if it is.
//                      Used by the add_library_dependency test to
//                      prove DT_NEEDED was honoured.
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <dlfcn.h>

int32_t greet_double(int32_t n);
int32_t greet_offset(int32_t n);
extern int32_t greet_ctor_marker;

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "ctor") == 0) {
        printf("ctor_marker=%d\n", greet_ctor_marker);
        return 0;
    }
    if (argc > 1 && strcmp(argv[1], "libdep") == 0) {
        // Look up the marker via the global resolution scope.
        // If libdep is in DT_NEEDED, the loader has pulled it
        // in before main() and dlsym finds the symbol; if not,
        // dlsym returns NULL and we report 0.
        int32_t *marker = (int32_t *)dlsym(RTLD_DEFAULT, "libdep_loaded_marker");
        printf("libdep_marker=%d\n", marker ? *marker : 0);
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
