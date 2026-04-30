.text
.globl _fixture_sys_alias
_fixture_sys_alias:
    at s1e1r, x0
    dc zva, x1
    ic ivau, x2
    ic iallu
    tlbi vale1, x3
    tlbi vmalle1
