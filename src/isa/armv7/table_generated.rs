// AUTO-GENERATED — do not edit by hand.
//
// Regenerate with:
//
//   python3 tools/import_thumb_opcodes.py PATH_TO_arm-dis.c \
//       > src/isa/armv7/table_generated.rs
//
// Source: GNU binutils opcodes/arm-dis.c (GPL-2.0-or-later).
// The generator extracts the (opcode, mask, format) triples for
// the `thumb_opcodes` and `thumb32_opcodes` arrays and emits a
// `ThumbOpcode` row per entry. Operand shapes default to
// `Unspecified` — operand decoding for new shapes is wired up
// in `decode_operands` as needed.
//
// The mnemonic enum below is auto-generated from the union of
// mnemonics seen in binutils' format strings. Each variant's
// name is PascalCase of the mnemonic with non-alphanumeric
// chars treated as separators (e.g. `vmla.f32` → `VmlaF32`).

#![allow(dead_code, non_camel_case_types)]

use super::table::ThumbWidth;

/// Mnemonic identifier. Auto-generated from binutils.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ThumbMnemonicGenerated {
    /// `adc`
    Adc,
    /// `add`
    Add,
    /// `addw`
    Addw,
    /// `and`
    And,
    /// `asr`
    Asr,
    /// `aut`
    Aut,
    /// `autg`
    Autg,
    /// `b`
    B,
    /// `bf`
    Bf,
    /// `bfc`
    Bfc,
    /// `bfcsel`
    Bfcsel,
    /// `bfi`
    Bfi,
    /// `bfl`
    Bfl,
    /// `bflx`
    Bflx,
    /// `bfx`
    Bfx,
    /// `bic`
    Bic,
    /// `bkpt`
    Bkpt,
    /// `bl`
    Bl,
    /// `blx`
    Blx,
    /// `blxns`
    Blxns,
    /// `bti`
    Bti,
    /// `bx`
    Bx,
    /// `bxaut`
    Bxaut,
    /// `bxj`
    Bxj,
    /// `bxns`
    Bxns,
    /// `cbnz`
    Cbnz,
    /// `cbz`
    Cbz,
    /// `clrex`
    Clrex,
    /// `clrm`
    Clrm,
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
    /// `cpsid.w`
    CpsidW,
    /// `cpsie`
    Cpsie,
    /// `cpsie.w`
    CpsieW,
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
    /// `dcps`
    Dcps,
    /// `dfb`
    Dfb,
    /// `dls`
    Dls,
    /// `dlstp.`
    Dlstp,
    /// `dmb`
    Dmb,
    /// `dsb`
    Dsb,
    /// `eor`
    Eor,
    /// `esb`
    Esb,
    /// `hlt`
    Hlt,
    /// `hvc`
    Hvc,
    /// `isb`
    Isb,
    /// `it`
    It,
    /// `lctp`
    Lctp,
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
    /// `ldmdb`
    Ldmdb,
    /// `ldmia`
    Ldmia,
    /// `ldr`
    Ldr,
    /// `ldrb`
    Ldrb,
    /// `ldrd`
    Ldrd,
    /// `ldrex`
    Ldrex,
    /// `ldrexd`
    Ldrexd,
    /// `ldrh`
    Ldrh,
    /// `ldrs`
    Ldrs,
    /// `le`
    Le,
    /// `letp`
    Letp,
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
    /// `neg`
    Neg,
    /// `nop`
    Nop,
    /// `orn`
    Orn,
    /// `orr`
    Orr,
    /// `pac`
    Pac,
    /// `pacbti`
    Pacbti,
    /// `pacg`
    Pacg,
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
    /// `rfedb`
    Rfedb,
    /// `rfeia`
    Rfeia,
    /// `ror`
    Ror,
    /// `rsb`
    Rsb,
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
    /// `sbfx`
    Sbfx,
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
    /// `sg`
    Sg,
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
    /// `smla`
    Smla,
    /// `smlad`
    Smlad,
    /// `smlal`
    Smlal,
    /// `smlald`
    Smlald,
    /// `smlaw`
    Smlaw,
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
    /// `smul`
    Smul,
    /// `smull`
    Smull,
    /// `smulw`
    Smulw,
    /// `smusd`
    Smusd,
    /// `srsdb`
    Srsdb,
    /// `srsia`
    Srsia,
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
    /// `stmdb`
    Stmdb,
    /// `stmia`
    Stmia,
    /// `str`
    Str,
    /// `strb`
    Strb,
    /// `strd`
    Strd,
    /// `strex`
    Strex,
    /// `strexd`
    Strexd,
    /// `strh`
    Strh,
    /// `sub`
    Sub,
    /// `subs`
    Subs,
    /// `subw`
    Subw,
    /// `svc`
    Svc,
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
    /// `tbb`
    Tbb,
    /// `tbh`
    Tbh,
    /// `teq`
    Teq,
    /// `tst`
    Tst,
    /// `tt`
    Tt,
    /// `tta`
    Tta,
    /// `ttat`
    Ttat,
    /// `ttt`
    Ttt,
    /// `uadd16`
    Uadd16,
    /// `uadd8`
    Uadd8,
    /// `uasx`
    Uasx,
    /// `ubfx`
    Ubfx,
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
    /// `umlal`
    Umlal,
    /// `umull`
    Umull,
    /// `undefined`
    Undefined,
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
    /// `wls`
    Wls,
    /// `wlstp.`
    Wlstp,
    /// `yield`
    Yield,
}

impl ThumbMnemonicGenerated {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Adc => "adc",
            Self::Add => "add",
            Self::Addw => "addw",
            Self::And => "and",
            Self::Asr => "asr",
            Self::Aut => "aut",
            Self::Autg => "autg",
            Self::B => "b",
            Self::Bf => "bf",
            Self::Bfc => "bfc",
            Self::Bfcsel => "bfcsel",
            Self::Bfi => "bfi",
            Self::Bfl => "bfl",
            Self::Bflx => "bflx",
            Self::Bfx => "bfx",
            Self::Bic => "bic",
            Self::Bkpt => "bkpt",
            Self::Bl => "bl",
            Self::Blx => "blx",
            Self::Blxns => "blxns",
            Self::Bti => "bti",
            Self::Bx => "bx",
            Self::Bxaut => "bxaut",
            Self::Bxj => "bxj",
            Self::Bxns => "bxns",
            Self::Cbnz => "cbnz",
            Self::Cbz => "cbz",
            Self::Clrex => "clrex",
            Self::Clrm => "clrm",
            Self::Clz => "clz",
            Self::Cmn => "cmn",
            Self::Cmp => "cmp",
            Self::Cps => "cps",
            Self::Cpsid => "cpsid",
            Self::CpsidW => "cpsid.w",
            Self::Cpsie => "cpsie",
            Self::CpsieW => "cpsie.w",
            Self::Crc32b => "crc32b",
            Self::Crc32cb => "crc32cb",
            Self::Crc32ch => "crc32ch",
            Self::Crc32cw => "crc32cw",
            Self::Crc32h => "crc32h",
            Self::Crc32w => "crc32w",
            Self::Csdb => "csdb",
            Self::Dbg => "dbg",
            Self::Dcps => "dcps",
            Self::Dfb => "dfb",
            Self::Dls => "dls",
            Self::Dlstp => "dlstp.",
            Self::Dmb => "dmb",
            Self::Dsb => "dsb",
            Self::Eor => "eor",
            Self::Esb => "esb",
            Self::Hlt => "hlt",
            Self::Hvc => "hvc",
            Self::Isb => "isb",
            Self::It => "it",
            Self::Lctp => "lctp",
            Self::Lda => "lda",
            Self::Ldab => "ldab",
            Self::Ldaex => "ldaex",
            Self::Ldaexb => "ldaexb",
            Self::Ldaexd => "ldaexd",
            Self::Ldaexh => "ldaexh",
            Self::Ldah => "ldah",
            Self::Ldmdb => "ldmdb",
            Self::Ldmia => "ldmia",
            Self::Ldr => "ldr",
            Self::Ldrb => "ldrb",
            Self::Ldrd => "ldrd",
            Self::Ldrex => "ldrex",
            Self::Ldrexd => "ldrexd",
            Self::Ldrh => "ldrh",
            Self::Ldrs => "ldrs",
            Self::Le => "le",
            Self::Letp => "letp",
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
            Self::Neg => "neg",
            Self::Nop => "nop",
            Self::Orn => "orn",
            Self::Orr => "orr",
            Self::Pac => "pac",
            Self::Pacbti => "pacbti",
            Self::Pacg => "pacg",
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
            Self::Rfedb => "rfedb",
            Self::Rfeia => "rfeia",
            Self::Ror => "ror",
            Self::Rsb => "rsb",
            Self::Sadd16 => "sadd16",
            Self::Sadd8 => "sadd8",
            Self::Sasx => "sasx",
            Self::Sb => "sb",
            Self::Sbc => "sbc",
            Self::Sbfx => "sbfx",
            Self::Sdiv => "sdiv",
            Self::Sel => "sel",
            Self::Setend => "setend",
            Self::Setpan => "setpan",
            Self::Sev => "sev",
            Self::Sevl => "sevl",
            Self::Sg => "sg",
            Self::Shadd16 => "shadd16",
            Self::Shadd8 => "shadd8",
            Self::Shasx => "shasx",
            Self::Shsax => "shsax",
            Self::Shsub16 => "shsub16",
            Self::Shsub8 => "shsub8",
            Self::Smc => "smc",
            Self::Smla => "smla",
            Self::Smlad => "smlad",
            Self::Smlal => "smlal",
            Self::Smlald => "smlald",
            Self::Smlaw => "smlaw",
            Self::Smlsd => "smlsd",
            Self::Smlsld => "smlsld",
            Self::Smmla => "smmla",
            Self::Smmls => "smmls",
            Self::Smmul => "smmul",
            Self::Smuad => "smuad",
            Self::Smul => "smul",
            Self::Smull => "smull",
            Self::Smulw => "smulw",
            Self::Smusd => "smusd",
            Self::Srsdb => "srsdb",
            Self::Srsia => "srsia",
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
            Self::Stmdb => "stmdb",
            Self::Stmia => "stmia",
            Self::Str => "str",
            Self::Strb => "strb",
            Self::Strd => "strd",
            Self::Strex => "strex",
            Self::Strexd => "strexd",
            Self::Strh => "strh",
            Self::Sub => "sub",
            Self::Subs => "subs",
            Self::Subw => "subw",
            Self::Svc => "svc",
            Self::Sxtab => "sxtab",
            Self::Sxtab16 => "sxtab16",
            Self::Sxtah => "sxtah",
            Self::Sxtb => "sxtb",
            Self::Sxtb16 => "sxtb16",
            Self::Sxth => "sxth",
            Self::Tbb => "tbb",
            Self::Tbh => "tbh",
            Self::Teq => "teq",
            Self::Tst => "tst",
            Self::Tt => "tt",
            Self::Tta => "tta",
            Self::Ttat => "ttat",
            Self::Ttt => "ttt",
            Self::Uadd16 => "uadd16",
            Self::Uadd8 => "uadd8",
            Self::Uasx => "uasx",
            Self::Ubfx => "ubfx",
            Self::Udf => "udf",
            Self::Udiv => "udiv",
            Self::Uhadd16 => "uhadd16",
            Self::Uhadd8 => "uhadd8",
            Self::Uhasx => "uhasx",
            Self::Uhsax => "uhsax",
            Self::Uhsub16 => "uhsub16",
            Self::Uhsub8 => "uhsub8",
            Self::Umaal => "umaal",
            Self::Umlal => "umlal",
            Self::Umull => "umull",
            Self::Undefined => "undefined",
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
            Self::Wls => "wls",
            Self::Wlstp => "wlstp.",
            Self::Yield => "yield",
        }
    }

    /// Display alias for the canonical mnemonic, when the
    /// disassembler renders the instruction with a different
    /// name than [`Self::as_str`]. Returns `None` for the
    /// vast majority of Thumb mnemonics — Thumb-2's condition
    /// suffixes (`b.eq`, `bcc.n`, etc.) are driven by the
    /// format string's `%c` and `%X`/`%x` size markers, not
    /// by a separate alias mnemonic the way AArch64 handles
    /// `b.eq`. Provided for API symmetry with
    /// `aarch64::Aarch64Mnemonic::display_alias`.
    pub fn display_alias(&self) -> Option<&'static str> {
        None
    }
}

/// Auto-generated table covering every Thumb instruction
/// binutils 2.41 recognises.
pub static THUMB_OPCODE_TABLE_GENERATED: &[ThumbOpcodeGenerated] = &[
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Blxns, opcode: 0x00004784, mask: 0x0000ff87, width: ThumbWidth::Halfword, format: "blxns\t%3-6r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bxns, opcode: 0x00004704, mask: 0x0000ff87, width: ThumbWidth::Halfword, format: "bxns\t%3-6r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sevl, opcode: 0x0000bf50, mask: 0x0000ffff, width: ThumbWidth::Halfword, format: "sevl%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Hlt, opcode: 0x0000ba80, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "hlt\t%0-5x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Setpan, opcode: 0x0000b610, mask: 0x0000fff7, width: ThumbWidth::Halfword, format: "setpan\t%{I:#%3-3d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Nop, opcode: 0x0000bf00, mask: 0x0000ffff, width: ThumbWidth::Halfword, format: "nop%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Yield, opcode: 0x0000bf10, mask: 0x0000ffff, width: ThumbWidth::Halfword, format: "yield%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Wfe, opcode: 0x0000bf20, mask: 0x0000ffff, width: ThumbWidth::Halfword, format: "wfe%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Wfi, opcode: 0x0000bf30, mask: 0x0000ffff, width: ThumbWidth::Halfword, format: "wfi%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sev, opcode: 0x0000bf40, mask: 0x0000ffff, width: ThumbWidth::Halfword, format: "sev%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Nop, opcode: 0x0000bf00, mask: 0x0000ff0f, width: ThumbWidth::Halfword, format: "nop%c\t{%4-7d}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cbnz, opcode: 0x0000b900, mask: 0x0000fd00, width: ThumbWidth::Halfword, format: "cbnz\t%0-2r, %b%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cbz, opcode: 0x0000b100, mask: 0x0000fd00, width: ThumbWidth::Halfword, format: "cbz\t%0-2r, %b%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::It, opcode: 0x0000bf00, mask: 0x0000ff00, width: ThumbWidth::Halfword, format: "it%I%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cpsie, opcode: 0x0000b660, mask: 0x0000fff8, width: ThumbWidth::Halfword, format: "cpsie\t%{B:%2'a%1'i%0'f%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cpsid, opcode: 0x0000b670, mask: 0x0000fff8, width: ThumbWidth::Halfword, format: "cpsid\t%{B:%2'a%1'i%0'f%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mov, opcode: 0x00004600, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "mov%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rev, opcode: 0x0000ba00, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "rev%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rev16, opcode: 0x0000ba40, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "rev16%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Revsh, opcode: 0x0000bac0, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "revsh%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Setend, opcode: 0x0000b650, mask: 0x0000fff7, width: ThumbWidth::Halfword, format: "setend\t%{B:%3?ble%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxth, opcode: 0x0000b200, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "sxth%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxtb, opcode: 0x0000b240, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "sxtb%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxth, opcode: 0x0000b280, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "uxth%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxtb, opcode: 0x0000b2c0, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "uxtb%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bkpt, opcode: 0x0000be00, mask: 0x0000ff00, width: ThumbWidth::Halfword, format: "bkpt\t%0-7x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Blx, opcode: 0x00004780, mask: 0x0000ff87, width: ThumbWidth::Halfword, format: "blx%c\t%3-6r%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Nop, opcode: 0x000046c0, mask: 0x0000ffff, width: ThumbWidth::Halfword, format: "nop%c\t\t\t@ (mov r8, r8)" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::And, opcode: 0x00004000, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "and%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Eor, opcode: 0x00004040, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "eor%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lsl, opcode: 0x00004080, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "lsl%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lsr, opcode: 0x000040c0, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "lsr%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Asr, opcode: 0x00004100, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "asr%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Adc, opcode: 0x00004140, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "adc%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sbc, opcode: 0x00004180, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "sbc%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ror, opcode: 0x000041c0, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "ror%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Tst, opcode: 0x00004200, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "tst%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Neg, opcode: 0x00004240, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "neg%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmp, opcode: 0x00004280, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "cmp%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmn, opcode: 0x000042c0, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "cmn%c\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Orr, opcode: 0x00004300, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "orr%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mul, opcode: 0x00004340, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "mul%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bic, opcode: 0x00004380, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "bic%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mvn, opcode: 0x000043c0, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "mvn%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0x0000b000, mask: 0x0000ff80, width: ThumbWidth::Halfword, format: "add%c\t%{R:sp%}, %{I:#%0-6W%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sub, opcode: 0x0000b080, mask: 0x0000ff80, width: ThumbWidth::Halfword, format: "sub%c\t%{R:sp%}, %{I:#%0-6W%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bx, opcode: 0x00004700, mask: 0x0000ff80, width: ThumbWidth::Halfword, format: "bx%c\t%S%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0x00004400, mask: 0x0000ff00, width: ThumbWidth::Halfword, format: "add%c\t%D, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmp, opcode: 0x00004500, mask: 0x0000ff00, width: ThumbWidth::Halfword, format: "cmp%c\t%D, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mov, opcode: 0x00004600, mask: 0x0000ff00, width: ThumbWidth::Halfword, format: "mov%c\t%D, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Push, opcode: 0x0000b400, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "push%c\t%N" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pop, opcode: 0x0000bc00, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "pop%c\t%O" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0x00001800, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "add%C\t%0-2r, %3-5r, %6-8r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sub, opcode: 0x00001a00, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "sub%C\t%0-2r, %3-5r, %6-8r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0x00001c00, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "add%C\t%0-2r, %3-5r, %{I:#%6-8d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sub, opcode: 0x00001e00, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "sub%C\t%0-2r, %3-5r, %{I:#%6-8d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strh, opcode: 0x00005200, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "strh%c\t%0-2r, [%3-5r, %6-8r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrh, opcode: 0x00005a00, mask: 0x0000fe00, width: ThumbWidth::Halfword, format: "ldrh%c\t%0-2r, [%3-5r, %6-8r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrs, opcode: 0x00005600, mask: 0x0000f600, width: ThumbWidth::Halfword, format: "ldrs%11?hb%c\t%0-2r, [%3-5r, %6-8r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Str, opcode: 0x00005000, mask: 0x0000fa00, width: ThumbWidth::Halfword, format: "str%10'b%c\t%0-2r, [%3-5r, %6-8r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldr, opcode: 0x00005800, mask: 0x0000fa00, width: ThumbWidth::Halfword, format: "ldr%10'b%c\t%0-2r, [%3-5r, %6-8r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mov, opcode: 0x00000000, mask: 0x0000ffc0, width: ThumbWidth::Halfword, format: "mov%C\t%0-2r, %3-5r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lsl, opcode: 0x00000000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "lsl%C\t%0-2r, %3-5r, %{I:#%6-10d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lsr, opcode: 0x00000800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "lsr%C\t%0-2r, %3-5r, %s" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Asr, opcode: 0x00001000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "asr%C\t%0-2r, %3-5r, %s" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mov, opcode: 0x00002000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "mov%C\t%8-10r, %{I:#%0-7d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmp, opcode: 0x00002800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "cmp%c\t%8-10r, %{I:#%0-7d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0x00003000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "add%C\t%8-10r, %{I:#%0-7d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sub, opcode: 0x00003800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "sub%C\t%8-10r, %{I:#%0-7d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldr, opcode: 0x00004800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "ldr%c\t%8-10r, [%{R:pc%}, %{I:#%0-7W%}]\t@ (%0-7a)" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Str, opcode: 0x00006000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "str%c\t%0-2r, [%3-5r, %{I:#%6-10W%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldr, opcode: 0x00006800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "ldr%c\t%0-2r, [%3-5r, %{I:#%6-10W%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strb, opcode: 0x00007000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "strb%c\t%0-2r, [%3-5r, %{I:#%6-10d%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrb, opcode: 0x00007800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "ldrb%c\t%0-2r, [%3-5r, %{I:#%6-10d%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strh, opcode: 0x00008000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "strh%c\t%0-2r, [%3-5r, %{I:#%6-10H%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrh, opcode: 0x00008800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "ldrh%c\t%0-2r, [%3-5r, %{I:#%6-10H%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Str, opcode: 0x00009000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "str%c\t%8-10r, [%{R:sp%}, %{I:#%0-7W%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldr, opcode: 0x00009800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "ldr%c\t%8-10r, [%{R:sp%}, %{I:#%0-7W%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0x0000a000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "add%c\t%8-10r, %{R:pc%}, %{I:#%0-7W%}\t@ (adr %8-10r, %0-7a)" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0x0000a800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "add%c\t%8-10r, %{R:sp%}, %{I:#%0-7W%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stmia, opcode: 0x0000c000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "stmia%c\t%8-10r!, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldmia, opcode: 0x0000c800, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "ldmia%c\t%8-10r%W, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Svc, opcode: 0x0000df00, mask: 0x0000ff00, width: ThumbWidth::Halfword, format: "svc%c\t%0-7d" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Udf, opcode: 0x0000de00, mask: 0x0000ff00, width: ThumbWidth::Halfword, format: "udf%c\t%{I:#%0-7d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::B, opcode: 0x0000d000, mask: 0x0000f000, width: ThumbWidth::Halfword, format: "b%8-11c.n\t%0-7B%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::B, opcode: 0x0000e000, mask: 0x0000f800, width: ThumbWidth::Halfword, format: "b%c.n\t%0-10B%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Aut, opcode: 0xf3af802d, mask: 0xffffffff, width: ThumbWidth::Word, format: "aut\t%{R:r12%}, %{R:lr%}, %{R:sp%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Autg, opcode: 0xfb500f00, mask: 0xfff00ff0, width: ThumbWidth::Word, format: "autg%c\t%12-15r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bti, opcode: 0xf3af800f, mask: 0xffffffff, width: ThumbWidth::Word, format: "bti" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bxaut, opcode: 0xfb500f10, mask: 0xfff00ff0, width: ThumbWidth::Word, format: "bxaut%c\t%12-15r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pac, opcode: 0xf3af801d, mask: 0xffffffff, width: ThumbWidth::Word, format: "pac\t%{R:r12%}, %{R:lr%}, %{R:sp%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pacbti, opcode: 0xf3af800d, mask: 0xffffffff, width: ThumbWidth::Word, format: "pacbti\t%{R:r12%}, %{R:lr%}, %{R:sp%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pacg, opcode: 0xfb60f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "pacg%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lctp, opcode: 0xf00fe001, mask: 0xffffffff, width: ThumbWidth::Word, format: "lctp%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Le, opcode: 0xf02fc001, mask: 0xfffff001, width: ThumbWidth::Word, format: "le\t%P" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Le, opcode: 0xf00fc001, mask: 0xfffff001, width: ThumbWidth::Word, format: "le\t%{R:lr%}, %P" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Letp, opcode: 0xf01fc001, mask: 0xfffff001, width: ThumbWidth::Word, format: "letp\t%{R:lr%}, %P" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Wls, opcode: 0xf040c001, mask: 0xfff0f001, width: ThumbWidth::Word, format: "wls\t%{R:lr%}, %16-19S, %Q" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Wlstp, opcode: 0xf000c001, mask: 0xffc0f001, width: ThumbWidth::Word, format: "wlstp.%20-21s\t%{R:lr%}, %16-19S, %Q" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dls, opcode: 0xf040e001, mask: 0xfff0ffff, width: ThumbWidth::Word, format: "dls\t%{R:lr%}, %16-19S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dlstp, opcode: 0xf000e001, mask: 0xffc0ffff, width: ThumbWidth::Word, format: "dlstp.%20-21s\t%{R:lr%}, %16-19S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bf, opcode: 0xf040e001, mask: 0xf860f001, width: ThumbWidth::Word, format: "bf%c\t%G, %W" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bfx, opcode: 0xf060e001, mask: 0xf8f0f001, width: ThumbWidth::Word, format: "bfx%c\t%G, %16-19S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bfl, opcode: 0xf000c001, mask: 0xf800f001, width: ThumbWidth::Word, format: "bfl%c\t%G, %Y" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bflx, opcode: 0xf070e001, mask: 0xf8f0f001, width: ThumbWidth::Word, format: "bflx%c\t%G, %16-19S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bfcsel, opcode: 0xf000e001, mask: 0xf840f001, width: ThumbWidth::Word, format: "bfcsel\t%G, %Z, %{B:%18-21c%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Clrm, opcode: 0xe89f0000, mask: 0xffff2000, width: ThumbWidth::Word, format: "clrm%c\t%n" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sg, opcode: 0xe97fe97f, mask: 0xffffffff, width: ThumbWidth::Word, format: "sg" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Tt, opcode: 0xe840f000, mask: 0xfff0f0ff, width: ThumbWidth::Word, format: "tt\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ttt, opcode: 0xe840f040, mask: 0xfff0f0ff, width: ThumbWidth::Word, format: "ttt\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Tta, opcode: 0xe840f080, mask: 0xfff0f0ff, width: ThumbWidth::Word, format: "tta\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ttat, opcode: 0xe840f0c0, mask: 0xfff0f0ff, width: ThumbWidth::Word, format: "ttat\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Esb, opcode: 0xf3af8010, mask: 0xffffffff, width: ThumbWidth::Word, format: "esb" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sevl, opcode: 0xf3af8005, mask: 0xffffffff, width: ThumbWidth::Word, format: "sevl%c.w" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dcps, opcode: 0xf78f8000, mask: 0xfffffffc, width: ThumbWidth::Word, format: "dcps%0-1d" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stlb, opcode: 0xe8c00f8f, mask: 0xfff00fff, width: ThumbWidth::Word, format: "stlb%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stlh, opcode: 0xe8c00f9f, mask: 0xfff00fff, width: ThumbWidth::Word, format: "stlh%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stl, opcode: 0xe8c00faf, mask: 0xfff00fff, width: ThumbWidth::Word, format: "stl%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stlexb, opcode: 0xe8c00fc0, mask: 0xfff00ff0, width: ThumbWidth::Word, format: "stlexb%c\t%0-3r, %12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stlexh, opcode: 0xe8c00fd0, mask: 0xfff00ff0, width: ThumbWidth::Word, format: "stlexh%c\t%0-3r, %12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stlex, opcode: 0xe8c00fe0, mask: 0xfff00ff0, width: ThumbWidth::Word, format: "stlex%c\t%0-3r, %12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stlexd, opcode: 0xe8c000f0, mask: 0xfff000f0, width: ThumbWidth::Word, format: "stlexd%c\t%0-3r, %12-15r, %8-11r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldab, opcode: 0xe8d00f8f, mask: 0xfff00fff, width: ThumbWidth::Word, format: "ldab%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldah, opcode: 0xe8d00f9f, mask: 0xfff00fff, width: ThumbWidth::Word, format: "ldah%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lda, opcode: 0xe8d00faf, mask: 0xfff00fff, width: ThumbWidth::Word, format: "lda%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldaexb, opcode: 0xe8d00fcf, mask: 0xfff00fff, width: ThumbWidth::Word, format: "ldaexb%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldaexh, opcode: 0xe8d00fdf, mask: 0xfff00fff, width: ThumbWidth::Word, format: "ldaexh%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldaex, opcode: 0xe8d00fef, mask: 0xfff00fff, width: ThumbWidth::Word, format: "ldaex%c\t%12-15r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldaexd, opcode: 0xe8d000ff, mask: 0xfff000ff, width: ThumbWidth::Word, format: "ldaexd%c\t%12-15r, %8-11r, [%16-19R]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dfb, opcode: 0xf3bf8f4c, mask: 0xffffffff, width: ThumbWidth::Word, format: "dfb%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Crc32b, opcode: 0xfac0f080, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "crc32b\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Crc32h, opcode: 0xfac0f090, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "crc32h\t%9-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Crc32w, opcode: 0xfac0f0a0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "crc32w\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Crc32cb, opcode: 0xfad0f080, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "crc32cb\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Crc32ch, opcode: 0xfad0f090, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "crc32ch\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Crc32cw, opcode: 0xfad0f0a0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "crc32cw\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Csdb, opcode: 0xf3af8014, mask: 0xffffffff, width: ThumbWidth::Word, format: "csdb" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ssbb, opcode: 0xf3bf8f40, mask: 0xffffffff, width: ThumbWidth::Word, format: "ssbb" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pssbb, opcode: 0xf3bf8f44, mask: 0xffffffff, width: ThumbWidth::Word, format: "pssbb" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pli, opcode: 0xf910f000, mask: 0xff70f000, width: ThumbWidth::Word, format: "pli%c\t%a" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dbg, opcode: 0xf3af80f0, mask: 0xfffffff0, width: ThumbWidth::Word, format: "dbg%c\t%{I:#%0-3d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dmb, opcode: 0xf3bf8f51, mask: 0xfffffff3, width: ThumbWidth::Word, format: "dmb%c\t%U" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dsb, opcode: 0xf3bf8f41, mask: 0xfffffff3, width: ThumbWidth::Word, format: "dsb%c\t%U" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dmb, opcode: 0xf3bf8f50, mask: 0xfffffff0, width: ThumbWidth::Word, format: "dmb%c\t%U" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Dsb, opcode: 0xf3bf8f40, mask: 0xfffffff0, width: ThumbWidth::Word, format: "dsb%c\t%U" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Isb, opcode: 0xf3bf8f60, mask: 0xfffffff0, width: ThumbWidth::Word, format: "isb%c\t%U" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sdiv, opcode: 0xfb90f0f0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "sdiv%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Udiv, opcode: 0xfbb0f0f0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "udiv%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Hvc, opcode: 0xf7e08000, mask: 0xfff0f000, width: ThumbWidth::Word, format: "hvc%c\t%V" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pldw, opcode: 0xf830f000, mask: 0xff70f000, width: ThumbWidth::Word, format: "pldw%c\t%a" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smc, opcode: 0xf7f08000, mask: 0xfff0f000, width: ThumbWidth::Word, format: "smc%c\t%K" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sb, opcode: 0xf3bf8f70, mask: 0xffffffff, width: ThumbWidth::Word, format: "sb" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Nop, opcode: 0xf3af8000, mask: 0xffffffff, width: ThumbWidth::Word, format: "nop%c.w" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Yield, opcode: 0xf3af8001, mask: 0xffffffff, width: ThumbWidth::Word, format: "yield%c.w" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Wfe, opcode: 0xf3af8002, mask: 0xffffffff, width: ThumbWidth::Word, format: "wfe%c.w" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Wfi, opcode: 0xf3af8003, mask: 0xffffffff, width: ThumbWidth::Word, format: "wfi%c.w" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sev, opcode: 0xf3af8004, mask: 0xffffffff, width: ThumbWidth::Word, format: "sev%c.w" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Nop, opcode: 0xf3af8000, mask: 0xffffff00, width: ThumbWidth::Word, format: "nop%c.w\t{%{I:%0-7d%}}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Udf, opcode: 0xf7f0a000, mask: 0xfff0f000, width: ThumbWidth::Word, format: "udf%c.w\t%H" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Clrex, opcode: 0xf3bf8f2f, mask: 0xffffffff, width: ThumbWidth::Word, format: "clrex%c" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::CpsieW, opcode: 0xf3af8400, mask: 0xffffff1f, width: ThumbWidth::Word, format: "cpsie.w\t%{B:%7'a%6'i%5'f%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::CpsidW, opcode: 0xf3af8600, mask: 0xffffff1f, width: ThumbWidth::Word, format: "cpsid.w\t%{B:%7'a%6'i%5'f%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bxj, opcode: 0xf3c08f00, mask: 0xfff0ffff, width: ThumbWidth::Word, format: "bxj%c\t%16-19r%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rfedb, opcode: 0xe810c000, mask: 0xffd0ffff, width: ThumbWidth::Word, format: "rfedb%c\t%16-19r%21'!" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rfeia, opcode: 0xe990c000, mask: 0xffd0ffff, width: ThumbWidth::Word, format: "rfeia%c\t%16-19r%21'!" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mrs, opcode: 0xf3e08000, mask: 0xffe0f000, width: ThumbWidth::Word, format: "mrs%c\t%8-11r, %D" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cps, opcode: 0xf3af8100, mask: 0xffffffe0, width: ThumbWidth::Word, format: "cps\t%{I:#%0-4d%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Tbb, opcode: 0xe8d0f000, mask: 0xfff0fff0, width: ThumbWidth::Word, format: "tbb%c\t[%16-19r, %0-3r]%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Tbh, opcode: 0xe8d0f010, mask: 0xfff0fff0, width: ThumbWidth::Word, format: "tbh%c\t[%16-19r, %0-3r, %{B:lsl%} %{I:#1%}]%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cpsie, opcode: 0xf3af8500, mask: 0xffffff00, width: ThumbWidth::Word, format: "cpsie\t%{B:%7'a%6'i%5'f%}, %{I:#%0-4d%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cpsid, opcode: 0xf3af8700, mask: 0xffffff00, width: ThumbWidth::Word, format: "cpsid\t%{B:%7'a%6'i%5'f%}, %{I:#%0-4d%}%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Subs, opcode: 0xf3de8f00, mask: 0xffffff00, width: ThumbWidth::Word, format: "subs%c\t%{R:pc%}, %{R:lr%}, %{I:#%0-7d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Msr, opcode: 0xf3808000, mask: 0xffe0f000, width: ThumbWidth::Word, format: "msr%c\t%C, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrex, opcode: 0xe8500f00, mask: 0xfff00fff, width: ThumbWidth::Word, format: "ldrex%c\t%12-15r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrex, opcode: 0xe8d00f4f, mask: 0xfff00fef, width: ThumbWidth::Word, format: "ldrex%4?hb%c\t%12-15r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Srsdb, opcode: 0xe800c000, mask: 0xffd0ffe0, width: ThumbWidth::Word, format: "srsdb%c\t%16-19r%21'!, %{I:#%0-4d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Srsia, opcode: 0xe980c000, mask: 0xffd0ffe0, width: ThumbWidth::Word, format: "srsia%c\t%16-19r%21'!, %{I:#%0-4d%}" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxth, opcode: 0xfa0ff080, mask: 0xfffff0c0, width: ThumbWidth::Word, format: "sxth%c.w\t%8-11r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxth, opcode: 0xfa1ff080, mask: 0xfffff0c0, width: ThumbWidth::Word, format: "uxth%c.w\t%8-11r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxtb16, opcode: 0xfa2ff080, mask: 0xfffff0c0, width: ThumbWidth::Word, format: "sxtb16%c\t%8-11r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxtb16, opcode: 0xfa3ff080, mask: 0xfffff0c0, width: ThumbWidth::Word, format: "uxtb16%c\t%8-11r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxtb, opcode: 0xfa4ff080, mask: 0xfffff0c0, width: ThumbWidth::Word, format: "sxtb%c.w\t%8-11r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxtb, opcode: 0xfa5ff080, mask: 0xfffff0c0, width: ThumbWidth::Word, format: "uxtb%c.w\t%8-11r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strex, opcode: 0xe8400000, mask: 0xfff000ff, width: ThumbWidth::Word, format: "strex%c\t%8-11r, %12-15r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrexd, opcode: 0xe8d0007f, mask: 0xfff000ff, width: ThumbWidth::Word, format: "ldrexd%c\t%12-15r, %8-11r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sadd8, opcode: 0xfa80f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "sadd8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qadd8, opcode: 0xfa80f010, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qadd8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Shadd8, opcode: 0xfa80f020, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "shadd8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uadd8, opcode: 0xfa80f040, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uadd8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uqadd8, opcode: 0xfa80f050, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uqadd8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uhadd8, opcode: 0xfa80f060, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uhadd8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qadd, opcode: 0xfa80f080, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qadd%c\t%8-11r, %0-3r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qdadd, opcode: 0xfa80f090, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qdadd%c\t%8-11r, %0-3r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qsub, opcode: 0xfa80f0a0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qsub%c\t%8-11r, %0-3r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qdsub, opcode: 0xfa80f0b0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qdsub%c\t%8-11r, %0-3r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sadd16, opcode: 0xfa90f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "sadd16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qadd16, opcode: 0xfa90f010, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qadd16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Shadd16, opcode: 0xfa90f020, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "shadd16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uadd16, opcode: 0xfa90f040, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uadd16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uqadd16, opcode: 0xfa90f050, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uqadd16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uhadd16, opcode: 0xfa90f060, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uhadd16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rev, opcode: 0xfa90f080, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "rev%c.w\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rev16, opcode: 0xfa90f090, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "rev16%c.w\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rbit, opcode: 0xfa90f0a0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "rbit%c\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Revsh, opcode: 0xfa90f0b0, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "revsh%c.w\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sasx, opcode: 0xfaa0f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "sasx%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qasx, opcode: 0xfaa0f010, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qasx%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Shasx, opcode: 0xfaa0f020, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "shasx%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uasx, opcode: 0xfaa0f040, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uasx%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uqasx, opcode: 0xfaa0f050, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uqasx%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uhasx, opcode: 0xfaa0f060, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uhasx%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sel, opcode: 0xfaa0f080, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "sel%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Clz, opcode: 0xfab0f080, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "clz%c\t%8-11r, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ssub8, opcode: 0xfac0f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "ssub8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qsub8, opcode: 0xfac0f010, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qsub8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Shsub8, opcode: 0xfac0f020, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "shsub8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Usub8, opcode: 0xfac0f040, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "usub8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uqsub8, opcode: 0xfac0f050, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uqsub8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uhsub8, opcode: 0xfac0f060, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uhsub8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ssub16, opcode: 0xfad0f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "ssub16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qsub16, opcode: 0xfad0f010, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qsub16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Shsub16, opcode: 0xfad0f020, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "shsub16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Usub16, opcode: 0xfad0f040, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "usub16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uqsub16, opcode: 0xfad0f050, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uqsub16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uhsub16, opcode: 0xfad0f060, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uhsub16%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ssax, opcode: 0xfae0f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "ssax%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Qsax, opcode: 0xfae0f010, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "qsax%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Shsax, opcode: 0xfae0f020, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "shsax%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Usax, opcode: 0xfae0f040, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "usax%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uqsax, opcode: 0xfae0f050, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uqsax%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uhsax, opcode: 0xfae0f060, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "uhsax%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mul, opcode: 0xfb00f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "mul%c.w\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Usad8, opcode: 0xfb70f000, mask: 0xfff0f0f0, width: ThumbWidth::Word, format: "usad8%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lsl, opcode: 0xfa00f000, mask: 0xffe0f0f0, width: ThumbWidth::Word, format: "lsl%20's%c.w\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Lsr, opcode: 0xfa20f000, mask: 0xffe0f0f0, width: ThumbWidth::Word, format: "lsr%20's%c.w\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Asr, opcode: 0xfa40f000, mask: 0xffe0f0f0, width: ThumbWidth::Word, format: "asr%20's%c.w\t%8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ror, opcode: 0xfa60f000, mask: 0xffe0f0f0, width: ThumbWidth::Word, format: "ror%20's%c.w\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strex, opcode: 0xe8c00f40, mask: 0xfff00fe0, width: ThumbWidth::Word, format: "strex%4?hb%c\t%0-3r, %12-15r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ssat16, opcode: 0xf3200000, mask: 0xfff0f0e0, width: ThumbWidth::Word, format: "ssat16%c\t%8-11r, %{I:#%0-4D%}, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Usat16, opcode: 0xf3a00000, mask: 0xfff0f0e0, width: ThumbWidth::Word, format: "usat16%c\t%8-11r, %{I:#%0-4d%}, %16-19r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smuad, opcode: 0xfb20f000, mask: 0xfff0f0e0, width: ThumbWidth::Word, format: "smuad%4'x%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smulw, opcode: 0xfb30f000, mask: 0xfff0f0e0, width: ThumbWidth::Word, format: "smulw%4?tb%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smusd, opcode: 0xfb40f000, mask: 0xfff0f0e0, width: ThumbWidth::Word, format: "smusd%4'x%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smmul, opcode: 0xfb50f000, mask: 0xfff0f0e0, width: ThumbWidth::Word, format: "smmul%4'r%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxtah, opcode: 0xfa00f080, mask: 0xfff0f0c0, width: ThumbWidth::Word, format: "sxtah%c\t%8-11r, %16-19r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxtah, opcode: 0xfa10f080, mask: 0xfff0f0c0, width: ThumbWidth::Word, format: "uxtah%c\t%8-11r, %16-19r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxtab16, opcode: 0xfa20f080, mask: 0xfff0f0c0, width: ThumbWidth::Word, format: "sxtab16%c\t%8-11r, %16-19r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxtab16, opcode: 0xfa30f080, mask: 0xfff0f0c0, width: ThumbWidth::Word, format: "uxtab16%c\t%8-11r, %16-19r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sxtab, opcode: 0xfa40f080, mask: 0xfff0f0c0, width: ThumbWidth::Word, format: "sxtab%c\t%8-11r, %16-19r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Uxtab, opcode: 0xfa50f080, mask: 0xfff0f0c0, width: ThumbWidth::Word, format: "uxtab%c\t%8-11r, %16-19r, %0-3r%R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smul, opcode: 0xfb10f000, mask: 0xfff0f0c0, width: ThumbWidth::Word, format: "smul%5?tb%4?tb%c\t%8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bfc, opcode: 0xf36f0000, mask: 0xffff8020, width: ThumbWidth::Word, format: "bfc%c\t%8-11r, %E" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Tst, opcode: 0xea100f00, mask: 0xfff08f00, width: ThumbWidth::Word, format: "tst%c.w\t%16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Teq, opcode: 0xea900f00, mask: 0xfff08f00, width: ThumbWidth::Word, format: "teq%c\t%16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmn, opcode: 0xeb100f00, mask: 0xfff08f00, width: ThumbWidth::Word, format: "cmn%c.w\t%16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmp, opcode: 0xebb00f00, mask: 0xfff08f00, width: ThumbWidth::Word, format: "cmp%c.w\t%16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Tst, opcode: 0xf0100f00, mask: 0xfbf08f00, width: ThumbWidth::Word, format: "tst%c.w\t%16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Teq, opcode: 0xf0900f00, mask: 0xfbf08f00, width: ThumbWidth::Word, format: "teq%c\t%16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmn, opcode: 0xf1100f00, mask: 0xfbf08f00, width: ThumbWidth::Word, format: "cmn%c.w\t%16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Cmp, opcode: 0xf1b00f00, mask: 0xfbf08f00, width: ThumbWidth::Word, format: "cmp%c.w\t%16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mov, opcode: 0xea4f0000, mask: 0xffef8000, width: ThumbWidth::Word, format: "mov%20's%c.w\t%8-11r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mvn, opcode: 0xea6f0000, mask: 0xffef8000, width: ThumbWidth::Word, format: "mvn%20's%c.w\t%8-11r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strexd, opcode: 0xe8c00070, mask: 0xfff000f0, width: ThumbWidth::Word, format: "strexd%c\t%0-3r, %12-15r, %8-11r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mla, opcode: 0xfb000000, mask: 0xfff000f0, width: ThumbWidth::Word, format: "mla%c\t%8-11r, %16-19r, %0-3r, %12-15r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mls, opcode: 0xfb000010, mask: 0xfff000f0, width: ThumbWidth::Word, format: "mls%c\t%8-11r, %16-19r, %0-3r, %12-15r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Usada8, opcode: 0xfb700000, mask: 0xfff000f0, width: ThumbWidth::Word, format: "usada8%c\t%8-11R, %16-19R, %0-3R, %12-15R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smull, opcode: 0xfb800000, mask: 0xfff000f0, width: ThumbWidth::Word, format: "smull%c\t%12-15R, %8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Umull, opcode: 0xfba00000, mask: 0xfff000f0, width: ThumbWidth::Word, format: "umull%c\t%12-15R, %8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smlal, opcode: 0xfbc00000, mask: 0xfff000f0, width: ThumbWidth::Word, format: "smlal%c\t%12-15R, %8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Umlal, opcode: 0xfbe00000, mask: 0xfff000f0, width: ThumbWidth::Word, format: "umlal%c\t%12-15R, %8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Umaal, opcode: 0xfbe00060, mask: 0xfff000f0, width: ThumbWidth::Word, format: "umaal%c\t%12-15R, %8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrex, opcode: 0xe8500f00, mask: 0xfff00f00, width: ThumbWidth::Word, format: "ldrex%c\t%12-15r, [%16-19r, %{I:#%0-7W%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mov, opcode: 0xf04f0000, mask: 0xfbef8000, width: ThumbWidth::Word, format: "mov%20's%c.w\t%8-11r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Mvn, opcode: 0xf06f0000, mask: 0xfbef8000, width: ThumbWidth::Word, format: "mvn%20's%c.w\t%8-11r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pld, opcode: 0xf810f000, mask: 0xff70f000, width: ThumbWidth::Word, format: "pld%c\t%a" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smlad, opcode: 0xfb200000, mask: 0xfff000e0, width: ThumbWidth::Word, format: "smlad%4'x%c\t%8-11R, %16-19R, %0-3R, %12-15R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smlaw, opcode: 0xfb300000, mask: 0xfff000e0, width: ThumbWidth::Word, format: "smlaw%4?tb%c\t%8-11R, %16-19R, %0-3R, %12-15R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smlsd, opcode: 0xfb400000, mask: 0xfff000e0, width: ThumbWidth::Word, format: "smlsd%4'x%c\t%8-11R, %16-19R, %0-3R, %12-15R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smmla, opcode: 0xfb500000, mask: 0xfff000e0, width: ThumbWidth::Word, format: "smmla%4'r%c\t%8-11R, %16-19R, %0-3R, %12-15R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smmls, opcode: 0xfb600000, mask: 0xfff000e0, width: ThumbWidth::Word, format: "smmls%4'r%c\t%8-11R, %16-19R, %0-3R, %12-15R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smlald, opcode: 0xfbc000c0, mask: 0xfff000e0, width: ThumbWidth::Word, format: "smlald%4'x%c\t%12-15R, %8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smlsld, opcode: 0xfbd000c0, mask: 0xfff000e0, width: ThumbWidth::Word, format: "smlsld%4'x%c\t%12-15R, %8-11R, %16-19R, %0-3R" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pkhbt, opcode: 0xeac00000, mask: 0xfff08030, width: ThumbWidth::Word, format: "pkhbt%c\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Pkhtb, opcode: 0xeac00020, mask: 0xfff08030, width: ThumbWidth::Word, format: "pkhtb%c\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sbfx, opcode: 0xf3400000, mask: 0xfff08020, width: ThumbWidth::Word, format: "sbfx%c\t%8-11r, %16-19r, %F" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ubfx, opcode: 0xf3c00000, mask: 0xfff08020, width: ThumbWidth::Word, format: "ubfx%c\t%8-11r, %16-19r, %F" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Str, opcode: 0xf8000e00, mask: 0xff900f00, width: ThumbWidth::Word, format: "str%wt%c\t%12-15r, %a" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smla, opcode: 0xfb100000, mask: 0xfff000c0, width: ThumbWidth::Word, format: "smla%5?tb%4?tb%c\t%8-11r, %16-19r, %0-3r, %12-15r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Smlal, opcode: 0xfbc00080, mask: 0xfff000c0, width: ThumbWidth::Word, format: "smlal%5?tb%4?tb%c\t%12-15r, %8-11r, %16-19r, %0-3r" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bfi, opcode: 0xf3600000, mask: 0xfff08020, width: ThumbWidth::Word, format: "bfi%c\t%8-11r, %16-19r, %E" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldr, opcode: 0xf8100e00, mask: 0xfe900f00, width: ThumbWidth::Word, format: "ldr%wt%c\t%12-15r, %a" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ssat, opcode: 0xf3000000, mask: 0xffd08020, width: ThumbWidth::Word, format: "ssat%c\t%8-11r, %{I:#%0-4D%}, %16-19r%s" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Usat, opcode: 0xf3800000, mask: 0xffd08020, width: ThumbWidth::Word, format: "usat%c\t%8-11r, %{I:#%0-4d%}, %16-19r%s" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Addw, opcode: 0xf2000000, mask: 0xfbf08000, width: ThumbWidth::Word, format: "addw%c\t%8-11r, %16-19r, %I" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Movw, opcode: 0xf2400000, mask: 0xfbf08000, width: ThumbWidth::Word, format: "movw%c\t%8-11r, %J" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Subw, opcode: 0xf2a00000, mask: 0xfbf08000, width: ThumbWidth::Word, format: "subw%c\t%8-11r, %16-19r, %I" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Movt, opcode: 0xf2c00000, mask: 0xfbf08000, width: ThumbWidth::Word, format: "movt%c\t%8-11r, %J" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::And, opcode: 0xea000000, mask: 0xffe08000, width: ThumbWidth::Word, format: "and%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bic, opcode: 0xea200000, mask: 0xffe08000, width: ThumbWidth::Word, format: "bic%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Orr, opcode: 0xea400000, mask: 0xffe08000, width: ThumbWidth::Word, format: "orr%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Orn, opcode: 0xea600000, mask: 0xffe08000, width: ThumbWidth::Word, format: "orn%20's%c\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Eor, opcode: 0xea800000, mask: 0xffe08000, width: ThumbWidth::Word, format: "eor%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0xeb000000, mask: 0xffe08000, width: ThumbWidth::Word, format: "add%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Adc, opcode: 0xeb400000, mask: 0xffe08000, width: ThumbWidth::Word, format: "adc%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sbc, opcode: 0xeb600000, mask: 0xffe08000, width: ThumbWidth::Word, format: "sbc%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sub, opcode: 0xeba00000, mask: 0xffe08000, width: ThumbWidth::Word, format: "sub%20's%c.w\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rsb, opcode: 0xebc00000, mask: 0xffe08000, width: ThumbWidth::Word, format: "rsb%20's%c\t%8-11r, %16-19r, %S" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strex, opcode: 0xe8400000, mask: 0xfff00000, width: ThumbWidth::Word, format: "strex%c\t%8-11r, %12-15r, [%16-19r, %{I:#%0-7W%}]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::And, opcode: 0xf0000000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "and%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bic, opcode: 0xf0200000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "bic%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Orr, opcode: 0xf0400000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "orr%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Orn, opcode: 0xf0600000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "orn%20's%c\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Eor, opcode: 0xf0800000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "eor%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Add, opcode: 0xf1000000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "add%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Adc, opcode: 0xf1400000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "adc%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sbc, opcode: 0xf1600000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "sbc%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Sub, opcode: 0xf1a00000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "sub%20's%c.w\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Rsb, opcode: 0xf1c00000, mask: 0xfbe08000, width: ThumbWidth::Word, format: "rsb%20's%c\t%8-11r, %16-19r, %M" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stmia, opcode: 0xe8800000, mask: 0xffd00000, width: ThumbWidth::Word, format: "stmia%c.w\t%16-19r%21'!, %m" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldmia, opcode: 0xe8900000, mask: 0xffd00000, width: ThumbWidth::Word, format: "ldmia%c.w\t%16-19r%21'!, %m" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Stmdb, opcode: 0xe9000000, mask: 0xffd00000, width: ThumbWidth::Word, format: "stmdb%c\t%16-19r%21'!, %m" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldmdb, opcode: 0xe9100000, mask: 0xffd00000, width: ThumbWidth::Word, format: "ldmdb%c\t%16-19r%21'!, %m" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strd, opcode: 0xe9c00000, mask: 0xffd000ff, width: ThumbWidth::Word, format: "strd%c\t%12-15r, %8-11r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrd, opcode: 0xe9d00000, mask: 0xffd000ff, width: ThumbWidth::Word, format: "ldrd%c\t%12-15r, %8-11r, [%16-19r]" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strd, opcode: 0xe9400000, mask: 0xff500000, width: ThumbWidth::Word, format: "strd%c\t%12-15r, %8-11r, [%16-19r, %{I:#%23`-%0-7W%}]%21'!%L" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrd, opcode: 0xe9500000, mask: 0xff500000, width: ThumbWidth::Word, format: "ldrd%c\t%12-15r, %8-11r, [%16-19r, %{I:#%23`-%0-7W%}]%21'!%L" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Strd, opcode: 0xe8600000, mask: 0xff700000, width: ThumbWidth::Word, format: "strd%c\t%12-15r, %8-11r, [%16-19r], %{I:#%23`-%0-7W%}%L" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldrd, opcode: 0xe8700000, mask: 0xff700000, width: ThumbWidth::Word, format: "ldrd%c\t%12-15r, %8-11r, [%16-19r], %{I:#%23`-%0-7W%}%L" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Str, opcode: 0xf8000000, mask: 0xff100000, width: ThumbWidth::Word, format: "str%w%c.w\t%12-15r, %a" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Ldr, opcode: 0xf8100000, mask: 0xfe100000, width: ThumbWidth::Word, format: "ldr%w%c.w\t%12-15r, %a" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Undefined, opcode: 0xf3c08000, mask: 0xfbc0d000, width: ThumbWidth::Word, format: "undefined (bcc, cond=0xF)" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Undefined, opcode: 0xf3808000, mask: 0xfbc0d000, width: ThumbWidth::Word, format: "undefined (bcc, cond=0xE)" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::B, opcode: 0xf0008000, mask: 0xf800d000, width: ThumbWidth::Word, format: "b%22-25c.w\t%b%X" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::B, opcode: 0xf0009000, mask: 0xf800d000, width: ThumbWidth::Word, format: "b%c.w\t%B%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Blx, opcode: 0xf000c000, mask: 0xf800d001, width: ThumbWidth::Word, format: "blx%c\t%B%x" },
    ThumbOpcodeGenerated { mnemonic: ThumbMnemonicGenerated::Bl, opcode: 0xf000d000, mask: 0xf800d000, width: ThumbWidth::Word, format: "bl%c\t%B%x" },
];

/// Auto-generated row type. Carries the binutils format
/// string verbatim so callers can pattern-match operand
/// shapes against it without re-parsing.
#[derive(Debug, Copy, Clone)]
pub struct ThumbOpcodeGenerated {
    pub mnemonic: ThumbMnemonicGenerated,
    pub opcode: u32,
    pub mask: u32,
    pub width: ThumbWidth,
    pub format: &'static str,
}

impl ThumbOpcodeGenerated {
    /// Bit ranges (within the 32-bit working word) occupied
    /// by each operand of this opcode, in left-to-right
    /// order. See [`super::format_bit_ranges`] for the
    /// extraction model and word-layout convention. Empty
    /// inner Vec means the operand's bit pattern isn't
    /// modelled.
    pub fn operand_bit_ranges(&self) -> Vec<Vec<std::ops::Range<u8>>> {
        let width = match self.width {
            ThumbWidth::Halfword => 2,
            ThumbWidth::Word => 4,
        };
        super::format_bit_ranges::extract_operand_bit_ranges(self.format, width)
    }
}

/// Find the first generated table entry whose mask + opcode
/// matches the input word for the given width.
pub fn match_generated(word: u32, width: ThumbWidth) -> Option<&'static ThumbOpcodeGenerated> {
    THUMB_OPCODE_TABLE_GENERATED
        .iter()
        .find(|row| row.width == width && (word & row.mask) == row.opcode)
}

/// Iterate every Thumb opcode row in the static table. Intended
/// for external tooling that builds its own index (e.g. an
/// assembler / autocomplete UI) over the full table without
/// taking a heap copy. Mirrors `aarch64::iter_opcodes`.
pub fn iter_opcodes() -> impl Iterator<Item = &'static ThumbOpcodeGenerated> {
    THUMB_OPCODE_TABLE_GENERATED.iter()
}

