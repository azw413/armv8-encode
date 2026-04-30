.text
.globl _fixture_simd_list
_fixture_simd_list:
    tbl v0.16b, {v1.16b}, v2.16b
    tbl v3.16b, {v4.16b, v5.16b}, v6.16b
