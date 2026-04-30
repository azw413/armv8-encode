.text
.globl _fixture_fppair
_fixture_fppair:
    stp d0, d1, [x2, #16]
    ldp d3, d4, [x5, #-16]
    stp d6, d7, [x8], #16
    ldp d9, d10, [x11, #-16]!
    stnp d12, d13, [x14, #32]
    ldnp d15, d16, [x17, #-32]
