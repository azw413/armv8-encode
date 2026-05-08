// Tiny library used by the add_library_dependency runtime
// test. Its constructor sets `libdep_loaded_marker` to 0xab
// so the test can prove the loader actually pulled libdep in
// (via the rewritten libgreet.so's DT_NEEDED).
//
// Without DT_NEEDED, libdep is not loaded and a dlsym lookup
// of `libdep_loaded_marker` returns NULL. With DT_NEEDED, the
// dynamic linker pulls libdep in before libgreet's init runs,
// the marker is set, and the host can read it.
#include <stdint.h>

int32_t libdep_loaded_marker = 0;

__attribute__((constructor))
static void libdep_ctor(void) {
    libdep_loaded_marker = 0xab;
}
