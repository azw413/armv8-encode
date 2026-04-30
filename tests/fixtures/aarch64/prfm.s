.text
.globl _fixture_prfm
_fixture_prfm:
    prfm pldl1keep, [x0]
    prfm pldl2strm, [x1, #16]
    prfm #7, [x2]
Lliteral:
    prfm pldl1keep, Lliteral
