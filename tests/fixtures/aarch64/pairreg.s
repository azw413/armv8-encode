.text
.globl _fixture_pairreg
_fixture_pairreg:
    casp w0, w1, w2, w3, [x4]
    casp x6, x7, x8, x9, [x10]
