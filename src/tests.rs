use crate::armv8::*;

#[cfg(test)]
mod tests {
    use crate::Instruction;
    use crate::Operands::*;
    use crate::OpCode::*;
    use crate::aarch64::disassemble;
    use crate::armv8::armv8;

    #[test]
    fn simple()
    {
        let bl = Instruction(BL, ADR26(-825));
        let adc = Instruction(ADC, R_R_R(1, 2, 4));
        println!("{:?}", bl);


        let instruction = 0b11010001000001100000000011011110; // ADCS X30, X24, X0
        let decoded = armv8::decode(instruction);
        println!("Decoded: {:?}", decoded);

        //let encoded = armv8::encode(&decoded);
        //println!("Encoded: {:#034b}", encoded.unwrap());
    }

    #[test]
    fn disassembly()
    {
        /*
        crypto_box_curve25519xsalsa20poly1305_tweet_keypair(unsigned char*, unsigned char*) (0x0045f128):
		0x0045f128: FC 6F BA A9        stp x28, x27, [sp, #-0x60]!
		0x0045f12c: FA 67 01 A9        stp x26, x25, [sp, #0x10]
		0x0045f130: F8 5F 02 A9        stp x24, x23, [sp, #0x20]
		0x0045f134: F6 57 03 A9        stp x22, x21, [sp, #0x30]

		0x0045f138: F4 4F 04 A9        stp x20, x19, [sp, #0x40]
		0x0045f13c: FD 7B 05 A9        stp x29, x30, [sp, #0x50]
		0x0045f140: FD 43 01 91        add x29, sp, #0x50
		0x0045f144: FF C3 22 D1        sub sp, sp, #0x8b0

		0x0045f148: F4 03 01 AA        mov x20, x1
		0x0045f14c: E0 03 00 F9        str x0, [sp]
		0x0045f150: 01 04 80 52        movz w1, #0x20
		0x0045f154: E0 03 14 AA        mov x0, x20

		0x0045f158: F3 03 14 91        add x19, sp, #0x500
		0x0045f15c: F5 03 0E 91        add x21, sp, #0x380
		0x0045f160: A0 08 2B 94        bl #0xf213e0 <dl_iterate_phdr +1059952 @ .cfo>
		0x0045f164: 80 F2 C0 3C        ldur q0, [x20, #0xf]

		0x0045f168: 81 02 C0 3D        ldr q1, [x20]
		0x0045f16c: 49 E8 FF F0        adrp x9, #0x16a000
		0x0045f170: 08 08 80 52        movz w8, #0x40
		0x0045f174: A0 F3 90 3C        stur q0, [x29, #0xffffffffffffff0f]

		0x0045f178: A1 03 90 3C        stur q1, [x29, #0xffffffffffffff00]
		0x0045f17c: 21 65 C2 3D        ldr q1, [x9, #0x990]
		0x0045f180: 89 7E 40 39        ldrb w9, [x20, #0x1f]
		0x0045f184: 00 E4 00 6F        movi v0.2d, #0000000000000000

		0x0045f188: 2F 00 80 52        movz w15, #0x1
		0x0045f18c: CC 1F 80 52        movz w12, #0xfe
		0x0045f190: 28 15 00 33        bfxil w8, w9, #0, #6
		0x0045f194: A8 F3 11 38        sturb w8, [x29, #0xffffffffffffff1f]

         */

        let data: [u32; 28] = [  0xFC6FBAA9, 0xFA6701A9, 0xF85F02A9, 0xF65703A9,
                                 0xF44F04A9, 0xFD7B05A9, 0xFD430191, 0xFFC322D1,
                                 0xF40301AA, 0xE00300F9, 0x01048052, 0xE00314AA,
                                 0xF3031491, 0xF5030E91, 0xA0082B94, 0x80F2C03C,
                                 0x8102C03D, 0x49E8FFF0, 0x08088052, 0xA0F3903C,
                                 0xA103903C, 0x2165C23D, 0x897E4039, 0x00E4006F,
                                 0x2F008052, 0xCC1F8052, 0x28150033, 0xA8F31138 ];

        let i = disassemble(&data);
        println!("{:} instructions", i.len());
        for i in i
        {
            println!("{:?}", i);
        }


        /*
        let i = Instruction::fromUInt32(data[10]);
        match i {
            Ok(i) => { println!("{:?}", i); }
            Err(e) => { panic!("{}", e.msg); }
        }

        for d in data {
            let i = Instruction::fromUInt32(d);
            match i {
                Ok(i) => { println!("{:?}", i); }
                Err(e) => { panic!("{}", e.msg); }
            }
        }
        */
    }

}