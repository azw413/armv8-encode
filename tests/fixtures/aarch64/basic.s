.text
.globl _fixture_basic
_fixture_basic:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    add x0, x0, #1
    sub x1, x1, #2
    adc x2, x2, x3
    cbz x0, Ldone
    bl _callee
Ldone:
    ldp x29, x30, [sp], #16
    ret
_callee:
    ret
