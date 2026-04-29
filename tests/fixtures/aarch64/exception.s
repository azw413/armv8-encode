.text
.globl _fixture_exception
_fixture_exception:
    svc #0
    svc #0x1234
    hvc #0x2345
    smc #0x3456
    brk #0x4567
    hlt #0x5678
    dcps1
    dcps1 #0x1111
    dcps2 #0x2222
    dcps3 #0x3333
