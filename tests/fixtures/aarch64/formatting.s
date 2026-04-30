.text
.globl _formatting
_formatting:
    adrp x8, #0xc000
    str xzr, [x21, #16]
    str wzr, [x8, #48]
    strb wzr, [x24, #208]
    stur w8, [x29, #-8]
    sturb w9, [x29, #-41]
    ldurh w0, [x29, #-94]
    csel x16, x16, xzr, ls
    mov w2, #0x7fffffff
    mov w0, #-1
    mov w10, #-0x412
    and w9, w25, #0xfffffffd
