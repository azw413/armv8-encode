// Mach-O variant of the ELF runtime fixture's host_dlopen.c.
// Resolves a function in libgreet.dylib by name at runtime via
// dlopen + dlsym. Used by the Phase 5 add_function_exported
// acceptance test.
//
// The host doesn't link against libgreet.dylib statically — so
// dlsym is the only path to the new export, isolating the
// export trie + symtab regeneration from any other lookup
// mechanism.
//
// Usage:
//   ./host_dlopen <symbol_name> <integer_arg>
//
// Prints `result=<n>` on success or `dlerror: <msg>` on failure.
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: host_dlopen <symbol_name> <integer_arg>\n");
        return 2;
    }
    const char *symbol_name = argv[1];
    int32_t arg = (int32_t)atoi(argv[2]);

    void *handle = dlopen("./libgreet.dylib", RTLD_NOW);
    if (!handle) {
        fprintf(stderr, "dlerror: %s\n", dlerror());
        return 1;
    }

    int32_t (*fn)(int32_t) = (int32_t (*)(int32_t))dlsym(handle, symbol_name);
    if (!fn) {
        fprintf(stderr, "dlerror: %s\n", dlerror());
        return 1;
    }

    printf("result=%d\n", fn(arg));
    return 0;
}
