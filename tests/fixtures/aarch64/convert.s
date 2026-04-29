.text
.globl _fixture_convert
_fixture_convert:
    scvtf s0, w1
    scvtf d2, x3
    ucvtf s4, w5
    ucvtf d6, x7
    fcvtzs w8, s9
    fcvtzu x10, d11
    fcvtns w12, s13
    fcvtnu x14, d15
    fcvtas w16, s17
    fcvtau x18, d19
    fcvtps w20, s21
    fcvtpu x22, d23
    fcvtms w24, s25
    fcvtmu x26, d27
    fmov w0, s1
    fmov x2, d3
    fmov s4, w5
    fmov d6, x7
    scvtf s8, w9, #8
    scvtf d10, x11, #16
    ucvtf s12, w13, #4
    ucvtf d14, x15, #32
    fcvtzs w16, s17, #8
    fcvtzu x18, d19, #16
