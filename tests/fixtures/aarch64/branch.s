.text
.globl _fixture_branch
_fixture_branch:
    b Ltarget
    bl Ltarget
    b.eq Ltarget
    b.ne Ltarget
    cbz w0, Ltarget
    cbz x1, Ltarget
    cbnz w2, Ltarget
    cbnz x3, Ltarget
    tbz x0, #3, Ltarget
    tbnz x1, #4, Ltarget
    tbz x2, #40, Ltarget
    tbnz x3, #41, Ltarget
    br x2
    blr x3
    ret x4
    csel x5, x6, x7, eq
    cinc x8, x9, ne
    ccmp x10, x11, #0, lt
    ccmp x10, #7, #0, lt
Ltarget:
    ret
    eret
    drps
