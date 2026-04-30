.text
.globl _simd_remaining
_simd_remaining:
    movi v0.8b, #0x12
    movi v1.4h, #0x34, lsl #8
    movi v2.2s, #0x56, lsl #16
    mvni v3.4h, #0x12, lsl #8
    fmov v4.2s, #1.0
    fmov v5.2d, #2.0
    movi d6, #0xff000000ff000000
    ext v7.16b, v8.16b, v9.16b, #5
    dup v10.4s, v11.s[2]
    smov w12, v13.b[7]
    umov x14, v15.d[1]
    mov v16.b[3], w17
    mov v18.s[1], v19.s[2]
    fmla v20.4s, v21.4s, v22.s[3]
    sshr v23.4s, v24.4s, #7
    shl v25.8h, v26.8h, #3
    sqshrun v27.8b, v28.8h, #4
    ld1 { v0.b }[7], [x0]
    ld2 { v1.h, v2.h }[3], [x1], #4
    st4 { v3.s, v4.s, v5.s, v6.s }[1], [x2], x3
    .inst 0x13862483
