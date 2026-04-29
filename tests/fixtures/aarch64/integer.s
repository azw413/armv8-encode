.text
.globl _fixture_integer
_fixture_integer:
    add x4, x5, x6, lsl #3
    and x7, x7, #0xff
    eor x8, x8, #0xff00
    movz x9, #0x1234
    movk x9, #0xabcd, lsl #16
    movn x10, #0x55
    ubfx x11, x12, #8, #16
    bfxil x13, x14, #4, #12
    lsl x15, x16, #5
    lsr x17, x18, #6
    adr x19, Ltarget
Ltarget:
    nop
