.text
.globl _fixture_sys_generic
_fixture_sys_generic:
    sys #1, c2, c3, #4, x5
    sys #1, c2, c3, #4
    sysl x6, #1, c2, c3, #4
