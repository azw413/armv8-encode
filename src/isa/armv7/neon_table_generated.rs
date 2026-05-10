// AUTO-GENERATED — do not edit by hand.
//
// Regenerate with:
//
//   python3 tools/import_thumb_opcodes.py PATH_TO_arm-dis.c neon \
//       > src/isa/armv7/neon_table_generated.rs
//
// Source: GNU binutils opcodes/arm-dis.c — neon_opcodes,
// coprocessor_opcodes, generic_coprocessor_opcodes arrays
// (GPL-2.0-or-later).
//
// Each row carries an `IsaApplicability` tag controlling
// which decoder paths consider it: NEON rows apply to both
// modes (with a small Thumb-to-ARM word transform performed
// at match time); coprocessor rows are tagged ARM/Thumb/Any
// based on binutils' ANY/T32/ARM selector.

#![allow(dead_code, non_camel_case_types)]

/// Mode-applicability tag from binutils' table sources.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum IsaApplicability {
    /// NEON row — applies to both ARM and Thumb encodings.
    /// The Thumb encoding requires a small word-level
    /// transform before matching (see `normalise_neon_thumb`).
    Neon,
    /// Coprocessor row applicable to both ARM and Thumb.
    Any,
    /// ARM-only.
    Arm,
    /// Thumb-only.
    Thumb,
}

/// Auto-generated mnemonic enum for NEON / VFP / coprocessor
/// instructions.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum NeonMnemonicGenerated {
    /// `abs`
    Abs,
    /// `acs`
    Acs,
    /// `adf`
    Adf,
    /// `aesd`
    Aesd,
    /// `aese`
    Aese,
    /// `aesimc`
    Aesimc,
    /// `aesmc`
    Aesmc,
    /// `asn`
    Asn,
    /// `atn`
    Atn,
    /// `cdp`
    Cdp,
    /// `cdp2`
    Cdp2,
    /// `cfabs32`
    Cfabs32,
    /// `cfabs64`
    Cfabs64,
    /// `cfabsd`
    Cfabsd,
    /// `cfabss`
    Cfabss,
    /// `cfadd32`
    Cfadd32,
    /// `cfadd64`
    Cfadd64,
    /// `cfaddd`
    Cfaddd,
    /// `cfadds`
    Cfadds,
    /// `cfcmp32`
    Cfcmp32,
    /// `cfcmp64`
    Cfcmp64,
    /// `cfcmpd`
    Cfcmpd,
    /// `cfcmps`
    Cfcmps,
    /// `cfcpyd`
    Cfcpyd,
    /// `cfcpys`
    Cfcpys,
    /// `cfcvt32d`
    Cfcvt32d,
    /// `cfcvt32s`
    Cfcvt32s,
    /// `cfcvt64d`
    Cfcvt64d,
    /// `cfcvt64s`
    Cfcvt64s,
    /// `cfcvtd32`
    Cfcvtd32,
    /// `cfcvtds`
    Cfcvtds,
    /// `cfcvts32`
    Cfcvts32,
    /// `cfcvtsd`
    Cfcvtsd,
    /// `cfldr32`
    Cfldr32,
    /// `cfldr64`
    Cfldr64,
    /// `cfldrd`
    Cfldrd,
    /// `cfldrs`
    Cfldrs,
    /// `cfmac32`
    Cfmac32,
    /// `cfmadd32`
    Cfmadd32,
    /// `cfmadda32`
    Cfmadda32,
    /// `cfmsc32`
    Cfmsc32,
    /// `cfmsub32`
    Cfmsub32,
    /// `cfmsuba32`
    Cfmsuba32,
    /// `cfmul32`
    Cfmul32,
    /// `cfmul64`
    Cfmul64,
    /// `cfmuld`
    Cfmuld,
    /// `cfmuls`
    Cfmuls,
    /// `cfmv32a`
    Cfmv32a,
    /// `cfmv32ah`
    Cfmv32ah,
    /// `cfmv32al`
    Cfmv32al,
    /// `cfmv32am`
    Cfmv32am,
    /// `cfmv32sc`
    Cfmv32sc,
    /// `cfmv64a`
    Cfmv64a,
    /// `cfmv64hr`
    Cfmv64hr,
    /// `cfmv64lr`
    Cfmv64lr,
    /// `cfmva32`
    Cfmva32,
    /// `cfmva64`
    Cfmva64,
    /// `cfmvah32`
    Cfmvah32,
    /// `cfmval32`
    Cfmval32,
    /// `cfmvam32`
    Cfmvam32,
    /// `cfmvdhr`
    Cfmvdhr,
    /// `cfmvdlr`
    Cfmvdlr,
    /// `cfmvr64h`
    Cfmvr64h,
    /// `cfmvr64l`
    Cfmvr64l,
    /// `cfmvrdh`
    Cfmvrdh,
    /// `cfmvrdl`
    Cfmvrdl,
    /// `cfmvrs`
    Cfmvrs,
    /// `cfmvsc32`
    Cfmvsc32,
    /// `cfmvsr`
    Cfmvsr,
    /// `cfneg32`
    Cfneg32,
    /// `cfneg64`
    Cfneg64,
    /// `cfnegd`
    Cfnegd,
    /// `cfnegs`
    Cfnegs,
    /// `cfrshl32`
    Cfrshl32,
    /// `cfrshl64`
    Cfrshl64,
    /// `cfsh32`
    Cfsh32,
    /// `cfsh64`
    Cfsh64,
    /// `cfstr32`
    Cfstr32,
    /// `cfstr64`
    Cfstr64,
    /// `cfstrd`
    Cfstrd,
    /// `cfstrs`
    Cfstrs,
    /// `cfsub32`
    Cfsub32,
    /// `cfsub64`
    Cfsub64,
    /// `cfsubd`
    Cfsubd,
    /// `cfsubs`
    Cfsubs,
    /// `cftruncd32`
    Cftruncd32,
    /// `cftruncs32`
    Cftruncs32,
    /// `cmf`
    Cmf,
    /// `cmfe`
    Cmfe,
    /// `cnf`
    Cnf,
    /// `cnfe`
    Cnfe,
    /// `cos`
    Cos,
    /// `dvf`
    Dvf,
    /// `exp`
    Exp,
    /// `fdv`
    Fdv,
    /// `fix`
    Fix,
    /// `fldmdbx`
    Fldmdbx,
    /// `fldmiax`
    Fldmiax,
    /// `flt`
    Flt,
    /// `fml`
    Fml,
    /// `frd`
    Frd,
    /// `fstmdbx`
    Fstmdbx,
    /// `fstmiax`
    Fstmiax,
    /// `ldc`
    Ldc,
    /// `ldc2`
    Ldc2,
    /// `ldf`
    Ldf,
    /// `lfm`
    Lfm,
    /// `lgn`
    Lgn,
    /// `log`
    Log,
    /// `mar`
    Mar,
    /// `mcr`
    Mcr,
    /// `mcr2`
    Mcr2,
    /// `mcrr`
    Mcrr,
    /// `mcrr2`
    Mcrr2,
    /// `mia`
    Mia,
    /// `miaph`
    Miaph,
    /// `mnf`
    Mnf,
    /// `mra`
    Mra,
    /// `mrc`
    Mrc,
    /// `mrc2`
    Mrc2,
    /// `mrrc`
    Mrrc,
    /// `mrrc2`
    Mrrc2,
    /// `muf`
    Muf,
    /// `mvf`
    Mvf,
    /// `nrm`
    Nrm,
    /// `pol`
    Pol,
    /// `pow`
    Pow,
    /// `rdf`
    Rdf,
    /// `rfc`
    Rfc,
    /// `rfs`
    Rfs,
    /// `rmf`
    Rmf,
    /// `rnd`
    Rnd,
    /// `rpw`
    Rpw,
    /// `rsf`
    Rsf,
    /// `sfm`
    Sfm,
    /// `sha1c`
    Sha1c,
    /// `sha1h`
    Sha1h,
    /// `sha1m`
    Sha1m,
    /// `sha1p`
    Sha1p,
    /// `sha1su0`
    Sha1su0,
    /// `sha1su1`
    Sha1su1,
    /// `sha256h`
    Sha256h,
    /// `sha256h2`
    Sha256h2,
    /// `sha256su0`
    Sha256su0,
    /// `sha256su1`
    Sha256su1,
    /// `sin`
    Sin,
    /// `sqt`
    Sqt,
    /// `stc`
    Stc,
    /// `stc2`
    Stc2,
    /// `stf`
    Stf,
    /// `suf`
    Suf,
    /// `tan`
    Tan,
    /// `tandc`
    Tandc,
    /// `tbcst`
    Tbcst,
    /// `textrc`
    Textrc,
    /// `textrm`
    Textrm,
    /// `tinsr`
    Tinsr,
    /// `tmcr`
    Tmcr,
    /// `tmcrr`
    Tmcrr,
    /// `tmia`
    Tmia,
    /// `tmiaph`
    Tmiaph,
    /// `tmovmsk`
    Tmovmsk,
    /// `tmrc`
    Tmrc,
    /// `tmrrc`
    Tmrrc,
    /// `torc`
    Torc,
    /// `torvsc`
    Torvsc,
    /// `urd`
    Urd,
    /// `v`
    V,
    /// `vaba`
    Vaba,
    /// `vabal`
    Vabal,
    /// `vabd`
    Vabd,
    /// `vabdl`
    Vabdl,
    /// `vabs`
    Vabs,
    /// `vacge`
    Vacge,
    /// `vacgt`
    Vacgt,
    /// `vadd`
    Vadd,
    /// `vaddhn`
    Vaddhn,
    /// `vaddl`
    Vaddl,
    /// `vaddw`
    Vaddw,
    /// `vand`
    Vand,
    /// `vbic`
    Vbic,
    /// `vbif`
    Vbif,
    /// `vbit`
    Vbit,
    /// `vbsl`
    Vbsl,
    /// `vcadd`
    Vcadd,
    /// `vceq`
    Vceq,
    /// `vcge`
    Vcge,
    /// `vcgt`
    Vcgt,
    /// `vcle`
    Vcle,
    /// `vcls`
    Vcls,
    /// `vclt`
    Vclt,
    /// `vclz`
    Vclz,
    /// `vcmla`
    Vcmla,
    /// `vcmp`
    Vcmp,
    /// `vcnt`
    Vcnt,
    /// `vcvt`
    Vcvt,
    /// `vdiv`
    Vdiv,
    /// `vdot.bf16`
    VdotBf16,
    /// `vdup`
    Vdup,
    /// `veor`
    Veor,
    /// `vext`
    Vext,
    /// `vfma`
    Vfma,
    /// `vfmal.f16`
    VfmalF16,
    /// `vfms`
    Vfms,
    /// `vfmsl.f16`
    VfmslF16,
    /// `vfnma`
    Vfnma,
    /// `vfnms`
    Vfnms,
    /// `vhadd`
    Vhadd,
    /// `vhsub`
    Vhsub,
    /// `vins.f16`
    VinsF16,
    /// `vjcvt`
    Vjcvt,
    /// `vld1`
    Vld1,
    /// `vld2`
    Vld2,
    /// `vld3`
    Vld3,
    /// `vld4`
    Vld4,
    /// `vldmdb`
    Vldmdb,
    /// `vldmia`
    Vldmia,
    /// `vldr`
    Vldr,
    /// `vlldm`
    Vlldm,
    /// `vlstm`
    Vlstm,
    /// `vmax`
    Vmax,
    /// `vmaxnm`
    Vmaxnm,
    /// `vmin`
    Vmin,
    /// `vminnm`
    Vminnm,
    /// `vmla`
    Vmla,
    /// `vmlal`
    Vmlal,
    /// `vmls`
    Vmls,
    /// `vmlsl`
    Vmlsl,
    /// `vmmla.bf16`
    VmmlaBf16,
    /// `vmov`
    Vmov,
    /// `vmovl`
    Vmovl,
    /// `vmovn`
    Vmovn,
    /// `vmovx`
    Vmovx,
    /// `vmrs`
    Vmrs,
    /// `vmsr`
    Vmsr,
    /// `vmul`
    Vmul,
    /// `vmull`
    Vmull,
    /// `vmvn`
    Vmvn,
    /// `vneg`
    Vneg,
    /// `vnmla`
    Vnmla,
    /// `vnmls`
    Vnmls,
    /// `vnmul`
    Vnmul,
    /// `vorn`
    Vorn,
    /// `vorr`
    Vorr,
    /// `vpadal`
    Vpadal,
    /// `vpadd`
    Vpadd,
    /// `vpaddl`
    Vpaddl,
    /// `vpmax`
    Vpmax,
    /// `vpmin`
    Vpmin,
    /// `vpop`
    Vpop,
    /// `vpush`
    Vpush,
    /// `vqabs`
    Vqabs,
    /// `vqadd`
    Vqadd,
    /// `vqdmlal`
    Vqdmlal,
    /// `vqdmlsl`
    Vqdmlsl,
    /// `vqdmulh`
    Vqdmulh,
    /// `vqdmull`
    Vqdmull,
    /// `vqmovn`
    Vqmovn,
    /// `vqmovun`
    Vqmovun,
    /// `vqneg`
    Vqneg,
    /// `vqrdmlah`
    Vqrdmlah,
    /// `vqrdmlsh`
    Vqrdmlsh,
    /// `vqrdmulh`
    Vqrdmulh,
    /// `vqrshl`
    Vqrshl,
    /// `vqrshrn`
    Vqrshrn,
    /// `vqrshrun`
    Vqrshrun,
    /// `vqshl`
    Vqshl,
    /// `vqshlu`
    Vqshlu,
    /// `vqshrn`
    Vqshrn,
    /// `vqshrun`
    Vqshrun,
    /// `vqsub`
    Vqsub,
    /// `vraddhn`
    Vraddhn,
    /// `vrecpe`
    Vrecpe,
    /// `vrecps`
    Vrecps,
    /// `vrev16`
    Vrev16,
    /// `vrev32`
    Vrev32,
    /// `vrev64`
    Vrev64,
    /// `vrhadd`
    Vrhadd,
    /// `vrint`
    Vrint,
    /// `vrshl`
    Vrshl,
    /// `vrshr`
    Vrshr,
    /// `vrshrn`
    Vrshrn,
    /// `vrsqrte`
    Vrsqrte,
    /// `vrsqrts`
    Vrsqrts,
    /// `vrsra`
    Vrsra,
    /// `vrsubhn`
    Vrsubhn,
    /// `vscclrm`
    Vscclrm,
    /// `vsel`
    Vsel,
    /// `vshl`
    Vshl,
    /// `vshll`
    Vshll,
    /// `vshr`
    Vshr,
    /// `vshrn`
    Vshrn,
    /// `vsli`
    Vsli,
    /// `vsmmla.s8`
    VsmmlaS8,
    /// `vsqrt`
    Vsqrt,
    /// `vsra`
    Vsra,
    /// `vsri`
    Vsri,
    /// `vstmdb`
    Vstmdb,
    /// `vstmia`
    Vstmia,
    /// `vstr`
    Vstr,
    /// `vsub`
    Vsub,
    /// `vsubhn`
    Vsubhn,
    /// `vsubl`
    Vsubl,
    /// `vsubw`
    Vsubw,
    /// `vsudot.u8`
    VsudotU8,
    /// `vswp`
    Vswp,
    /// `vtbl`
    Vtbl,
    /// `vtbx`
    Vtbx,
    /// `vtrn`
    Vtrn,
    /// `vtst`
    Vtst,
    /// `vummla.u8`
    VummlaU8,
    /// `vusdot.s8`
    VusdotS8,
    /// `vusmmla.s8`
    VusmmlaS8,
    /// `vuzp`
    Vuzp,
    /// `vzip`
    Vzip,
    /// `wabs`
    Wabs,
    /// `wabsdiff`
    Wabsdiff,
    /// `wacc`
    Wacc,
    /// `wadd`
    Wadd,
    /// `waddbhus`
    Waddbhus,
    /// `waddsubhx`
    Waddsubhx,
    /// `waligni`
    Waligni,
    /// `walignr`
    Walignr,
    /// `wand`
    Wand,
    /// `wavg2`
    Wavg2,
    /// `wavg4`
    Wavg4,
    /// `wcmpeq`
    Wcmpeq,
    /// `wcmpgt`
    Wcmpgt,
    /// `wfc`
    Wfc,
    /// `wfs`
    Wfs,
    /// `wldr`
    Wldr,
    /// `wldrd`
    Wldrd,
    /// `wldrw`
    Wldrw,
    /// `wmac`
    Wmac,
    /// `wmadd`
    Wmadd,
    /// `wmax`
    Wmax,
    /// `wmerge`
    Wmerge,
    /// `wmia`
    Wmia,
    /// `wmiaw`
    Wmiaw,
    /// `wmin`
    Wmin,
    /// `wmul`
    Wmul,
    /// `wmulwl`
    Wmulwl,
    /// `wmulwsm`
    Wmulwsm,
    /// `wmulwum`
    Wmulwum,
    /// `wor`
    Wor,
    /// `wpack`
    Wpack,
    /// `wqmia`
    Wqmia,
    /// `wqmulm`
    Wqmulm,
    /// `wqmulwm`
    Wqmulwm,
    /// `wror`
    Wror,
    /// `wsad`
    Wsad,
    /// `wshufh`
    Wshufh,
    /// `wsll`
    Wsll,
    /// `wsra`
    Wsra,
    /// `wsrl`
    Wsrl,
    /// `wstr`
    Wstr,
    /// `wstrd`
    Wstrd,
    /// `wstrw`
    Wstrw,
    /// `wsub`
    Wsub,
    /// `wsubaddhx`
    Wsubaddhx,
    /// `wunpckeh`
    Wunpckeh,
    /// `wunpckel`
    Wunpckel,
    /// `wunpckih`
    Wunpckih,
    /// `wunpckil`
    Wunpckil,
    /// `wxor`
    Wxor,
}

impl NeonMnemonicGenerated {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Abs => "abs",
            Self::Acs => "acs",
            Self::Adf => "adf",
            Self::Aesd => "aesd",
            Self::Aese => "aese",
            Self::Aesimc => "aesimc",
            Self::Aesmc => "aesmc",
            Self::Asn => "asn",
            Self::Atn => "atn",
            Self::Cdp => "cdp",
            Self::Cdp2 => "cdp2",
            Self::Cfabs32 => "cfabs32",
            Self::Cfabs64 => "cfabs64",
            Self::Cfabsd => "cfabsd",
            Self::Cfabss => "cfabss",
            Self::Cfadd32 => "cfadd32",
            Self::Cfadd64 => "cfadd64",
            Self::Cfaddd => "cfaddd",
            Self::Cfadds => "cfadds",
            Self::Cfcmp32 => "cfcmp32",
            Self::Cfcmp64 => "cfcmp64",
            Self::Cfcmpd => "cfcmpd",
            Self::Cfcmps => "cfcmps",
            Self::Cfcpyd => "cfcpyd",
            Self::Cfcpys => "cfcpys",
            Self::Cfcvt32d => "cfcvt32d",
            Self::Cfcvt32s => "cfcvt32s",
            Self::Cfcvt64d => "cfcvt64d",
            Self::Cfcvt64s => "cfcvt64s",
            Self::Cfcvtd32 => "cfcvtd32",
            Self::Cfcvtds => "cfcvtds",
            Self::Cfcvts32 => "cfcvts32",
            Self::Cfcvtsd => "cfcvtsd",
            Self::Cfldr32 => "cfldr32",
            Self::Cfldr64 => "cfldr64",
            Self::Cfldrd => "cfldrd",
            Self::Cfldrs => "cfldrs",
            Self::Cfmac32 => "cfmac32",
            Self::Cfmadd32 => "cfmadd32",
            Self::Cfmadda32 => "cfmadda32",
            Self::Cfmsc32 => "cfmsc32",
            Self::Cfmsub32 => "cfmsub32",
            Self::Cfmsuba32 => "cfmsuba32",
            Self::Cfmul32 => "cfmul32",
            Self::Cfmul64 => "cfmul64",
            Self::Cfmuld => "cfmuld",
            Self::Cfmuls => "cfmuls",
            Self::Cfmv32a => "cfmv32a",
            Self::Cfmv32ah => "cfmv32ah",
            Self::Cfmv32al => "cfmv32al",
            Self::Cfmv32am => "cfmv32am",
            Self::Cfmv32sc => "cfmv32sc",
            Self::Cfmv64a => "cfmv64a",
            Self::Cfmv64hr => "cfmv64hr",
            Self::Cfmv64lr => "cfmv64lr",
            Self::Cfmva32 => "cfmva32",
            Self::Cfmva64 => "cfmva64",
            Self::Cfmvah32 => "cfmvah32",
            Self::Cfmval32 => "cfmval32",
            Self::Cfmvam32 => "cfmvam32",
            Self::Cfmvdhr => "cfmvdhr",
            Self::Cfmvdlr => "cfmvdlr",
            Self::Cfmvr64h => "cfmvr64h",
            Self::Cfmvr64l => "cfmvr64l",
            Self::Cfmvrdh => "cfmvrdh",
            Self::Cfmvrdl => "cfmvrdl",
            Self::Cfmvrs => "cfmvrs",
            Self::Cfmvsc32 => "cfmvsc32",
            Self::Cfmvsr => "cfmvsr",
            Self::Cfneg32 => "cfneg32",
            Self::Cfneg64 => "cfneg64",
            Self::Cfnegd => "cfnegd",
            Self::Cfnegs => "cfnegs",
            Self::Cfrshl32 => "cfrshl32",
            Self::Cfrshl64 => "cfrshl64",
            Self::Cfsh32 => "cfsh32",
            Self::Cfsh64 => "cfsh64",
            Self::Cfstr32 => "cfstr32",
            Self::Cfstr64 => "cfstr64",
            Self::Cfstrd => "cfstrd",
            Self::Cfstrs => "cfstrs",
            Self::Cfsub32 => "cfsub32",
            Self::Cfsub64 => "cfsub64",
            Self::Cfsubd => "cfsubd",
            Self::Cfsubs => "cfsubs",
            Self::Cftruncd32 => "cftruncd32",
            Self::Cftruncs32 => "cftruncs32",
            Self::Cmf => "cmf",
            Self::Cmfe => "cmfe",
            Self::Cnf => "cnf",
            Self::Cnfe => "cnfe",
            Self::Cos => "cos",
            Self::Dvf => "dvf",
            Self::Exp => "exp",
            Self::Fdv => "fdv",
            Self::Fix => "fix",
            Self::Fldmdbx => "fldmdbx",
            Self::Fldmiax => "fldmiax",
            Self::Flt => "flt",
            Self::Fml => "fml",
            Self::Frd => "frd",
            Self::Fstmdbx => "fstmdbx",
            Self::Fstmiax => "fstmiax",
            Self::Ldc => "ldc",
            Self::Ldc2 => "ldc2",
            Self::Ldf => "ldf",
            Self::Lfm => "lfm",
            Self::Lgn => "lgn",
            Self::Log => "log",
            Self::Mar => "mar",
            Self::Mcr => "mcr",
            Self::Mcr2 => "mcr2",
            Self::Mcrr => "mcrr",
            Self::Mcrr2 => "mcrr2",
            Self::Mia => "mia",
            Self::Miaph => "miaph",
            Self::Mnf => "mnf",
            Self::Mra => "mra",
            Self::Mrc => "mrc",
            Self::Mrc2 => "mrc2",
            Self::Mrrc => "mrrc",
            Self::Mrrc2 => "mrrc2",
            Self::Muf => "muf",
            Self::Mvf => "mvf",
            Self::Nrm => "nrm",
            Self::Pol => "pol",
            Self::Pow => "pow",
            Self::Rdf => "rdf",
            Self::Rfc => "rfc",
            Self::Rfs => "rfs",
            Self::Rmf => "rmf",
            Self::Rnd => "rnd",
            Self::Rpw => "rpw",
            Self::Rsf => "rsf",
            Self::Sfm => "sfm",
            Self::Sha1c => "sha1c",
            Self::Sha1h => "sha1h",
            Self::Sha1m => "sha1m",
            Self::Sha1p => "sha1p",
            Self::Sha1su0 => "sha1su0",
            Self::Sha1su1 => "sha1su1",
            Self::Sha256h => "sha256h",
            Self::Sha256h2 => "sha256h2",
            Self::Sha256su0 => "sha256su0",
            Self::Sha256su1 => "sha256su1",
            Self::Sin => "sin",
            Self::Sqt => "sqt",
            Self::Stc => "stc",
            Self::Stc2 => "stc2",
            Self::Stf => "stf",
            Self::Suf => "suf",
            Self::Tan => "tan",
            Self::Tandc => "tandc",
            Self::Tbcst => "tbcst",
            Self::Textrc => "textrc",
            Self::Textrm => "textrm",
            Self::Tinsr => "tinsr",
            Self::Tmcr => "tmcr",
            Self::Tmcrr => "tmcrr",
            Self::Tmia => "tmia",
            Self::Tmiaph => "tmiaph",
            Self::Tmovmsk => "tmovmsk",
            Self::Tmrc => "tmrc",
            Self::Tmrrc => "tmrrc",
            Self::Torc => "torc",
            Self::Torvsc => "torvsc",
            Self::Urd => "urd",
            Self::V => "v",
            Self::Vaba => "vaba",
            Self::Vabal => "vabal",
            Self::Vabd => "vabd",
            Self::Vabdl => "vabdl",
            Self::Vabs => "vabs",
            Self::Vacge => "vacge",
            Self::Vacgt => "vacgt",
            Self::Vadd => "vadd",
            Self::Vaddhn => "vaddhn",
            Self::Vaddl => "vaddl",
            Self::Vaddw => "vaddw",
            Self::Vand => "vand",
            Self::Vbic => "vbic",
            Self::Vbif => "vbif",
            Self::Vbit => "vbit",
            Self::Vbsl => "vbsl",
            Self::Vcadd => "vcadd",
            Self::Vceq => "vceq",
            Self::Vcge => "vcge",
            Self::Vcgt => "vcgt",
            Self::Vcle => "vcle",
            Self::Vcls => "vcls",
            Self::Vclt => "vclt",
            Self::Vclz => "vclz",
            Self::Vcmla => "vcmla",
            Self::Vcmp => "vcmp",
            Self::Vcnt => "vcnt",
            Self::Vcvt => "vcvt",
            Self::Vdiv => "vdiv",
            Self::VdotBf16 => "vdot.bf16",
            Self::Vdup => "vdup",
            Self::Veor => "veor",
            Self::Vext => "vext",
            Self::Vfma => "vfma",
            Self::VfmalF16 => "vfmal.f16",
            Self::Vfms => "vfms",
            Self::VfmslF16 => "vfmsl.f16",
            Self::Vfnma => "vfnma",
            Self::Vfnms => "vfnms",
            Self::Vhadd => "vhadd",
            Self::Vhsub => "vhsub",
            Self::VinsF16 => "vins.f16",
            Self::Vjcvt => "vjcvt",
            Self::Vld1 => "vld1",
            Self::Vld2 => "vld2",
            Self::Vld3 => "vld3",
            Self::Vld4 => "vld4",
            Self::Vldmdb => "vldmdb",
            Self::Vldmia => "vldmia",
            Self::Vldr => "vldr",
            Self::Vlldm => "vlldm",
            Self::Vlstm => "vlstm",
            Self::Vmax => "vmax",
            Self::Vmaxnm => "vmaxnm",
            Self::Vmin => "vmin",
            Self::Vminnm => "vminnm",
            Self::Vmla => "vmla",
            Self::Vmlal => "vmlal",
            Self::Vmls => "vmls",
            Self::Vmlsl => "vmlsl",
            Self::VmmlaBf16 => "vmmla.bf16",
            Self::Vmov => "vmov",
            Self::Vmovl => "vmovl",
            Self::Vmovn => "vmovn",
            Self::Vmovx => "vmovx",
            Self::Vmrs => "vmrs",
            Self::Vmsr => "vmsr",
            Self::Vmul => "vmul",
            Self::Vmull => "vmull",
            Self::Vmvn => "vmvn",
            Self::Vneg => "vneg",
            Self::Vnmla => "vnmla",
            Self::Vnmls => "vnmls",
            Self::Vnmul => "vnmul",
            Self::Vorn => "vorn",
            Self::Vorr => "vorr",
            Self::Vpadal => "vpadal",
            Self::Vpadd => "vpadd",
            Self::Vpaddl => "vpaddl",
            Self::Vpmax => "vpmax",
            Self::Vpmin => "vpmin",
            Self::Vpop => "vpop",
            Self::Vpush => "vpush",
            Self::Vqabs => "vqabs",
            Self::Vqadd => "vqadd",
            Self::Vqdmlal => "vqdmlal",
            Self::Vqdmlsl => "vqdmlsl",
            Self::Vqdmulh => "vqdmulh",
            Self::Vqdmull => "vqdmull",
            Self::Vqmovn => "vqmovn",
            Self::Vqmovun => "vqmovun",
            Self::Vqneg => "vqneg",
            Self::Vqrdmlah => "vqrdmlah",
            Self::Vqrdmlsh => "vqrdmlsh",
            Self::Vqrdmulh => "vqrdmulh",
            Self::Vqrshl => "vqrshl",
            Self::Vqrshrn => "vqrshrn",
            Self::Vqrshrun => "vqrshrun",
            Self::Vqshl => "vqshl",
            Self::Vqshlu => "vqshlu",
            Self::Vqshrn => "vqshrn",
            Self::Vqshrun => "vqshrun",
            Self::Vqsub => "vqsub",
            Self::Vraddhn => "vraddhn",
            Self::Vrecpe => "vrecpe",
            Self::Vrecps => "vrecps",
            Self::Vrev16 => "vrev16",
            Self::Vrev32 => "vrev32",
            Self::Vrev64 => "vrev64",
            Self::Vrhadd => "vrhadd",
            Self::Vrint => "vrint",
            Self::Vrshl => "vrshl",
            Self::Vrshr => "vrshr",
            Self::Vrshrn => "vrshrn",
            Self::Vrsqrte => "vrsqrte",
            Self::Vrsqrts => "vrsqrts",
            Self::Vrsra => "vrsra",
            Self::Vrsubhn => "vrsubhn",
            Self::Vscclrm => "vscclrm",
            Self::Vsel => "vsel",
            Self::Vshl => "vshl",
            Self::Vshll => "vshll",
            Self::Vshr => "vshr",
            Self::Vshrn => "vshrn",
            Self::Vsli => "vsli",
            Self::VsmmlaS8 => "vsmmla.s8",
            Self::Vsqrt => "vsqrt",
            Self::Vsra => "vsra",
            Self::Vsri => "vsri",
            Self::Vstmdb => "vstmdb",
            Self::Vstmia => "vstmia",
            Self::Vstr => "vstr",
            Self::Vsub => "vsub",
            Self::Vsubhn => "vsubhn",
            Self::Vsubl => "vsubl",
            Self::Vsubw => "vsubw",
            Self::VsudotU8 => "vsudot.u8",
            Self::Vswp => "vswp",
            Self::Vtbl => "vtbl",
            Self::Vtbx => "vtbx",
            Self::Vtrn => "vtrn",
            Self::Vtst => "vtst",
            Self::VummlaU8 => "vummla.u8",
            Self::VusdotS8 => "vusdot.s8",
            Self::VusmmlaS8 => "vusmmla.s8",
            Self::Vuzp => "vuzp",
            Self::Vzip => "vzip",
            Self::Wabs => "wabs",
            Self::Wabsdiff => "wabsdiff",
            Self::Wacc => "wacc",
            Self::Wadd => "wadd",
            Self::Waddbhus => "waddbhus",
            Self::Waddsubhx => "waddsubhx",
            Self::Waligni => "waligni",
            Self::Walignr => "walignr",
            Self::Wand => "wand",
            Self::Wavg2 => "wavg2",
            Self::Wavg4 => "wavg4",
            Self::Wcmpeq => "wcmpeq",
            Self::Wcmpgt => "wcmpgt",
            Self::Wfc => "wfc",
            Self::Wfs => "wfs",
            Self::Wldr => "wldr",
            Self::Wldrd => "wldrd",
            Self::Wldrw => "wldrw",
            Self::Wmac => "wmac",
            Self::Wmadd => "wmadd",
            Self::Wmax => "wmax",
            Self::Wmerge => "wmerge",
            Self::Wmia => "wmia",
            Self::Wmiaw => "wmiaw",
            Self::Wmin => "wmin",
            Self::Wmul => "wmul",
            Self::Wmulwl => "wmulwl",
            Self::Wmulwsm => "wmulwsm",
            Self::Wmulwum => "wmulwum",
            Self::Wor => "wor",
            Self::Wpack => "wpack",
            Self::Wqmia => "wqmia",
            Self::Wqmulm => "wqmulm",
            Self::Wqmulwm => "wqmulwm",
            Self::Wror => "wror",
            Self::Wsad => "wsad",
            Self::Wshufh => "wshufh",
            Self::Wsll => "wsll",
            Self::Wsra => "wsra",
            Self::Wsrl => "wsrl",
            Self::Wstr => "wstr",
            Self::Wstrd => "wstrd",
            Self::Wstrw => "wstrw",
            Self::Wsub => "wsub",
            Self::Wsubaddhx => "wsubaddhx",
            Self::Wunpckeh => "wunpckeh",
            Self::Wunpckel => "wunpckel",
            Self::Wunpckih => "wunpckih",
            Self::Wunpckil => "wunpckil",
            Self::Wxor => "wxor",
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct NeonOpcodeGenerated {
    pub mnemonic: NeonMnemonicGenerated,
    pub opcode: u32,
    pub mask: u32,
    pub isa: IsaApplicability,
    pub format: &'static str,
}

pub static NEON_OPCODE_TABLE_GENERATED: &[NeonOpcodeGenerated] = &[
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vext, opcode: 0xf2b00840, mask: 0xffb00850, isa: IsaApplicability::Neon, format: "vext%c.8\t%12-15,22R, %16-19,7R, %0-3,5R, %{I:#%8-11d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vext, opcode: 0xf2b00000, mask: 0xffb00810, isa: IsaApplicability::Neon, format: "vext%c.8\t%12-15,22R, %16-19,7R, %0-3,5R, %{I:#%8-11d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0x0e800b10, mask: 0x0ff00f70, isa: IsaApplicability::Neon, format: "vdup%c.32\t%16-19,7D, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0x0e800b30, mask: 0x0ff00f70, isa: IsaApplicability::Neon, format: "vdup%c.16\t%16-19,7D, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0x0ea00b10, mask: 0x0ff00f70, isa: IsaApplicability::Neon, format: "vdup%c.32\t%16-19,7Q, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0x0ea00b30, mask: 0x0ff00f70, isa: IsaApplicability::Neon, format: "vdup%c.16\t%16-19,7Q, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0x0ec00b10, mask: 0x0ff00f70, isa: IsaApplicability::Neon, format: "vdup%c.8\t%16-19,7D, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0x0ee00b10, mask: 0x0ff00f70, isa: IsaApplicability::Neon, format: "vdup%c.8\t%16-19,7Q, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0xf3b40c00, mask: 0xffb70f90, isa: IsaApplicability::Neon, format: "vdup%c.32\t%12-15,22R, %{R:%0-3,5D[%19d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0xf3b20c00, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vdup%c.16\t%12-15,22R, %{R:%0-3,5D[%18-19d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdup, opcode: 0xf3b10c00, mask: 0xffb10f90, isa: IsaApplicability::Neon, format: "vdup%c.8\t%12-15,22R, %{R:%0-3,5D[%17-19d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vtbl, opcode: 0xf3b00800, mask: 0xffb00c50, isa: IsaApplicability::Neon, format: "vtbl%c.8\t%12-15,22D, %F, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vtbx, opcode: 0xf3b00840, mask: 0xffb00c50, isa: IsaApplicability::Neon, format: "vtbx%c.8\t%12-15,22D, %F, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf3b60600, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "vcvt%c.f16.f32\t%12-15,22D, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf3b60700, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "vcvt%c.f32.f16\t%12-15,22Q, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfma, opcode: 0xf2000c10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vfma%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfma, opcode: 0xf2100c10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vfma%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfms, opcode: 0xf2200c10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vfms%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfms, opcode: 0xf2300c10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vfms%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VdotBf16, opcode: 0xfc000d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vdot.bf16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VdotBf16, opcode: 0xfe000d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vdot.bf16\t%12-15,22R, %16-19,7R, %{R:d%0-3d[%5d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VmmlaBf16, opcode: 0xfc000c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vmmla.bf16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf3b60640, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "vcvt%c.bf16.f32\t%12-15,22D, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfma, opcode: 0xfc300810, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vfma%6?tb.bf16\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfma, opcode: 0xfe300810, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vfma%6?tb.bf16\t%12-15,22Q, %16-19,7Q, %{R:%0-2D[%3,5d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VsmmlaS8, opcode: 0xfc200c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vsmmla.s8\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VummlaU8, opcode: 0xfc200c50, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vummla.u8\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VusmmlaS8, opcode: 0xfca00c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vusmmla.s8\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VusdotS8, opcode: 0xfca00d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vusdot.s8\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VusdotS8, opcode: 0xfe800d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vusdot.s8\t%12-15,22R, %16-19,7R, %{R:d%0-3d[%5d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VsudotU8, opcode: 0xfe800d10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vsudot.u8\t%12-15,22R, %16-19,7R, %{R:d%0-3d[%5d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0xf3ba0400, mask: 0xffbf0c10, isa: IsaApplicability::Neon, format: "vrint%7-9?p?m?zaxn%u.f32\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0xf3b60400, mask: 0xffbf0c10, isa: IsaApplicability::Neon, format: "vrint%7-9?p?m?zaxn%u.f16\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf3bb0000, mask: 0xffbf0c10, isa: IsaApplicability::Neon, format: "vcvt%8-9?mpna%u.%7?us32.f32\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf3b70000, mask: 0xffbf0c10, isa: IsaApplicability::Neon, format: "vcvt%8-9?mpna%u.%7?us16.f16\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Aese, opcode: 0xf3b00300, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "aese%u.8\t%12-15,22Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Aesd, opcode: 0xf3b00340, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "aesd%u.8\t%12-15,22Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Aesmc, opcode: 0xf3b00380, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "aesmc%u.8\t%12-15,22Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Aesimc, opcode: 0xf3b003c0, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "aesimc%u.8\t%12-15,22Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha1h, opcode: 0xf3b902c0, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "sha1h%u.32\t%12-15,22Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha1su1, opcode: 0xf3ba0380, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "sha1su1%u.32\t%12-15,22Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha256su0, opcode: 0xf3ba03c0, mask: 0xffbf0fd0, isa: IsaApplicability::Neon, format: "sha256su0%u.32\t%12-15,22Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmovl, opcode: 0xf2880a10, mask: 0xfebf0fd0, isa: IsaApplicability::Neon, format: "vmovl%c.%24?us8\t%12-15,22Q, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmovl, opcode: 0xf2900a10, mask: 0xfebf0fd0, isa: IsaApplicability::Neon, format: "vmovl%c.%24?us16\t%12-15,22Q, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmovl, opcode: 0xf2a00a10, mask: 0xfebf0fd0, isa: IsaApplicability::Neon, format: "vmovl%c.%24?us32\t%12-15,22Q, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcnt, opcode: 0xf3b00500, mask: 0xffbf0f90, isa: IsaApplicability::Neon, format: "vcnt%c.8\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmvn, opcode: 0xf3b00580, mask: 0xffbf0f90, isa: IsaApplicability::Neon, format: "vmvn%c\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vswp, opcode: 0xf3b20000, mask: 0xffbf0f90, isa: IsaApplicability::Neon, format: "vswp%c\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmovn, opcode: 0xf3b20200, mask: 0xffb30fd0, isa: IsaApplicability::Neon, format: "vmovn%c.i%18-19T2\t%12-15,22D, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqmovun, opcode: 0xf3b20240, mask: 0xffb30fd0, isa: IsaApplicability::Neon, format: "vqmovun%c.s%18-19T2\t%12-15,22D, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqmovn, opcode: 0xf3b20280, mask: 0xffb30fd0, isa: IsaApplicability::Neon, format: "vqmovn%c.s%18-19T2\t%12-15,22D, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqmovn, opcode: 0xf3b202c0, mask: 0xffb30fd0, isa: IsaApplicability::Neon, format: "vqmovn%c.u%18-19T2\t%12-15,22D, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshll, opcode: 0xf3b20300, mask: 0xffb30fd0, isa: IsaApplicability::Neon, format: "vshll%c.i%18-19S2\t%12-15,22Q, %0-3,5D, %{I:#%18-19S2%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrecpe, opcode: 0xf3bb0400, mask: 0xffbf0e90, isa: IsaApplicability::Neon, format: "vrecpe%c.%8?fu%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrecpe, opcode: 0xf3b70400, mask: 0xffbf0e90, isa: IsaApplicability::Neon, format: "vrecpe%c.%8?fu16\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsqrte, opcode: 0xf3bb0480, mask: 0xffbf0e90, isa: IsaApplicability::Neon, format: "vrsqrte%c.%8?fu%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsqrte, opcode: 0xf3b70480, mask: 0xffbf0e90, isa: IsaApplicability::Neon, format: "vrsqrte%c.%8?fu16\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrev64, opcode: 0xf3b00000, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vrev64%c.%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrev32, opcode: 0xf3b00080, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vrev32%c.%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrev16, opcode: 0xf3b00100, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vrev16%c.%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcls, opcode: 0xf3b00400, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vcls%c.s%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vclz, opcode: 0xf3b00480, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vclz%c.i%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqabs, opcode: 0xf3b00700, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vqabs%c.s%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqneg, opcode: 0xf3b00780, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vqneg%c.s%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vtrn, opcode: 0xf3b20080, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vtrn%c.%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vuzp, opcode: 0xf3b20100, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vuzp%c.%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vzip, opcode: 0xf3b20180, mask: 0xffb30f90, isa: IsaApplicability::Neon, format: "vzip%c.%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcgt, opcode: 0xf3b10000, mask: 0xffb30b90, isa: IsaApplicability::Neon, format: "vcgt%c.%10?fs%18-19S2\t%12-15,22R, %0-3,5R, %{I:#0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcge, opcode: 0xf3b10080, mask: 0xffb30b90, isa: IsaApplicability::Neon, format: "vcge%c.%10?fs%18-19S2\t%12-15,22R, %0-3,5R, %{I:#0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vceq, opcode: 0xf3b10100, mask: 0xffb30b90, isa: IsaApplicability::Neon, format: "vceq%c.%10?fi%18-19S2\t%12-15,22R, %0-3,5R, %{I:#0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcle, opcode: 0xf3b10180, mask: 0xffb30b90, isa: IsaApplicability::Neon, format: "vcle%c.%10?fs%18-19S2\t%12-15,22R, %0-3,5R, %{I:#0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vclt, opcode: 0xf3b10200, mask: 0xffb30b90, isa: IsaApplicability::Neon, format: "vclt%c.%10?fs%18-19S2\t%12-15,22R, %0-3,5R, %{I:#0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabs, opcode: 0xf3b10300, mask: 0xffb30b90, isa: IsaApplicability::Neon, format: "vabs%c.%10?fs%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vneg, opcode: 0xf3b10380, mask: 0xffb30b90, isa: IsaApplicability::Neon, format: "vneg%c.%10?fs%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpaddl, opcode: 0xf3b00200, mask: 0xffb30f10, isa: IsaApplicability::Neon, format: "vpaddl%c.%7?us%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpadal, opcode: 0xf3b00600, mask: 0xffb30f10, isa: IsaApplicability::Neon, format: "vpadal%c.%7?us%18-19S2\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf3bb0600, mask: 0xffbf0e10, isa: IsaApplicability::Neon, format: "vcvt%c.%7-8?usff%18-19Sa.%7-8?ffus%18-19Sa\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf3b70600, mask: 0xffbf0e10, isa: IsaApplicability::Neon, format: "vcvt%c.%7-8?usff16.%7-8?ffus16\t%12-15,22R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha1c, opcode: 0xf2000c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "sha1c%u.32\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha1p, opcode: 0xf2100c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "sha1p%u.32\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha1m, opcode: 0xf2200c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "sha1m%u.32\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha1su0, opcode: 0xf2300c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "sha1su0%u.32\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha256h, opcode: 0xf3000c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "sha256h%u.32\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha256h2, opcode: 0xf3100c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "sha256h2%u.32\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sha256su1, opcode: 0xf3200c40, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "sha256su1%u.32\t%12-15,22Q, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmaxnm, opcode: 0xf3000f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmaxnm%u.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmaxnm, opcode: 0xf3100f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmaxnm%u.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vminnm, opcode: 0xf3200f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vminnm%u.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vminnm, opcode: 0xf3300f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vminnm%u.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vand, opcode: 0xf2000110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vand%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vbic, opcode: 0xf2100110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vbic%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vorr, opcode: 0xf2200110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vorr%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vorn, opcode: 0xf2300110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vorn%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Veor, opcode: 0xf3000110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "veor%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vbsl, opcode: 0xf3100110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vbsl%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vbit, opcode: 0xf3200110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vbit%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vbif, opcode: 0xf3300110, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vbif%c\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vadd, opcode: 0xf2000d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vadd%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vadd, opcode: 0xf2100d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vadd%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf2000d10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmla%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf2100d10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmla%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vceq, opcode: 0xf2000e00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vceq%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vceq, opcode: 0xf2100e00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vceq%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmax, opcode: 0xf2000f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmax%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmax, opcode: 0xf2100f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmax%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrecps, opcode: 0xf2000f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vrecps%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrecps, opcode: 0xf2100f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vrecps%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsub, opcode: 0xf2200d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vsub%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsub, opcode: 0xf2300d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vsub%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf2200d10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmls%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf2300d10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmls%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmin, opcode: 0xf2200f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmin%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmin, opcode: 0xf2300f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmin%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsqrts, opcode: 0xf2200f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vrsqrts%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsqrts, opcode: 0xf2300f10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vrsqrts%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpadd, opcode: 0xf3000d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vpadd%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpadd, opcode: 0xf3100d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vpadd%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf3000d10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmul%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf3100d10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vmul%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcge, opcode: 0xf3000e00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vcge%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcge, opcode: 0xf3100e00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vcge%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vacge, opcode: 0xf3000e10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vacge%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vacge, opcode: 0xf3100e10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vacge%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpmax, opcode: 0xf3000f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vpmax%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpmax, opcode: 0xf3100f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vpmax%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabd, opcode: 0xf3200d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vabd%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabd, opcode: 0xf3300d00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vabd%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcgt, opcode: 0xf3200e00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vcgt%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcgt, opcode: 0xf3300e00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vcgt%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vacgt, opcode: 0xf3200e10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vacgt%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vacgt, opcode: 0xf3300e10, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vacgt%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpmin, opcode: 0xf3200f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vpmin%c.f32\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpmin, opcode: 0xf3300f00, mask: 0xffb00f10, isa: IsaApplicability::Neon, format: "vpmin%c.f16\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vadd, opcode: 0xf2000800, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vadd%c.i%20-21S3\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vtst, opcode: 0xf2000810, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vtst%c.%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf2000900, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vmla%c.i%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmulh, opcode: 0xf2000b00, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vqdmulh%c.s%20-21S6\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpadd, opcode: 0xf2000b10, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vpadd%c.i%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsub, opcode: 0xf3000800, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vsub%c.i%20-21S3\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vceq, opcode: 0xf3000810, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vceq%c.i%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf3000900, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vmls%c.i%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmulh, opcode: 0xf3000b00, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vqrdmulh%c.s%20-21S6\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vhadd, opcode: 0xf2000000, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vhadd%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqadd, opcode: 0xf2000010, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vqadd%c.%24?us%20-21S3\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrhadd, opcode: 0xf2000100, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vrhadd%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vhsub, opcode: 0xf2000200, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vhsub%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqsub, opcode: 0xf2000210, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vqsub%c.%24?us%20-21S3\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcgt, opcode: 0xf2000300, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vcgt%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcge, opcode: 0xf2000310, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vcge%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshl, opcode: 0xf2000400, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vshl%c.%24?us%20-21S3\t%12-15,22R, %0-3,5R, %16-19,7R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshl, opcode: 0xf2000410, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vqshl%c.%24?us%20-21S3\t%12-15,22R, %0-3,5R, %16-19,7R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshl, opcode: 0xf2000500, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vrshl%c.%24?us%20-21S3\t%12-15,22R, %0-3,5R, %16-19,7R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrshl, opcode: 0xf2000510, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vqrshl%c.%24?us%20-21S3\t%12-15,22R, %0-3,5R, %16-19,7R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmax, opcode: 0xf2000600, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vmax%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmin, opcode: 0xf2000610, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vmin%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabd, opcode: 0xf2000700, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vabd%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vaba, opcode: 0xf2000710, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vaba%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf2000910, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vmul%c.%24?pi%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpmax, opcode: 0xf2000a00, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vpmax%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpmin, opcode: 0xf2000a10, mask: 0xfe800f10, isa: IsaApplicability::Neon, format: "vpmin%c.%24?us%20-21S2\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmlah, opcode: 0xf3000b10, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vqrdmlah%c.s%20-21S6\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmlsh, opcode: 0xf3000c10, mask: 0xff800f10, isa: IsaApplicability::Neon, format: "vqrdmlsh%c.s%20-21S6\t%12-15,22R, %16-19,7R, %0-3,5R" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0xf2800e10, mask: 0xfeb80fb0, isa: IsaApplicability::Neon, format: "vmov%c.i8\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0xf2800e30, mask: 0xfeb80fb0, isa: IsaApplicability::Neon, format: "vmov%c.i64\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0xf2800f10, mask: 0xfeb80fb0, isa: IsaApplicability::Neon, format: "vmov%c.f32\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0xf2800810, mask: 0xfeb80db0, isa: IsaApplicability::Neon, format: "vmov%c.i16\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmvn, opcode: 0xf2800830, mask: 0xfeb80db0, isa: IsaApplicability::Neon, format: "vmvn%c.i16\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vorr, opcode: 0xf2800910, mask: 0xfeb80db0, isa: IsaApplicability::Neon, format: "vorr%c.i16\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vbic, opcode: 0xf2800930, mask: 0xfeb80db0, isa: IsaApplicability::Neon, format: "vbic%c.i16\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0xf2800c10, mask: 0xfeb80eb0, isa: IsaApplicability::Neon, format: "vmov%c.i32\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmvn, opcode: 0xf2800c30, mask: 0xfeb80eb0, isa: IsaApplicability::Neon, format: "vmvn%c.i32\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vorr, opcode: 0xf2800110, mask: 0xfeb809b0, isa: IsaApplicability::Neon, format: "vorr%c.i32\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vbic, opcode: 0xf2800130, mask: 0xfeb809b0, isa: IsaApplicability::Neon, format: "vbic%c.i32\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0xf2800010, mask: 0xfeb808b0, isa: IsaApplicability::Neon, format: "vmov%c.i32\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmvn, opcode: 0xf2800030, mask: 0xfeb808b0, isa: IsaApplicability::Neon, format: "vmvn%c.i32\t%12-15,22R, %E" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshrn, opcode: 0xf2880810, mask: 0xffb80fd0, isa: IsaApplicability::Neon, format: "vshrn%c.i16\t%12-15,22D, %0-3,5Q, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshrn, opcode: 0xf2880850, mask: 0xffb80fd0, isa: IsaApplicability::Neon, format: "vrshrn%c.i16\t%12-15,22D, %0-3,5Q, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshrun, opcode: 0xf2880810, mask: 0xfeb80fd0, isa: IsaApplicability::Neon, format: "vqshrun%c.s16\t%12-15,22D, %0-3,5Q, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrshrun, opcode: 0xf2880850, mask: 0xfeb80fd0, isa: IsaApplicability::Neon, format: "vqrshrun%c.s16\t%12-15,22D, %0-3,5Q, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshrn, opcode: 0xf2880910, mask: 0xfeb80fd0, isa: IsaApplicability::Neon, format: "vqshrn%c.%24?us16\t%12-15,22D, %0-3,5Q, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrshrn, opcode: 0xf2880950, mask: 0xfeb80fd0, isa: IsaApplicability::Neon, format: "vqrshrn%c.%24?us16\t%12-15,22D, %0-3,5Q, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshll, opcode: 0xf2880a10, mask: 0xfeb80fd0, isa: IsaApplicability::Neon, format: "vshll%c.%24?us8\t%12-15,22Q, %0-3,5D, %{I:#%16-18d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshrn, opcode: 0xf2900810, mask: 0xffb00fd0, isa: IsaApplicability::Neon, format: "vshrn%c.i32\t%12-15,22D, %0-3,5Q, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshrn, opcode: 0xf2900850, mask: 0xffb00fd0, isa: IsaApplicability::Neon, format: "vrshrn%c.i32\t%12-15,22D, %0-3,5Q, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshl, opcode: 0xf2880510, mask: 0xffb80f90, isa: IsaApplicability::Neon, format: "vshl%c.%24?us8\t%12-15,22R, %0-3,5R, %{I:#%16-18d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsri, opcode: 0xf3880410, mask: 0xffb80f90, isa: IsaApplicability::Neon, format: "vsri%c.8\t%12-15,22R, %0-3,5R, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsli, opcode: 0xf3880510, mask: 0xffb80f90, isa: IsaApplicability::Neon, format: "vsli%c.8\t%12-15,22R, %0-3,5R, %{I:#%16-18d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshlu, opcode: 0xf3880610, mask: 0xffb80f90, isa: IsaApplicability::Neon, format: "vqshlu%c.s8\t%12-15,22R, %0-3,5R, %{I:#%16-18d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshrun, opcode: 0xf2900810, mask: 0xfeb00fd0, isa: IsaApplicability::Neon, format: "vqshrun%c.s32\t%12-15,22D, %0-3,5Q, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrshrun, opcode: 0xf2900850, mask: 0xfeb00fd0, isa: IsaApplicability::Neon, format: "vqrshrun%c.s32\t%12-15,22D, %0-3,5Q, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshrn, opcode: 0xf2900910, mask: 0xfeb00fd0, isa: IsaApplicability::Neon, format: "vqshrn%c.%24?us32\t%12-15,22D, %0-3,5Q, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrshrn, opcode: 0xf2900950, mask: 0xfeb00fd0, isa: IsaApplicability::Neon, format: "vqrshrn%c.%24?us32\t%12-15,22D, %0-3,5Q, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshll, opcode: 0xf2900a10, mask: 0xfeb00fd0, isa: IsaApplicability::Neon, format: "vshll%c.%24?us16\t%12-15,22Q, %0-3,5D, %{I:#%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshr, opcode: 0xf2880010, mask: 0xfeb80f90, isa: IsaApplicability::Neon, format: "vshr%c.%24?us8\t%12-15,22R, %0-3,5R, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsra, opcode: 0xf2880110, mask: 0xfeb80f90, isa: IsaApplicability::Neon, format: "vsra%c.%24?us8\t%12-15,22R, %0-3,5R, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshr, opcode: 0xf2880210, mask: 0xfeb80f90, isa: IsaApplicability::Neon, format: "vrshr%c.%24?us8\t%12-15,22R, %0-3,5R, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsra, opcode: 0xf2880310, mask: 0xfeb80f90, isa: IsaApplicability::Neon, format: "vrsra%c.%24?us8\t%12-15,22R, %0-3,5R, %{I:#%16-18e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshl, opcode: 0xf2880710, mask: 0xfeb80f90, isa: IsaApplicability::Neon, format: "vqshl%c.%24?us8\t%12-15,22R, %0-3,5R, %{I:#%16-18d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshrn, opcode: 0xf2a00810, mask: 0xffa00fd0, isa: IsaApplicability::Neon, format: "vshrn%c.i64\t%12-15,22D, %0-3,5Q, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshrn, opcode: 0xf2a00850, mask: 0xffa00fd0, isa: IsaApplicability::Neon, format: "vrshrn%c.i64\t%12-15,22D, %0-3,5Q, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshl, opcode: 0xf2900510, mask: 0xffb00f90, isa: IsaApplicability::Neon, format: "vshl%c.%24?us16\t%12-15,22R, %0-3,5R, %{I:#%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsri, opcode: 0xf3900410, mask: 0xffb00f90, isa: IsaApplicability::Neon, format: "vsri%c.16\t%12-15,22R, %0-3,5R, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsli, opcode: 0xf3900510, mask: 0xffb00f90, isa: IsaApplicability::Neon, format: "vsli%c.16\t%12-15,22R, %0-3,5R, %{I:#%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshlu, opcode: 0xf3900610, mask: 0xffb00f90, isa: IsaApplicability::Neon, format: "vqshlu%c.s16\t%12-15,22R, %0-3,5R, %{I:#%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshll, opcode: 0xf2a00a10, mask: 0xfea00fd0, isa: IsaApplicability::Neon, format: "vshll%c.%24?us32\t%12-15,22Q, %0-3,5D, %{I:#%16-20d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshr, opcode: 0xf2900010, mask: 0xfeb00f90, isa: IsaApplicability::Neon, format: "vshr%c.%24?us16\t%12-15,22R, %0-3,5R, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsra, opcode: 0xf2900110, mask: 0xfeb00f90, isa: IsaApplicability::Neon, format: "vsra%c.%24?us16\t%12-15,22R, %0-3,5R, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshr, opcode: 0xf2900210, mask: 0xfeb00f90, isa: IsaApplicability::Neon, format: "vrshr%c.%24?us16\t%12-15,22R, %0-3,5R, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsra, opcode: 0xf2900310, mask: 0xfeb00f90, isa: IsaApplicability::Neon, format: "vrsra%c.%24?us16\t%12-15,22R, %0-3,5R, %{I:#%16-19e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshl, opcode: 0xf2900710, mask: 0xfeb00f90, isa: IsaApplicability::Neon, format: "vqshl%c.%24?us16\t%12-15,22R, %0-3,5R, %{I:#%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshrun, opcode: 0xf2a00810, mask: 0xfea00fd0, isa: IsaApplicability::Neon, format: "vqshrun%c.s64\t%12-15,22D, %0-3,5Q, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrshrun, opcode: 0xf2a00850, mask: 0xfea00fd0, isa: IsaApplicability::Neon, format: "vqrshrun%c.s64\t%12-15,22D, %0-3,5Q, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshrn, opcode: 0xf2a00910, mask: 0xfea00fd0, isa: IsaApplicability::Neon, format: "vqshrn%c.%24?us64\t%12-15,22D, %0-3,5Q, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrshrn, opcode: 0xf2a00950, mask: 0xfea00fd0, isa: IsaApplicability::Neon, format: "vqrshrn%c.%24?us64\t%12-15,22D, %0-3,5Q, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshl, opcode: 0xf2a00510, mask: 0xffa00f90, isa: IsaApplicability::Neon, format: "vshl%c.%24?us32\t%12-15,22R, %0-3,5R, %{I:#%16-20d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsri, opcode: 0xf3a00410, mask: 0xffa00f90, isa: IsaApplicability::Neon, format: "vsri%c.32\t%12-15,22R, %0-3,5R, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsli, opcode: 0xf3a00510, mask: 0xffa00f90, isa: IsaApplicability::Neon, format: "vsli%c.32\t%12-15,22R, %0-3,5R, %{I:#%16-20d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshlu, opcode: 0xf3a00610, mask: 0xffa00f90, isa: IsaApplicability::Neon, format: "vqshlu%c.s32\t%12-15,22R, %0-3,5R, %{I:#%16-20d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshr, opcode: 0xf2a00010, mask: 0xfea00f90, isa: IsaApplicability::Neon, format: "vshr%c.%24?us32\t%12-15,22R, %0-3,5R, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsra, opcode: 0xf2a00110, mask: 0xfea00f90, isa: IsaApplicability::Neon, format: "vsra%c.%24?us32\t%12-15,22R, %0-3,5R, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshr, opcode: 0xf2a00210, mask: 0xfea00f90, isa: IsaApplicability::Neon, format: "vrshr%c.%24?us32\t%12-15,22R, %0-3,5R, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsra, opcode: 0xf2a00310, mask: 0xfea00f90, isa: IsaApplicability::Neon, format: "vrsra%c.%24?us32\t%12-15,22R, %0-3,5R, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshl, opcode: 0xf2a00710, mask: 0xfea00f90, isa: IsaApplicability::Neon, format: "vqshl%c.%24?us32\t%12-15,22R, %0-3,5R, %{I:#%16-20d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshl, opcode: 0xf2800590, mask: 0xff800f90, isa: IsaApplicability::Neon, format: "vshl%c.%24?us64\t%12-15,22R, %0-3,5R, %{I:#%16-21d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsri, opcode: 0xf3800490, mask: 0xff800f90, isa: IsaApplicability::Neon, format: "vsri%c.64\t%12-15,22R, %0-3,5R, %{I:#%16-21e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsli, opcode: 0xf3800590, mask: 0xff800f90, isa: IsaApplicability::Neon, format: "vsli%c.64\t%12-15,22R, %0-3,5R, %{I:#%16-21d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshlu, opcode: 0xf3800690, mask: 0xff800f90, isa: IsaApplicability::Neon, format: "vqshlu%c.s64\t%12-15,22R, %0-3,5R, %{I:#%16-21d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vshr, opcode: 0xf2800090, mask: 0xfe800f90, isa: IsaApplicability::Neon, format: "vshr%c.%24?us64\t%12-15,22R, %0-3,5R, %{I:#%16-21e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsra, opcode: 0xf2800190, mask: 0xfe800f90, isa: IsaApplicability::Neon, format: "vsra%c.%24?us64\t%12-15,22R, %0-3,5R, %{I:#%16-21e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrshr, opcode: 0xf2800290, mask: 0xfe800f90, isa: IsaApplicability::Neon, format: "vrshr%c.%24?us64\t%12-15,22R, %0-3,5R, %{I:#%16-21e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsra, opcode: 0xf2800390, mask: 0xfe800f90, isa: IsaApplicability::Neon, format: "vrsra%c.%24?us64\t%12-15,22R, %0-3,5R, %{I:#%16-21e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqshl, opcode: 0xf2800790, mask: 0xfe800f90, isa: IsaApplicability::Neon, format: "vqshl%c.%24?us64\t%12-15,22R, %0-3,5R, %{I:#%16-21d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf2a00e10, mask: 0xfea00e90, isa: IsaApplicability::Neon, format: "vcvt%c.%24,8?usff32.%24,8?ffus32\t%12-15,22R, %0-3,5R, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xf2a00c10, mask: 0xfea00e90, isa: IsaApplicability::Neon, format: "vcvt%c.%24,8?usff16.%24,8?ffus16\t%12-15,22R, %0-3,5R, %{I:#%16-20e%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmull, opcode: 0xf2a00e00, mask: 0xfeb00f50, isa: IsaApplicability::Neon, format: "vmull%c.p64\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmull, opcode: 0xf2800e00, mask: 0xfea00f50, isa: IsaApplicability::Neon, format: "vmull%c.p%20S0\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vaddhn, opcode: 0xf2800400, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vaddhn%c.i%20-21T2\t%12-15,22D, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsubhn, opcode: 0xf2800600, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vsubhn%c.i%20-21T2\t%12-15,22D, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmlal, opcode: 0xf2800900, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmlal%c.s%20-21S6\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmlsl, opcode: 0xf2800b00, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmlsl%c.s%20-21S6\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmull, opcode: 0xf2800d00, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmull%c.s%20-21S6\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vraddhn, opcode: 0xf3800400, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vraddhn%c.i%20-21T2\t%12-15,22D, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrsubhn, opcode: 0xf3800600, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vrsubhn%c.i%20-21T2\t%12-15,22D, %16-19,7Q, %0-3,5Q" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vaddl, opcode: 0xf2800000, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vaddl%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vaddw, opcode: 0xf2800100, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vaddw%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7Q, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsubl, opcode: 0xf2800200, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vsubl%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsubw, opcode: 0xf2800300, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vsubw%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7Q, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabal, opcode: 0xf2800500, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vabal%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabdl, opcode: 0xf2800700, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vabdl%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmlal, opcode: 0xf2800800, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vmlal%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmlsl, opcode: 0xf2800a00, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vmlsl%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmull, opcode: 0xf2800c00, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vmull%c.%24?us%20-21S2\t%12-15,22Q, %16-19,7D, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf2800040, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vmla%c.i%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf2800140, mask: 0xff900f50, isa: IsaApplicability::Neon, format: "vmla%c.f%20-21Sa\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf2900140, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vmla%c.f16\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmlal, opcode: 0xf2800340, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmlal%c.s%20-21S6\t%12-15,22Q, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf2800440, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vmls%c.i%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf2800540, mask: 0xff900f50, isa: IsaApplicability::Neon, format: "vmls%c.f%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf2900540, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vmls%c.f16\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmlsl, opcode: 0xf2800740, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmlsl%c.s%20-21S6\t%12-15,22Q, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf2800840, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vmul%c.i%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf2800940, mask: 0xff900f50, isa: IsaApplicability::Neon, format: "vmul%c.f%20-21Sa\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf2900940, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vmul%c.f16\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmull, opcode: 0xf2800b40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmull%c.s%20-21S6\t%12-15,22Q, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmulh, opcode: 0xf2800c40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmulh%c.s%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmulh, opcode: 0xf2800d40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqrdmulh%c.s%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf3800040, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vmla%c.i%20-21S6\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf3800140, mask: 0xff900f50, isa: IsaApplicability::Neon, format: "vmla%c.f%20-21Sa\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0xf3900140, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vmla%c.f16\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf3800440, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vmls%c.i%20-21S6\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf3800540, mask: 0xff900f50, isa: IsaApplicability::Neon, format: "vmls%c.f%20-21Sa\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0xf3900540, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vmls%c.f16\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf3800840, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vmul%c.i%20-21S6\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf3800940, mask: 0xff900f50, isa: IsaApplicability::Neon, format: "vmul%c.f%20-21Sa\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0xf3900940, mask: 0xffb00f50, isa: IsaApplicability::Neon, format: "vmul%c.f16\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqdmulh, opcode: 0xf3800c40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqdmulh%c.s%20-21S6\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmulh, opcode: 0xf3800d40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqrdmulh%c.s%20-21S6\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmlal, opcode: 0xf2800240, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vmlal%c.%24?us%20-21S6\t%12-15,22Q, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmlsl, opcode: 0xf2800640, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vmlsl%c.%24?us%20-21S6\t%12-15,22Q, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmull, opcode: 0xf2800a40, mask: 0xfe800f50, isa: IsaApplicability::Neon, format: "vmull%c.%24?us%20-21S6\t%12-15,22Q, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmlah, opcode: 0xf2800e40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqrdmlah%c.s%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmlsh, opcode: 0xf2800f40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqrdmlsh%c.s%20-21S6\t%12-15,22D, %16-19,7D, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmlah, opcode: 0xf3800e40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqrdmlah%c.s%20-21S6\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vqrdmlsh, opcode: 0xf3800f40, mask: 0xff800f50, isa: IsaApplicability::Neon, format: "vqrdmlsh%c.s%20-21S6\t%12-15,22Q, %16-19,7Q, %D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vld4, opcode: 0xf4a00fc0, mask: 0xffb00fc0, isa: IsaApplicability::Neon, format: "vld4%c.32\t%C" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vld1, opcode: 0xf4a00c00, mask: 0xffb00f00, isa: IsaApplicability::Neon, format: "vld1%c.%6-7S2\t%C" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vld2, opcode: 0xf4a00d00, mask: 0xffb00f00, isa: IsaApplicability::Neon, format: "vld2%c.%6-7S2\t%C" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vld3, opcode: 0xf4a00e00, mask: 0xffb00f00, isa: IsaApplicability::Neon, format: "vld3%c.%6-7S2\t%C" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vld4, opcode: 0xf4a00f00, mask: 0xffb00f00, isa: IsaApplicability::Neon, format: "vld4%c.%6-7S2\t%C" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000200, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt1%c.%6-7S3\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000300, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt2%c.%6-7S2\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000400, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt3%c.%6-7S2\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000500, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt3%c.%6-7S2\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000600, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt1%c.%6-7S3\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000700, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt1%c.%6-7S3\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000800, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt2%c.%6-7S2\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000900, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt2%c.%6-7S2\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000a00, mask: 0xff900f00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt1%c.%6-7S3\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4000000, mask: 0xff900e00, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt4%c.%6-7S2\t%A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4800000, mask: 0xff900300, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt1%c.%10-11S2\t%B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4800100, mask: 0xff900300, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt2%c.%10-11S2\t%B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4800200, mask: 0xff900300, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt3%c.%10-11S2\t%B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xf4800300, mask: 0xff900300, isa: IsaApplicability::Neon, format: "v%21?ls%21?dt4%c.%10-11S2\t%B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mia, opcode: 0x0e200010, mask: 0x0fff0ff0, isa: IsaApplicability::Any, format: "mia%c\t%{R:acc0%}, %0-3r, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Miaph, opcode: 0x0e280010, mask: 0x0fff0ff0, isa: IsaApplicability::Any, format: "miaph%c\t%{R:acc0%}, %0-3r, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mia, opcode: 0x0e2c0010, mask: 0x0ffc0ff0, isa: IsaApplicability::Any, format: "mia%17'T%17`B%16'T%16`B%c\t%{R:acc0%}, %0-3r, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mar, opcode: 0x0c400000, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "mar%c\t%{R:acc0%}, %12-15r, %16-19r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mra, opcode: 0x0c500000, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "mra%c\t%12-15r, %16-19r, %{R:acc0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tandc, opcode: 0x0e130130, mask: 0x0f3f0fff, isa: IsaApplicability::Any, format: "tandc%22-23w%c\t%12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tbcst, opcode: 0x0e400010, mask: 0x0ff00f3f, isa: IsaApplicability::Any, format: "tbcst%6-7w%c\t%16-19g, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Textrc, opcode: 0x0e130170, mask: 0x0f3f0ff8, isa: IsaApplicability::Any, format: "textrc%22-23w%c\t%12-15r, %{I:#%0-2d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Textrm, opcode: 0x0e100070, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "textrm%3?su%22-23w%c\t%12-15r, %16-19g, %{I:#%0-2d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tinsr, opcode: 0x0e600010, mask: 0x0ff00f38, isa: IsaApplicability::Any, format: "tinsr%6-7w%c\t%16-19g, %12-15r, %{I:#%0-2d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmcr, opcode: 0x0e000110, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "tmcr%c\t%16-19G, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmcrr, opcode: 0x0c400000, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "tmcrr%c\t%0-3g, %12-15r, %16-19r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmia, opcode: 0x0e2c0010, mask: 0x0ffc0e10, isa: IsaApplicability::Any, format: "tmia%17?tb%16?tb%c\t%5-8g, %0-3r, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmia, opcode: 0x0e200010, mask: 0x0fff0e10, isa: IsaApplicability::Any, format: "tmia%c\t%5-8g, %0-3r, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmiaph, opcode: 0x0e280010, mask: 0x0fff0e10, isa: IsaApplicability::Any, format: "tmiaph%c\t%5-8g, %0-3r, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmovmsk, opcode: 0x0e100030, mask: 0x0f300fff, isa: IsaApplicability::Any, format: "tmovmsk%22-23w%c\t%12-15r, %16-19g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmrc, opcode: 0x0e100110, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "tmrc%c\t%12-15r, %16-19G" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tmrrc, opcode: 0x0c500000, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "tmrrc%c\t%12-15r, %16-19r, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Torc, opcode: 0x0e130150, mask: 0x0f3f0fff, isa: IsaApplicability::Any, format: "torc%22-23w%c\t%12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Torvsc, opcode: 0x0e120190, mask: 0x0f3f0fff, isa: IsaApplicability::Any, format: "torvsc%22-23w%c\t%12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wabs, opcode: 0x0e2001c0, mask: 0x0f300fff, isa: IsaApplicability::Any, format: "wabs%22-23w%c\t%12-15g, %16-19g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wacc, opcode: 0x0e0001c0, mask: 0x0f300fff, isa: IsaApplicability::Any, format: "wacc%22-23w%c\t%12-15g, %16-19g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wadd, opcode: 0x0e000180, mask: 0x0f000ff0, isa: IsaApplicability::Any, format: "wadd%20-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Waddbhus, opcode: 0x0e2001a0, mask: 0x0fb00ff0, isa: IsaApplicability::Any, format: "waddbhus%22?ml%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Waddsubhx, opcode: 0x0ea001a0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "waddsubhx%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Waligni, opcode: 0x0e000020, mask: 0x0f800ff0, isa: IsaApplicability::Any, format: "waligni%c\t%12-15g, %16-19g, %0-3g, %{I:#%20-22d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Walignr, opcode: 0x0e800020, mask: 0x0fc00ff0, isa: IsaApplicability::Any, format: "walignr%20-21d%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wand, opcode: 0x0e200000, mask: 0x0fe00ff0, isa: IsaApplicability::Any, format: "wand%20'n%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wavg2, opcode: 0x0e800000, mask: 0x0fa00ff0, isa: IsaApplicability::Any, format: "wavg2%22?hb%20'r%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wavg4, opcode: 0x0e400000, mask: 0x0fe00ff0, isa: IsaApplicability::Any, format: "wavg4%20'r%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wcmpeq, opcode: 0x0e000060, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wcmpeq%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wcmpgt, opcode: 0x0e100060, mask: 0x0f100ff0, isa: IsaApplicability::Any, format: "wcmpgt%21?su%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wldrd, opcode: 0xfc500100, mask: 0xfe500f00, isa: IsaApplicability::Any, format: "wldrd\t%12-15g, %r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wldrw, opcode: 0xfc100100, mask: 0xfe500f00, isa: IsaApplicability::Any, format: "wldrw\t%12-15G, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wldr, opcode: 0x0c100000, mask: 0x0e100e00, isa: IsaApplicability::Any, format: "wldr%L%c\t%12-15g, %l" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmac, opcode: 0x0e400100, mask: 0x0fc00ff0, isa: IsaApplicability::Any, format: "wmac%21?su%20'z%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmadd, opcode: 0x0e800100, mask: 0x0fc00ff0, isa: IsaApplicability::Any, format: "wmadd%21?su%20'x%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmadd, opcode: 0x0ec00100, mask: 0x0fd00ff0, isa: IsaApplicability::Any, format: "wmadd%21?sun%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmax, opcode: 0x0e000160, mask: 0x0f100ff0, isa: IsaApplicability::Any, format: "wmax%21?su%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmerge, opcode: 0x0e000080, mask: 0x0f100fe0, isa: IsaApplicability::Any, format: "wmerge%c\t%12-15g, %16-19g, %0-3g, %{I:#%21-23d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmia, opcode: 0x0e0000a0, mask: 0x0f800ff0, isa: IsaApplicability::Any, format: "wmia%21?tb%20?tb%22'n%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmiaw, opcode: 0x0e800120, mask: 0x0f800ff0, isa: IsaApplicability::Any, format: "wmiaw%21?tb%20?tb%22'n%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmin, opcode: 0x0e100160, mask: 0x0f100ff0, isa: IsaApplicability::Any, format: "wmin%21?su%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmul, opcode: 0x0e000100, mask: 0x0fc00ff0, isa: IsaApplicability::Any, format: "wmul%21?su%20?ml%23'r%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmul, opcode: 0x0ed00100, mask: 0x0fd00ff0, isa: IsaApplicability::Any, format: "wmul%21?sumr%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmulwsm, opcode: 0x0ee000c0, mask: 0x0fe00ff0, isa: IsaApplicability::Any, format: "wmulwsm%20`r%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmulwum, opcode: 0x0ec000c0, mask: 0x0fe00ff0, isa: IsaApplicability::Any, format: "wmulwum%20`r%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wmulwl, opcode: 0x0eb000c0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "wmulwl%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wqmia, opcode: 0x0e8000a0, mask: 0x0f800ff0, isa: IsaApplicability::Any, format: "wqmia%21?tb%20?tb%22'n%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wqmulm, opcode: 0x0e100080, mask: 0x0fd00ff0, isa: IsaApplicability::Any, format: "wqmulm%21'r%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wqmulwm, opcode: 0x0ec000e0, mask: 0x0fd00ff0, isa: IsaApplicability::Any, format: "wqmulwm%21'r%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wor, opcode: 0x0e000000, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "wor%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wpack, opcode: 0x0e000080, mask: 0x0f000ff0, isa: IsaApplicability::Any, format: "wpack%20-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wror, opcode: 0xfe300040, mask: 0xff300ef0, isa: IsaApplicability::Any, format: "wror%22-23w\t%12-15g, %16-19g, %{I:#%i%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wror, opcode: 0x0e300040, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wror%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wror, opcode: 0x0e300140, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wror%22-23wg%c\t%12-15g, %16-19g, %0-3G" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsad, opcode: 0x0e000120, mask: 0x0fa00ff0, isa: IsaApplicability::Any, format: "wsad%22?hb%20'z%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wshufh, opcode: 0x0e0001e0, mask: 0x0f000ff0, isa: IsaApplicability::Any, format: "wshufh%c\t%12-15g, %16-19g, %{I:#%Z%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsll, opcode: 0xfe100040, mask: 0xff300ef0, isa: IsaApplicability::Any, format: "wsll%22-23w\t%12-15g, %16-19g, %{I:#%i%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsll, opcode: 0x0e100040, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wsll%22-23w%8'g%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsll, opcode: 0x0e100148, mask: 0x0f300ffc, isa: IsaApplicability::Any, format: "wsll%22-23w%8'g%c\t%12-15g, %16-19g, %0-3G" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsra, opcode: 0xfe000040, mask: 0xff300ef0, isa: IsaApplicability::Any, format: "wsra%22-23w\t%12-15g, %16-19g, %{I:#%i%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsra, opcode: 0x0e000040, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wsra%22-23w%8'g%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsra, opcode: 0x0e000148, mask: 0x0f300ffc, isa: IsaApplicability::Any, format: "wsra%22-23w%8'g%c\t%12-15g, %16-19g, %0-3G" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsrl, opcode: 0xfe200040, mask: 0xff300ef0, isa: IsaApplicability::Any, format: "wsrl%22-23w\t%12-15g, %16-19g, %{I:#%i%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsrl, opcode: 0x0e200040, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wsrl%22-23w%8'g%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsrl, opcode: 0x0e200148, mask: 0x0f300ffc, isa: IsaApplicability::Any, format: "wsrl%22-23w%8'g%c\t%12-15g, %16-19g, %0-3G" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wstrd, opcode: 0xfc400100, mask: 0xfe500f00, isa: IsaApplicability::Any, format: "wstrd\t%12-15g, %r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wstrw, opcode: 0xfc000100, mask: 0xfe500f00, isa: IsaApplicability::Any, format: "wstrw\t%12-15G, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wstr, opcode: 0x0c000000, mask: 0x0e100e00, isa: IsaApplicability::Any, format: "wstr%L%c\t%12-15g, %l" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsub, opcode: 0x0e0001a0, mask: 0x0f000ff0, isa: IsaApplicability::Any, format: "wsub%20-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wsubaddhx, opcode: 0x0ed001c0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "wsubaddhx%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wabsdiff, opcode: 0x0e1001c0, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wabsdiff%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wunpckeh, opcode: 0x0e0000c0, mask: 0x0fd00fff, isa: IsaApplicability::Any, format: "wunpckeh%21?sub%c\t%12-15g, %16-19g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wunpckeh, opcode: 0x0e4000c0, mask: 0x0fd00fff, isa: IsaApplicability::Any, format: "wunpckeh%21?suh%c\t%12-15g, %16-19g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wunpckeh, opcode: 0x0e8000c0, mask: 0x0fd00fff, isa: IsaApplicability::Any, format: "wunpckeh%21?suw%c\t%12-15g, %16-19g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wunpckel, opcode: 0x0e0000e0, mask: 0x0f100fff, isa: IsaApplicability::Any, format: "wunpckel%21?su%22-23w%c\t%12-15g, %16-19g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wunpckih, opcode: 0x0e1000c0, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wunpckih%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wunpckil, opcode: 0x0e1000e0, mask: 0x0f300ff0, isa: IsaApplicability::Any, format: "wunpckil%22-23w%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wxor, opcode: 0x0e100000, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "wxor%c\t%12-15g, %16-19g, %0-3g" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Adf, opcode: 0x0e000100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "adf%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Muf, opcode: 0x0e100100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "muf%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Suf, opcode: 0x0e200100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "suf%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Rsf, opcode: 0x0e300100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "rsf%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Dvf, opcode: 0x0e400100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "dvf%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Rdf, opcode: 0x0e500100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "rdf%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Pow, opcode: 0x0e600100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "pow%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Rpw, opcode: 0x0e700100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "rpw%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Rmf, opcode: 0x0e800100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "rmf%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Fml, opcode: 0x0e900100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "fml%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Fdv, opcode: 0x0ea00100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "fdv%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Frd, opcode: 0x0eb00100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "frd%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Pol, opcode: 0x0ec00100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "pol%c%P%R\t%12-14f, %16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mvf, opcode: 0x0e008100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "mvf%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mnf, opcode: 0x0e108100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "mnf%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Abs, opcode: 0x0e208100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "abs%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Rnd, opcode: 0x0e308100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "rnd%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sqt, opcode: 0x0e408100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "sqt%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Log, opcode: 0x0e508100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "log%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Lgn, opcode: 0x0e608100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "lgn%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Exp, opcode: 0x0e708100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "exp%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sin, opcode: 0x0e808100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "sin%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cos, opcode: 0x0e908100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "cos%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Tan, opcode: 0x0ea08100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "tan%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Asn, opcode: 0x0eb08100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "asn%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Acs, opcode: 0x0ec08100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "acs%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Atn, opcode: 0x0ed08100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "atn%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Urd, opcode: 0x0ee08100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "urd%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Nrm, opcode: 0x0ef08100, mask: 0x0ff08f10, isa: IsaApplicability::Any, format: "nrm%c%P%R\t%12-14f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Flt, opcode: 0x0e000110, mask: 0x0ff00f1f, isa: IsaApplicability::Any, format: "flt%c%P%R\t%16-18f, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Fix, opcode: 0x0e100110, mask: 0x0fff0f98, isa: IsaApplicability::Any, format: "fix%c%R\t%12-15r, %0-2f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wfs, opcode: 0x0e200110, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "wfs%c\t%12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Rfs, opcode: 0x0e300110, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "rfs%c\t%12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Wfc, opcode: 0x0e400110, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "wfc%c\t%12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Rfc, opcode: 0x0e500110, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "rfc%c\t%12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cmf, opcode: 0x0e90f110, mask: 0x0ff8fff0, isa: IsaApplicability::Any, format: "cmf%c\t%16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cnf, opcode: 0x0eb0f110, mask: 0x0ff8fff0, isa: IsaApplicability::Any, format: "cnf%c\t%16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cmfe, opcode: 0x0ed0f110, mask: 0x0ff8fff0, isa: IsaApplicability::Any, format: "cmfe%c\t%16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cnfe, opcode: 0x0ef0f110, mask: 0x0ff8fff0, isa: IsaApplicability::Any, format: "cnfe%c\t%16-18f, %0-3f" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Stf, opcode: 0x0c000100, mask: 0x0e100f00, isa: IsaApplicability::Any, format: "stf%c%Q\t%12-14f, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Ldf, opcode: 0x0c100100, mask: 0x0e100f00, isa: IsaApplicability::Any, format: "ldf%c%Q\t%12-14f, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Sfm, opcode: 0x0c000200, mask: 0x0e100f00, isa: IsaApplicability::Any, format: "sfm%c\t%12-14f, %F, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Lfm, opcode: 0x0c100200, mask: 0x0e100f00, isa: IsaApplicability::Any, format: "lfm%c\t%12-14f, %F, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vscclrm, opcode: 0xec9f0b00, mask: 0xffbf0f01, isa: IsaApplicability::Thumb, format: "vscclrm%c\t%C" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vscclrm, opcode: 0xec9f0a00, mask: 0xffbf0f00, isa: IsaApplicability::Thumb, format: "vscclrm%c\t%C" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vlldm, opcode: 0xec300a00, mask: 0xfff0ffff, isa: IsaApplicability::Any, format: "vlldm\t%16-19r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vlstm, opcode: 0xec200a00, mask: 0xfff0ffff, isa: IsaApplicability::Any, format: "vlstm\t%16-19r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpush, opcode: 0x0d2d0b00, mask: 0x0fbf0f01, isa: IsaApplicability::Any, format: "vpush%c\t%B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstmdb, opcode: 0x0d200b00, mask: 0x0fb00f01, isa: IsaApplicability::Any, format: "vstmdb%c\t%16-19r!, %B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldmdb, opcode: 0x0d300b00, mask: 0x0fb00f01, isa: IsaApplicability::Any, format: "vldmdb%c\t%16-19r!, %B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstmia, opcode: 0x0c800b00, mask: 0x0f900f01, isa: IsaApplicability::Any, format: "vstmia%c\t%16-19r%21'!, %B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpop, opcode: 0x0cbd0b00, mask: 0x0fbf0f01, isa: IsaApplicability::Any, format: "vpop%c\t%B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldmia, opcode: 0x0c900b00, mask: 0x0f900f01, isa: IsaApplicability::Any, format: "vldmia%c\t%16-19r%21'!, %B" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstr, opcode: 0x0d000b00, mask: 0x0f300f00, isa: IsaApplicability::Any, format: "vstr%c\t%12-15,22D, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldr, opcode: 0x0d100b00, mask: 0x0f300f00, isa: IsaApplicability::Any, format: "vldr%c\t%12-15,22D, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpush, opcode: 0x0d2d0a00, mask: 0x0fbf0f00, isa: IsaApplicability::Any, format: "vpush%c\t%y3" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstmdb, opcode: 0x0d200a00, mask: 0x0fb00f00, isa: IsaApplicability::Any, format: "vstmdb%c\t%16-19r!, %y3" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldmdb, opcode: 0x0d300a00, mask: 0x0fb00f00, isa: IsaApplicability::Any, format: "vldmdb%c\t%16-19r!, %y3" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstmia, opcode: 0x0c800a00, mask: 0x0f900f00, isa: IsaApplicability::Any, format: "vstmia%c\t%16-19r%21'!, %y3" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vpop, opcode: 0x0cbd0a00, mask: 0x0fbf0f00, isa: IsaApplicability::Any, format: "vpop%c\t%y3" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldmia, opcode: 0x0c900a00, mask: 0x0f900f00, isa: IsaApplicability::Any, format: "vldmia%c\t%16-19r%21'!, %y3" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstr, opcode: 0x0d000a00, mask: 0x0f300f00, isa: IsaApplicability::Any, format: "vstr%c\t%y1, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldr, opcode: 0x0d100a00, mask: 0x0f300f00, isa: IsaApplicability::Any, format: "vldr%c\t%y1, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldr, opcode: 0xec100f80, mask: 0xfe101f80, isa: IsaApplicability::Any, format: "vldr%c\t%J, %K" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstr, opcode: 0xec000f80, mask: 0xfe101f80, isa: IsaApplicability::Any, format: "vstr%c\t%J, %K" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Fstmdbx, opcode: 0x0d200b01, mask: 0x0fb00f01, isa: IsaApplicability::Any, format: "fstmdbx%c\t%16-19r!, %z3\t@ Deprecated" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Fldmdbx, opcode: 0x0d300b01, mask: 0x0fb00f01, isa: IsaApplicability::Any, format: "fldmdbx%c\t%16-19r!, %z3\t@ Deprecated" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Fstmiax, opcode: 0x0c800b01, mask: 0x0f900f01, isa: IsaApplicability::Any, format: "fstmiax%c\t%16-19r%21'!, %z3\t@ Deprecated" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Fldmiax, opcode: 0x0c900b01, mask: 0x0f900f01, isa: IsaApplicability::Any, format: "fldmiax%c\t%16-19r%21'!, %z3\t@ Deprecated" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0c400b10, mask: 0x0ff00fd0, isa: IsaApplicability::Any, format: "vmov%c\t%0-3,5D, %12-15r, %16-19r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0c500b10, mask: 0x0ff00fd0, isa: IsaApplicability::Any, format: "vmov%c\t%12-15r, %16-19r, %0-3,5D" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e000b10, mask: 0x0fd00f70, isa: IsaApplicability::Any, format: "vmov%c.32\t%{R:%16-19,7D[%21d]%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e100b10, mask: 0x0f500f70, isa: IsaApplicability::Any, format: "vmov%c.32\t%12-15r, %{R:%16-19,7D[%21d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e000b30, mask: 0x0fd00f30, isa: IsaApplicability::Any, format: "vmov%c.16\t%{R:%16-19,7D[%6,21d]%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e100b30, mask: 0x0f500f30, isa: IsaApplicability::Any, format: "vmov%c.%23?us16\t%12-15r, %{R:%16-19,7D[%6,21d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e400b10, mask: 0x0fd00f10, isa: IsaApplicability::Any, format: "vmov%c.8\t%{R:%16-19,7D[%5,6,21d]%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e500b10, mask: 0x0f500f10, isa: IsaApplicability::Any, format: "vmov%c.%23?us8\t%12-15r, %{R:%16-19,7D[%5,6,21d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb20b40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%7?tb%c.f64.f16\t%z1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb30b40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%7?tb%c.f16.f64\t%y1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb20a40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%7?tb%c.f32.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb30a40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%7?tb%c.f16.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee00a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpsid%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee10a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpscr%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee20a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpscr_nzcvqc%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee60a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:mvfr1%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee70a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:mvfr0%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee50a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:mvfr2%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee80a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpexc%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee90a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpinst%}, %12-15r\t@ Impl def" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0eea0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpinst2%}, %12-15r\t@ Impl def" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0eec0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:vpr%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0eed0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:p0%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0eee0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpcxt_ns%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0eef0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmsr%c\t%{R:fpcxt_s%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef00a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpsid%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef1fa10, mask: 0x0fffffff, isa: IsaApplicability::Any, format: "vmrs%c\t%{R:APSR_nzcv%}, %{R:fpscr%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef10a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpscr%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef20a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpscr_nzcvqc%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef50a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:mvfr2%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef60a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:mvfr1%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef70a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:mvfr0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef80a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpexc%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef90a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpinst%}\t@ Impl def" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0efa0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpinst2%}\t@ Impl def" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0efc0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:vpr%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0efd0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:p0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0efe0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpcxt_ns%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0eff0a10, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, %{R:fpcxt_s%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e000b10, mask: 0x0fd00fff, isa: IsaApplicability::Any, format: "vmov%c.32\t%z2[%{I:%21d%}], %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e100b10, mask: 0x0fd00fff, isa: IsaApplicability::Any, format: "vmov%c.32\t%12-15r, %z2[%{I:%21d%}]" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmsr, opcode: 0x0ee00a10, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "vmsr%c\t<impl def %16-19x>, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmrs, opcode: 0x0ef00a10, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "vmrs%c\t%12-15r, <impl def %16-19x>" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e000a10, mask: 0x0ff00f7f, isa: IsaApplicability::Any, format: "vmov%c\t%y2, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e100a10, mask: 0x0ff00f7f, isa: IsaApplicability::Any, format: "vmov%c\t%12-15r, %y2" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmp, opcode: 0x0eb50a40, mask: 0x0fbf0f70, isa: IsaApplicability::Any, format: "vcmp%7'e%c.f32\t%y1, %{I:#0.0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmp, opcode: 0x0eb50b40, mask: 0x0fbf0f70, isa: IsaApplicability::Any, format: "vcmp%7'e%c.f64\t%z1, %{I:#0.0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0eb00a40, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vmov%c.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabs, opcode: 0x0eb00ac0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vabs%c.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0eb00b40, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vmov%c.f64\t%z1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabs, opcode: 0x0eb00bc0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vabs%c.f64\t%z1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vneg, opcode: 0x0eb10a40, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vneg%c.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsqrt, opcode: 0x0eb10ac0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vsqrt%c.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vneg, opcode: 0x0eb10b40, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vneg%c.f64\t%z1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsqrt, opcode: 0x0eb10bc0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vsqrt%c.f64\t%z1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb70ac0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vcvt%c.f64.f32\t%z1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb70bc0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vcvt%c.f32.f64\t%y1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb80a40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%c.f32.%7?su32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb80b40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%c.f64.%7?su32\t%z1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmp, opcode: 0x0eb40a40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcmp%7'e%c.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmp, opcode: 0x0eb40b40, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcmp%7'e%c.f64\t%z1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eba0a40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vcvt%c.f32.%16?us%7?31%7?26\t%y1, %y1, %{I:#%5,0-3k%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eba0b40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vcvt%c.f64.%16?us%7?31%7?26\t%z1, %z1, %{I:#%5,0-3k%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0ebc0a40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vcvt%7`r%c.%16?su32.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0ebc0b40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vcvt%7`r%c.%16?su32.f64\t%y1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0ebe0a40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vcvt%c.%16?us%7?31%7?26.f32\t%y1, %y1, %{I:#%5,0-3k%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0ebe0b40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vcvt%c.%16?us%7?31%7?26.f64\t%z1, %z1, %{I:#%5,0-3k%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0c500b10, mask: 0x0fb00ff0, isa: IsaApplicability::Any, format: "vmov%c\t%12-15r, %16-19r, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0eb00a00, mask: 0x0fb00ff0, isa: IsaApplicability::Any, format: "vmov%c.f32\t%y1, %{I:#%0-3,16-19E%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0eb00b00, mask: 0x0fb00ff0, isa: IsaApplicability::Any, format: "vmov%c.f64\t%z1, %{I:#%0-3,16-19E%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0c400a10, mask: 0x0ff00fd0, isa: IsaApplicability::Any, format: "vmov%c\t%y4, %12-15r, %16-19r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0c400b10, mask: 0x0ff00fd0, isa: IsaApplicability::Any, format: "vmov%c\t%z0, %12-15r, %16-19r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0c500a10, mask: 0x0ff00fd0, isa: IsaApplicability::Any, format: "vmov%c\t%12-15r, %16-19r, %y4" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0x0e000a00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmla%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0x0e000a40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmls%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0x0e000b00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmla%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0x0e000b40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmls%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmls, opcode: 0x0e100a00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmls%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmla, opcode: 0x0e100a40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmla%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmls, opcode: 0x0e100b00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmls%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmla, opcode: 0x0e100b40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmla%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0x0e200a00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmul%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmul, opcode: 0x0e200a40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmul%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0x0e200b00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmul%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmul, opcode: 0x0e200b40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmul%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vadd, opcode: 0x0e300a00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vadd%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsub, opcode: 0x0e300a40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vsub%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vadd, opcode: 0x0e300b00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vadd%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsub, opcode: 0x0e300b40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vsub%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdiv, opcode: 0x0e800a00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vdiv%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdiv, opcode: 0x0e800b00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vdiv%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldrs, opcode: 0x0d100400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldrs%c\t%{R:mvf%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldrs, opcode: 0x0c100400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldrs%c\t%{R:mvf%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldrd, opcode: 0x0d500400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldrd%c\t%{R:mvd%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldrd, opcode: 0x0c500400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldrd%c\t%{R:mvd%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldr32, opcode: 0x0d100500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldr32%c\t%{R:mvfx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldr32, opcode: 0x0c100500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldr32%c\t%{R:mvfx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldr64, opcode: 0x0d500500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldr64%c\t%{R:mvdx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfldr64, opcode: 0x0c500500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfldr64%c\t%{R:mvdx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstrs, opcode: 0x0d000400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstrs%c\t%{R:mvf%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstrs, opcode: 0x0c000400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstrs%c\t%{R:mvf%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstrd, opcode: 0x0d400400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstrd%c\t%{R:mvd%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstrd, opcode: 0x0c400400, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstrd%c\t%{R:mvd%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstr32, opcode: 0x0d000500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstr32%c\t%{R:mvfx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstr32, opcode: 0x0c000500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstr32%c\t%{R:mvfx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstr64, opcode: 0x0d400500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstr64%c\t%{R:mvdx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfstr64, opcode: 0x0c400500, mask: 0x0f500f00, isa: IsaApplicability::Any, format: "cfstr64%c\t%{R:mvdx%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvsr, opcode: 0x0e000450, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmvsr%c\t%{R:mvf%16-19d%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvrs, opcode: 0x0e100450, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmvrs%c\t%12-15r, %{R:mvf%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvdlr, opcode: 0x0e000410, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmvdlr%c\t%{R:mvd%16-19d%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvrdl, opcode: 0x0e100410, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmvrdl%c\t%12-15r, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvdhr, opcode: 0x0e000430, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmvdhr%c\t%{R:mvd%16-19d%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvrdh, opcode: 0x0e100430, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmvrdh%c\t%12-15r, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv64lr, opcode: 0x0e000510, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmv64lr%c\t%{R:mvdx%16-19d%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvr64l, opcode: 0x0e100510, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmvr64l%c\t%12-15r, %{R:mvdx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv64hr, opcode: 0x0e000530, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmv64hr%c\t%{R:mvdx%16-19d%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvr64h, opcode: 0x0e100530, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmvr64h%c\t%12-15r, %{R:mvdx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmval32, opcode: 0x0e200440, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmval32%c\t%{R:mvax%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv32al, opcode: 0x0e100440, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmv32al%c\t%{R:mvfx%12-15d%}, %{R:mvax%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvam32, opcode: 0x0e200460, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmvam32%c\t%{R:mvax%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv32am, opcode: 0x0e100460, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmv32am%c\t%{R:mvfx%12-15d%}, %{R:mvax%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvah32, opcode: 0x0e200480, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmvah32%c\t%{R:mvax%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv32ah, opcode: 0x0e100480, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmv32ah%c\t%{R:mvfx%12-15d%}, %{R:mvax%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmva32, opcode: 0x0e2004a0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmva32%c\t%{R:mvax%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv32a, opcode: 0x0e1004a0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmv32a%c\t%{R:mvfx%12-15d%}, %{R:mvax%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmva64, opcode: 0x0e2004c0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmva64%c\t%{R:mvax%12-15d%}, %{R:mvdx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv64a, opcode: 0x0e1004c0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfmv64a%c\t%{R:mvdx%12-15d%}, %{R:mvax%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmvsc32, opcode: 0x0e2004e0, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "cfmvsc32%c\t%{R:dspsc%}, %{R:mvdx%12-15d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmv32sc, opcode: 0x0e1004e0, mask: 0x0fff0fff, isa: IsaApplicability::Any, format: "cfmv32sc%c\t%{R:mvdx%12-15d%}, %{R:dspsc%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcpys, opcode: 0x0e000400, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcpys%c\t%{R:mvf%12-15d%}, %{R:mvf%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcpyd, opcode: 0x0e000420, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcpyd%c\t%{R:mvd%12-15d%}, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvtsd, opcode: 0x0e000460, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvtsd%c\t%{R:mvd%12-15d%}, %{R:mvf%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvtds, opcode: 0x0e000440, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvtds%c\t%{R:mvf%12-15d%}, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvt32s, opcode: 0x0e000480, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvt32s%c\t%{R:mvf%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvt32d, opcode: 0x0e0004a0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvt32d%c\t%{R:mvd%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvt64s, opcode: 0x0e0004c0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvt64s%c\t%{R:mvf%12-15d%}, %{R:mvdx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvt64d, opcode: 0x0e0004e0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvt64d%c\t%{R:mvd%12-15d%}, %{R:mvdx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvts32, opcode: 0x0e100580, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvts32%c\t%{R:mvfx%12-15d%}, %{R:mvf%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcvtd32, opcode: 0x0e1005a0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfcvtd32%c\t%{R:mvfx%12-15d%}, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cftruncs32, opcode: 0x0e1005c0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cftruncs32%c\t%{R:mvfx%12-15d%}, %{R:mvf%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cftruncd32, opcode: 0x0e1005e0, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cftruncd32%c\t%{R:mvfx%12-15d%}, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfrshl32, opcode: 0x0e000550, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfrshl32%c\t%{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfrshl64, opcode: 0x0e000570, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfrshl64%c\t%{R:mvdx%16-19d%}, %{R:mvdx%0-3d%}, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfsh32, opcode: 0x0e000500, mask: 0x0ff00f10, isa: IsaApplicability::Any, format: "cfsh32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{I:#%I%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfsh64, opcode: 0x0e200500, mask: 0x0ff00f10, isa: IsaApplicability::Any, format: "cfsh64%c\t%{R:mvdx%12-15d%}, %{R:mvdx%16-19d%}, %{I:#%I%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcmps, opcode: 0x0e100490, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfcmps%c\t%12-15r, %{R:mvf%16-19d%}, %{R:mvf%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcmpd, opcode: 0x0e1004b0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfcmpd%c\t%12-15r, %{R:mvd%16-19d%}, %{R:mvd%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcmp32, opcode: 0x0e100590, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfcmp32%c\t%12-15r, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfcmp64, opcode: 0x0e1005b0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfcmp64%c\t%12-15r, %{R:mvdx%16-19d%}, %{R:mvdx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfabss, opcode: 0x0e300400, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfabss%c\t%{R:mvf%12-15d%}, %{R:mvf%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfabsd, opcode: 0x0e300420, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfabsd%c\t%{R:mvd%12-15d%}, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfnegs, opcode: 0x0e300440, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfnegs%c\t%{R:mvf%12-15d%}, %{R:mvf%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfnegd, opcode: 0x0e300460, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfnegd%c\t%{R:mvd%12-15d%}, %{R:mvd%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfadds, opcode: 0x0e300480, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfadds%c\t%{R:mvf%12-15d%}, %{R:mvf%16-19d%}, %{R:mvf%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfaddd, opcode: 0x0e3004a0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfaddd%c\t%{R:mvd%12-15d%}, %{R:mvd%16-19d%}, %{R:mvd%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfsubs, opcode: 0x0e3004c0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfsubs%c\t%{R:mvf%12-15d%}, %{R:mvf%16-19d%}, %{R:mvf%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfsubd, opcode: 0x0e3004e0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfsubd%c\t%{R:mvd%12-15d%}, %{R:mvd%16-19d%}, %{R:mvd%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmuls, opcode: 0x0e100400, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmuls%c\t%{R:mvf%12-15d%}, %{R:mvf%16-19d%}, %{R:mvf%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmuld, opcode: 0x0e100420, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmuld%c\t%{R:mvd%12-15d%}, %{R:mvd%16-19d%}, %{R:mvd%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfabs32, opcode: 0x0e300500, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfabs32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfabs64, opcode: 0x0e300520, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfabs64%c\t%{R:mvdx%12-15d%}, %{R:mvdx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfneg32, opcode: 0x0e300540, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfneg32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfneg64, opcode: 0x0e300560, mask: 0x0ff00fff, isa: IsaApplicability::Any, format: "cfneg64%c\t%{R:mvdx%12-15d%}, %{R:mvdx%16-19d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfadd32, opcode: 0x0e300580, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfadd32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfadd64, opcode: 0x0e3005a0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfadd64%c\t%{R:mvdx%12-15d%}, %{R:mvdx%16-19d%}, %{R:mvdx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfsub32, opcode: 0x0e3005c0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfsub32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfsub64, opcode: 0x0e3005e0, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfsub64%c\t%{R:mvdx%12-15d%}, %{R:mvdx%16-19d%}, %{R:mvdx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmul32, opcode: 0x0e100500, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmul32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmul64, opcode: 0x0e100520, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmul64%c\t%{R:mvdx%12-15d%}, %{R:mvdx%16-19d%}, %{R:mvdx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmac32, opcode: 0x0e100540, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmac32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmsc32, opcode: 0x0e100560, mask: 0x0ff00ff0, isa: IsaApplicability::Any, format: "cfmsc32%c\t%{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmadd32, opcode: 0x0e000600, mask: 0x0ff00f10, isa: IsaApplicability::Any, format: "cfmadd32%c\t%{R:mvax%5-7d%}, %{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmsub32, opcode: 0x0e100600, mask: 0x0ff00f10, isa: IsaApplicability::Any, format: "cfmsub32%c\t%{R:mvax%5-7d%}, %{R:mvfx%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmadda32, opcode: 0x0e200600, mask: 0x0ff00f10, isa: IsaApplicability::Any, format: "cfmadda32%c\t%{R:mvax%5-7d%}, %{R:mvax%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cfmsuba32, opcode: 0x0e300600, mask: 0x0ff00f10, isa: IsaApplicability::Any, format: "cfmsuba32%c\t%{R:mvax%5-7d%}, %{R:mvax%12-15d%}, %{R:mvfx%16-19d%}, %{R:mvfx%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfma, opcode: 0x0ea00a00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfma%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfma, opcode: 0x0ea00b00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfma%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfms, opcode: 0x0ea00a40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfms%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfms, opcode: 0x0ea00b40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfms%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfnma, opcode: 0x0e900a40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfnma%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfnma, opcode: 0x0e900b40, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfnma%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfnms, opcode: 0x0e900a00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfnms%c.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfnms, opcode: 0x0e900b00, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfnms%c.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsel, opcode: 0xfe000a00, mask: 0xff800f50, isa: IsaApplicability::Any, format: "vsel%20-21c%u.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsel, opcode: 0xfe000b00, mask: 0xff800f50, isa: IsaApplicability::Any, format: "vsel%20-21c%u.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmaxnm, opcode: 0xfe800a00, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vmaxnm%u.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmaxnm, opcode: 0xfe800b00, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vmaxnm%u.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vminnm, opcode: 0xfe800a40, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vminnm%u.f32\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vminnm, opcode: 0xfe800b40, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vminnm%u.f64\t%z1, %z2, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xfebc0a40, mask: 0xffbc0f50, isa: IsaApplicability::Any, format: "vcvt%16-17?mpna%u.%7?su32.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xfebc0b40, mask: 0xffbc0f50, isa: IsaApplicability::Any, format: "vcvt%16-17?mpna%u.%7?su32.f64\t%y1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0x0eb60a40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vrint%7,16??xzr%c.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0x0eb60b40, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vrint%7,16??xzr%c.f64\t%z1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0xfeb80a40, mask: 0xffbc0fd0, isa: IsaApplicability::Any, format: "vrint%16-17?mpna%u.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0xfeb80b40, mask: 0xffbc0fd0, isa: IsaApplicability::Any, format: "vrint%16-17?mpna%u.f64\t%z1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcadd, opcode: 0xfc800800, mask: 0xfeb00f10, isa: IsaApplicability::Any, format: "vcadd%c.f16\t%12-15,22V, %16-19,7V, %0-3,5V, %{I:#%24?29%24'70%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcadd, opcode: 0xfc900800, mask: 0xfeb00f10, isa: IsaApplicability::Any, format: "vcadd%c.f32\t%12-15,22V, %16-19,7V, %0-3,5V, %{I:#%24?29%24'70%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfc200800, mask: 0xff300f10, isa: IsaApplicability::Any, format: "vcmla%c.f16\t%12-15,22V, %16-19,7V, %0-3,5V, %{I:#%23'90%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfd200800, mask: 0xff300f10, isa: IsaApplicability::Any, format: "vcmla%c.f16\t%12-15,22V, %16-19,7V, %0-3,5V, %{I:#%23?21%23?780%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfc300800, mask: 0xff300f10, isa: IsaApplicability::Any, format: "vcmla%c.f32\t%12-15,22V, %16-19,7V, %0-3,5V, %{I:#%23'90%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfd300800, mask: 0xff300f10, isa: IsaApplicability::Any, format: "vcmla%c.f32\t%12-15,22V, %16-19,7V, %0-3,5V, %{I:#%23?21%23?780%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfe000800, mask: 0xffa00f10, isa: IsaApplicability::Any, format: "vcmla%c.f16\t%12-15,22V, %16-19,7V, %{R:%0-3D[%5?10]%}, %{I:#%20'90%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfe200800, mask: 0xffa00f10, isa: IsaApplicability::Any, format: "vcmla%c.f16\t%12-15,22V, %16-19,7V, %{R:%0-3D[%5?10]%}, %{I:#%20?21%20?780%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfe800800, mask: 0xffa00f10, isa: IsaApplicability::Any, format: "vcmla%c.f32\t%12-15,22V, %16-19,7V, %{R:%0-3,5D[0]%}, %{I:#%20'90%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmla, opcode: 0xfea00800, mask: 0xffa00f10, isa: IsaApplicability::Any, format: "vcmla%c.f32\t%12-15,22V, %16-19,7V, %{R:%0-3,5D[0]%}, %{I:#%20?21%20?780%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb30940, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%7?tb%b.bf16.f32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xfc200d00, mask: 0xffb00f00, isa: IsaApplicability::Any, format: "v%4?usdot.%4?us8\t%12-15,22V, %16-19,7V, %0-3,5V" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::V, opcode: 0xfe200d00, mask: 0xff200f00, isa: IsaApplicability::Any, format: "v%4?usdot.%4?us8\t%12-15,22V, %16-19,7V, %{R:%0-3D[%5?10]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmalF16, opcode: 0xfc200810, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmal.f16\t%12-15,22D, %{R:s%7,16-19d%}, %{R:s%5,0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmslF16, opcode: 0xfca00810, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmsl.f16\t%12-15,22D, %{R:s%7,16-19d%}, %{R:s%5,0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmalF16, opcode: 0xfc200850, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmal.f16\t%12-15,22Q, %{R:d%16-19,7d%}, %{R:d%0-3,5d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmslF16, opcode: 0xfca00850, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmsl.f16\t%12-15,22Q, %{R:d%16-19,7d%}, %{R:d%0-3,5d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmalF16, opcode: 0xfe000810, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmal.f16\t%12-15,22D, %{R:s%7,16-19d%}, %{R:s%5,0-2d[%3d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmslF16, opcode: 0xfe100810, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmsl.f16\t%12-15,22D, %{R:s%7,16-19d%}, %{R:s%5,0-2d[%3d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmalF16, opcode: 0xfe000850, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmal.f16\t%12-15,22Q, %{R:d%16-19,7d%}, %{R:d%0-2d[%3,5d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VfmslF16, opcode: 0xfe100850, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vfmsl.f16\t%12-15,22Q, %{R:d%16-19,7d%}, %{R:d%0-2d[%3,5d]%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vabs, opcode: 0x0eb009c0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vabs%c.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vadd, opcode: 0x0e300900, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vadd%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmp, opcode: 0x0eb40940, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcmp%7'e%c.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcmp, opcode: 0x0eb50940, mask: 0x0fbf0f70, isa: IsaApplicability::Any, format: "vcmp%7'e%c.f16\t%y1, %{I:#0.0%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eba09c0, mask: 0x0fbe0fd0, isa: IsaApplicability::Any, format: "vcvt%c.f16.%16?us%7?31%7?26\t%y1, %y1, %{I:#%5,0-3k%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0ebe09c0, mask: 0x0fbe0fd0, isa: IsaApplicability::Any, format: "vcvt%c.%16?us%7?31%7?26.f16\t%y1, %y1, %{I:#%5,0-3k%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0ebc0940, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vcvt%7`r%c.%16?su32.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0x0eb80940, mask: 0x0fbf0f50, isa: IsaApplicability::Any, format: "vcvt%c.f16.%7?su32\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vcvt, opcode: 0xfebc0940, mask: 0xffbc0f50, isa: IsaApplicability::Any, format: "vcvt%16-17?mpna%u.%7?su32.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vdiv, opcode: 0x0e800900, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vdiv%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfma, opcode: 0x0ea00900, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfma%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfms, opcode: 0x0ea00940, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfms%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfnma, opcode: 0x0e900940, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfnma%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vfnms, opcode: 0x0e900900, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vfnms%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::VinsF16, opcode: 0xfeb00ac0, mask: 0xffbf0fd0, isa: IsaApplicability::Any, format: "vins.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmovx, opcode: 0xfeb00a40, mask: 0xffbf0fd0, isa: IsaApplicability::Any, format: "vmovx%c.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vldr, opcode: 0x0d100900, mask: 0x0f300f00, isa: IsaApplicability::Any, format: "vldr%c.16\t%y1, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vstr, opcode: 0x0d000900, mask: 0x0f300f00, isa: IsaApplicability::Any, format: "vstr%c.16\t%y1, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmaxnm, opcode: 0xfe800900, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vmaxnm%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vminnm, opcode: 0xfe800940, mask: 0xffb00f50, isa: IsaApplicability::Any, format: "vminnm%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmla, opcode: 0x0e000900, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmla%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmls, opcode: 0x0e000940, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmls%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e100910, mask: 0x0ff00f7f, isa: IsaApplicability::Any, format: "vmov%c.f16\t%12-15r, %y2" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0e000910, mask: 0x0ff00f7f, isa: IsaApplicability::Any, format: "vmov%c.f16\t%y2, %12-15r" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmov, opcode: 0x0eb00900, mask: 0x0fb00ff0, isa: IsaApplicability::Any, format: "vmov%c.f16\t%y1, %{I:#%0-3,16-19E%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vmul, opcode: 0x0e200900, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vmul%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vneg, opcode: 0x0eb10940, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vneg%c.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmla, opcode: 0x0e100940, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmla%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmls, opcode: 0x0e100900, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmls%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vnmul, opcode: 0x0e200940, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vnmul%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0x0eb60940, mask: 0x0fbe0f50, isa: IsaApplicability::Any, format: "vrint%7,16??xzr%c.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vrint, opcode: 0xfeb80940, mask: 0xffbc0fd0, isa: IsaApplicability::Any, format: "vrint%16-17?mpna%u.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsel, opcode: 0xfe000900, mask: 0xff800f50, isa: IsaApplicability::Any, format: "vsel%20-21c%u.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsqrt, opcode: 0x0eb109c0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vsqrt%c.f16\t%y1, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vsub, opcode: 0x0e300940, mask: 0x0fb00f50, isa: IsaApplicability::Any, format: "vsub%c.f16\t%y1, %y2, %y0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Vjcvt, opcode: 0x0eb90bc0, mask: 0x0fbf0fd0, isa: IsaApplicability::Any, format: "vjcvt%c.s32.f64\t%y1, %z0" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mcrr, opcode: 0x0c400000, mask: 0x0ff00000, isa: IsaApplicability::Any, format: "mcrr%c\t%{I:%8-11d%}, %{I:%4-7d%}, %12-15R, %16-19r, %{R:cr%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mrrc, opcode: 0x0c500000, mask: 0x0ff00000, isa: IsaApplicability::Any, format: "mrrc%c\t%{I:%8-11d%}, %{I:%4-7d%}, %12-15Ru, %16-19Ru, %{R:cr%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cdp, opcode: 0x0e000000, mask: 0x0f000010, isa: IsaApplicability::Any, format: "cdp%c\t%{I:%8-11d%}, %{I:%20-23d%}, %{R:cr%12-15d%}, %{R:cr%16-19d%}, %{R:cr%0-3d%}, {%{I:%5-7d%}}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mrc, opcode: 0x0e10f010, mask: 0x0f10f010, isa: IsaApplicability::Any, format: "mrc%c\t%{I:%8-11d%}, %{I:%21-23d%}, %{R:APSR_nzcv%}, %{R:cr%16-19d%}, %{R:cr%0-3d%}, {%{I:%5-7d%}}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mrc, opcode: 0x0e100010, mask: 0x0f100010, isa: IsaApplicability::Any, format: "mrc%c\t%{I:%8-11d%}, %{I:%21-23d%}, %12-15r, %{R:cr%16-19d%}, %{R:cr%0-3d%}, {%{I:%5-7d%}}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mcr, opcode: 0x0e000010, mask: 0x0f100010, isa: IsaApplicability::Any, format: "mcr%c\t%{I:%8-11d%}, %{I:%21-23d%}, %12-15R, %{R:cr%16-19d%}, %{R:cr%0-3d%}, {%{I:%5-7d%}}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Stc, opcode: 0x0c000000, mask: 0x0e100000, isa: IsaApplicability::Any, format: "stc%22'l%c\t%{I:%8-11d%}, %{R:cr%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Ldc, opcode: 0x0c100000, mask: 0x0e100000, isa: IsaApplicability::Any, format: "ldc%22'l%c\t%{I:%8-11d%}, %{R:cr%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mrrc2, opcode: 0xfc500000, mask: 0xfff00000, isa: IsaApplicability::Any, format: "mrrc2%c\t%{I:%8-11d%}, %{I:%4-7d%}, %12-15Ru, %16-19Ru, %{R:cr%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mcrr2, opcode: 0xfc400000, mask: 0xfff00000, isa: IsaApplicability::Any, format: "mcrr2%c\t%{I:%8-11d%}, %{I:%4-7d%}, %12-15R, %16-19R, %{R:cr%0-3d%}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Ldc2, opcode: 0xfc100000, mask: 0xfe100000, isa: IsaApplicability::Any, format: "ldc2%22'l%c\t%{I:%8-11d%}, %{R:cr%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Stc2, opcode: 0xfc000000, mask: 0xfe100000, isa: IsaApplicability::Any, format: "stc2%22'l%c\t%{I:%8-11d%}, %{R:cr%12-15d%}, %A" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Cdp2, opcode: 0xfe000000, mask: 0xff000010, isa: IsaApplicability::Any, format: "cdp2%c\t%{I:%8-11d%}, %{I:%20-23d%}, %{R:cr%12-15d%}, %{R:cr%16-19d%}, %{R:cr%0-3d%}, {%{I:%5-7d%}}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mcr2, opcode: 0xfe000010, mask: 0xff100010, isa: IsaApplicability::Any, format: "mcr2%c\t%{I:%8-11d%}, %{I:%21-23d%}, %12-15R, %{R:cr%16-19d%}, %{R:cr%0-3d%}, {%{I:%5-7d%}}" },
    NeonOpcodeGenerated { mnemonic: NeonMnemonicGenerated::Mrc2, opcode: 0xfe100010, mask: 0xff100010, isa: IsaApplicability::Any, format: "mrc2%c\t%{I:%8-11d%}, %{I:%21-23d%}, %12-15r, %{R:cr%16-19d%}, %{R:cr%0-3d%}, {%{I:%5-7d%}}" },
];

/// Translate a Thumb-encoded NEON word so it can be matched
/// against the table directly. Mirrors the prologue of
/// binutils' `print_insn_neon` (when the `thumb` flag is set).
///
/// Returns `Some(normalised_word)` when the input is a Thumb
/// NEON encoding the table can match, `None` if it falls
/// outside the NEON encoding space and should be tried
/// against other tables.
pub fn normalise_neon_thumb(given: u32) -> Option<u32> {
    if (given & 0xef000000) == 0xef000000 {
        // Move bit 28 to bit 24 to translate Thumb-2 to ARM.
        let bit28 = given & (1 << 28);
        let mut g = given & 0x00ffffff;
        g |= if bit28 != 0 { 0xf3000000 } else { 0xf2000000 };
        Some(g)
    } else if (given & 0xff000000) == 0xf9000000 {
        Some(given ^ (0xf9000000 ^ 0xf4000000))
    } else if (given & 0xff000000) == 0xfe000000
        || (given & 0xff000000) == 0xfc000000
    {
        // BFloat16 NEON: no top-byte transform.
        Some(given)
    } else if (given & 0xff900f5f) == 0xee800b10 {
        // vdup.
        Some(given)
    } else {
        None
    }
}

/// Match a Thumb-mode NEON / VFP / coprocessor word.
/// Returns the matched row plus the normalised word
/// (so callers can pass that to operand decoders).
pub fn match_thumb(word: u32) -> Option<(&'static NeonOpcodeGenerated, u32)> {
    // First try the NEON normalisation. If it produces a
    // Some, run only NEON rows; otherwise the word is a
    // coprocessor encoding and needs the cond-bit fix-up.
    if let Some(norm) = normalise_neon_thumb(word) {
        for row in NEON_OPCODE_TABLE_GENERATED.iter() {
            if !matches!(row.isa, IsaApplicability::Neon) {
                continue;
            }
            // For NEON rows whose mask leaves the high 4 bits
            // unspecified, the Thumb match requires those
            // bits == 0xe.
            let mut cond_mask = row.mask;
            let mut cond_value = row.opcode;
            if (cond_mask & 0xf0000000) == 0 {
                cond_mask |= 0xf0000000;
                cond_value |= 0xe0000000;
            }
            if (norm & cond_mask) == cond_value {
                return Some((row, norm));
            }
        }
    }
    // Coprocessor encodings: for Thumb the high 4 bits are
    // forced to 0xe before matching.
    let coproc_word = (word & 0x0fffffff) | 0xe0000000;
    for row in NEON_OPCODE_TABLE_GENERATED.iter() {
        if matches!(row.isa, IsaApplicability::Neon | IsaApplicability::Arm) {
            continue;
        }
        let mask = row.mask | 0xf0000000;
        let value = row.opcode | 0xe0000000;
        if (coproc_word & mask) == value {
            return Some((row, coproc_word));
        }
    }
    None
}

/// Match an ARM-mode NEON / VFP / coprocessor word.
pub fn match_arm(word: u32) -> Option<&'static NeonOpcodeGenerated> {
    for row in NEON_OPCODE_TABLE_GENERATED.iter() {
        if matches!(row.isa, IsaApplicability::Thumb) {
            continue;
        }
        if (word & row.mask) == row.opcode {
            return Some(row);
        }
    }
    None
}
