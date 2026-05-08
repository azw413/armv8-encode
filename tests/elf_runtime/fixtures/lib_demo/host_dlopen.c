// Host program that resolves a function in libgreet.so by name
// at runtime via dlopen + dlsym. Used by the runtime-harness
// test that exercises TextEditor::add_function_exported: we
// rewrite libgreet.so to add a new dynsym export, then run this
// host to confirm the dynamic linker can find the new symbol
// through the regenerated .gnu.hash.
//
// The host doesn't link against libgreet.so statically — that
// way `dlsym` is the only path to the new export, isolating the
// hash regeneration from any other lookup mechanism.
//
// Usage:
//   ./host_dlopen <symbol_name> <integer_arg>
//
// Prints either `result=<n>` on success or `dlerror: <message>`
// on failure, then exits.
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

    // Use $ORIGIN-style lookup: load libgreet.so from the same
    // directory as this binary. Matches the rpath the static
    // host uses, so the harness can place the rewritten library
    // next to either host.
    void *handle = dlopen("./libgreet.so", RTLD_NOW);
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
