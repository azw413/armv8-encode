// Runtime harness fixture.
//
// Three C functions plus a `.data` function-pointer array exercise the
// relocation patterns the rewriter must preserve through round-trip:
//
//   .text relocations:
//     - `bl answer` (intra-section call)      → R_AARCH64_CALL26
//     - `bl printf` (extern call)             → R_AARCH64_CALL26
//     - `adrp+add` for the printf format str  → R_AARCH64_ADR_PREL_PG_HI21
//                                               + R_AARCH64_ADD_ABS_LO12_NC
//     - `adrp+ldr` for the global counter     → R_AARCH64_ADR_PREL_PG_HI21
//                                               + R_AARCH64_LDST32_ABS_LO12_NC
//
//   data relocations:
//     - `funcs` table entries pointing at     → R_AARCH64_ABS64
//       answer / replacement
//
// `replacement` exists so a real-edit harness test can redirect the
// `bl answer` call to it via `RewritePlan::redirect_branch` and observe
// the output change from "answer=42\n" to "answer=99\n".
//
// `funcs[]` is a non-const data array of function pointers — placed in
// `.data` (SHT_PROGBITS, SHF_ALLOC|SHF_WRITE) so the rewriter sees both
// pointer slots paired with `R_AARCH64_ABS64` relocations. The Stage 3
// data-rebuild test redirects `funcs[0]` from `answer` → `replacement`.
//
// `static volatile` on `counter` keeps clang from constant-folding the
// load away at -O0.
#include <stdio.h>

static volatile int counter = 7;

int answer(void) {
    return counter * 6;
}

int replacement(void) {
    return 99;
}

int (*funcs[2])(void) = { answer, replacement };

int main(int argc, char **argv) {
    (void)argv;
    // `argc` selects which function pointer to invoke. Default (no
    // args) = funcs[0] = answer; pass any argument to call funcs[1].
    int index = (argc > 1) ? 1 : 0;
    printf("answer=%d\n", funcs[index]());
    return 0;
}
