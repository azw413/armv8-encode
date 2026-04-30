.text
.globl _fixture_sysreg
_fixture_sysreg:
    mrs x0, nzcv
    msr nzcv, x1
    mrs x2, tpidr_el0
    msr tpidr_el0, x3
    mrs x4, cntvct_el0
