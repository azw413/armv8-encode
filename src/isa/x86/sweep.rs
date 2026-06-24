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

    /// If this is `lea r32, [base + index*scale + disp]` (32-bit destination),
    /// return `(rd, base, index, scale_log2, disp)` where `base`/`index` are
    /// `Some(gpr 0..15)` or `None` (absent), `scale_log2` ∈ 0..=3 (×1/2/4/8), and
    /// `disp` is the 32-bit displacement. `None` for 64-bit-dest lea, RIP-relative
    /// lea (an address load, not guest-register arithmetic), or non-lea. For the
    /// VM's LEA op — the workhorse most `-O2` arithmetic folds into.
    pub fn lea_parts(&self) -> Option<(u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind, Register};
        if self.instr.mnemonic() != Mnemonic::Lea
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let rd_reg = self.instr.op0_register();
        if !rd_reg.is_gpr32() {
            return None; // 32-bit destination only (Phase 3)
        }
        let rd = gpr64_index(rd_reg)?;
        let base = self.instr.memory_base();
        if matches!(base, Register::RIP | Register::EIP) {
            return None; // RIP-relative = address load, not guest arithmetic
        }
        let base = match base {
            Register::None => None,
            r => Some(gpr64_index(r)?),
        };
        let index = match self.instr.memory_index() {
            Register::None => None,
            r => Some(gpr64_index(r)?),
        };
        let scale_log2 = match self.instr.memory_index_scale() {
            1 => 0,
            2 => 1,
            4 => 2,
            8 => 3,
            _ => return None,
        };
        let disp = self.instr.memory_displacement64() as u32;
        Some((rd, base, index, scale_log2, disp))
    }

    /// If this is a register-to-register `op r32, r32` (both operands 32-bit GPRs),
    /// return `(op, rd, rs)` — `rd` is the destination/left, `rs` the source/right.
    /// Covers mov/add/sub/and/or/xor and 2-operand imul; `None` otherwise. For the
    /// VM's reg-reg ops. (Flag effects are irrelevant: a flag *reader* isn't in the
    /// VM's op set, so any function whose result depends on these flags fails to
    /// lower and falls back to mutation.)
    pub fn binary_rr(&self) -> Option<(X86RrOp, u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        let op = match self.instr.mnemonic() {
            Mnemonic::Mov => X86RrOp::Mov,
            Mnemonic::Add => X86RrOp::Add,
            Mnemonic::Sub => X86RrOp::Sub,
            Mnemonic::And => X86RrOp::And,
            Mnemonic::Or => X86RrOp::Or,
            Mnemonic::Xor => X86RrOp::Xor,
            Mnemonic::Imul => X86RrOp::Imul,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let rd_reg = self.instr.op0_register();
        let rs_reg = self.instr.op1_register();
        if !rd_reg.is_gpr32() || !rs_reg.is_gpr32() {
            return None;
        }
        Some((op, gpr64_index(rd_reg)?, gpr64_index(rs_reg)?))
    }

    /// Extract a memory operand's `(base, index, scale_log2, disp)` — `base`/`index`
    /// are `Some(gpr 0..15)` or `None`; `disp` is 32-bit. `None` for RIP-relative
    /// (PIE global), a segment-overridden operand (`fs:`/`gs:` — TLS / stack
    /// canary; the address isn't `base+index+disp`), or an unusual scale. Shared by
    /// load/store projections.
    fn mem_addr(&self) -> Option<(Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::Register;
        if self.instr.segment_prefix() != Register::None {
            return None; // fs:/gs: — segment base not modelled
        }
        let base = self.instr.memory_base();
        if matches!(base, Register::RIP | Register::EIP) {
            return None;
        }
        let base = match base {
            Register::None => None,
            r => Some(gpr64_index(r)?),
        };
        let index = match self.instr.memory_index() {
            Register::None => None,
            r => Some(gpr64_index(r)?),
        };
        let scale_log2 = match self.instr.memory_index_scale() {
            1 => 0,
            2 => 1,
            4 => 2,
            8 => 3,
            _ => return None,
        };
        Some((base, index, scale_log2, self.instr.memory_displacement64() as u32))
    }

    /// Like [`Self::mem_addr`] but for an `fs:`/`gs:`-overridden operand: returns
    /// `(is_gs, base, index, scale_log2, disp)`. `None` if there is no fs/gs prefix
    /// (a plain memory operand — use `mem_addr`), for RIP-relative, or an odd scale.
    /// The VM re-emits the access with the same segment prefix; since the interpreter
    /// runs on the same thread, fs/gs resolve to the same TLS base (stack canary).
    fn mem_addr_seg(&self) -> Option<(bool, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::Register;
        let is_gs = match self.instr.segment_prefix() {
            Register::FS => false,
            Register::GS => true,
            _ => return None,
        };
        let base = self.instr.memory_base();
        if matches!(base, Register::RIP | Register::EIP) {
            return None;
        }
        let base = match base {
            Register::None => None,
            r => Some(gpr64_index(r)?),
        };
        let index = match self.instr.memory_index() {
            Register::None => None,
            r => Some(gpr64_index(r)?),
        };
        let scale_log2 = match self.instr.memory_index_scale() {
            1 => 0,
            2 => 1,
            4 => 2,
            8 => 3,
            _ => return None,
        };
        Some((is_gs, base, index, scale_log2, self.instr.memory_displacement64() as u32))
    }

    /// `mov reg, fs/gs:[mem]` (segment-relative load, 32/64 — e.g. the stack-canary
    /// `mov rax, fs:0x28`) → `(is_gs, is64, rd, base, index, scale_log2, disp)`.
    /// For the VM SEG_LOAD op.
    pub fn seg_load(&self) -> Option<(bool, bool, u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let rd = self.instr.op0_register();
        let is64 = rd.is_gpr64();
        if !rd.is_gpr32() && !is64 {
            return None;
        }
        let (is_gs, base, index, scale, disp) = self.mem_addr_seg()?;
        Some((is_gs, is64, gpr64_index(rd)?, base, index, scale, disp))
    }

    /// `op reg, fs/gs:[mem]` (segment-relative arithmetic, 32/64) for add/sub/and/or/
    /// xor — e.g. the canary check `xor rax, fs:0x28` → `(kind, is_gs, is64, rd, base,
    /// index, scale_log2, disp)`, kind as in [`Self::arith_rm`]. For the VM SEG_ARITH op.
    pub fn seg_arith(&self) -> Option<(u8, bool, bool, u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        let kind = match self.instr.mnemonic() {
            Mnemonic::Add => 0,
            Mnemonic::Sub => 1,
            Mnemonic::And => 2,
            Mnemonic::Or => 3,
            Mnemonic::Xor => 4,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let rd = self.instr.op0_register();
        let is64 = rd.is_gpr64();
        if !rd.is_gpr32() && !is64 {
            return None;
        }
        let (is_gs, base, index, scale, disp) = self.mem_addr_seg()?;
        Some((kind, is_gs, is64, gpr64_index(rd)?, base, index, scale, disp))
    }

    /// `mov r32, [base+index*scale+disp]` (a 32-bit load) → `(rd, base, index,
    /// scale_log2, disp)`, else `None`. For the VM LOAD op.
    pub fn load_parts(&self) -> Option<(u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let rd = self.instr.op0_register();
        if !rd.is_gpr32() {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((gpr64_index(rd)?, base, index, scale, disp))
    }

    /// `mov [base+index*scale+disp], r32` (a 32-bit store) → `(base, index,
    /// scale_log2, disp, rs)`, else `None`. For the VM STORE op.
    pub fn store_parts(&self) -> Option<(Option<u8>, Option<u8>, u8, u32, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let rs = self.instr.op1_register();
        if !rs.is_gpr32() {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((base, index, scale, disp, gpr64_index(rs)?))
    }

    /// 64-bit `mov r64, [mem]` → `(rd, base, index, scale_log2, disp)`. For VM LOAD64.
    pub fn load_parts64(&self) -> Option<(u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let rd = self.instr.op0_register();
        if !rd.is_gpr64() {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((gpr64_index(rd)?, base, index, scale, disp))
    }

    /// 64-bit `mov [mem], r64` → `(base, index, scale_log2, disp, rs)`. For VM STORE64.
    pub fn store_parts64(&self) -> Option<(Option<u8>, Option<u8>, u8, u32, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let rs = self.instr.op1_register();
        if !rs.is_gpr64() {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((base, index, scale, disp, gpr64_index(rs)?))
    }

    /// 64-bit-dest `lea r64, [base+index*scale+disp]` → `(rd, base, index,
    /// scale_log2, disp)`; `None` for RIP-relative. For VM LEA64 (pointer math).
    pub fn lea_parts64(&self) -> Option<(u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Lea
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let rd = self.instr.op0_register();
        if !rd.is_gpr64() {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((gpr64_index(rd)?, base, index, scale, disp))
    }

    /// Register-to-register `op r64, r64` for add/sub/and/or/xor/imul (NOT mov — see
    /// [`Self::mov_rr64`]) → `(op, rd, rs)`. For the VM's 64-bit reg-reg ops.
    pub fn binary_rr64(&self) -> Option<(X86RrOp, u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        let op = match self.instr.mnemonic() {
            Mnemonic::Add => X86RrOp::Add,
            Mnemonic::Sub => X86RrOp::Sub,
            Mnemonic::And => X86RrOp::And,
            Mnemonic::Or => X86RrOp::Or,
            Mnemonic::Xor => X86RrOp::Xor,
            Mnemonic::Imul => X86RrOp::Imul,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let rd = self.instr.op0_register();
        let rs = self.instr.op1_register();
        if !rd.is_gpr64() || !rs.is_gpr64() {
            return None;
        }
        Some((op, gpr64_index(rd)?, gpr64_index(rs)?))
    }

    /// `mov r64, imm` (sign-extended imm32, or `movabs` imm64) → `(rd, imm)`. For
    /// VM MOV64_IMM.
    pub fn mov_imm64(&self) -> Option<(u8, i64)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let rd = self.instr.op0_register();
        if !rd.is_gpr64() {
            return None;
        }
        let imm = match self.instr.op1_kind() {
            OpKind::Immediate32to64 => self.instr.immediate32to64(),
            OpKind::Immediate64 => self.instr.immediate64() as i64,
            _ => return None,
        };
        Some((gpr64_index(rd)?, imm))
    }

    /// `test r, r` (32/64, any register pair) → `(is64, a, b)`. Flags-only (AND-based).
    /// For the VM TEST op. (Same-register `test` is also matched by
    /// [`Self::test_rr_self`], which the lowerer checks first.)
    pub fn test_rr(&self) -> Option<(bool, u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Test
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let a = self.instr.op0_register();
        let b = self.instr.op1_register();
        if a.is_gpr32() && b.is_gpr32() {
            Some((false, gpr64_index(a)?, gpr64_index(b)?))
        } else if a.is_gpr64() && b.is_gpr64() {
            Some((true, gpr64_index(a)?, gpr64_index(b)?))
        } else {
            None
        }
    }

    /// `test r, imm` (32/64) → `(is64, a, imm)`. For the VM TEST op (imm form).
    pub fn test_ri(&self) -> Option<(bool, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Test
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let a = self.instr.op0_register();
        let is64 = a.is_gpr64();
        if !a.is_gpr32() && !is64 {
            return None;
        }
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate8 | OpKind::Immediate8to32 | OpKind::Immediate8to64
                | OpKind::Immediate32 | OpKind::Immediate32to64
        ) {
            return None;
        }
        Some((is64, gpr64_index(a)?, self.instr.immediate(1) as u32))
    }

    /// `and`/`or`/`xor reg, imm` (32/64) → `(kind, is64, rd, imm)`, kind =
    /// 2=and,3=or,4=xor (matching [`Self::arith_rm`]). `imm` is the low 32 bits of
    /// the (sign-extended) immediate; the VM re-extends. For the VM ALU_IMM op.
    pub fn alu_imm(&self) -> Option<(u8, bool, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        let kind = match self.instr.mnemonic() {
            Mnemonic::And => 2,
            Mnemonic::Or => 3,
            Mnemonic::Xor => 4,
            _ => return None,
        };
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Register {
            return None;
        }
        let rd = self.instr.op0_register();
        let is64 = rd.is_gpr64();
        if !rd.is_gpr32() && !is64 {
            return None;
        }
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate8 | OpKind::Immediate8to32 | OpKind::Immediate8to64
                | OpKind::Immediate32 | OpKind::Immediate32to64
        ) {
            return None;
        }
        Some((kind, is64, gpr64_index(rd)?, self.instr.immediate(1) as u32))
    }

    /// `op reg, [mem]` (load-fused arithmetic) for add/sub/and/or/xor →
    /// `(kind, is64, reg, base, index, scale_log2, disp)`, where kind =
    /// 0=add,1=sub,2=and,3=or,4=xor. `reg` is the dest (= left operand), `[mem]` the
    /// right. 32/64-bit. For the VM ARITH_RM op.
    pub fn arith_rm(&self) -> Option<(u8, bool, u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        let kind = match self.instr.mnemonic() {
            Mnemonic::Add => 0,
            Mnemonic::Sub => 1,
            Mnemonic::And => 2,
            Mnemonic::Or => 3,
            Mnemonic::Xor => 4,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let reg = self.instr.op0_register();
        let is64 = reg.is_gpr64();
        if !reg.is_gpr32() && !is64 {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((kind, is64, gpr64_index(reg)?, base, index, scale, disp))
    }

    /// `op [mem], reg` (memory read-modify-write) for add/sub/and/or/xor →
    /// `(kind, is64, base, index, scale_log2, disp, reg)`. For VM ARITH_MR (reg form).
    pub fn arith_mr_reg(&self) -> Option<(u8, bool, Option<u8>, Option<u8>, u8, u32, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        let kind = match self.instr.mnemonic() {
            Mnemonic::Add => 0,
            Mnemonic::Sub => 1,
            Mnemonic::And => 2,
            Mnemonic::Or => 3,
            Mnemonic::Xor => 4,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let reg = self.instr.op1_register();
        let is64 = reg.is_gpr64();
        if !reg.is_gpr32() && !is64 {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((kind, is64, base, index, scale, disp, gpr64_index(reg)?))
    }

    /// `op [mem]:{32,64}, imm` (memory RMW with immediate) for add/sub/and/or/xor →
    /// `(kind, is64, base, index, scale_log2, disp, imm)`. For VM ARITH_MR (imm form).
    pub fn arith_mr_imm(&self) -> Option<(u8, bool, Option<u8>, Option<u8>, u8, u32, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        let kind = match self.instr.mnemonic() {
            Mnemonic::Add => 0,
            Mnemonic::Sub => 1,
            Mnemonic::And => 2,
            Mnemonic::Or => 3,
            Mnemonic::Xor => 4,
            _ => return None,
        };
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Memory {
            return None;
        }
        let is64 = match self.instr.memory_size().size() {
            4 => false,
            8 => true,
            _ => return None,
        };
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate8 | OpKind::Immediate8to32 | OpKind::Immediate8to64
                | OpKind::Immediate32 | OpKind::Immediate32to64
        ) {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((kind, is64, base, index, scale, disp, self.instr.immediate(1) as u32))
    }

    /// `shl`/`shr`/`sar reg, imm8` or `reg, cl` (32/64) → `(kind, is64, is_cl, rd,
    /// imm_count)`, kind = 0=shl,1=shr,2=sar. For VM SHIFT.
    pub fn shift_op(&self) -> Option<(u8, bool, bool, u8, u8)> {
        use iced_x86::{Mnemonic, OpKind, Register};
        let kind = match self.instr.mnemonic() {
            Mnemonic::Shl => 0,
            Mnemonic::Shr => 1,
            Mnemonic::Sar => 2,
            _ => return None,
        };
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Register {
            return None;
        }
        let rd = self.instr.op0_register();
        let is64 = rd.is_gpr64();
        if !rd.is_gpr32() && !is64 {
            return None;
        }
        let idx = gpr64_index(rd)?;
        match self.instr.op1_kind() {
            OpKind::Immediate8 => Some((kind, is64, false, idx, self.instr.immediate8())),
            OpKind::Register if self.instr.op1_register() == Register::CL => {
                Some((kind, is64, true, idx, 0))
            }
            _ => None,
        }
    }

    /// 8-bit `op r8, r8` for mov/add/sub/and/or/xor/cmp/test → `(kind, rd, rs)`,
    /// where kind = 0..7 in that order. Low-byte registers only (rejects ah/ch/dh/bh).
    /// For the VM NARROW8 op (partial-register byte ops).
    pub fn narrow8_rr(&self) -> Option<(u8, u8, u8)> {
        use iced_x86::OpKind;
        let kind = byte_alu_kind(self.instr.mnemonic())?;
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let d = self.instr.op0_register();
        let s = self.instr.op1_register();
        Some((kind, low_gpr8(d)?, low_gpr8(s)?))
    }

    /// 8-bit `op r8, imm8` (same kinds as [`Self::narrow8_rr`]) → `(kind, rd, imm)`.
    pub fn narrow8_ri(&self) -> Option<(u8, u8, u8)> {
        use iced_x86::OpKind;
        let kind = byte_alu_kind(self.instr.mnemonic())?;
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Register {
            return None;
        }
        if !matches!(self.instr.op1_kind(), OpKind::Immediate8) {
            return None;
        }
        let d = self.instr.op0_register();
        Some((kind, low_gpr8(d)?, self.instr.immediate(1) as u8))
    }

    /// 8-bit load `mov r8, [mem]` → `(rd, base, index, scale_log2, disp)`. For VM LOAD8.
    pub fn load8(&self) -> Option<(u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
            || self.instr.memory_size().size() != 1
        {
            return None;
        }
        let rd = low_gpr8(self.instr.op0_register())?;
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((rd, base, index, scale, disp))
    }

    /// 8-bit store `mov [mem], r8` → `(base, index, scale_log2, disp, rs)`. For VM STORE8.
    pub fn store8(&self) -> Option<(Option<u8>, Option<u8>, u8, u32, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.op1_kind() != OpKind::Register
            || self.instr.memory_size().size() != 1
        {
            return None;
        }
        let rs = low_gpr8(self.instr.op1_register())?;
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((base, index, scale, disp, rs))
    }

    /// 16-bit `op r16, r16` for mov/add/sub/and/or/xor/cmp/test → `(kind, rd, rs)`
    /// (kind as in [`Self::narrow8_rr`]). For the VM NARROW16 op (partial-word ops).
    pub fn narrow16_rr(&self) -> Option<(u8, u8, u8)> {
        use iced_x86::OpKind;
        let kind = byte_alu_kind(self.instr.mnemonic())?;
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let d = self.instr.op0_register();
        let s = self.instr.op1_register();
        if !d.is_gpr16() || !s.is_gpr16() {
            return None;
        }
        Some((kind, gpr64_index(d)?, gpr64_index(s)?))
    }

    /// 16-bit `op r16, imm16` (same kinds) → `(kind, rd, imm)`.
    pub fn narrow16_ri(&self) -> Option<(u8, u8, u16)> {
        use iced_x86::OpKind;
        let kind = byte_alu_kind(self.instr.mnemonic())?;
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Register {
            return None;
        }
        let d = self.instr.op0_register();
        if !d.is_gpr16() {
            return None;
        }
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate16 | OpKind::Immediate8to16
        ) {
            return None;
        }
        Some((kind, gpr64_index(d)?, self.instr.immediate(1) as u16))
    }

    /// 16-bit load `mov r16, [mem]` → `(rd, base, index, scale_log2, disp)`. For VM LOAD16.
    pub fn load16(&self) -> Option<(u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
            || self.instr.memory_size().size() != 2
            || !self.instr.op0_register().is_gpr16()
        {
            return None;
        }
        let rd = gpr64_index(self.instr.op0_register())?;
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((rd, base, index, scale, disp))
    }

    /// 16-bit store `mov [mem], r16` → `(base, index, scale_log2, disp, rs)`. For VM STORE16.
    pub fn store16(&self) -> Option<(Option<u8>, Option<u8>, u8, u32, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.op1_kind() != OpKind::Register
            || self.instr.memory_size().size() != 2
            || !self.instr.op1_register().is_gpr16()
        {
            return None;
        }
        let rs = gpr64_index(self.instr.op1_register())?;
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((base, index, scale, disp, rs))
    }

    /// `movzx`/`movsx`/`movsxd r, r` (width-changing register move) →
    /// `(signed, src_size_log2, dst64, dst, src)`. `src_size_log2`: 0=byte, 1=word,
    /// 2=dword (movsxd). `None` for a high-byte src (ah/ch/dh/bh) or odd widths.
    pub fn ext_rr(&self) -> Option<(bool, u8, bool, u8, u8)> {
        use iced_x86::{Mnemonic, OpKind, Register};
        let (signed, fixed) = match self.instr.mnemonic() {
            Mnemonic::Movzx => (false, None),
            Mnemonic::Movsx => (true, None),
            Mnemonic::Movsxd => (true, Some(2u8)),
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let dst = self.instr.op0_register();
        if !dst.is_gpr32() && !dst.is_gpr64() {
            return None;
        }
        let src = self.instr.op1_register();
        if matches!(src, Register::AH | Register::CH | Register::DH | Register::BH) {
            return None;
        }
        let size_log2 = match fixed {
            Some(s) => {
                if !src.is_gpr32() {
                    return None;
                }
                s
            }
            None if src.is_gpr8() => 0,
            None if src.is_gpr16() => 1,
            None => return None,
        };
        Some((signed, size_log2, dst.is_gpr64(), gpr64_index(dst)?, gpr64_index(src)?))
    }

    /// `movzx`/`movsx`/`movsxd r, [mem]` (width-changing load) → `(signed,
    /// src_size_log2, dst64, dst, base, index, scale_log2, disp)`. `None` for RIP or
    /// odd widths.
    pub fn ext_rm(&self) -> Option<(bool, u8, bool, u8, Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        let signed = match self.instr.mnemonic() {
            Mnemonic::Movzx => false,
            Mnemonic::Movsx | Mnemonic::Movsxd => true,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
        {
            return None;
        }
        let dst = self.instr.op0_register();
        if !dst.is_gpr32() && !dst.is_gpr64() {
            return None;
        }
        let size_log2 = match self.instr.memory_size().size() {
            1 => 0,
            2 => 1,
            4 => 2,
            _ => return None,
        };
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((signed, size_log2, dst.is_gpr64(), gpr64_index(dst)?, base, index, scale, disp))
    }

    /// `mov [mem], imm` (non-RIP) → `(size_log2, base, index, scale_log2, disp, imm)`,
    /// where `size_log2` is 0/1/2/3 for byte/word/dword/qword (e.g. `mov byte[p],0`).
    /// For VM STORE_IMM. `None` for RIP, segment, or a non-immediate source.
    pub fn store_imm_mem(&self) -> Option<(u8, Option<u8>, Option<u8>, u8, u32, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
        {
            return None;
        }
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate8 | OpKind::Immediate8to16 | OpKind::Immediate8to32
                | OpKind::Immediate8to64 | OpKind::Immediate16 | OpKind::Immediate32
                | OpKind::Immediate32to64
        ) {
            return None;
        }
        let size_log2 = match self.instr.memory_size().size() {
            1 => 0,
            2 => 1,
            4 => 2,
            8 => 3,
            _ => return None,
        };
        let imm = self.instr.immediate(1) as u32;
        let (base, index, scale, disp) = self.mem_addr()?; // None for RIP/segment
        Some((size_log2, base, index, scale, disp, imm))
    }

    /// `mov [rip+disp], imm` at a 32/64-bit width → `(is64, target_vaddr, imm)`.
    /// For VM RIP_STORE_IMM.
    pub fn rip_store_imm(&self) -> Option<(bool, u64, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || !self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate32 | OpKind::Immediate32to64 | OpKind::Immediate8
        ) {
            return None;
        }
        let is64 = match self.instr.memory_size().size() {
            4 => false,
            8 => true,
            _ => return None,
        };
        Some((is64, self.instr.memory_displacement64(), self.instr.immediate(1) as u32))
    }

    /// `cmp`/`test` of a register against a (non-RIP) memory operand →
    /// `(is_test, size_log2, mem_is_left, base, index, scale_log2, disp, reg)`, where
    /// `size_log2` is 0/1/2/3 for byte/word/dword/qword (the register's width). For VM
    /// CMP_MEM_REG. `None` for a high-byte reg or RIP.
    pub fn cmp_mem_reg(&self) -> Option<(bool, u8, bool, Option<u8>, Option<u8>, u8, u32, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        let is_test = match self.instr.mnemonic() {
            Mnemonic::Cmp => false,
            Mnemonic::Test => true,
            _ => return None,
        };
        if self.instr.op_count() != 2 {
            return None;
        }
        let (mem_left, reg) = match (self.instr.op0_kind(), self.instr.op1_kind()) {
            (OpKind::Memory, OpKind::Register) => (true, self.instr.op1_register()),
            (OpKind::Register, OpKind::Memory) => (false, self.instr.op0_register()),
            _ => return None,
        };
        let size_log2 = gpr_size_log2(reg)?;
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((is_test, size_log2, mem_left, base, index, scale, disp, gpr64_index(reg)?))
    }

    /// `lea r64, [rip+disp]` → `(rd, target_vaddr)` (the absolute address the `lea`
    /// computes). For VM RIP_LEA (global/string addresses in a PIE).
    pub fn rip_lea(&self) -> Option<(u8, u64)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Lea
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
            || !self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        let rd = self.instr.op0_register();
        if !rd.is_gpr64() {
            return None;
        }
        Some((gpr64_index(rd)?, self.instr.memory_displacement64()))
    }

    /// `mov r32/r64, [rip+disp]` → `(rd, target_vaddr, is64)`. For VM RIP_LOAD
    /// (PIE global reads + GOT loads).
    pub fn rip_load(&self) -> Option<(u8, u64, bool)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Memory
            || !self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        let rd = self.instr.op0_register();
        let is64 = rd.is_gpr64();
        if !rd.is_gpr32() && !is64 {
            return None;
        }
        Some((gpr64_index(rd)?, self.instr.memory_displacement64(), is64))
    }

    /// `mov [rip+disp], r32/r64` → `(target_vaddr, rs, is64)`. For VM RIP_STORE.
    pub fn rip_store(&self) -> Option<(u64, u8, bool)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.op1_kind() != OpKind::Register
            || !self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        let rs = self.instr.op1_register();
        let is64 = rs.is_gpr64();
        if !rs.is_gpr32() && !is64 {
            return None;
        }
        Some((self.instr.memory_displacement64(), gpr64_index(rs)?, is64))
    }

    /// `cmp`/`test` of a 32/64-bit register against an `[rip+disp]` operand →
    /// `(is_test, is64, mem_is_left, target_vaddr, reg)` — PIE compares against a
    /// global. For VM RIP_CMP_REG. `None` for 8/16-bit or a non-RIP memory operand.
    pub fn rip_cmp_reg(&self) -> Option<(bool, bool, bool, u64, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        let is_test = match self.instr.mnemonic() {
            Mnemonic::Cmp => false,
            Mnemonic::Test => true,
            _ => return None,
        };
        if self.instr.op_count() != 2 || !self.instr.is_ip_rel_memory_operand() {
            return None;
        }
        let (mem_left, reg) = match (self.instr.op0_kind(), self.instr.op1_kind()) {
            (OpKind::Memory, OpKind::Register) => (true, self.instr.op1_register()),
            (OpKind::Register, OpKind::Memory) => (false, self.instr.op0_register()),
            _ => return None,
        };
        let is64 = reg.is_gpr64();
        if !reg.is_gpr32() && !is64 {
            return None;
        }
        Some((is_test, is64, mem_left, self.instr.memory_displacement64(), gpr64_index(reg)?))
    }

    /// `cmp`/`test [rip+disp], imm` (32/64) → `(is_test, is64, target_vaddr, imm)`.
    /// For VM RIP_CMP_IMM (PIE compare of a global against a constant).
    pub fn rip_cmp_imm(&self) -> Option<(bool, bool, u64, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        let is_test = match self.instr.mnemonic() {
            Mnemonic::Cmp => false,
            Mnemonic::Test => true,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Memory
            || !self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        let is64 = match self.instr.memory_size().size() {
            4 => false,
            8 => true,
            _ => return None,
        };
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate8 | OpKind::Immediate8to16 | OpKind::Immediate8to32
                | OpKind::Immediate8to64 | OpKind::Immediate16 | OpKind::Immediate32
                | OpKind::Immediate32to64
        ) {
            return None;
        }
        Some((is_test, is64, self.instr.memory_displacement64(), self.instr.immediate(1) as u32))
    }

    /// A direct near `call` → the absolute target vaddr, else `None` (not a call,
    /// or an indirect `call reg`/`call [mem]`). Covers both intra-binary calls and
    /// `call func@plt` (the target is the PLT stub). For the VM CALL op.
    pub fn call_target(&self) -> Option<u64> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Call {
            return None;
        }
        matches!(
            self.instr.op0_kind(),
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
        )
        .then(|| self.instr.near_branch_target())
    }

    /// Indirect `call reg` (`call rax` — function pointer / C++ vtable thunk) → the
    /// GPR index, else `None`. For the VM CALL_REG op.
    pub fn call_reg(&self) -> Option<u8> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Call
            || self.instr.op_count() != 1
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let r = self.instr.op0_register();
        r.is_gpr64().then(|| gpr64_index(r)).flatten()
    }

    /// Indirect `call [base+index*scale+disp]` (non-RIP — vtable slot / fn-ptr in
    /// memory) → `(base, index, scale_log2, disp)`. For the VM CALL_MEM op. RIP is
    /// handled by [`Self::call_rip`]; segment-overridden operands are rejected.
    pub fn call_mem(&self) -> Option<(Option<u8>, Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Call
            || self.instr.op_count() != 1
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        self.mem_addr()
    }

    /// Indirect `call [rip+disp]` (a GOT / PLT-less indirect call) → the GOT-slot
    /// vaddr (`*slot` is the callee). For the VM CALL_RIP op.
    pub fn call_rip(&self) -> Option<u64> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Call
            || self.instr.op_count() != 1
            || self.instr.op0_kind() != OpKind::Memory
            || !self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        Some(self.instr.memory_displacement64())
    }

    /// Indirect tail-call `jmp [rip+disp]` (a GOT / PLT-style tail call to an external
    /// function — `jmp *func@GOTPCREL`) → the GOT-slot vaddr. The VM lowers it as an
    /// indirect call + ret. For VM CALL_RIP+RET.
    pub fn tail_jmp_rip(&self) -> Option<u64> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Jmp
            || self.instr.op_count() != 1
            || self.instr.op0_kind() != OpKind::Memory
            || !self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        Some(self.instr.memory_displacement64())
    }

    /// Indirect tail-call `jmp [base+disp]` (non-RIP, NO index — a fn-ptr or vtable-slot
    /// tail call, e.g. `jmp *(%rbx)` / `jmp *0x10(%rax)`) → `(base, scale_log2, disp)`.
    /// An INDEXED `jmp [base+idx*s]` is a switch jump table (intra-function target) and
    /// is rejected — lowering it as a tail call would be wrong. For VM CALL_MEM+RET.
    pub fn tail_jmp_mem(&self) -> Option<(Option<u8>, u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Jmp
            || self.instr.op_count() != 1
            || self.instr.op0_kind() != OpKind::Memory
            || self.instr.is_ip_rel_memory_operand()
        {
            return None;
        }
        let (base, index, scale, disp) = self.mem_addr()?;
        if index.is_some() {
            return None; // indexed -> jump table, not a tail call
        }
        Some((base, scale, disp))
    }

    /// `push r64` → the GPR index (register form only), else `None`. For the VM
    /// PUSH op (always 64-bit in x86-64).
    pub fn push_reg(&self) -> Option<u8> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Push
            || self.instr.op_count() != 1
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let r = self.instr.op0_register();
        r.is_gpr64().then(|| gpr64_index(r)).flatten()
    }

    /// `pop r64` → the GPR index (register form only), else `None`. For the VM POP op.
    pub fn pop_reg(&self) -> Option<u8> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Pop
            || self.instr.op_count() != 1
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let r = self.instr.op0_register();
        r.is_gpr64().then(|| gpr64_index(r)).flatten()
    }

    /// `mov r64, r64` (a full 64-bit register copy) → `(rd, rs)`, else `None`. For
    /// the VM MOV64_RR op (e.g. `mov rbp, rsp`).
    pub fn mov_rr64(&self) -> Option<(u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Mov
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let rd = self.instr.op0_register();
        let rs = self.instr.op1_register();
        if !rd.is_gpr64() || !rs.is_gpr64() {
            return None;
        }
        Some((gpr64_index(rd)?, gpr64_index(rs)?))
    }

    /// `add/sub r64, imm` (a 64-bit register/immediate op) → `(rd, imm, is_sub)`,
    /// else `None`. `imm` is the low 32 bits of the sign-extended immediate (the VM
    /// re-sign-extends); covers the imm8 and imm32 encodings. For the VM
    /// ADD64_IMM/SUB64_IMM ops (e.g. `sub rsp, N`).
    pub fn add_sub_imm64(&self) -> Option<(u8, u32, bool)> {
        use iced_x86::{Mnemonic, OpKind};
        let is_sub = match self.instr.mnemonic() {
            Mnemonic::Add => false,
            Mnemonic::Sub => true,
            _ => return None,
        };
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Register {
            return None;
        }
        let rd = self.instr.op0_register();
        if !rd.is_gpr64() {
            return None;
        }
        let imm = match self.instr.op1_kind() {
            OpKind::Immediate8to64 => self.instr.immediate8to64() as u32,
            OpKind::Immediate32to64 => self.instr.immediate32to64() as u32,
            _ => return None,
        };
        Some((gpr64_index(rd)?, imm, is_sub))
    }

    /// True for `leave` (= `mov rsp, rbp; pop rbp`). The VM lowers it to those two ops.
    pub fn is_leave(&self) -> bool {
        self.instr.mnemonic() == iced_x86::Mnemonic::Leave
    }

    /// `setcc r8` (a conditional byte set) → `(cc, rd)` where `cc` is the x86
    /// condition code (same encoding as [`Self::branch`]) and `rd` is the
    /// destination GPR (the low byte gets 0/1; upper bits are preserved). `None`
    /// for non-setcc, or a legacy high-byte dest (`ah/ch/dh/bh` — different merge).
    pub fn setcc_parts(&self) -> Option<(u8, u8)> {
        use iced_x86::{Mnemonic, OpKind, Register};
        let cc = match self.instr.mnemonic() {
            Mnemonic::Sete => 0x4,
            Mnemonic::Setne => 0x5,
            Mnemonic::Setb => 0x2,
            Mnemonic::Setae => 0x3,
            Mnemonic::Setbe => 0x6,
            Mnemonic::Seta => 0x7,
            Mnemonic::Sets => 0x8,
            Mnemonic::Setns => 0x9,
            Mnemonic::Setl => 0xc,
            Mnemonic::Setge => 0xd,
            Mnemonic::Setle => 0xe,
            Mnemonic::Setg => 0xf,
            _ => return None,
        };
        if self.instr.op_count() != 1 || self.instr.op0_kind() != OpKind::Register {
            return None;
        }
        let rd = self.instr.op0_register();
        if matches!(rd, Register::AH | Register::CH | Register::DH | Register::BH) || !rd.is_gpr8() {
            return None;
        }
        gpr64_index(rd).map(|i| (cc, i))
    }

    /// `cmovcc r32, r32` (a 32-bit conditional move) → `(cc, rd, rs)` where `cc`
    /// is the x86 condition code (same encoding as [`Self::branch`]). `rd = cc ? rs
    /// : rd`. `None` otherwise. For the VM CMOV op (branchless conditionals).
    pub fn cmov_parts(&self) -> Option<(u8, u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        let cc = match self.instr.mnemonic() {
            Mnemonic::Cmove => 0x4,
            Mnemonic::Cmovne => 0x5,
            Mnemonic::Cmovb => 0x2,
            Mnemonic::Cmovae => 0x3,
            Mnemonic::Cmovbe => 0x6,
            Mnemonic::Cmova => 0x7,
            Mnemonic::Cmovs => 0x8,
            Mnemonic::Cmovns => 0x9,
            Mnemonic::Cmovl => 0xc,
            Mnemonic::Cmovge => 0xd,
            Mnemonic::Cmovle => 0xe,
            Mnemonic::Cmovg => 0xf,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let rd = self.instr.op0_register();
        let rs = self.instr.op1_register();
        if !rd.is_gpr32() || !rs.is_gpr32() {
            return None;
        }
        Some((cc, gpr64_index(rd)?, gpr64_index(rs)?))
    }

    /// 64-bit `cmovcc r64, r64` → `(cc, rd, rs)`. For VM CMOV (64-bit move variant).
    pub fn cmov_parts64(&self) -> Option<(u8, u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        let cc = match self.instr.mnemonic() {
            Mnemonic::Cmove => 0x4,
            Mnemonic::Cmovne => 0x5,
            Mnemonic::Cmovb => 0x2,
            Mnemonic::Cmovae => 0x3,
            Mnemonic::Cmovbe => 0x6,
            Mnemonic::Cmova => 0x7,
            Mnemonic::Cmovs => 0x8,
            Mnemonic::Cmovns => 0x9,
            Mnemonic::Cmovl => 0xc,
            Mnemonic::Cmovge => 0xd,
            Mnemonic::Cmovle => 0xe,
            Mnemonic::Cmovg => 0xf,
            _ => return None,
        };
        if self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let rd = self.instr.op0_register();
        let rs = self.instr.op1_register();
        if !rd.is_gpr64() || !rs.is_gpr64() {
            return None;
        }
        Some((cc, gpr64_index(rd)?, gpr64_index(rs)?))
    }

    /// `cmp r32, r32` → `(left_gpr, right_gpr)`, else `None`. For the VM CMP op.
    pub fn cmp_rr(&self) -> Option<(u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Cmp
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let l = self.instr.op0_register();
        let r = self.instr.op1_register();
        if !l.is_gpr32() || !r.is_gpr32() {
            return None;
        }
        Some((gpr64_index(l)?, gpr64_index(r)?))
    }

    /// `cmp r32, imm32` → `(left_gpr, imm)`, else `None`. For the VM CMP op.
    pub fn cmp_imm(&self) -> Option<(u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Cmp
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let l = self.instr.op0_register();
        if !l.is_gpr32() {
            return None;
        }
        let imm = match self.instr.op1_kind() {
            OpKind::Immediate8to32 => self.instr.immediate8to32() as u32,
            OpKind::Immediate32 => self.instr.immediate32(),
            _ => return None,
        };
        Some((gpr64_index(l)?, imm))
    }

    /// `test r32, r32` with the SAME register (the `test eax,eax` zero/sign idiom)
    /// → that gpr. Flag-equivalent to `cmp r,0`, so the VM lowers it that way.
    pub fn test_rr_self(&self) -> Option<u8> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Test
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let l = self.instr.op0_register();
        if !l.is_gpr32() || l != self.instr.op1_register() {
            return None;
        }
        gpr64_index(l)
    }

    /// `cmp r64, r64` → `(left, right)`. For VM CMP64_RR.
    pub fn cmp_rr64(&self) -> Option<(u8, u8)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Cmp
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let l = self.instr.op0_register();
        let r = self.instr.op1_register();
        if !l.is_gpr64() || !r.is_gpr64() {
            return None;
        }
        Some((gpr64_index(l)?, gpr64_index(r)?))
    }

    /// `cmp r64, imm` → `(left, imm)` (imm = low 32 bits of the sign-extended
    /// immediate; the VM re-sign-extends). For VM CMP64_IMM.
    pub fn cmp_imm64(&self) -> Option<(u8, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Cmp
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
        {
            return None;
        }
        let l = self.instr.op0_register();
        if !l.is_gpr64() {
            return None;
        }
        let imm = match self.instr.op1_kind() {
            OpKind::Immediate8to64 => self.instr.immediate8to64() as u32,
            OpKind::Immediate32to64 => self.instr.immediate32to64() as u32,
            _ => return None,
        };
        Some((gpr64_index(l)?, imm))
    }

    /// `test r64, r64` with the SAME register (64-bit `test rax,rax` null check) →
    /// that gpr. Flag-equivalent to `cmp r,0`; the VM lowers it that way (64-bit).
    pub fn test_rr_self64(&self) -> Option<u8> {
        use iced_x86::{Mnemonic, OpKind};
        if self.instr.mnemonic() != Mnemonic::Test
            || self.instr.op_count() != 2
            || self.instr.op0_kind() != OpKind::Register
            || self.instr.op1_kind() != OpKind::Register
        {
            return None;
        }
        let l = self.instr.op0_register();
        if !l.is_gpr64() || l != self.instr.op1_register() {
            return None;
        }
        gpr64_index(l)
    }

    /// `cmp [mem], imm` / `test [mem], imm` → `(is_test, size_log2, base, index,
    /// scale_log2, disp, imm)`, where `size_log2` is 0/1/2/3 for byte/word/dword/
    /// qword. `None` for RIP-relative or a non-immediate. For VM CMP_MEM_IMM.
    pub fn cmp_mem_imm(&self) -> Option<(bool, u8, Option<u8>, Option<u8>, u8, u32, u32)> {
        use iced_x86::{Mnemonic, OpKind};
        let is_test = match self.instr.mnemonic() {
            Mnemonic::Cmp => false,
            Mnemonic::Test => true,
            _ => return None,
        };
        if self.instr.op_count() != 2 || self.instr.op0_kind() != OpKind::Memory {
            return None;
        }
        let size_log2 = match self.instr.memory_size().size() {
            1 => 0,
            2 => 1,
            4 => 2,
            8 => 3,
            _ => return None,
        };
        if !matches!(
            self.instr.op1_kind(),
            OpKind::Immediate8 | OpKind::Immediate8to16 | OpKind::Immediate8to32
                | OpKind::Immediate8to64 | OpKind::Immediate16 | OpKind::Immediate32
                | OpKind::Immediate32to64
        ) {
            return None;
        }
        let imm = self.instr.immediate(1) as u32;
        let (base, index, scale, disp) = self.mem_addr()?;
        Some((is_test, size_log2, base, index, scale, disp, imm))
    }

    /// A direct near branch: `(None, target)` for `jmp`, `(Some(cc), target)` for a
    /// conditional `jcc`, where `cc` is the x86 condition code (E=4, NE=5, B=2,
    /// AE=3, BE=6, A=7, S=8, NS=9, L=0xc, GE=0xd, LE=0xe, G=0xf). `None` for
    /// non-branches and indirect branches (no static target). `target` is absolute.
    pub fn branch(&self) -> Option<(Option<u8>, u64)> {
        use iced_x86::{Mnemonic, OpKind};
        let near = matches!(
            self.instr.op0_kind(),
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
        );
        let m = self.instr.mnemonic();
        if m == Mnemonic::Jmp {
            return near.then(|| (None, self.instr.near_branch_target()));
        }
        let cc = match m {
            Mnemonic::Je => 0x4,
            Mnemonic::Jne => 0x5,
            Mnemonic::Jb => 0x2,
            Mnemonic::Jae => 0x3,
            Mnemonic::Jbe => 0x6,
            Mnemonic::Ja => 0x7,
            Mnemonic::Js => 0x8,
            Mnemonic::Jns => 0x9,
            Mnemonic::Jl => 0xc,
            Mnemonic::Jge => 0xd,
            Mnemonic::Jle => 0xe,
            Mnemonic::Jg => 0xf,
            _ => return None,
        };
        near.then(|| (Some(cc), self.instr.near_branch_target()))
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

/// A register-to-register binary operation recognized by
/// [`X86DecodedInstruction::binary_rr`]. `Mov` writes `rd = rs`; the rest are
/// read-modify-write `rd = rd op rs`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum X86RrOp {
    Mov,
    Add,
    Sub,
    And,
    Or,
    Xor,
    Imul,
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
/// Kind code for a byte ALU mnemonic (0=mov,1=add,2=sub,3=and,4=or,5=xor,6=cmp,
/// 7=test), or `None`. Shared by the VM's NARROW8 reg-reg / reg-imm projections.
fn byte_alu_kind(m: iced_x86::Mnemonic) -> Option<u8> {
    use iced_x86::Mnemonic::*;
    Some(match m {
        Mov => 0,
        Add => 1,
        Sub => 2,
        And => 3,
        Or => 4,
        Xor => 5,
        Cmp => 6,
        Test => 7,
        _ => return None,
    })
}

/// A GPR's operand width as a log2 byte count (gpr8→0, gpr16→1, gpr32→2, gpr64→3);
/// `None` for a legacy high-byte reg (ah/ch/dh/bh) or a non-GPR. Shared by the
/// width-aware cmp/test-vs-memory projections.
fn gpr_size_log2(reg: Register) -> Option<u8> {
    if matches!(reg, Register::AH | Register::CH | Register::DH | Register::BH) {
        return None;
    }
    if reg.is_gpr8() {
        Some(0)
    } else if reg.is_gpr16() {
        Some(1)
    } else if reg.is_gpr32() {
        Some(2)
    } else if reg.is_gpr64() {
        Some(3)
    } else {
        None
    }
}

/// A LOW-byte GPR (al/bl/.../sil/dil/r8b..r15b) → its 0..15 index; `None` for a
/// legacy high-byte reg (ah/ch/dh/bh — different bit position) or a non-gpr8.
fn low_gpr8(reg: Register) -> Option<u8> {
    if matches!(reg, Register::AH | Register::CH | Register::DH | Register::BH) || !reg.is_gpr8() {
        return None;
    }
    gpr64_index(reg)
}

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
    fn cmp_64bit_and_mem_projections() {
        // cmp rax,rdx (48 39 d0) -> CMP64_RR (0,2).
        assert_eq!(decode(&[0x48, 0x39, 0xd0]).cmp_rr64(), Some((0, 2)));
        // cmp rdi,5 (48 83 ff 05) -> CMP64_IMM (7, 5).
        assert_eq!(decode(&[0x48, 0x83, 0xff, 0x05]).cmp_imm64(), Some((7, 5)));
        // test rdi,rdi (48 85 ff) -> test_rr_self64 (7).
        assert_eq!(decode(&[0x48, 0x85, 0xff]).test_rr_self64(), Some(7));
        // cmpq 42,(rdi) (48 83 3f 2a) -> CMP_MEM_IMM (cmp, size_log2=3 qword, base=7, disp 0, imm 42).
        assert_eq!(
            decode(&[0x48, 0x83, 0x3f, 0x2a]).cmp_mem_imm(),
            Some((false, 3, Some(7), None, 0, 0, 42))
        );
        // cmpl 99,(rdi) (83 3f 63) -> dword (size_log2=2).
        assert_eq!(
            decode(&[0x83, 0x3f, 0x63]).cmp_mem_imm(),
            Some((false, 2, Some(7), None, 0, 0, 99))
        );
        // cmpb 0,(rdi) (80 3f 00) -> byte (size_log2=0).
        assert_eq!(
            decode(&[0x80, 0x3f, 0x00]).cmp_mem_imm(),
            Some((false, 0, Some(7), None, 0, 0, 0))
        );
        // test byte [rdi],1 (f6 07 01) -> test, byte.
        assert_eq!(decode(&[0xf6, 0x07, 0x01]).cmp_mem_imm(), Some((true, 0, Some(7), None, 0, 0, 1)));
        // 32-bit register forms must not match the 64-bit projections.
        assert_eq!(decode(&[0x39, 0xd0]).cmp_rr64(), None); // cmp eax,edx
    }

    #[test]
    fn data_ops_64bit_projections() {
        // mov rax,[rdi] (48 8b 07) -> LOAD64 rd=0 base=7.
        assert_eq!(decode(&[0x48, 0x8b, 0x07]).load_parts64(), Some((0, Some(7), None, 0, 0)));
        // mov [rdi],rax (48 89 07) -> STORE64.
        assert_eq!(decode(&[0x48, 0x89, 0x07]).store_parts64(), Some((Some(7), None, 0, 0, 0)));
        // lea rax,[rdi+8] (48 8d 47 08) -> LEA64.
        assert_eq!(decode(&[0x48, 0x8d, 0x47, 0x08]).lea_parts64(), Some((0, Some(7), None, 0, 8)));
        // imul rax,rsi (48 0f af c6) -> binary_rr64 Imul rd=0 rs=6.
        assert_eq!(decode(&[0x48, 0x0f, 0xaf, 0xc6]).binary_rr64(), Some((X86RrOp::Imul, 0, 6)));
        // movabs rax,0x123456789 (48 b8 ..) -> MOV64_IMM.
        assert_eq!(
            decode(&[0x48, 0xb8, 0x89, 0x67, 0x45, 0x23, 0x01, 0x00, 0x00, 0x00]).mov_imm64(),
            Some((0, 0x1_2345_6789))
        );
        // mov rax,5 (48 c7 c0 05 00 00 00) -> sign-extended imm.
        assert_eq!(decode(&[0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00]).mov_imm64(), Some((0, 5)));
        // 32-bit forms must NOT match the 64-bit projections.
        assert_eq!(decode(&[0x8b, 0x07]).load_parts64(), None); // mov eax,[rdi]
        assert_eq!(decode(&[0x48, 0x89, 0xf0]).binary_rr64(), None); // mov rax,rsi (use mov_rr64)
    }

    #[test]
    fn test_projections() {
        // test edi,esi (85 f7) -> rr, 32-bit, a=7, b=6 (different regs).
        assert_eq!(decode(&[0x85, 0xf7]).test_rr(), Some((false, 7, 6)));
        // test rax,rcx (48 85 c8) -> rr, 64-bit, a=0, b=1.
        assert_eq!(decode(&[0x48, 0x85, 0xc8]).test_rr(), Some((true, 0, 1)));
        // test eax,0x40 (a9 40 ..) -> ri, 32-bit, a=0, imm=0x40.
        assert_eq!(decode(&[0xa9, 0x40, 0x00, 0x00, 0x00]).test_ri(), Some((false, 0, 0x40)));
        // cmp is not test.
        assert_eq!(decode(&[0x39, 0xc8]).test_rr(), None);
    }

    #[test]
    fn alu_imm_shift_rmw_projections() {
        // and eax,0x12345 (25 ..) -> kind and(2), 32-bit, rd=0, imm.
        assert_eq!(decode(&[0x25, 0x45, 0x23, 0x01, 0x00]).alu_imm(), Some((2, false, 0, 0x12345)));
        // or rax,5 (48 83 c8 05) -> kind or(3), 64-bit, imm 5.
        assert_eq!(decode(&[0x48, 0x83, 0xc8, 0x05]).alu_imm(), Some((3, true, 0, 5)));
        // add [rdi],esi (01 37) -> ARITH_MR reg, kind add(0), 32, base=7, reg=6.
        assert_eq!(decode(&[0x01, 0x37]).arith_mr_reg(), Some((0, false, Some(7), None, 0, 0, 6)));
        // or dword[rdi],0x80 (81 0f 80 ..) -> ARITH_MR imm, kind or(3), base=7, imm 0x80.
        assert_eq!(
            decode(&[0x81, 0x0f, 0x80, 0x00, 0x00, 0x00]).arith_mr_imm(),
            Some((3, false, Some(7), None, 0, 0, 0x80))
        );
        // shl eax,cl (d3 e0) -> SHIFT kind shl(0), 32, is_cl.
        assert_eq!(decode(&[0xd3, 0xe0]).shift_op(), Some((0, false, true, 0, 0)));
        // sar rax,5 (48 c1 f8 05) -> kind sar(2), 64, imm 5.
        assert_eq!(decode(&[0x48, 0xc1, 0xf8, 0x05]).shift_op(), Some((2, true, false, 0, 5)));
        // add reg,imm is NOT alu_imm (only and/or/xor).
        assert_eq!(decode(&[0x83, 0xc0, 0x05]).alu_imm(), None);
    }

    #[test]
    fn arith_rm_projection() {
        // add eax,[rdi] (03 07) -> kind add(0), 32-bit, reg=0, base=7.
        assert_eq!(decode(&[0x03, 0x07]).arith_rm(), Some((0, false, 0, Some(7), None, 0, 0)));
        // xor eax,[rdi] (33 07) -> kind xor(4).
        assert_eq!(decode(&[0x33, 0x07]).arith_rm(), Some((4, false, 0, Some(7), None, 0, 0)));
        // and rax,[rsi+8] (48 23 46 08) -> kind and(2), 64-bit, reg=0, base=6, disp 8.
        assert_eq!(decode(&[0x48, 0x23, 0x46, 0x08]).arith_rm(), Some((2, true, 0, Some(6), None, 0, 8)));
        // add eax,ecx (01 c8) reg-reg -> not arith_rm.
        assert_eq!(decode(&[0x01, 0xc8]).arith_rm(), None);
    }

    #[test]
    fn seg_projections() {
        // mov rax,fs:0x28 (64 48 8b 04 25 28..) -> SEG_LOAD fs, 64-bit, reg=0, disp 0x28.
        assert_eq!(
            decode(&[0x64, 0x48, 0x8b, 0x04, 0x25, 0x28, 0x00, 0x00, 0x00]).seg_load(),
            Some((false, true, 0, None, None, 0, 0x28))
        );
        // sub rax,fs:0x28 (64 48 2b 04 25 28..) -> SEG_ARITH kind=sub(1), fs, 64-bit.
        assert_eq!(
            decode(&[0x64, 0x48, 0x2b, 0x04, 0x25, 0x28, 0x00, 0x00, 0x00]).seg_arith(),
            Some((1, false, true, 0, None, None, 0, 0x28))
        );
        // mov rax,gs:0x10 (65 48 8b 04 25 10..) -> SEG_LOAD gs.
        assert_eq!(
            decode(&[0x65, 0x48, 0x8b, 0x04, 0x25, 0x10, 0x00, 0x00, 0x00]).seg_load(),
            Some((true, true, 0, None, None, 0, 0x10))
        );
        // a plain (non-segment) load is not a SEG_LOAD.
        assert_eq!(decode(&[0x48, 0x8b, 0x07]).seg_load(), None);
    }

    #[test]
    fn word_op_projections() {
        // cmp di,si (66 39 f7) -> NARROW16 kind=cmp(6), rd=7, rs=6.
        assert_eq!(decode(&[0x66, 0x39, 0xf7]).narrow16_rr(), Some((6, 7, 6)));
        // or ax,0x8000 (66 0d 00 80) -> kind=or(4), rd=0, imm=0x8000.
        assert_eq!(decode(&[0x66, 0x0d, 0x00, 0x80]).narrow16_ri(), Some((4, 0, 0x8000)));
        // mov ax,[rsi] (66 8b 06) -> LOAD16 rd=0, base=6.
        assert_eq!(decode(&[0x66, 0x8b, 0x06]).load16(), Some((0, Some(6), None, 0, 0)));
        // mov [rdi],ax (66 89 07) -> STORE16 base=7, rs=0.
        assert_eq!(decode(&[0x66, 0x89, 0x07]).store16(), Some((Some(7), None, 0, 0, 0)));
        // 32-bit cmp is not a word op.
        assert_eq!(decode(&[0x39, 0xf7]).narrow16_rr(), None);
    }

    #[test]
    fn byte_op_projections() {
        // cmp dil,sil (40 38 f7) -> NARROW8 kind=cmp(6), rd=7, rs=6.
        assert_eq!(decode(&[0x40, 0x38, 0xf7]).narrow8_rr(), Some((6, 7, 6)));
        // add al,cl (00 c8) -> kind=add(1), rd=0, rs=1.
        assert_eq!(decode(&[0x00, 0xc8]).narrow8_rr(), Some((1, 0, 1)));
        // and al,0x0f (24 0f) -> kind=and(3), rd=0, imm=0x0f.
        assert_eq!(decode(&[0x24, 0x0f]).narrow8_ri(), Some((3, 0, 0x0f)));
        // mov al,[rsi] (8a 06) -> LOAD8 rd=0, base=6.
        assert_eq!(decode(&[0x8a, 0x06]).load8(), Some((0, Some(6), None, 0, 0)));
        // mov [rdi],al (88 07) -> STORE8 base=7, rs=0.
        assert_eq!(decode(&[0x88, 0x07]).store8(), Some((Some(7), None, 0, 0, 0)));
        // high-byte reg rejected: cmp al,ah (38 e0).
        assert_eq!(decode(&[0x38, 0xe0]).narrow8_rr(), None);
        // a 32-bit op is not a byte op.
        assert_eq!(decode(&[0x01, 0xc8]).narrow8_rr(), None); // add eax,ecx
    }

    #[test]
    fn ext_projections() {
        // movsxd rdi,edi (48 63 ff) -> EXT_RR signed, dword, dst64, dst=7, src=7.
        assert_eq!(decode(&[0x48, 0x63, 0xff]).ext_rr(), Some((true, 2, true, 7, 7)));
        // movzx eax,cl (0f b6 c1) -> unsigned, byte, dst32, dst=0, src=1.
        assert_eq!(decode(&[0x0f, 0xb6, 0xc1]).ext_rr(), Some((false, 0, false, 0, 1)));
        // movzx eax,byte[rdi] (0f b6 07) -> EXT_RM unsigned, byte, dst32, dst=0, base=7.
        assert_eq!(
            decode(&[0x0f, 0xb6, 0x07]).ext_rm(),
            Some((false, 0, false, 0, Some(7), None, 0, 0))
        );
        // movsx eax,word[rdi] (0f bf 07) -> signed, word.
        assert_eq!(
            decode(&[0x0f, 0xbf, 0x07]).ext_rm(),
            Some((true, 1, false, 0, Some(7), None, 0, 0))
        );
        // a plain mov is not an ext.
        assert_eq!(decode(&[0x89, 0xc8]).ext_rr(), None);
    }

    #[test]
    fn store_imm_and_mem_cmp_projections() {
        // mov dword[rdi],5 (c7 07 05 ..) -> STORE_IMM size_log2=2 (dword), base=7, imm=5.
        assert_eq!(decode(&[0xc7, 0x07, 0x05, 0, 0, 0]).store_imm_mem(), Some((2, Some(7), None, 0, 0, 5)));
        // mov qword[rdi],0 (48 c7 07 ..) -> qword (size_log2=3).
        assert_eq!(decode(&[0x48, 0xc7, 0x07, 0, 0, 0, 0]).store_imm_mem(), Some((3, Some(7), None, 0, 0, 0)));
        // mov byte[rdi],0x41 (c6 07 41) -> byte (size_log2=0).
        assert_eq!(decode(&[0xc6, 0x07, 0x41]).store_imm_mem(), Some((0, Some(7), None, 0, 0, 0x41)));
        // mov word[rdi],0x1234 (66 c7 07 34 12) -> word (size_log2=1).
        assert_eq!(decode(&[0x66, 0xc7, 0x07, 0x34, 0x12]).store_imm_mem(), Some((1, Some(7), None, 0, 0, 0x1234)));
        // cmp [rdi],esi (39 37) -> CMP_MEM_REG mem_left, size_log2=2 (dword), base=7, reg=6.
        assert_eq!(
            decode(&[0x39, 0x37]).cmp_mem_reg(),
            Some((false, 2, true, Some(7), None, 0, 0, 6))
        );
        // cmp eax,[rdi] (3b 07) -> reg-left (mem_left=false), dword, reg=0.
        assert_eq!(
            decode(&[0x3b, 0x07]).cmp_mem_reg(),
            Some((false, 2, false, Some(7), None, 0, 0, 0))
        );
        // cmp [rdi],sil (40 38 37) -> byte (size_log2=0), mem_left, reg=6.
        assert_eq!(
            decode(&[0x40, 0x38, 0x37]).cmp_mem_reg(),
            Some((false, 0, true, Some(7), None, 0, 0, 6))
        );
        // mov [rip+0],imm (c7 05 .. dword) -> rip_store_imm (not store_imm_mem).
        assert_eq!(decode(&[0xc7, 0x05, 0, 0, 0, 0, 0x07, 0, 0, 0]).store_imm_mem(), None);
    }

    #[test]
    fn indirect_tail_jmp_projections() {
        // jmp [rdi] (ff 27) -> tail_jmp_mem base=7, no index.
        assert_eq!(decode(&[0xff, 0x27]).tail_jmp_mem(), Some((Some(7), 0, 0)));
        // jmp [rbx+0x10] (ff 63 10) -> base=3, disp 0x10 (vtable-slot tail call).
        assert_eq!(decode(&[0xff, 0x63, 0x10]).tail_jmp_mem(), Some((Some(3), 0, 0x10)));
        // jmp [rax+rcx*8] (ff 24 c8) -> INDEXED -> rejected (jump table, not a tail call).
        assert_eq!(decode(&[0xff, 0x24, 0xc8]).tail_jmp_mem(), None);
        // jmp [rip+0] (ff 25 ..) len 6 -> tail_jmp_rip target 0x1006.
        assert_eq!(decode(&[0xff, 0x25, 0, 0, 0, 0]).tail_jmp_rip(), Some(0x1006));
        // jmp rax (ff e0) -> neither (register, not memory).
        assert_eq!(decode(&[0xff, 0xe0]).tail_jmp_mem(), None);
        assert_eq!(decode(&[0xff, 0xe0]).tail_jmp_rip(), None);
    }

    #[test]
    fn indirect_call_projections() {
        // call rax (ff d0) -> CALL_REG reg=0.
        assert_eq!(decode(&[0xff, 0xd0]).call_reg(), Some(0));
        // call [rdi] (ff 17) -> CALL_MEM base=7.
        assert_eq!(decode(&[0xff, 0x17]).call_mem(), Some((Some(7), None, 0, 0)));
        // call [rax+rcx*8+0x10] (ff 54 c8 10) -> CALL_MEM base=0, index=1, scale 3, disp 0x10.
        assert_eq!(decode(&[0xff, 0x54, 0xc8, 0x10]).call_mem(), Some((Some(0), Some(1), 3, 0x10)));
        // call [rip+0] (ff 15 ..) len 6 -> CALL_RIP target 0x1006.
        assert_eq!(decode(&[0xff, 0x15, 0, 0, 0, 0]).call_rip(), Some(0x1006));
        // a direct call is none of these.
        assert_eq!(decode(&[0xe8, 0, 0, 0, 0]).call_reg(), None);
        assert_eq!(decode(&[0xe8, 0, 0, 0, 0]).call_mem(), None);
    }

    #[test]
    fn rip_cmp_projections() {
        // cmp [rip+0],edi (39 3d ..) len 6 -> mem_left, target 0x1006, reg=7.
        assert_eq!(decode(&[0x39, 0x3d, 0, 0, 0, 0]).rip_cmp_reg(), Some((false, false, true, 0x1006, 7)));
        // cmp edi,[rip+0] (3b 3d ..) -> reg_left.
        assert_eq!(decode(&[0x3b, 0x3d, 0, 0, 0, 0]).rip_cmp_reg(), Some((false, false, false, 0x1006, 7)));
        // cmp rdi,[rip+0] (48 3b 3d ..) len 7 -> 64-bit, target 0x1007.
        assert_eq!(decode(&[0x48, 0x3b, 0x3d, 0, 0, 0, 0]).rip_cmp_reg(), Some((false, true, false, 0x1007, 7)));
        // cmp qword[rip+0],5 (48 83 3d .. 05) len 8 -> imm form, is64, target 0x1008, imm 5.
        assert_eq!(decode(&[0x48, 0x83, 0x3d, 0, 0, 0, 0, 0x05]).rip_cmp_imm(), Some((false, true, 0x1008, 5)));
        // a non-RIP cmp is not a rip_cmp.
        assert_eq!(decode(&[0x39, 0xc8]).rip_cmp_reg(), None);
    }

    #[test]
    fn rip_relative_projections() {
        // Decoded at base 0x1000. mov eax,[rip+0] (8b 05 ..) len 6 -> target 0x1006.
        assert_eq!(decode(&[0x8b, 0x05, 0, 0, 0, 0]).rip_load(), Some((0, 0x1006, false)));
        // mov rax,[rip+0] (48 8b 05 ..) len 7 -> target 0x1007, is64.
        assert_eq!(decode(&[0x48, 0x8b, 0x05, 0, 0, 0, 0]).rip_load(), Some((0, 0x1007, true)));
        // lea rax,[rip+0] (48 8d 05 ..) len 7 -> target 0x1007.
        assert_eq!(decode(&[0x48, 0x8d, 0x05, 0, 0, 0, 0]).rip_lea(), Some((0, 0x1007)));
        // mov [rip+0],rax (48 89 05 ..) len 7 -> target 0x1007, is64.
        assert_eq!(decode(&[0x48, 0x89, 0x05, 0, 0, 0, 0]).rip_store(), Some((0x1007, 0, true)));
        // A non-RIP load must not match rip_load.
        assert_eq!(decode(&[0x8b, 0x07]).rip_load(), None); // mov eax,[rdi]
    }

    #[test]
    fn call_target_projection() {
        // call rel32=0 (e8 00 00 00 00) at 0x1000 -> target 0x1005.
        assert_eq!(decode(&[0xe8, 0x00, 0x00, 0x00, 0x00]).call_target(), Some(0x1005));
        // call rax (ff d0): indirect -> None.
        assert_eq!(decode(&[0xff, 0xd0]).call_target(), None);
        // jmp (e9 ...): not a call.
        assert_eq!(decode(&[0xe9, 0x00, 0x00, 0x00, 0x00]).call_target(), None);
    }

    #[test]
    fn stack_and_frame_projections() {
        // push rbp (55) / pop rbp (5d): rbp=5.
        assert_eq!(decode(&[0x55]).push_reg(), Some(5));
        assert_eq!(decode(&[0x5d]).pop_reg(), Some(5));
        // mov rbp,rsp (48 89 e5): rd=rbp(5), rs=rsp(4).
        assert_eq!(decode(&[0x48, 0x89, 0xe5]).mov_rr64(), Some((5, 4)));
        // sub rsp,0x10 (48 83 ec 10): rd=rsp(4), imm=0x10, is_sub.
        assert_eq!(decode(&[0x48, 0x83, 0xec, 0x10]).add_sub_imm64(), Some((4, 0x10, true)));
        // add rsp,0x10 (48 83 c4 10): not sub.
        assert_eq!(decode(&[0x48, 0x83, 0xc4, 0x10]).add_sub_imm64(), Some((4, 0x10, false)));
        // leave (c9).
        assert!(decode(&[0xc9]).is_leave());
        // mov ebp,esp (89 e5): 32-bit -> not mov_rr64.
        assert_eq!(decode(&[0x89, 0xe5]).mov_rr64(), None);
        // segment-overridden load (mov rax, fs:0x28 = 64 48 8b 04 25 28 00 00 00) -> rejected.
        assert_eq!(decode(&[0x64, 0x48, 0x8b, 0x04, 0x25, 0x28, 0x00, 0x00, 0x00]).load_parts(), None);
    }

    #[test]
    fn setcc_projection() {
        // setne al (0f 95 c0): cc=NE(5), rd=0.
        assert_eq!(decode(&[0x0f, 0x95, 0xc0]).setcc_parts(), Some((0x5, 0)));
        // setg bl (0f 9f c3): cc=G(0xf), rd=3.
        assert_eq!(decode(&[0x0f, 0x9f, 0xc3]).setcc_parts(), Some((0xf, 3)));
        // sete dil (40 0f 94 c7): REX-required low byte of rdi -> rd=7.
        assert_eq!(decode(&[0x40, 0x0f, 0x94, 0xc7]).setcc_parts(), Some((0x4, 7)));
        // sete ah (0f 94 c4): legacy high byte -> None (different merge).
        assert_eq!(decode(&[0x0f, 0x94, 0xc4]).setcc_parts(), None);
    }

    #[test]
    fn cmov_projection() {
        // cmovge eax,esi (0f 4d c6): cc=GE(0xd), rd=0, rs=6.
        assert_eq!(decode(&[0x0f, 0x4d, 0xc6]).cmov_parts(), Some((0xd, 0, 6)));
        // cmove edx,ecx (0f 44 d1): cc=E(4), rd=2, rs=1.
        assert_eq!(decode(&[0x0f, 0x44, 0xd1]).cmov_parts(), Some((0x4, 2, 1)));
        // mov eax,esi (89 f0): not a cmov.
        assert_eq!(decode(&[0x89, 0xf0]).cmov_parts(), None);
    }

    #[test]
    fn load_store_projections() {
        // mov eax,[rdi] (8b 07): rd=0, base=7, no index, disp 0.
        assert_eq!(decode(&[0x8b, 0x07]).load_parts(), Some((0, Some(7), None, 0, 0)));
        // mov eax,[rdi+rsi*4+8] (8b 44 b7 08): rd=0, base=7, index=6, ×4, +8.
        assert_eq!(decode(&[0x8b, 0x44, 0xb7, 0x08]).load_parts(), Some((0, Some(7), Some(6), 2, 8)));
        // mov [rdi],eax (89 07): base=7, no index, disp 0, rs=0.
        assert_eq!(decode(&[0x89, 0x07]).store_parts(), Some((Some(7), None, 0, 0, 0)));
        // mov eax,[rip+0x10] (8b 05 ...): RIP-relative -> None (PIE global).
        assert_eq!(decode(&[0x8b, 0x05, 0x10, 0, 0, 0]).load_parts(), None);
        // mov eax,ecx (89 c8): register source, not a load/store.
        assert_eq!(decode(&[0x89, 0xc8]).load_parts(), None);
        assert_eq!(decode(&[0x89, 0xc8]).store_parts(), None);
    }

    #[test]
    fn cmp_and_branch_projections() {
        // cmp eax,ecx (39 c8); cmp eax,5 (83 f8 05); test eax,eax (85 c0).
        assert_eq!(decode(&[0x39, 0xc8]).cmp_rr(), Some((0, 1)));
        assert_eq!(decode(&[0x83, 0xf8, 0x05]).cmp_imm(), Some((0, 5)));
        assert_eq!(decode(&[0x85, 0xc0]).test_rr_self(), Some(0));
        // Branches decode at base 0x1000: jmp +0 -> 0x1005, je +0 -> 0x1002, jl -> 0x1002.
        assert_eq!(decode(&[0xe9, 0, 0, 0, 0]).branch(), Some((None, 0x1005)));
        assert_eq!(decode(&[0x74, 0x00]).branch(), Some((Some(0x4), 0x1002)));
        assert_eq!(decode(&[0x7c, 0x00]).branch(), Some((Some(0xc), 0x1002)));
        // Non-branches / indirect.
        assert_eq!(decode(&[0xc3]).branch(), None); // ret
        assert_eq!(decode(&[0x01, 0xc8]).cmp_rr(), None); // add, not cmp
    }

    #[test]
    fn binary_rr_projection() {
        // mov eax,ecx (89 c8): Mov rd=0 rs=1.
        assert_eq!(decode(&[0x89, 0xc8]).binary_rr(), Some((X86RrOp::Mov, 0, 1)));
        // add eax,ecx (01 c8): Add.
        assert_eq!(decode(&[0x01, 0xc8]).binary_rr(), Some((X86RrOp::Add, 0, 1)));
        // imul eax,ecx (0f af c1): Imul rd=0 rs=1.
        assert_eq!(decode(&[0x0f, 0xaf, 0xc1]).binary_rr(), Some((X86RrOp::Imul, 0, 1)));
        // xor r8d,r9d (45 31 c8): rd=8 rs=9 (REX-extended).
        assert_eq!(decode(&[0x45, 0x31, 0xc8]).binary_rr(), Some((X86RrOp::Xor, 8, 9)));
        // add eax,5 (imm source) -> not reg-reg.
        assert_eq!(decode(&[0x83, 0xc0, 0x05]).binary_rr(), None);
        // imul eax,ecx,7 (3-operand) -> not the 2-operand reg-reg form.
        assert_eq!(decode(&[0x6b, 0xc1, 0x07]).binary_rr(), None);
    }

    #[test]
    fn lea_parts_projection() {
        // lea eax,[rdi+rdi*2+7] (8d 44 7f 07): rd=0, base=rdi(7), index=rdi(7), ×2, +7.
        assert_eq!(decode(&[0x8d, 0x44, 0x7f, 0x07]).lea_parts(), Some((0, Some(7), Some(7), 1, 7)));
        // lea eax,[rdi+5] (8d 47 05): base only.
        assert_eq!(decode(&[0x8d, 0x47, 0x05]).lea_parts(), Some((0, Some(7), None, 0, 5)));
        // lea rax,[rdi+5] (48 8d 47 05): 64-bit dest -> None (Phase 3 is 32-bit).
        assert_eq!(decode(&[0x48, 0x8d, 0x47, 0x05]).lea_parts(), None);
        // lea eax,[rip+0x10] (8d 05 ...): RIP-relative address load -> None.
        assert_eq!(decode(&[0x8d, 0x05, 0x10, 0x00, 0x00, 0x00]).lea_parts(), None);
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
