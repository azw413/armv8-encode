.text
.globl _fixture_loadstore
_fixture_loadstore:
    str x0, [x1, #16]
    ldr x2, [x3, #24]
    stur x4, [x5, #-8]
    ldur x6, [x7, #-16]
    str x8, [x9], #8
    ldr x10, [x11, #8]!
    ldr x12, [x13, x14]
    ldr x15, [x16, x17, lsl #3]
    ldr w18, Lliteral
    ldrsw x19, Lliteral
    ldxr x20, [x21]
    stxr w22, x23, [x24]
Lliteral:
    .long 0x12345678
