//! Linear-sweep disassembly for x86 / x86_64.
//!
//! Given a base address and a contiguous instruction-stream byte slice,
//! decode every instruction in order via `iced-x86`. Unlike AArch64
//! (fixed 4-byte words), x86 instructions are 1–15 bytes, so the sweep
//! advances by each instruction's decoded length.
//!
//! Like the AArch64 sweep, this is fail-fast: the first byte sequence
//! that doesn't decode to a valid instruction aborts the whole
//! disassembly with the offending address. Tolerating embedded data
//! (jump tables, padding) is the job of a future recursive-descent pass
//! with section + relocation context.

use iced_x86::{
    Decoder, DecoderOptions, Instruction, InstructionInfoFactory, OpAccess, OpKind, Register,
    RflagsBits,
};

/// Crate-neutral x86 operand surfaced to the rewrite layer. Minimal: only the
/// PC-relative branch/call target the layer must introspect to relocate. All
/// other operands ride inside the iced `Instruction` and are reproduced by
/// verbatim copy ([`crate::isa::Isa::decode`]). (RIP-relative *data* refs — x86's
/// `adrp+add` analogue — are not surfaced yet; see docs/x86-backend.md.)
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum X86Operand {
    /// Direct near branch/call target (absolute address).
    Branch(u64),
}

/// Project the relocatable operands out of an iced instruction: a single
/// [`X86Operand::Branch`] for a near branch/call, else none.
pub fn project_operands(instr: &Instruction) -> Vec<X86Operand> {
    for i in 0..instr.op_count() {
        if matches!(
            instr.op_kind(i),
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
        ) {
            return vec![X86Operand::Branch(instr.near_branch_target())];
        }
    }
    Vec::new()
}

/// Decode width. x86-64 decodes in 64-bit mode; i386 in 32-bit mode.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Bitness {
    Bits32,
    Bits64,
}

impl Bitness {
    /// The `u32` bitness value `iced-x86` expects.
    pub fn as_u32(self) -> u32 {
        match self {
            Bitness::Bits32 => 32,
            Bitness::Bits64 => 64,
        }
    }
}

/// A decoded x86 instruction: the raw `iced-x86` instruction plus the
/// crate-level absolute address it was decoded at.
///
/// The full `iced_x86::Instruction` is retained (it is `Copy` and
/// self-contained) so the encode / rewrite path can re-emit or mutate
/// the instruction without re-deriving operands from a lossy model.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct X86DecodedInstruction {
    /// Absolute address of the instruction.
    pub address: u64,
    /// The underlying `iced-x86` instruction. Its `ip()` matches
    /// `address` and its `len()` is the encoded byte length.
    pub instr: Instruction,
    /// Relocatable operands projected from `instr` (branch/call targets) so the
    /// rewrite layer can see them without depending on iced. Empty for
    /// instructions with no relocatable operand.
    pub operands: Vec<X86Operand>,
}

impl X86DecodedInstruction {
    /// Encoded byte length (1–15).
    pub fn size_bytes(&self) -> u64 {
        self.instr.len() as u64
    }

    /// iced mnemonic (e.g. `Jmp`, `Mov`). Coarser than `code()`.
    pub fn mnemonic(&self) -> iced_x86::Mnemonic {
        self.instr.mnemonic()
    }

    /// iced `Code` — the exact encoding form. Re-encoding uses this.
    pub fn code(&self) -> iced_x86::Code {
        self.instr.code()
    }

    /// Per-GPR read/write effects for liveness analysis, projected from iced's
    /// instruction-info so consumers don't depend on iced directly. GPR-only:
    /// every sub-register is folded to its 64-bit parent (`al`/`ax`/`eax`/`rax`
    /// → index 0, …); FP/SIMD/segment/control registers are ignored. Implicit
    /// operands are included (e.g. `mul` writes rdx:rax, `push` reads/writes rsp,
    /// string ops touch rsi/rdi/rcx).
    ///
    /// Note the [`X86RegUse::writes_full`] vs [`X86RegUse::reads`] distinction
    /// encodes the x86 partial-register rule: only a ≥32-bit write fully
    /// overwrites the 64-bit register (32-bit writes zero-extend); 8/16-bit
    /// writes merge (so the parent is also read) and conditional writes may leave
    /// the old value in place. A liveness pass should kill a register only on
    /// `writes_full`.
    pub fn register_effects(&self) -> Vec<X86RegUse> {
        let mut factory = InstructionInfoFactory::new();
        let info = factory.info(&self.instr);
        let mut out = Vec::new();
        for ur in info.used_registers() {
            let reg = ur.register();
            let Some(gpr) = gpr64_index(reg) else { continue };
            let access = ur.access();
            let definite_write = matches!(access, OpAccess::Write | OpAccess::ReadWrite);
            let read_access = matches!(
                access,
                OpAccess::Read
                    | OpAccess::CondRead
                    | OpAccess::ReadWrite
                    | OpAccess::ReadCondWrite
                    | OpAccess::CondWrite
            );
            let full = reg.size() >= 4;
            out.push(X86RegUse {
                gpr,
                // A sub-32-bit definite write merges with the old value, so the
                // parent is effectively read; a conditional write may preserve it.
                reads: read_access || (definite_write && !full),
                writes_full: definite_write && full,
            });
        }
        out
    }

    /// If this is a flag-setting `add`/`sub` of an immediate into a **32-bit** GPR
    /// (register-direct destination), return `(dest_gpr 0..15, imm32, is_sub)`,
    /// else `None`. The immediate is the 32-bit value added/subtracted (an `imm8`
    /// form is sign-extended to 32 bits). 64-bit forms are intentionally excluded
    /// (a consumer splitting the immediate stays in clean mod-2^32 arithmetic).
    /// Lets a consumer recognize the constant-obfuscation target without iced.
    pub fn add_sub_imm(&self) -> Option<(u8, u32, bool)> {
        use iced_x86::{Mnemonic, OpKind};
        let is_sub = match self.instr.mnemonic() {
            Mnemonic::Add => false,
            Mnemonic::Sub => true,
            _ => return None,
        };
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Register {
            return None;
        }
        let reg = self.instr.op0_register();
        if !reg.is_gpr32() {
            return None;
        }
        let gpr = gpr64_index(reg)?;
        let imm = match self.instr.op1_kind() {
            OpKind::Immediate8to32 => self.instr.immediate8to32() as u32,
            OpKind::Immediate32 => self.instr.immediate32(),
            _ => return None,
        };
        Some((gpr, imm, is_sub))
    }

    /// True for a near/far `ret` (function return). Lets a consumer recognize a
    /// VM terminator without naming `iced_x86`.
    pub fn is_ret(&self) -> bool {
        matches!(
            self.instr.mnemonic(),
            iced_x86::Mnemonic::Ret | iced_x86::Mnemonic::Retf
        )
    }

    /// If this is `mov r32, imm32` (register-direct 32-bit destination, immediate
    /// source), return `(dest_gpr 0..15, imm32)`, else `None`. The immediate
    /// zero-extends the 64-bit register, as x86 does. For the VM's MOV_IMM op.
    pub fn mov_imm(&self) -> Option<(u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let reg = self.instr.op0_register();
        if !reg.is_gpr32() {
            return None;
        }
        let gpr = gpr64_index(reg)?;
        if self.instr.op1_kind() != OpKind::Immediate32 {
            return None;
        }
        Some((gpr, self.instr.immediate32()))
    }

    /// True for call/syscall/interrupt instructions whose full register impact —
    /// the callee's ABI clobbers and argument reads — is NOT captured by
    /// [`Self::register_effects`] (iced reports only the direct stack/target
    /// effects). A liveness pass must treat these as a barrier (read everything,
    /// kill nothing) so it never reuses a register holding a call argument or one
    /// the callee will clobber.
    pub fn is_call_like(&self) -> bool {
        use iced_x86::Mnemonic;
        matches!(
            self.instr.mnemonic(),
            Mnemonic::Call
                | Mnemonic::Syscall
                | Mnemonic::Sysenter
                | Mnemonic::Int
                | Mnemonic::Int1
                | Mnemonic::Int3
                | Mnemonic::Into
        )
    }

    /// Arithmetic-flag read/def masks for liveness, as `(reads, defs)` over the
    /// six flags in [`flag_bits`]. `defs` is every flag the instruction defines
    /// in any way (written / cleared / set / left undefined) — i.e. flags whose
    /// prior value does not survive. Projected from iced's rflags info.
    pub fn flag_effects(&self) -> (u8, u8) {
        (
            map_rflags(self.instr.rflags_read()),
            map_rflags(self.instr.rflags_modified()),
        )
    }
}

/// How one GPR is used by an instruction (see
/// [`X86DecodedInstruction::register_effects`]). GPR-only; sub-registers folded
/// to their 64-bit parent.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct X86RegUse {
    /// 0..15 in x86-64 encoding order: rax, rcx, rdx, rbx, rsp, rbp, rsi, rdi,
    /// r8..r15.
    pub gpr: u8,
    /// The instruction reads the parent register (a true read, a merging 8/16-bit
    /// write, or a conditional write whose old value may survive).
    pub reads: bool,
    /// The instruction definitely and fully overwrites the 64-bit register (a
    /// ≥32-bit write). Only this kills liveness — partial/conditional writes don't.
    pub writes_full: bool,
}

/// Bit positions of the six tracked arithmetic flags, shared with consumers so
/// they decode [`X86DecodedInstruction::flag_effects`] consistently.
pub mod flag_bits {
    pub const CF: u8 = 1 << 0;
    pub const PF: u8 = 1 << 1;
    pub const AF: u8 = 1 << 2;
    pub const ZF: u8 = 1 << 3;
    pub const SF: u8 = 1 << 4;
    pub const OF: u8 = 1 << 5;
    /// All six tracked flags.
    pub const ALL: u8 = CF | PF | AF | ZF | SF | OF;
}

/// Fold an iced `RflagsBits` set down to the six tracked arithmetic flags.
fn map_rflags(bits: u32) -> u8 {
    let mut m = 0u8;
    if bits & RflagsBits::CF != 0 {
        m |= flag_bits::CF;
    }
    if bits & RflagsBits::PF != 0 {
        m |= flag_bits::PF;
    }
    if bits & RflagsBits::AF != 0 {
        m |= flag_bits::AF;
    }
    if bits & RflagsBits::ZF != 0 {
        m |= flag_bits::ZF;
    }
    if bits & RflagsBits::SF != 0 {
        m |= flag_bits::SF;
    }
    if bits & RflagsBits::OF != 0 {
        m |= flag_bits::OF;
    }
    m
}

/// Map an iced register to its 0..15 GPR index (folding sub-registers to the
/// 64-bit parent), or `None` for non-GPR registers.
fn gpr64_index(reg: Register) -> Option<u8> {
    if !reg.is_gpr() {
        return None;
    }
    Some(match reg.full_register() {
        Register::RAX => 0,
        Register::RCX => 1,
        Register::RDX => 2,
        Register::RBX => 3,
        Register::RSP => 4,
        Register::RBP => 5,
        Register::RSI => 6,
        Register::RDI => 7,
        Register::R8 => 8,
        Register::R9 => 9,
        Register::R10 => 10,
        Register::R11 => 11,
        Register::R12 => 12,
        Register::R13 => 13,
        Register::R14 => 14,
        Register::R15 => 15,
        _ => return None,
    })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DisassembleError {
    /// A byte sequence at `address` did not decode to a valid
    /// instruction. `bytes` carries the (up to 15) raw bytes iced
    /// consumed so the caller can point at the offending input.
    DecodeFailed { address: u64, bytes: Vec<u8> },
}

impl std::fmt::Display for DisassembleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisassembleError::DecodeFailed { address, bytes } => {
                write!(f, "x86 decode failed at {address:#x} (bytes: {bytes:02x?})")
            }
        }
    }
}

impl std::error::Error for DisassembleError {}

#[cfg(test)]
mod effects_tests {
    use super::*;

    fn decode(bytes: &[u8]) -> X86DecodedInstruction {
        disassemble_bytes(0x1000, bytes, Bitness::Bits64).unwrap().remove(0)
    }

    fn use_of(insn: &X86DecodedInstruction, gpr: u8) -> Option<X86RegUse> {
        insn.register_effects().into_iter().find(|u| u.gpr == gpr)
    }

    #[test]
    fn full_32bit_write_kills_parent() {
        // mov eax, 1  (b8 01 00 00 00): writes EAX -> fully defines RAX.
        let u = use_of(&decode(&[0xb8, 0x01, 0x00, 0x00, 0x00]), 0).unwrap();
        assert!(u.writes_full, "32-bit write should fully overwrite rax");
        assert!(!u.reads, "mov imm does not read the destination");
    }

    #[test]
    fn partial_8bit_write_does_not_kill_parent() {
        // mov al, 1  (b0 01): writes AL only -> merges, so rax is read, not killed.
        let u = use_of(&decode(&[0xb0, 0x01]), 0).unwrap();
        assert!(!u.writes_full, "8-bit write must not kill the 64-bit parent");
        assert!(u.reads, "8-bit write merges, so the parent is effectively read");
    }

    #[test]
    fn implicit_registers_are_reported() {
        // mul rcx (48 f7 e1): reads rax+rcx, writes rdx:rax (full).
        let m = decode(&[0x48, 0xf7, 0xe1]);
        assert!(use_of(&m, 0).unwrap().reads, "mul reads rax");
        assert!(use_of(&m, 0).unwrap().writes_full, "mul writes rax");
        assert!(use_of(&m, 2).unwrap().writes_full, "mul writes rdx");
        assert!(use_of(&m, 1).unwrap().reads, "mul reads rcx operand");
    }

    #[test]
    fn lea_writes_dest_reads_base_and_touches_no_flags() {
        // lea rax, [rdi + rdi*2 + 7]  (48 8d 44 7f 07): writes rax, reads rdi.
        let lea = decode(&[0x48, 0x8d, 0x44, 0x7f, 0x07]);
        assert!(use_of(&lea, 0).unwrap().writes_full, "lea writes rax");
        assert!(use_of(&lea, 7).unwrap().reads, "lea reads rdi");
        let (reads, defs) = lea.flag_effects();
        assert_eq!((reads, defs), (0, 0), "lea must not read or define any flags");
    }

    #[test]
    fn add_defines_all_six_flags_but_inc_spares_cf() {
        // add rax, rbx (48 01 d8): defines all six arithmetic flags.
        let add = decode(&[0x48, 0x01, 0xd8]);
        assert_eq!(add.flag_effects().1, flag_bits::ALL, "add defines all six flags");
        // inc rax (48 ff c0): defines OF/SF/ZF/AF/PF but NOT CF.
        let inc = decode(&[0x48, 0xff, 0xc0]);
        let defs = inc.flag_effects().1;
        assert_eq!(defs & flag_bits::CF, 0, "inc must not define CF");
        assert_eq!(defs, flag_bits::ALL & !flag_bits::CF, "inc defines the other five");
    }

    #[test]
    fn add_sub_imm_projection() {
        // add eax, 5  (83 c0 05): imm8 sign-extended -> (rax=0, 5, add).
        assert_eq!(decode(&[0x83, 0xc0, 0x05]).add_sub_imm(), Some((0, 5, false)));
        // sub ecx, 0x1234 (81 e9 34 12 00 00): imm32 -> (rcx=1, 0x1234, sub).
        assert_eq!(
            decode(&[0x81, 0xe9, 0x34, 0x12, 0x00, 0x00]).add_sub_imm(),
            Some((1, 0x1234, true)),
        );
        // add r8d, 1 (41 83 c0 01): REX.B -> (r8=8, 1, add).
        assert_eq!(decode(&[0x41, 0x83, 0xc0, 0x01]).add_sub_imm(), Some((8, 1, false)));
        // add rax, 5 (48 83 c0 05): 64-bit -> excluded.
        assert_eq!(decode(&[0x48, 0x83, 0xc0, 0x05]).add_sub_imm(), None);
        // add eax, ecx (01 c8): register source, not an immediate -> None.
        assert_eq!(decode(&[0x01, 0xc8]).add_sub_imm(), None);
    }

    #[test]
    fn conditional_branch_reads_flags() {
        // je +0 (74 00): reads ZF, defines nothing.
        let je = decode(&[0x74, 0x00]);
        let (reads, defs) = je.flag_effects();
        assert_eq!(reads & flag_bits::ZF, flag_bits::ZF, "je reads ZF");
        assert_eq!(defs, 0, "je defines no flags");
    }

    #[test]
    fn cmov_conditional_write_does_not_kill() {
        // cmove rax, rcx (48 0f 44 c1): conditional write of rax -> not a kill.
        let cmov = use_of(&decode(&[0x48, 0x0f, 0x44, 0xc1]), 0).unwrap();
        assert!(!cmov.writes_full, "conditional write must not kill the register");
        assert!(cmov.reads, "cmov's destination may keep its old value (read)");
    }
}

/// Decode every instruction in `bytes`, treated as a contiguous x86
/// instruction stream beginning at `base_address`, in the given
/// [`Bitness`].
pub fn disassemble_bytes(
    base_address: u64,
    bytes: &[u8],
    bitness: Bitness,
) -> Result<Vec<X86DecodedInstruction>, DisassembleError> {
    let mut decoder = Decoder::with_ip(bitness.as_u32(), bytes, base_address, DecoderOptions::NONE);
    let mut out = Vec::new();

    while decoder.can_decode() {
        let address = decoder.ip();
        let instr = decoder.decode();
        if instr.is_invalid() {
            // iced consumed one byte for the invalid sequence; report
            // the remaining window (clamped to a max instruction) so
            // the caller has context.
            let offset = (address - base_address) as usize;
            let end = (offset + 15).min(bytes.len());
            return Err(DisassembleError::DecodeFailed {
                address,
                bytes: bytes[offset..end].to_vec(),
            });
        }
        out.push(X86DecodedInstruction { address, instr, operands: project_operands(&instr) });
    }

    Ok(out)
}
