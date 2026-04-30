.text
.globl _sum_u64
_sum_u64:
    cbz x1, Ldone
    mov x2, #0
Lloop:
    ldr x3, [x0], #8
    add x2, x2, x3
    subs x1, x1, #1
    b.ne Lloop
Ldone:
    mov x0, x2
    ret

.globl _select_and_scale
_select_and_scale:
    cmp w0, #0
    csel w2, w1, w0, gt
    lsl w0, w2, #3
    ret

.globl _vector_mix
_vector_mix:
    ld1 { v0.4s, v1.4s }, [x0]
    add v2.4s, v0.4s, v1.4s
    fmla v2.4s, v0.4s, v1.s[2]
    st1 { v2.4s }, [x1]
    ret
