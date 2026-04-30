.text
.globl _fixture_system
_fixture_system:
    hint #19
    clrex
    clrex #3
    dsb sy
    dsb ish
    dmb ishst
    isb #3
    isb sy
    msr daifset, #2
    msr daifclr, #4
