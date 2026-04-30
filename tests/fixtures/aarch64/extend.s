.text
.globl _fixture_extend
_fixture_extend:
    add x0, x1, w2, uxtb
    add x3, sp, w4, uxth #1
    adds x5, x6, w7, uxtw #2
    cmn x8, w9, sxtb #3
    sub x10, x11, x12, uxtx #4
    subs x13, x14, w15, sxth #1
    cmp sp, x16, sxtx #4
    sub sp, sp, x17, sxtx
