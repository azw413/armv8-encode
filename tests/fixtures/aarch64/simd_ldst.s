.text
.globl _fixture_simd_ldst
_fixture_simd_ldst:
    ld1 {v0.16b}, [x1]
    st1 {v2.16b}, [x3], #16
    ld1 {v4.16b, v5.16b}, [x6], x7
    ld1r {v8.16b}, [x9]
    ld1r {v10.16b}, [x11], #1
