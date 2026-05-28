// AUTO-GENERATED — do not edit by hand.
//
// Regenerate with:
//
//   python3 tools/import_thumb_opcodes.py PATH_TO_arm-dis.c arm \
//       > src/isa/armv7/arm/table_generated.rs
//
// Source: GNU binutils opcodes/arm-dis.c, `arm_opcodes` array
// (32-bit ARM-mode instructions, GPL-2.0-or-later).
//
// Format strings are carried verbatim. The mnemonic enum is
// auto-generated from the union of distinct mnemonics in the
// table; PascalCase, with non-alphanumeric chars treated as
// separators (e.g. `vmla.f32` → `VmlaF32`).

#![allow(dead_code, non_camel_case_types)]

/// Mnemonic identifier. Auto-generated from binutils' arm_opcodes.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ArmMnemonicGenerated {
    /// `<unknown>`
    Unknown,
    /// `adc`
    Adc,
    /// `add`
    Add,
    /// `and`
    And,
    /// `asr`
    Asr,
    /// `b`
    B,
    /// `bfc`
    Bfc,
    /// `bfi`
    Bfi,
    /// `bic`
    Bic,
    /// `bkpt`
    Bkpt,
    /// `blx`
    Blx,
    /// `bx`
    Bx,
    /// `bxj`
    Bxj,
    /// `clrex`
    Clrex,
    /// `clz`
    Clz,
    /// `cmn`
    Cmn,
    /// `cmp`
    Cmp,
    /// `cps`
    Cps,
    /// `cpsid`
    Cpsid,
    /// `cpsie`
    Cpsie,
    /// `crc32b`
    Crc32b,
    /// `crc32cb`
    Crc32cb,
    /// `crc32ch`
    Crc32ch,
    /// `crc32cw`
    Crc32cw,
    /// `crc32h`
    Crc32h,
    /// `crc32w`
    Crc32w,
    /// `csdb`
    Csdb,
    /// `dbg`
    Dbg,
    /// `dfb`
    Dfb,
    /// `dmb`
    Dmb,
    /// `dsb`
    Dsb,
    /// `eor`
    Eor,
    /// `eret`
    Eret,
    /// `esb`
    Esb,
    /// `hlt`
    Hlt,
    /// `hvc`
    Hvc,
    /// `isb`
    Isb,
    /// `lda`
    Lda,
    /// `ldab`
    Ldab,
    /// `ldaex`
    Ldaex,
    /// `ldaexb`
    Ldaexb,
    /// `ldaexd`
    Ldaexd,
    /// `ldaexh`
    Ldaexh,
    /// `ldah`
    Ldah,
    /// `ldm`
    Ldm,
    /// `ldmfd`
    Ldmfd,
    /// `ldr`
    Ldr,
    /// `ldrb`
    Ldrb,
    /// `ldrd`
    Ldrd,
    /// `ldrex`
    Ldrex,
    /// `ldrexb`
    Ldrexb,
    /// `ldrexd`
    Ldrexd,
    /// `ldrexh`
    Ldrexh,
    /// `ldrt`
    Ldrt,
    /// `lsl`
    Lsl,
    /// `lsr`
    Lsr,
    /// `mla`
    Mla,
    /// `mls`
    Mls,
    /// `mov`
    Mov,
    /// `movt`
    Movt,
    /// `movw`
    Movw,
    /// `mrs`
    Mrs,
    /// `msr`
    Msr,
    /// `mul`
    Mul,
    /// `mvn`
    Mvn,
    /// `nop`
    Nop,
    /// `orr`
    Orr,
    /// `pkhbt`
    Pkhbt,
    /// `pkhtb`
    Pkhtb,
    /// `pld`
    Pld,
    /// `pldw`
    Pldw,
    /// `pli`
    Pli,
    /// `pop`
    Pop,
    /// `pssbb`
    Pssbb,
    /// `push`
    Push,
    /// `qadd`
    Qadd,
    /// `qadd16`
    Qadd16,
    /// `qadd8`
    Qadd8,
    /// `qasx`
    Qasx,
    /// `qdadd`
    Qdadd,
    /// `qdsub`
    Qdsub,
    /// `qsax`
    Qsax,
    /// `qsub`
    Qsub,
    /// `qsub16`
    Qsub16,
    /// `qsub8`
    Qsub8,
    /// `rbit`
    Rbit,
    /// `rev`
    Rev,
    /// `rev16`
    Rev16,
    /// `revsh`
    Revsh,
    /// `rfe`
    Rfe,
    /// `ror`
    Ror,
    /// `rrx`
    Rrx,
    /// `rsb`
    Rsb,
    /// `rsc`
    Rsc,
    /// `sadd16`
    Sadd16,
    /// `sadd8`
    Sadd8,
    /// `sasx`
    Sasx,
    /// `sb`
    Sb,
    /// `sbc`
    Sbc,
    /// `sdiv`
    Sdiv,
    /// `sel`
    Sel,
    /// `setend`
    Setend,
    /// `setpan`
    Setpan,
    /// `sev`
    Sev,
    /// `sevl`
    Sevl,
    /// `shadd16`
    Shadd16,
    /// `shadd8`
    Shadd8,
    /// `shasx`
    Shasx,
    /// `shsax`
    Shsax,
    /// `shsub16`
    Shsub16,
    /// `shsub8`
    Shsub8,
    /// `smc`
    Smc,
    /// `smlabb`
    Smlabb,
    /// `smlabt`
    Smlabt,
    /// `smlad`
    Smlad,
    /// `smlalbb`
    Smlalbb,
    /// `smlalbt`
    Smlalbt,
    /// `smlald`
    Smlald,
    /// `smlaltb`
    Smlaltb,
    /// `smlaltt`
    Smlaltt,
    /// `smlatb`
    Smlatb,
    /// `smlatt`
    Smlatt,
    /// `smlawb`
    Smlawb,
    /// `smlawt`
    Smlawt,
    /// `smlsd`
    Smlsd,
    /// `smlsld`
    Smlsld,
    /// `smmla`
    Smmla,
    /// `smmls`
    Smmls,
    /// `smmul`
    Smmul,
    /// `smuad`
    Smuad,
    /// `smulbb`
    Smulbb,
    /// `smulbt`
    Smulbt,
    /// `smultb`
    Smultb,
    /// `smultt`
    Smultt,
    /// `smulwb`
    Smulwb,
    /// `smulwt`
    Smulwt,
    /// `smusd`
    Smusd,
    /// `srs`
    Srs,
    /// `ssat`
    Ssat,
    /// `ssat16`
    Ssat16,
    /// `ssax`
    Ssax,
    /// `ssbb`
    Ssbb,
    /// `ssub16`
    Ssub16,
    /// `ssub8`
    Ssub8,
    /// `stl`
    Stl,
    /// `stlb`
    Stlb,
    /// `stlex`
    Stlex,
    /// `stlexb`
    Stlexb,
    /// `stlexd`
    Stlexd,
    /// `stlexh`
    Stlexh,
    /// `stlh`
    Stlh,
    /// `stm`
    Stm,
    /// `stmfd`
    Stmfd,
    /// `str`
    Str,
    /// `strb`
    Strb,
    /// `strd`
    Strd,
    /// `strex`
    Strex,
    /// `strexb`
    Strexb,
    /// `strexd`
    Strexd,
    /// `strexh`
    Strexh,
    /// `strh`
    Strh,
    /// `strht`
    Strht,
    /// `sub`
    Sub,
    /// `svc`
    Svc,
    /// `swp`
    Swp,
    /// `sxtab`
    Sxtab,
    /// `sxtab16`
    Sxtab16,
    /// `sxtah`
    Sxtah,
    /// `sxtb`
    Sxtb,
    /// `sxtb16`
    Sxtb16,
    /// `sxth`
    Sxth,
    /// `teq`
    Teq,
    /// `tst`
    Tst,
    /// `uadd16`
    Uadd16,
    /// `uadd8`
    Uadd8,
    /// `uasx`
    Uasx,
    /// `udf`
    Udf,
    /// `udiv`
    Udiv,
    /// `uhadd16`
    Uhadd16,
    /// `uhadd8`
    Uhadd8,
    /// `uhasx`
    Uhasx,
    /// `uhsax`
    Uhsax,
    /// `uhsub16`
    Uhsub16,
    /// `uhsub8`
    Uhsub8,
    /// `umaal`
    Umaal,
    /// `uqadd16`
    Uqadd16,
    /// `uqadd8`
    Uqadd8,
    /// `uqasx`
    Uqasx,
    /// `uqsax`
    Uqsax,
    /// `uqsub16`
    Uqsub16,
    /// `uqsub8`
    Uqsub8,
    /// `usad8`
    Usad8,
    /// `usada8`
    Usada8,
    /// `usat`
    Usat,
    /// `usat16`
    Usat16,
    /// `usax`
    Usax,
    /// `usub16`
    Usub16,
    /// `usub8`
    Usub8,
    /// `uxtab`
    Uxtab,
    /// `uxtab16`
    Uxtab16,
    /// `uxtah`
    Uxtah,
    /// `uxtb`
    Uxtb,
    /// `uxtb16`
    Uxtb16,
    /// `uxth`
    Uxth,
    /// `wfe`
    Wfe,
    /// `wfi`
    Wfi,
    /// `yield`
    Yield,
}

impl ArmMnemonicGenerated {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "<unknown>",
            Self::Adc => "adc",
            Self::Add => "add",
            Self::And => "and",
            Self::Asr => "asr",
            Self::B => "b",
            Self::Bfc => "bfc",
            Self::Bfi => "bfi",
            Self::Bic => "bic",
            Self::Bkpt => "bkpt",
            Self::Blx => "blx",
            Self::Bx => "bx",
            Self::Bxj => "bxj",
            Self::Clrex => "clrex",
            Self::Clz => "clz",
            Self::Cmn => "cmn",
            Self::Cmp => "cmp",
            Self::Cps => "cps",
            Self::Cpsid => "cpsid",
            Self::Cpsie => "cpsie",
            Self::Crc32b => "crc32b",
            Self::Crc32cb => "crc32cb",
            Self::Crc32ch => "crc32ch",
            Self::Crc32cw => "crc32cw",
            Self::Crc32h => "crc32h",
            Self::Crc32w => "crc32w",
            Self::Csdb => "csdb",
            Self::Dbg => "dbg",
            Self::Dfb => "dfb",
            Self::Dmb => "dmb",
            Self::Dsb => "dsb",
            Self::Eor => "eor",
            Self::Eret => "eret",
            Self::Esb => "esb",
            Self::Hlt => "hlt",
            Self::Hvc => "hvc",
            Self::Isb => "isb",
            Self::Lda => "lda",
            Self::Ldab => "ldab",
            Self::Ldaex => "ldaex",
            Self::Ldaexb => "ldaexb",
            Self::Ldaexd => "ldaexd",
            Self::Ldaexh => "ldaexh",
            Self::Ldah => "ldah",
            Self::Ldm => "ldm",
            Self::Ldmfd => "ldmfd",
            Self::Ldr => "ldr",
            Self::Ldrb => "ldrb",
            Self::Ldrd => "ldrd",
            Self::Ldrex => "ldrex",
            Self::Ldrexb => "ldrexb",
            Self::Ldrexd => "ldrexd",
            Self::Ldrexh => "ldrexh",
            Self::Ldrt => "ldrt",
            Self::Lsl => "lsl",
            Self::Lsr => "lsr",
            Self::Mla => "mla",
            Self::Mls => "mls",
            Self::Mov => "mov",
            Self::Movt => "movt",
            Self::Movw => "movw",
            Self::Mrs => "mrs",
            Self::Msr => "msr",
            Self::Mul => "mul",
            Self::Mvn => "mvn",
            Self::Nop => "nop",
            Self::Orr => "orr",
            Self::Pkhbt => "pkhbt",
            Self::Pkhtb => "pkhtb",
            Self::Pld => "pld",
            Self::Pldw => "pldw",
            Self::Pli => "pli",
            Self::Pop => "pop",
            Self::Pssbb => "pssbb",
            Self::Push => "push",
            Self::Qadd => "qadd",
            Self::Qadd16 => "qadd16",
            Self::Qadd8 => "qadd8",
            Self::Qasx => "qasx",
            Self::Qdadd => "qdadd",
            Self::Qdsub => "qdsub",
            Self::Qsax => "qsax",
            Self::Qsub => "qsub",
            Self::Qsub16 => "qsub16",
            Self::Qsub8 => "qsub8",
            Self::Rbit => "rbit",
            Self::Rev => "rev",
            Self::Rev16 => "rev16",
            Self::Revsh => "revsh",
            Self::Rfe => "rfe",
            Self::Ror => "ror",
            Self::Rrx => "rrx",
            Self::Rsb => "rsb",
            Self::Rsc => "rsc",
            Self::Sadd16 => "sadd16",
            Self::Sadd8 => "sadd8",
            Self::Sasx => "sasx",
            Self::Sb => "sb",
            Self::Sbc => "sbc",
            Self::Sdiv => "sdiv",
            Self::Sel => "sel",
            Self::Setend => "setend",
            Self::Setpan => "setpan",
            Self::Sev => "sev",
            Self::Sevl => "sevl",
            Self::Shadd16 => "shadd16",
            Self::Shadd8 => "shadd8",
            Self::Shasx => "shasx",
            Self::Shsax => "shsax",
            Self::Shsub16 => "shsub16",
            Self::Shsub8 => "shsub8",
            Self::Smc => "smc",
            Self::Smlabb => "smlabb",
            Self::Smlabt => "smlabt",
            Self::Smlad => "smlad",
            Self::Smlalbb => "smlalbb",
            Self::Smlalbt => "smlalbt",
            Self::Smlald => "smlald",
            Self::Smlaltb => "smlaltb",
            Self::Smlaltt => "smlaltt",
            Self::Smlatb => "smlatb",
            Self::Smlatt => "smlatt",
            Self::Smlawb => "smlawb",
            Self::Smlawt => "smlawt",
            Self::Smlsd => "smlsd",
            Self::Smlsld => "smlsld",
            Self::Smmla => "smmla",
            Self::Smmls => "smmls",
            Self::Smmul => "smmul",
            Self::Smuad => "smuad",
            Self::Smulbb => "smulbb",
            Self::Smulbt => "smulbt",
            Self::Smultb => "smultb",
            Self::Smultt => "smultt",
            Self::Smulwb => "smulwb",
            Self::Smulwt => "smulwt",
            Self::Smusd => "smusd",
            Self::Srs => "srs",
            Self::Ssat => "ssat",
            Self::Ssat16 => "ssat16",
            Self::Ssax => "ssax",
            Self::Ssbb => "ssbb",
            Self::Ssub16 => "ssub16",
            Self::Ssub8 => "ssub8",
            Self::Stl => "stl",
            Self::Stlb => "stlb",
            Self::Stlex => "stlex",
            Self::Stlexb => "stlexb",
            Self::Stlexd => "stlexd",
            Self::Stlexh => "stlexh",
            Self::Stlh => "stlh",
            Self::Stm => "stm",
            Self::Stmfd => "stmfd",
            Self::Str => "str",
            Self::Strb => "strb",
            Self::Strd => "strd",
            Self::Strex => "strex",
            Self::Strexb => "strexb",
            Self::Strexd => "strexd",
            Self::Strexh => "strexh",
            Self::Strh => "strh",
            Self::Strht => "strht",
            Self::Sub => "sub",
            Self::Svc => "svc",
            Self::Swp => "swp",
            Self::Sxtab => "sxtab",
            Self::Sxtab16 => "sxtab16",
            Self::Sxtah => "sxtah",
            Self::Sxtb => "sxtb",
            Self::Sxtb16 => "sxtb16",
            Self::Sxth => "sxth",
            Self::Teq => "teq",
            Self::Tst => "tst",
            Self::Uadd16 => "uadd16",
            Self::Uadd8 => "uadd8",
            Self::Uasx => "uasx",
            Self::Udf => "udf",
            Self::Udiv => "udiv",
            Self::Uhadd16 => "uhadd16",
            Self::Uhadd8 => "uhadd8",
            Self::Uhasx => "uhasx",
            Self::Uhsax => "uhsax",
            Self::Uhsub16 => "uhsub16",
            Self::Uhsub8 => "uhsub8",
            Self::Umaal => "umaal",
            Self::Uqadd16 => "uqadd16",
            Self::Uqadd8 => "uqadd8",
            Self::Uqasx => "uqasx",
            Self::Uqsax => "uqsax",
            Self::Uqsub16 => "uqsub16",
            Self::Uqsub8 => "uqsub8",
            Self::Usad8 => "usad8",
            Self::Usada8 => "usada8",
            Self::Usat => "usat",
            Self::Usat16 => "usat16",
            Self::Usax => "usax",
            Self::Usub16 => "usub16",
            Self::Usub8 => "usub8",
            Self::Uxtab => "uxtab",
            Self::Uxtab16 => "uxtab16",
            Self::Uxtah => "uxtah",
            Self::Uxtb => "uxtb",
            Self::Uxtb16 => "uxtb16",
            Self::Uxth => "uxth",
            Self::Wfe => "wfe",
            Self::Wfi => "wfi",
            Self::Yield => "yield",
        }
    }

    /// Display alias for the canonical mnemonic, when the
    /// disassembler renders the instruction with a different
    /// name than [`Self::as_str`]. Returns `None` for the
    /// vast majority of ARM-mode mnemonics — the condition
    /// suffix (`bne`, `addseq`, etc.) is driven by the
    /// format string's `%c`, not by a separate alias
    /// mnemonic. Provided for API symmetry with
    /// `aarch64::Aarch64Mnemonic::display_alias`.
    pub fn display_alias(&self) -> Option<&'static str> {
        None
    }
}

/// Auto-generated table: every ARM-mode instruction binutils 2.41 recognises.
pub static ARM_OPCODE_TABLE_GENERATED: &[ArmOpcodeGenerated] = &[
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Nop, opcode: 0xe1a00000, mask: 0xffffffff, format: "nop\t\t\t@ (mov r0, r0)" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Udf, opcode: 0xe7f000f0, mask: 0xfff000f0, format: "udf\t%{I:#%e%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bx, opcode: 0x012fff10, mask: 0x0ffffff0, format: "bx%c\t%0-3r" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mul, opcode: 0x00000090, mask: 0x0fe000f0, format: "mul%20's%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mla, opcode: 0x00200090, mask: 0x0fe000f0, format: "mla%20's%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Swp, opcode: 0x01000090, mask: 0x0fb00ff0, format: "swp%22'b%c\t%12-15RU, %0-3Ru, [%16-19RuU]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Unknown, opcode: 0x00800090, mask: 0x0fa000f0, format: "%22?sumull%20's%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Unknown, opcode: 0x00a00090, mask: 0x0fa000f0, format: "%22?sumlal%20's%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Esb, opcode: 0xe320f010, mask: 0xffffffff, format: "esb" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Dfb, opcode: 0xf57ff04c, mask: 0xffffffff, format: "dfb" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sevl, opcode: 0x0320f005, mask: 0x0fffffff, format: "sevl" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Hlt, opcode: 0xe1000070, mask: 0xfff000f0, format: "hlt\t%{I:0x%16-19X%12-15X%8-11X%0-3X%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stlex, opcode: 0x01800e90, mask: 0x0ff00ff0, format: "stlex%c\t%12-15r, %0-3r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldaex, opcode: 0x01900e9f, mask: 0x0ff00fff, format: "ldaex%c\t%12-15r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stlexd, opcode: 0x01a00e90, mask: 0x0ff00ff0, format: "stlexd%c\t%12-15r, %0-3r, %0-3T, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldaexd, opcode: 0x01b00e9f, mask: 0x0ff00fff, format: "ldaexd%c\t%12-15r, %12-15T, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stlexb, opcode: 0x01c00e90, mask: 0x0ff00ff0, format: "stlexb%c\t%12-15r, %0-3r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldaexb, opcode: 0x01d00e9f, mask: 0x0ff00fff, format: "ldaexb%c\t%12-15r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stlexh, opcode: 0x01e00e90, mask: 0x0ff00ff0, format: "stlexh%c\t%12-15r, %0-3r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldaexh, opcode: 0x01f00e9f, mask: 0x0ff00fff, format: "ldaexh%c\t%12-15r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stl, opcode: 0x0180fc90, mask: 0x0ff0fff0, format: "stl%c\t%0-3r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Lda, opcode: 0x01900c9f, mask: 0x0ff00fff, format: "lda%c\t%12-15r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stlb, opcode: 0x01c0fc90, mask: 0x0ff0fff0, format: "stlb%c\t%0-3r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldab, opcode: 0x01d00c9f, mask: 0x0ff00fff, format: "ldab%c\t%12-15r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stlh, opcode: 0x01e0fc90, mask: 0x0ff0fff0, format: "stlh%c\t%0-3r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldah, opcode: 0x01f00c9f, mask: 0x0ff00fff, format: "ldah%c\t%12-15r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Crc32b, opcode: 0xe1000040, mask: 0xfff00ff0, format: "crc32b\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Crc32h, opcode: 0xe1200040, mask: 0xfff00ff0, format: "crc32h\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Crc32w, opcode: 0xe1400040, mask: 0xfff00ff0, format: "crc32w\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Crc32cb, opcode: 0xe1000240, mask: 0xfff00ff0, format: "crc32cb\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Crc32ch, opcode: 0xe1200240, mask: 0xfff00ff0, format: "crc32ch\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Crc32cw, opcode: 0xe1400240, mask: 0xfff00ff0, format: "crc32cw\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Setpan, opcode: 0xf1100000, mask: 0xfffffdff, format: "setpan\t%{I:#%9-9d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Eret, opcode: 0x0160006e, mask: 0x0fffffff, format: "eret%c" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Hvc, opcode: 0x01400070, mask: 0x0ff000f0, format: "hvc%c\t%e" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sdiv, opcode: 0x0710f010, mask: 0x0ff0f0f0, format: "sdiv%c\t%16-19r, %0-3r, %8-11r" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Udiv, opcode: 0x0730f010, mask: 0x0ff0f0f0, format: "udiv%c\t%16-19r, %0-3r, %8-11r" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pldw, opcode: 0xf410f000, mask: 0xfc70f000, format: "pldw\t%a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Csdb, opcode: 0xe320f014, mask: 0xffffffff, format: "csdb" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssbb, opcode: 0xf57ff040, mask: 0xffffffff, format: "ssbb" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pssbb, opcode: 0xf57ff044, mask: 0xffffffff, format: "pssbb" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pli, opcode: 0xf450f000, mask: 0xfd70f000, format: "pli\t%P" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Dbg, opcode: 0x0320f0f0, mask: 0x0ffffff0, format: "dbg%c\t%{I:#%0-3d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Dmb, opcode: 0xf57ff051, mask: 0xfffffff3, format: "dmb\t%U" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Dsb, opcode: 0xf57ff041, mask: 0xfffffff3, format: "dsb\t%U" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Dmb, opcode: 0xf57ff050, mask: 0xfffffff0, format: "dmb\t%U" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Dsb, opcode: 0xf57ff040, mask: 0xfffffff0, format: "dsb\t%U" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Isb, opcode: 0xf57ff060, mask: 0xfffffff0, format: "isb\t%U" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Nop, opcode: 0x0320f000, mask: 0x0fffffff, format: "nop%c\t{%{I:%0-7d%}}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bfc, opcode: 0x07c0001f, mask: 0x0fe0007f, format: "bfc%c\t%12-15R, %E" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bfi, opcode: 0x07c00010, mask: 0x0fe00070, format: "bfi%c\t%12-15R, %0-3r, %E" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mls, opcode: 0x00600090, mask: 0x0ff000f0, format: "mls%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strht, opcode: 0x002000b0, mask: 0x0f3000f0, format: "strht%c\t%12-15R, %S" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldr, opcode: 0x00300090, mask: 0x0f300090, format: "ldr%6's%5?hbt%c\t%12-15R, %S" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Movw, opcode: 0x03000000, mask: 0x0ff00000, format: "movw%c\t%12-15R, %V" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Movt, opcode: 0x03400000, mask: 0x0ff00000, format: "movt%c\t%12-15R, %V" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rbit, opcode: 0x06ff0f30, mask: 0x0fff0ff0, format: "rbit%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Unknown, opcode: 0x07a00050, mask: 0x0fa00070, format: "%22?usbfx%c\t%12-15r, %0-3r, %{I:#%7-11d%}, %{I:#%16-20W%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smc, opcode: 0x01600070, mask: 0x0ff000f0, format: "smc%c\t%e" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Clrex, opcode: 0xf57ff01f, mask: 0xffffffff, format: "clrex" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldrexb, opcode: 0x01d00f9f, mask: 0x0ff00fff, format: "ldrexb%c\t%12-15R, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldrexd, opcode: 0x01b00f9f, mask: 0x0ff00fff, format: "ldrexd%c\t%12-15r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldrexh, opcode: 0x01f00f9f, mask: 0x0ff00fff, format: "ldrexh%c\t%12-15R, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strexb, opcode: 0x01c00f90, mask: 0x0ff00ff0, format: "strexb%c\t%12-15R, %0-3R, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strexd, opcode: 0x01a00f90, mask: 0x0ff00ff0, format: "strexd%c\t%12-15R, %0-3r, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strexh, opcode: 0x01e00f90, mask: 0x0ff00ff0, format: "strexh%c\t%12-15R, %0-3R, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sb, opcode: 0xf57ff070, mask: 0xffffffff, format: "sb" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Yield, opcode: 0x0320f001, mask: 0x0fffffff, format: "yield%c" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Wfe, opcode: 0x0320f002, mask: 0x0fffffff, format: "wfe%c" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Wfi, opcode: 0x0320f003, mask: 0x0fffffff, format: "wfi%c" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sev, opcode: 0x0320f004, mask: 0x0fffffff, format: "sev%c" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Nop, opcode: 0x0320f000, mask: 0x0fffff00, format: "nop%c\t{%{I:%0-7d%}}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cpsie, opcode: 0xf1080000, mask: 0xfffffe3f, format: "cpsie\t%{B:%8'a%7'i%6'f%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cpsie, opcode: 0xf10a0000, mask: 0xfffffe20, format: "cpsie\t%{B:%8'a%7'i%6'f%}, %{I:#%0-4d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cpsid, opcode: 0xf10c0000, mask: 0xfffffe3f, format: "cpsid\t%{B:%8'a%7'i%6'f%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cpsid, opcode: 0xf10e0000, mask: 0xfffffe20, format: "cpsid\t%{B:%8'a%7'i%6'f%}, %{I:#%0-4d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cps, opcode: 0xf1000000, mask: 0xfff1fe20, format: "cps\t%{I:#%0-4d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pkhbt, opcode: 0x06800010, mask: 0x0ff00ff0, format: "pkhbt%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pkhbt, opcode: 0x06800010, mask: 0x0ff00070, format: "pkhbt%c\t%12-15R, %16-19R, %0-3R, %{B:lsl%} %{I:#%7-11d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pkhtb, opcode: 0x06800050, mask: 0x0ff00ff0, format: "pkhtb%c\t%12-15R, %16-19R, %0-3R, %{B:asr%} %{I:#32%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pkhtb, opcode: 0x06800050, mask: 0x0ff00070, format: "pkhtb%c\t%12-15R, %16-19R, %0-3R, %{B:asr%} %{I:#%7-11d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldrex, opcode: 0x01900f9f, mask: 0x0ff00fff, format: "ldrex%c\t%{R:r%12-15d%}, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qadd16, opcode: 0x06200f10, mask: 0x0ff00ff0, format: "qadd16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qadd8, opcode: 0x06200f90, mask: 0x0ff00ff0, format: "qadd8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qasx, opcode: 0x06200f30, mask: 0x0ff00ff0, format: "qasx%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qsub16, opcode: 0x06200f70, mask: 0x0ff00ff0, format: "qsub16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qsub8, opcode: 0x06200ff0, mask: 0x0ff00ff0, format: "qsub8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qsax, opcode: 0x06200f50, mask: 0x0ff00ff0, format: "qsax%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sadd16, opcode: 0x06100f10, mask: 0x0ff00ff0, format: "sadd16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sadd8, opcode: 0x06100f90, mask: 0x0ff00ff0, format: "sadd8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sasx, opcode: 0x06100f30, mask: 0x0ff00ff0, format: "sasx%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Shadd16, opcode: 0x06300f10, mask: 0x0ff00ff0, format: "shadd16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Shadd8, opcode: 0x06300f90, mask: 0x0ff00ff0, format: "shadd8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Shasx, opcode: 0x06300f30, mask: 0x0ff00ff0, format: "shasx%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Shsub16, opcode: 0x06300f70, mask: 0x0ff00ff0, format: "shsub16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Shsub8, opcode: 0x06300ff0, mask: 0x0ff00ff0, format: "shsub8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Shsax, opcode: 0x06300f50, mask: 0x0ff00ff0, format: "shsax%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssub16, opcode: 0x06100f70, mask: 0x0ff00ff0, format: "ssub16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssub8, opcode: 0x06100ff0, mask: 0x0ff00ff0, format: "ssub8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssax, opcode: 0x06100f50, mask: 0x0ff00ff0, format: "ssax%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uadd16, opcode: 0x06500f10, mask: 0x0ff00ff0, format: "uadd16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uadd8, opcode: 0x06500f90, mask: 0x0ff00ff0, format: "uadd8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uasx, opcode: 0x06500f30, mask: 0x0ff00ff0, format: "uasx%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uhadd16, opcode: 0x06700f10, mask: 0x0ff00ff0, format: "uhadd16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uhadd8, opcode: 0x06700f90, mask: 0x0ff00ff0, format: "uhadd8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uhasx, opcode: 0x06700f30, mask: 0x0ff00ff0, format: "uhasx%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uhsub16, opcode: 0x06700f70, mask: 0x0ff00ff0, format: "uhsub16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uhsub8, opcode: 0x06700ff0, mask: 0x0ff00ff0, format: "uhsub8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uhsax, opcode: 0x06700f50, mask: 0x0ff00ff0, format: "uhsax%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uqadd16, opcode: 0x06600f10, mask: 0x0ff00ff0, format: "uqadd16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uqadd8, opcode: 0x06600f90, mask: 0x0ff00ff0, format: "uqadd8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uqasx, opcode: 0x06600f30, mask: 0x0ff00ff0, format: "uqasx%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uqsub16, opcode: 0x06600f70, mask: 0x0ff00ff0, format: "uqsub16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uqsub8, opcode: 0x06600ff0, mask: 0x0ff00ff0, format: "uqsub8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uqsax, opcode: 0x06600f50, mask: 0x0ff00ff0, format: "uqsax%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usub16, opcode: 0x06500f70, mask: 0x0ff00ff0, format: "usub16%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usub8, opcode: 0x06500ff0, mask: 0x0ff00ff0, format: "usub8%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usax, opcode: 0x06500f50, mask: 0x0ff00ff0, format: "usax%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rev, opcode: 0x06bf0f30, mask: 0x0fff0ff0, format: "rev%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rev16, opcode: 0x06bf0fb0, mask: 0x0fff0ff0, format: "rev16%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Revsh, opcode: 0x06ff0fb0, mask: 0x0fff0ff0, format: "revsh%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rfe, opcode: 0xf8100a00, mask: 0xfe50ffff, format: "rfe%23?id%24?ba\t%16-19r%21'!" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxth, opcode: 0x06bf0070, mask: 0x0fff0ff0, format: "sxth%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxth, opcode: 0x06bf0470, mask: 0x0fff0ff0, format: "sxth%c\t%12-15R, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxth, opcode: 0x06bf0870, mask: 0x0fff0ff0, format: "sxth%c\t%12-15R, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxth, opcode: 0x06bf0c70, mask: 0x0fff0ff0, format: "sxth%c\t%12-15R, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb16, opcode: 0x068f0070, mask: 0x0fff0ff0, format: "sxtb16%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb16, opcode: 0x068f0470, mask: 0x0fff0ff0, format: "sxtb16%c\t%12-15R, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb16, opcode: 0x068f0870, mask: 0x0fff0ff0, format: "sxtb16%c\t%12-15R, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb16, opcode: 0x068f0c70, mask: 0x0fff0ff0, format: "sxtb16%c\t%12-15R, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb, opcode: 0x06af0070, mask: 0x0fff0ff0, format: "sxtb%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb, opcode: 0x06af0470, mask: 0x0fff0ff0, format: "sxtb%c\t%12-15R, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb, opcode: 0x06af0870, mask: 0x0fff0ff0, format: "sxtb%c\t%12-15R, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtb, opcode: 0x06af0c70, mask: 0x0fff0ff0, format: "sxtb%c\t%12-15R, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxth, opcode: 0x06ff0070, mask: 0x0fff0ff0, format: "uxth%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxth, opcode: 0x06ff0470, mask: 0x0fff0ff0, format: "uxth%c\t%12-15R, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxth, opcode: 0x06ff0870, mask: 0x0fff0ff0, format: "uxth%c\t%12-15R, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxth, opcode: 0x06ff0c70, mask: 0x0fff0ff0, format: "uxth%c\t%12-15R, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb16, opcode: 0x06cf0070, mask: 0x0fff0ff0, format: "uxtb16%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb16, opcode: 0x06cf0470, mask: 0x0fff0ff0, format: "uxtb16%c\t%12-15R, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb16, opcode: 0x06cf0870, mask: 0x0fff0ff0, format: "uxtb16%c\t%12-15R, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb16, opcode: 0x06cf0c70, mask: 0x0fff0ff0, format: "uxtb16%c\t%12-15R, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb, opcode: 0x06ef0070, mask: 0x0fff0ff0, format: "uxtb%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb, opcode: 0x06ef0470, mask: 0x0fff0ff0, format: "uxtb%c\t%12-15R, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb, opcode: 0x06ef0870, mask: 0x0fff0ff0, format: "uxtb%c\t%12-15R, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtb, opcode: 0x06ef0c70, mask: 0x0fff0ff0, format: "uxtb%c\t%12-15R, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtah, opcode: 0x06b00070, mask: 0x0ff00ff0, format: "sxtah%c\t%12-15R, %16-19r, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtah, opcode: 0x06b00470, mask: 0x0ff00ff0, format: "sxtah%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtah, opcode: 0x06b00870, mask: 0x0ff00ff0, format: "sxtah%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtah, opcode: 0x06b00c70, mask: 0x0ff00ff0, format: "sxtah%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab16, opcode: 0x06800070, mask: 0x0ff00ff0, format: "sxtab16%c\t%12-15R, %16-19r, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab16, opcode: 0x06800470, mask: 0x0ff00ff0, format: "sxtab16%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab16, opcode: 0x06800870, mask: 0x0ff00ff0, format: "sxtab16%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab16, opcode: 0x06800c70, mask: 0x0ff00ff0, format: "sxtab16%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab, opcode: 0x06a00070, mask: 0x0ff00ff0, format: "sxtab%c\t%12-15R, %16-19r, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab, opcode: 0x06a00470, mask: 0x0ff00ff0, format: "sxtab%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab, opcode: 0x06a00870, mask: 0x0ff00ff0, format: "sxtab%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sxtab, opcode: 0x06a00c70, mask: 0x0ff00ff0, format: "sxtab%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtah, opcode: 0x06f00070, mask: 0x0ff00ff0, format: "uxtah%c\t%12-15R, %16-19r, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtah, opcode: 0x06f00470, mask: 0x0ff00ff0, format: "uxtah%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtah, opcode: 0x06f00870, mask: 0x0ff00ff0, format: "uxtah%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtah, opcode: 0x06f00c70, mask: 0x0ff00ff0, format: "uxtah%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab16, opcode: 0x06c00070, mask: 0x0ff00ff0, format: "uxtab16%c\t%12-15R, %16-19r, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab16, opcode: 0x06c00470, mask: 0x0ff00ff0, format: "uxtab16%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab16, opcode: 0x06c00870, mask: 0x0ff00ff0, format: "uxtab16%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab16, opcode: 0x06c00c70, mask: 0x0ff00ff0, format: "uxtab16%c\t%12-15R, %16-19r, %0-3R, ROR %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab, opcode: 0x06e00070, mask: 0x0ff00ff0, format: "uxtab%c\t%12-15R, %16-19r, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab, opcode: 0x06e00470, mask: 0x0ff00ff0, format: "uxtab%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#8%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab, opcode: 0x06e00870, mask: 0x0ff00ff0, format: "uxtab%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#16%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Uxtab, opcode: 0x06e00c70, mask: 0x0ff00ff0, format: "uxtab%c\t%12-15R, %16-19r, %0-3R, %{B:ror%} %{I:#24%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sel, opcode: 0x06800fb0, mask: 0x0ff00ff0, format: "sel%c\t%12-15R, %16-19R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Setend, opcode: 0xf1010000, mask: 0xfffffc00, format: "setend\t%{B:%9?ble%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smuad, opcode: 0x0700f010, mask: 0x0ff0f0d0, format: "smuad%5'x%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smusd, opcode: 0x0700f050, mask: 0x0ff0f0d0, format: "smusd%5'x%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlad, opcode: 0x07000010, mask: 0x0ff000d0, format: "smlad%5'x%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlald, opcode: 0x07400010, mask: 0x0ff000d0, format: "smlald%5'x%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlsd, opcode: 0x07000050, mask: 0x0ff000d0, format: "smlsd%5'x%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlsld, opcode: 0x07400050, mask: 0x0ff000d0, format: "smlsld%5'x%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smmul, opcode: 0x0750f010, mask: 0x0ff0f0d0, format: "smmul%5'r%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smmla, opcode: 0x07500010, mask: 0x0ff000d0, format: "smmla%5'r%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smmls, opcode: 0x075000d0, mask: 0x0ff000d0, format: "smmls%5'r%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Srs, opcode: 0xf84d0500, mask: 0xfe5fffe0, format: "srs%23?id%24?ba\t%16-19r%21'!, %{I:#%0-4d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssat, opcode: 0x06a00010, mask: 0x0fe00ff0, format: "ssat%c\t%12-15R, %{I:#%16-20W%}, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssat, opcode: 0x06a00010, mask: 0x0fe00070, format: "ssat%c\t%12-15R, %{I:#%16-20W%}, %0-3R, %{B:lsl%} %{I:#%7-11d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssat, opcode: 0x06a00050, mask: 0x0fe00070, format: "ssat%c\t%12-15R, %{I:#%16-20W%}, %0-3R, %{B:asr%} %{I:#%7-11d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ssat16, opcode: 0x06a00f30, mask: 0x0ff00ff0, format: "ssat16%c\t%12-15r, %{I:#%16-19W%}, %0-3r" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strex, opcode: 0x01800f90, mask: 0x0ff00ff0, format: "strex%c\t%12-15R, %0-3R, [%16-19R]" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Umaal, opcode: 0x00400090, mask: 0x0ff000f0, format: "umaal%c\t%12-15R, %16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usad8, opcode: 0x0780f010, mask: 0x0ff0f0f0, format: "usad8%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usada8, opcode: 0x07800010, mask: 0x0ff000f0, format: "usada8%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usat, opcode: 0x06e00010, mask: 0x0fe00ff0, format: "usat%c\t%12-15R, %{I:#%16-20d%}, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usat, opcode: 0x06e00010, mask: 0x0fe00070, format: "usat%c\t%12-15R, %{I:#%16-20d%}, %0-3R, %{B:lsl%} %{I:#%7-11d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usat, opcode: 0x06e00050, mask: 0x0fe00070, format: "usat%c\t%12-15R, %{I:#%16-20d%}, %0-3R, %{B:asr%} %{I:#%7-11d%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Usat16, opcode: 0x06e00f30, mask: 0x0ff00ff0, format: "usat16%c\t%12-15R, %{I:#%16-19d%}, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bxj, opcode: 0x012fff20, mask: 0x0ffffff0, format: "bxj%c\t%0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bkpt, opcode: 0xe1200070, mask: 0xfff000f0, format: "bkpt\t%{I:0x%16-19X%12-15X%8-11X%0-3X%}" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Blx, opcode: 0xfa000000, mask: 0xfe000000, format: "blx\t%B" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Blx, opcode: 0x012fff30, mask: 0x0ffffff0, format: "blx%c\t%0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Clz, opcode: 0x016f0f10, mask: 0x0fff0ff0, format: "clz%c\t%12-15R, %0-3R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldrd, opcode: 0x000000d0, mask: 0x0e1000f0, format: "ldrd%c\t%12-15r, %s" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strd, opcode: 0x000000f0, mask: 0x0e1000f0, format: "strd%c\t%12-15r, %s" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pld, opcode: 0xf450f000, mask: 0xfc70f000, format: "pld\t%a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlabb, opcode: 0x01000080, mask: 0x0ff000f0, format: "smlabb%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlatb, opcode: 0x010000a0, mask: 0x0ff000f0, format: "smlatb%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlabt, opcode: 0x010000c0, mask: 0x0ff000f0, format: "smlabt%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlatt, opcode: 0x010000e0, mask: 0x0ff000f0, format: "smlatt%c\t%16-19r, %0-3r, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlawb, opcode: 0x01200080, mask: 0x0ff000f0, format: "smlawb%c\t%16-19R, %0-3R, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlawt, opcode: 0x012000c0, mask: 0x0ff000f0, format: "smlawt%c\t%16-19R, %0-3r, %8-11R, %12-15R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlalbb, opcode: 0x01400080, mask: 0x0ff000f0, format: "smlalbb%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlaltb, opcode: 0x014000a0, mask: 0x0ff000f0, format: "smlaltb%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlalbt, opcode: 0x014000c0, mask: 0x0ff000f0, format: "smlalbt%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smlaltt, opcode: 0x014000e0, mask: 0x0ff000f0, format: "smlaltt%c\t%12-15Ru, %16-19Ru, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smulbb, opcode: 0x01600080, mask: 0x0ff0f0f0, format: "smulbb%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smultb, opcode: 0x016000a0, mask: 0x0ff0f0f0, format: "smultb%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smulbt, opcode: 0x016000c0, mask: 0x0ff0f0f0, format: "smulbt%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smultt, opcode: 0x016000e0, mask: 0x0ff0f0f0, format: "smultt%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smulwb, opcode: 0x012000a0, mask: 0x0ff0f0f0, format: "smulwb%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Smulwt, opcode: 0x012000e0, mask: 0x0ff0f0f0, format: "smulwt%c\t%16-19R, %0-3R, %8-11R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qadd, opcode: 0x01000050, mask: 0x0ff00ff0, format: "qadd%c\t%12-15R, %0-3R, %16-19R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qdadd, opcode: 0x01400050, mask: 0x0ff00ff0, format: "qdadd%c\t%12-15R, %0-3R, %16-19R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qsub, opcode: 0x01200050, mask: 0x0ff00ff0, format: "qsub%c\t%12-15R, %0-3R, %16-19R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Qdsub, opcode: 0x01600050, mask: 0x0ff00ff0, format: "qdsub%c\t%12-15R, %0-3R, %16-19R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Push, opcode: 0x052d0004, mask: 0x0fff0fff, format: "push%c\t{%12-15r}\t\t@ (str%c %12-15r, %a)" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strb, opcode: 0x04400000, mask: 0x0e500000, format: "strb%t%c\t%12-15R, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Str, opcode: 0x04000000, mask: 0x0e500000, format: "str%t%c\t%12-15r, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strb, opcode: 0x06400000, mask: 0x0e500ff0, format: "strb%t%c\t%12-15R, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Str, opcode: 0x06000000, mask: 0x0e500ff0, format: "str%t%c\t%12-15r, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strb, opcode: 0x04400000, mask: 0x0c500010, format: "strb%t%c\t%12-15R, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Str, opcode: 0x04000000, mask: 0x0c500010, format: "str%t%c\t%12-15r, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strb, opcode: 0x04400000, mask: 0x0e500000, format: "strb%c\t%12-15R, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strb, opcode: 0x06400000, mask: 0x0e500010, format: "strb%c\t%12-15R, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strh, opcode: 0x004000b0, mask: 0x0e5000f0, format: "strh%c\t%12-15R, %s" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Strh, opcode: 0x000000b0, mask: 0x0e500ff0, format: "strh%c\t%12-15R, %s" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldr, opcode: 0x00500090, mask: 0x0e500090, format: "ldr%6's%5?hb%c\t%12-15R, %s" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldr, opcode: 0x00100090, mask: 0x0e500f90, format: "ldr%6's%5?hb%c\t%12-15R, %s" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::And, opcode: 0x02000000, mask: 0x0fe00000, format: "and%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::And, opcode: 0x00000000, mask: 0x0fe00010, format: "and%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::And, opcode: 0x00000010, mask: 0x0fe00090, format: "and%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Eor, opcode: 0x02200000, mask: 0x0fe00000, format: "eor%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Eor, opcode: 0x00200000, mask: 0x0fe00010, format: "eor%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Eor, opcode: 0x00200010, mask: 0x0fe00090, format: "eor%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sub, opcode: 0x02400000, mask: 0x0fe00000, format: "sub%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sub, opcode: 0x00400000, mask: 0x0fe00010, format: "sub%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sub, opcode: 0x00400010, mask: 0x0fe00090, format: "sub%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rsb, opcode: 0x02600000, mask: 0x0fe00000, format: "rsb%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rsb, opcode: 0x00600000, mask: 0x0fe00010, format: "rsb%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rsb, opcode: 0x00600010, mask: 0x0fe00090, format: "rsb%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Add, opcode: 0x02800000, mask: 0x0fe00000, format: "add%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Add, opcode: 0x00800000, mask: 0x0fe00010, format: "add%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Add, opcode: 0x00800010, mask: 0x0fe00090, format: "add%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Adc, opcode: 0x02a00000, mask: 0x0fe00000, format: "adc%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Adc, opcode: 0x00a00000, mask: 0x0fe00010, format: "adc%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Adc, opcode: 0x00a00010, mask: 0x0fe00090, format: "adc%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sbc, opcode: 0x02c00000, mask: 0x0fe00000, format: "sbc%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sbc, opcode: 0x00c00000, mask: 0x0fe00010, format: "sbc%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Sbc, opcode: 0x00c00010, mask: 0x0fe00090, format: "sbc%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rsc, opcode: 0x02e00000, mask: 0x0fe00000, format: "rsc%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rsc, opcode: 0x00e00000, mask: 0x0fe00010, format: "rsc%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rsc, opcode: 0x00e00010, mask: 0x0fe00090, format: "rsc%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Msr, opcode: 0x0120f200, mask: 0x0fb0f200, format: "msr%c\t%C, %0-3r" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Msr, opcode: 0x0120f000, mask: 0x0db0f000, format: "msr%c\t%C, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mrs, opcode: 0x01000000, mask: 0x0fb00cff, format: "mrs%c\t%12-15R, %R" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Tst, opcode: 0x03000000, mask: 0x0fe00000, format: "tst%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Tst, opcode: 0x01000000, mask: 0x0fe00010, format: "tst%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Tst, opcode: 0x01000010, mask: 0x0fe00090, format: "tst%p%c\t%16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Teq, opcode: 0x03300000, mask: 0x0ff00000, format: "teq%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Teq, opcode: 0x01300000, mask: 0x0ff00010, format: "teq%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Teq, opcode: 0x01300010, mask: 0x0ff00010, format: "teq%p%c\t%16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cmp, opcode: 0x03400000, mask: 0x0fe00000, format: "cmp%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cmp, opcode: 0x01400000, mask: 0x0fe00010, format: "cmp%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cmp, opcode: 0x01400010, mask: 0x0fe00090, format: "cmp%p%c\t%16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cmn, opcode: 0x03600000, mask: 0x0fe00000, format: "cmn%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cmn, opcode: 0x01600000, mask: 0x0fe00010, format: "cmn%p%c\t%16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Cmn, opcode: 0x01600010, mask: 0x0fe00090, format: "cmn%p%c\t%16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Orr, opcode: 0x03800000, mask: 0x0fe00000, format: "orr%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Orr, opcode: 0x01800000, mask: 0x0fe00010, format: "orr%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Orr, opcode: 0x01800010, mask: 0x0fe00090, format: "orr%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mov, opcode: 0x03a00000, mask: 0x0fef0000, format: "mov%20's%c\t%12-15r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mov, opcode: 0x01a00000, mask: 0x0def0ff0, format: "mov%20's%c\t%12-15r, %0-3r" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Lsl, opcode: 0x01a00000, mask: 0x0def0060, format: "lsl%20's%c\t%12-15R, %q" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Lsr, opcode: 0x01a00020, mask: 0x0def0060, format: "lsr%20's%c\t%12-15R, %q" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Asr, opcode: 0x01a00040, mask: 0x0def0060, format: "asr%20's%c\t%12-15R, %q" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Rrx, opcode: 0x01a00060, mask: 0x0def0ff0, format: "rrx%20's%c\t%12-15r, %0-3r" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ror, opcode: 0x01a00060, mask: 0x0def0060, format: "ror%20's%c\t%12-15R, %q" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bic, opcode: 0x03c00000, mask: 0x0fe00000, format: "bic%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bic, opcode: 0x01c00000, mask: 0x0fe00010, format: "bic%20's%c\t%12-15r, %16-19r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Bic, opcode: 0x01c00010, mask: 0x0fe00090, format: "bic%20's%c\t%12-15R, %16-19R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mvn, opcode: 0x03e00000, mask: 0x0fe00000, format: "mvn%20's%c\t%12-15r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mvn, opcode: 0x01e00000, mask: 0x0fe00010, format: "mvn%20's%c\t%12-15r, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Mvn, opcode: 0x01e00010, mask: 0x0fe00090, format: "mvn%20's%c\t%12-15R, %o" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pop, opcode: 0x049d0004, mask: 0x0fff0fff, format: "pop%c\t{%12-15r}\t\t@ (ldr%c %12-15r, %a)" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldrb, opcode: 0x04500000, mask: 0x0c500000, format: "ldrb%t%c\t%12-15R, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldrt, opcode: 0x04300000, mask: 0x0d700000, format: "ldrt%c\t%12-15R, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldr, opcode: 0x04100000, mask: 0x0c500000, format: "ldr%c\t%12-15r, %a" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0001, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0002, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0004, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0008, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0010, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0020, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0040, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0080, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0100, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0200, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0400, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d0800, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d1000, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d2000, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d4000, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stmfd, opcode: 0x092d8000, mask: 0x0fffffff, format: "stmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Push, opcode: 0x092d0000, mask: 0x0fff0000, format: "push%c\t%m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stm, opcode: 0x08800000, mask: 0x0ff00000, format: "stm%c\t%16-19R%21'!, %m%22'^" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Stm, opcode: 0x08000000, mask: 0x0e100000, format: "stm%23?id%24?ba%c\t%16-19R%21'!, %m%22'^" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0001, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0002, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0004, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0008, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0010, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0020, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0040, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0080, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0100, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0200, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0400, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd0800, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd1000, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd2000, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd4000, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldmfd, opcode: 0x08bd8000, mask: 0x0fffffff, format: "ldmfd%c\t%16-19R!, %m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Pop, opcode: 0x08bd0000, mask: 0x0fff0000, format: "pop%c\t%m" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldm, opcode: 0x08900000, mask: 0x0f900000, format: "ldm%c\t%16-19R%21'!, %m%22'^" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Ldm, opcode: 0x08100000, mask: 0x0e100000, format: "ldm%23?id%24?ba%c\t%16-19R%21'!, %m%22'^" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::B, opcode: 0x0a000000, mask: 0x0e000000, format: "b%24'l%c\t%b" },
    ArmOpcodeGenerated { mnemonic: ArmMnemonicGenerated::Svc, opcode: 0x0f000000, mask: 0x0f000000, format: "svc%c\t%0-23x" },
];

/// Row type. Carries the binutils format string verbatim so
/// callers can pattern-match operand shapes without re-parsing.
#[derive(Debug, Copy, Clone)]
pub struct ArmOpcodeGenerated {
    pub mnemonic: ArmMnemonicGenerated,
    pub opcode: u32,
    pub mask: u32,
    pub format: &'static str,
}

impl ArmOpcodeGenerated {
    /// Bit ranges (within the 32-bit instruction word) occupied
    /// by each operand of this opcode, in left-to-right order.
    /// Re-uses the shared format-string walker in
    /// [`super::super::format_bit_ranges`]; ARM-mode rows are
    /// always 32-bit, so `width_bytes` is 4.
    pub fn operand_bit_ranges(&self) -> Vec<Vec<std::ops::Range<u8>>> {
        super::super::format_bit_ranges::extract_operand_bit_ranges(self.format, 4)
    }
}

/// Find the first row whose mask + opcode matches the input word.
pub fn match_generated(word: u32) -> Option<&'static ArmOpcodeGenerated> {
    ARM_OPCODE_TABLE_GENERATED
        .iter()
        .find(|row| (word & row.mask) == row.opcode)
}

/// Iterate every ARM-mode opcode row in the static table.
/// Mirrors `aarch64::iter_opcodes` and the Thumb equivalent.
pub fn iter_opcodes() -> impl Iterator<Item = &'static ArmOpcodeGenerated> {
    ARM_OPCODE_TABLE_GENERATED.iter()
}

