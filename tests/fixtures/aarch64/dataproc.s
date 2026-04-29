.text
.globl _fixture_dataproc
_fixture_dataproc:
    rbit w0, w1
    rbit x2, x3
    rev16 w4, w5
    rev16 x6, x7
    rev w8, w9
    rev x10, x11
    rev32 x12, x13
    clz w14, w15
    clz x16, x17
    cls w18, w19
    cls x20, x21
    udiv w18, w19, w20
    udiv x1, x2, x3
    sdiv w4, w5, w6
    sdiv x21, x22, x23
    lsl w24, w25, w26
    lsl x27, x28, x29
    lsr w30, w0, w1
    lsr x0, x1, x2
    asr w3, w4, w5
    asr x9, x10, x11
    ror w12, w13, w14
    ror x6, x7, x8
    crc32b w0, w1, w2
    crc32h w3, w4, w5
    crc32w w6, w7, w8
    crc32x w9, w10, x11
    crc32cb w12, w13, w14
    crc32ch w15, w16, w17
    crc32cw w18, w19, w20
    crc32cx w21, w22, x23
    madd w9, w10, w11, w12
    madd x0, x1, x2, x3
    msub w4, w5, w6, w7
    msub x13, x14, x15, x16
    mul w17, w18, w19
    mul x20, x21, x22
    mneg w23, w24, w25
    mneg x20, x21, x22
    smaddl x23, w24, w25, x26
    smsubl x27, w28, w29, x30
    smull x0, w1, w2
    smnegl x3, w4, w5
    smulh x6, x7, x8
    umaddl x9, w10, w11, x12
    umsubl x13, w14, w15, x16
    umull x17, w18, w19
    umnegl x20, w21, w22
    umulh x23, x24, x25
