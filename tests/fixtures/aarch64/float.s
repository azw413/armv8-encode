.text
.globl _fixture_float
_fixture_float:
    fadd s0, s1, s2
    fsub d3, d4, d5
    fmul s6, s7, s8
    fdiv d9, d10, d11
    fmadd s12, s13, s14, s15
    fmsub d16, d17, d18, d19
    fcmp s0, s1
    fcmp d2, #0.0
    fcsel s20, s21, s22, eq
    fmov s23, #1.0
    str s24, [x0, #4]
    ldr d25, [x1, #8]
