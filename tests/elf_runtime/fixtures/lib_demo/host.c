// Host program for the ELF runtime harness.
//
// Calls both functions in libgreet.so and prints results in a
// deterministic format. The harness asserts on stdout — any rewrite
// the .so undergoes that changes observable behaviour shows up here.
//
// Two invocation modes selected by argc:
//
//   ./host             prints both function results with the default
//                      `greet_base` value (100).
//
//   ./host x           prints only `greet_double(21)` — useful for
//                      isolating one function in a regression test.
#include <stdint.h>
#include <stdio.h>

int32_t greet_double(int32_t n);
int32_t greet_offset(int32_t n);

int main(int argc, char **argv) {
    (void)argv;
    int32_t doubled = greet_double(21);
    if (argc > 1) {
        printf("double=%d\n", doubled);
        return 0;
    }
    int32_t offset = greet_offset(7);
    printf("double=%d offset=%d\n", doubled, offset);
    return 0;
}
