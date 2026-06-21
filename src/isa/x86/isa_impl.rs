//! [`Isa`] implementation for x86-64 — SKELETON.
//!
//! Enough to compile as `impl Isa for X86Isa` and pin the type choices + the
//! cheap/real methods (decode, size, pcrel-branch, relocation mapping). The two
//! design-heavy pieces are marked TODO:
//!
//!  1. **Operand projection.** `decoded_operands` currently returns `&[]` — the
//!     decoded x86 operands live inside the iced `Instruction` and aren't yet
//!     projected into [`X86Operand`]. Until that lands, the generic lift can't see
//!     branch targets, so branch relocation isn't functional. The minimal need is
//!     to surface each instruction's `near_branch_target()` as
//!     `X86Operand::Branch` (and, later, RIP-relative memory as a relocatable
//!     data operand — x86's `adrp+add` analogue).
//!  2. **`encode` completeness.** Only the unconditional `jmp`/`call rel32` forms
//!     are emitted (the trampoline/stub + unconditional retargets). The `jcc`
//!     family and general re-encode are TODO; unmodified instructions are copied
//!     verbatim via [`Isa::decode`], so `encode` is only needed for the
//!     instructions gecko synthesizes.
//!
//! Design choice (see docs/x86-backend.md in gecko): always emit longest-form
//! (`rel32`) branches so there is no relaxation — every instruction has a fixed
//! size and the generic layout needs no x86 widening (`rel32` reaches ±2 GiB, so
//! appended code is always in range). Hence the widening hooks are inert.

use super::sweep::project_operands;
use super::{encode_instruction, Bitness, EncodeError, X86DecodedInstruction, X86Operand};
use crate::container::RelocationKind;
use crate::isa::{Isa, IsaEncodeOutput, PcRelKind};
use iced_x86::{Code, Decoder, DecoderOptions, Instruction, Mnemonic};

/// Marker type implementing [`Isa`] for x86-64.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct X86Isa;

/// x86 has no `adrp+add`/`movw+movt`-style fusion (RIP-relative is a single
/// instruction), so there are no macro kinds yet.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum X86MacroKind {}

/// rel32 reaches ±2 GiB; we always emit this form.
const REL32_HALF_RANGE: i64 = i32::MAX as i64;

/// Encoded size of the rel32 form we always emit for a near branch/call:
/// `jmp`/`call` = 5 (opcode + rel32), `jcc` = 6 (`0F 8x` + rel32).
fn branch_rel32_size(m: Mnemonic) -> Option<u64> {
    match m {
        Mnemonic::Jmp | Mnemonic::Call => Some(5),
        m if is_branch_mnemonic(m) => Some(6), // jcc family
        _ => None,
    }
}

/// True for the near branch/call mnemonics whose target is a `Branch` operand.
fn is_branch_mnemonic(m: Mnemonic) -> bool {
    matches!(
        m,
        Mnemonic::Jmp
            | Mnemonic::Call
            | Mnemonic::Je
            | Mnemonic::Jne
            | Mnemonic::Jb
            | Mnemonic::Jae
            | Mnemonic::Jbe
            | Mnemonic::Ja
            | Mnemonic::Js
            | Mnemonic::Jns
            | Mnemonic::Jp
            | Mnemonic::Jnp
            | Mnemonic::Jl
            | Mnemonic::Jge
            | Mnemonic::Jle
            | Mnemonic::Jg
    )
}

impl Isa for X86Isa {
    type Mnemonic = Mnemonic;
    type Register = iced_x86::Register;
    type Operand = X86Operand;
    type EncodeError = EncodeError;
    type MacroKind = X86MacroKind;
    type DecodedInstruction = X86DecodedInstruction;

    fn decoded_address(insn: &Self::DecodedInstruction) -> u64 {
        insn.address
    }

    fn decoded_mnemonic(insn: &Self::DecodedInstruction) -> Self::Mnemonic {
        insn.mnemonic()
    }

    /// Variable-length. A near branch/call is always re-emitted in the rel32 form
    /// when relocated, so the layout pass must reserve THAT size, not the original
    /// (possibly rel8) length — otherwise downstream addresses desync. Appended
    /// functions always relocate, so a branch never verbatim-copies at the wrong
    /// size. Non-branch instructions report their true decoded length.
    fn decoded_size(insn: &Self::DecodedInstruction) -> u64 {
        branch_rel32_size(insn.mnemonic()).unwrap_or_else(|| insn.size_bytes())
    }

    fn decoded_operands(insn: &Self::DecodedInstruction) -> &[Self::Operand] {
        // Branch/call targets projected at decode time; non-branch instructions
        // have none (copied verbatim). RIP-relative data refs are not surfaced
        // yet — see docs/x86-backend.md.
        &insn.operands
    }

    fn decode(address: u64, bytes: &[u8]) -> Option<Self::DecodedInstruction> {
        // 64-bit only for now (TODO: 32-bit via container bitness).
        let mut dec = Decoder::with_ip(64, bytes, address, DecoderOptions::NONE);
        if !dec.can_decode() {
            return None;
        }
        let instr = dec.decode();
        if instr.is_invalid() {
            return None;
        }
        Some(X86DecodedInstruction { address, instr, operands: project_operands(&instr) })
    }

    fn pcrel_kind(operand: &Self::Operand) -> Option<(PcRelKind, u64)> {
        match operand {
            X86Operand::Branch(addr) => Some((PcRelKind::Branch, *addr)),
        }
    }

    fn substitute_pcrel(operand: &Self::Operand, address: u64) -> Self::Operand {
        match operand {
            X86Operand::Branch(_) => X86Operand::Branch(address),
        }
    }

    fn make_pcrel_operand(_kind: PcRelKind, address: u64) -> Self::Operand {
        // x86 only produces `Branch`-style PC-relative operands (no `Page`).
        X86Operand::Branch(address)
    }

    fn pcrel_range_bytes(mnemonic: Self::Mnemonic) -> Option<i64> {
        is_branch_mnemonic(mnemonic).then_some(REL32_HALF_RANGE)
    }

    fn invert_conditional_branch(_mnemonic: Self::Mnemonic) -> Option<Self::Mnemonic> {
        // Unused: rel32 forms never go out of range, so the layout pass never
        // widens. TODO(x86): map Je<->Jne etc. if a widening path is ever added.
        None
    }

    fn widened_conditional_size() -> u64 {
        // Inert (no widening). A `jcc rel32; ` is self-sized.
        0
    }

    fn widened_conditional_range() -> i64 {
        REL32_HALF_RANGE
    }

    fn encode(
        mnemonic: Self::Mnemonic,
        operands: &[Self::Operand],
        address: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        // Skeleton: emit the synthesized near branch/call forms (rel32). The
        // unmodified body is copied verbatim via `decode`, so this only needs the
        // instructions gecko synthesizes (stub jump, retargeted branches).
        let target = operands.iter().find_map(|o| match o {
            X86Operand::Branch(a) => Some(*a),
        });
        if let Some(target) = target {
            let code = match mnemonic {
                Mnemonic::Jmp => Code::Jmp_rel32_64,
                Mnemonic::Call => Code::Call_rel32_64,
                // The jcc rel32 family (always the long form — no rel8 relaxation).
                Mnemonic::Je => Code::Je_rel32_64,
                Mnemonic::Jne => Code::Jne_rel32_64,
                Mnemonic::Jb => Code::Jb_rel32_64,
                Mnemonic::Jae => Code::Jae_rel32_64,
                Mnemonic::Jbe => Code::Jbe_rel32_64,
                Mnemonic::Ja => Code::Ja_rel32_64,
                Mnemonic::Js => Code::Js_rel32_64,
                Mnemonic::Jns => Code::Jns_rel32_64,
                Mnemonic::Jp => Code::Jp_rel32_64,
                Mnemonic::Jnp => Code::Jnp_rel32_64,
                Mnemonic::Jl => Code::Jl_rel32_64,
                Mnemonic::Jge => Code::Jge_rel32_64,
                Mnemonic::Jle => Code::Jle_rel32_64,
                Mnemonic::Jg => Code::Jg_rel32_64,
                other => {
                    return Err(EncodeError::Iced(format!(
                        "x86 encode: branch mnemonic {other:?} not yet mapped"
                    )))
                }
            };
            let instr = Instruction::with_branch(code, target)
                .map_err(|e| EncodeError::Iced(e.to_string()))?;
            let bytes = encode_instruction(&instr, address, Bitness::Bits64)?;
            return Ok(IsaEncodeOutput { bytes });
        }
        // TODO(x86): general re-encode from a full operand model.
        Err(EncodeError::Iced(format!(
            "x86 encode: {mnemonic:?} with non-branch operands not yet supported"
        )))
    }

    fn encode_widened_conditional(
        _mnemonic: Self::Mnemonic,
        _operands_template: &[Self::Operand],
        _here: u64,
        _far_target: u64,
    ) -> Result<IsaEncodeOutput, Self::EncodeError> {
        // Unreachable with the rel32-only policy (nothing widens).
        Err(EncodeError::Iced("x86 conditional widening is unused".into()))
    }

    fn relocation_kind_for(
        mnemonic: Self::Mnemonic,
        kind: PcRelKind,
    ) -> Option<RelocationKind> {
        match kind {
            PcRelKind::Branch => match mnemonic {
                Mnemonic::Call => Some(RelocationKind::X86Plt32),
                m if is_branch_mnemonic(m) => Some(RelocationKind::X86Pc32),
                _ => None,
            },
            // x86 has no page-relative operand.
            PcRelKind::Page => None,
        }
    }

    fn is_pc_relative_relocation(kind: RelocationKind) -> bool {
        matches!(
            kind,
            RelocationKind::X86Pc32 | RelocationKind::X86Plt32 | RelocationKind::X86GotPcRel
        )
    }

    fn is_lift_relevant_relocation(kind: RelocationKind) -> bool {
        Self::is_pc_relative_relocation(kind)
    }

    // fuse_macros / emit_macro / macro_source_size: defaults. instruction_source_size
    // defaults to 4 — wrong for synthesized x86 ops; gecko must set RewriteInstruction
    // .source_size explicitly (or this is overridden) once the mutation layer lands.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_reports_variable_length() {
        // ret (1 byte) and jmp rel32 (5 bytes).
        let ret = X86Isa::decode(0x1000, &[0xc3]).unwrap();
        assert_eq!(X86Isa::decoded_size(&ret), 1);
        assert_eq!(X86Isa::decoded_mnemonic(&ret), Mnemonic::Ret);

        let jmp = X86Isa::decode(0x1000, &[0xe9, 0x00, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(X86Isa::decoded_size(&jmp), 5);
        assert_eq!(X86Isa::decoded_mnemonic(&jmp), Mnemonic::Jmp);
    }

    #[test]
    fn encode_jmp_rel32() {
        // jmp 0x1000 placed at 0x10 -> E9 + (0x1000 - (0x10 + 5)).
        let out = X86Isa::encode(Mnemonic::Jmp, &[X86Operand::Branch(0x1000)], 0x10).unwrap();
        assert_eq!(out.bytes.len(), 5);
        assert_eq!(out.bytes[0], 0xE9);
        let disp = i32::from_le_bytes(out.bytes[1..5].try_into().unwrap());
        assert_eq!(disp as i64, 0x1000 - (0x10 + 5));
    }

    #[test]
    fn pcrel_branch_round_trips() {
        let op = X86Isa::make_pcrel_operand(PcRelKind::Branch, 0x4321);
        assert_eq!(X86Isa::pcrel_kind(&op), Some((PcRelKind::Branch, 0x4321)));
        assert_eq!(
            X86Isa::substitute_pcrel(&op, 0x9999),
            X86Operand::Branch(0x9999)
        );
    }
}
