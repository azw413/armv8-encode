// Mach-O variant of the ELF runtime fixture's host.c.
//
// Four invocation modes (matching the ELF host so tests can
// assert on the same stdout strings):
//
//   ./host              prints both function results.
//   ./host single       prints only `greet_double(21)`.
//   ./host ctor         prints `greet_ctor_marker`.
//   ./host libdep       prints `libdep_loaded_marker` via
//                       dlsym(RTLD_DEFAULT, ...). 0 if libdep
//                       isn't loaded, 0xab=171 if it is. Used
//                       by the add_library_dependency
//                       acceptance test.
//
// Linked at build time against libgreet.dylib via
// `-Wl,-rpath,@loader_path` so the loader finds it next to the
// host binary at runtime, no DYLD_LIBRARY_PATH needed.
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
        // Look up the marker via the global resolution
        // scope. If libdep is in libgreet's load list, dyld
        // has pulled it in before main() and dlsym finds the
        // symbol; if not, dlsym returns NULL.
        int32_t *marker =
            (int32_t *)dlsym(RTLD_DEFAULT, "libdep_loaded_marker");
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
