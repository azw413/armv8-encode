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

use iced_x86::{Decoder, DecoderOptions, Instruction, OpKind};

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
