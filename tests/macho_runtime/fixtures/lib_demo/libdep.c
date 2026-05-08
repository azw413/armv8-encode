// Mach-O variant of the ELF fixture's libdep.c.
// A tiny library NOT linked into the host or libgreet.dylib —
// used only by the Phase-7 add_library_dependency acceptance
// test, which rewrites libgreet.dylib to add an LC_LOAD_DYLIB
// pointing here so dyld pulls libdep in alongside libgreet.
//
// libdep's constructor sets `libdep_loaded_marker` to 0xab so
// the host can prove the dependency was honoured.
#include <stdint.h>

int32_t libdep_loaded_marker = 0;

__attribute__((constructor))
static void libdep_ctor(void) {
    libdep_loaded_marker = 0xab;
}
