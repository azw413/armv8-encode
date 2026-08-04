//! AArch64 control-flow classification.
//!
//! Maps each branch-shaped mnemonic onto an architecture-neutral
//! [`ControlFlow`] value. Direct PC-relative targets are resolved against the
//! instruction's own address by the decoder; this layer just reads them out
//! of the operand list.

use crate::isa::aarch64::{Aarch64Mnemonic, DecodedInstruction, DecodedOperand};
use crate::mc::{ControlFlow, InstructionInfo};

const INSTRUCTION_BYTES: u64 = 4;

/// Half the byte range of each PC-relative encoding form. `B`/`Bl` use
/// pcrel26 → ±128 MiB, `B.cond`/`Cbz`/`Cbnz` use pcrel19 → ±1 MiB,
/// `Tbz`/`Tbnz` use pcrel14 → ±32 KiB. Returns `None` for non-branch or
/// indirect-branch mnemonics.
pub fn pcrel_range_bytes(mnemonic: Aarch64Mnemonic) -> Option<i64> {
    Some(match mnemonic {
        Aarch64Mnemonic::B | Aarch64Mnemonic::Bl => 128 * 1024 * 1024,
        Aarch64Mnemonic::Beq
        | Aarch64Mnemonic::Bne
        | Aarch64Mnemonic::Bcs
        | Aarch64Mnemonic::Bcc
        | Aarch64Mnemonic::Bmi
        | Aarch64Mnemonic::Bpl
        | Aarch64Mnemonic::Bvs
        | Aarch64Mnemonic::Bvc
        | Aarch64Mnemonic::Bhi
        | Aarch64Mnemonic::Bls
        | Aarch64Mnemonic::Bge
        | Aarch64Mnemonic::Blt
        | Aarch64Mnemonic::Bgt
        | Aarch64Mnemonic::Ble
        | Aarch64Mnemonic::Cbz
        | Aarch64Mnemonic::Cbnz => 1024 * 1024,
        Aarch64Mnemonic::Tbz | Aarch64Mnemonic::Tbnz => 32 * 1024,
        // ADR is byte-granular ±1 MiB (21-bit signed). Not a branch, but it now
        // carries a PC-relative target the layout pass must keep in range; an
        // out-of-range ADR can't be widened (no inverse) and surfaces as a loud
        // `DisplacementTooLarge` rather than a stale offset.
        Aarch64Mnemonic::Adr => 1024 * 1024,
        _ => return None,
    })
}

/// Mnemonic with the inverted condition. Used when widening an
/// out-of-range conditional branch into `<inverted> .Lskip ; b far ; .Lskip:`.
pub fn invert_conditional_branch(mnemonic: Aarch64Mnemonic) -> Option<Aarch64Mnemonic> {
    Some(match mnemonic {
        Aarch64Mnemonic::Beq => Aarch64Mnemonic::Bne,
        Aarch64Mnemonic::Bne => Aarch64Mnemonic::Beq,
        Aarch64Mnemonic::Bcs => Aarch64Mnemonic::Bcc,
        Aarch64Mnemonic::Bcc => Aarch64Mnemonic::Bcs,
        Aarch64Mnemonic::Bmi => Aarch64Mnemonic::Bpl,
        Aarch64Mnemonic::Bpl => Aarch64Mnemonic::Bmi,
        Aarch64Mnemonic::Bvs => Aarch64Mnemonic::Bvc,
        Aarch64Mnemonic::Bvc => Aarch64Mnemonic::Bvs,
        Aarch64Mnemonic::Bhi => Aarch64Mnemonic::Bls,
        Aarch64Mnemonic::Bls => Aarch64Mnemonic::Bhi,
        Aarch64Mnemonic::Bge => Aarch64Mnemonic::Blt,
        Aarch64Mnemonic::Blt => Aarch64Mnemonic::Bge,
        Aarch64Mnemonic::Bgt => Aarch64Mnemonic::Ble,
        Aarch64Mnemonic::Ble => Aarch64Mnemonic::Bgt,
        Aarch64Mnemonic::Cbz => Aarch64Mnemonic::Cbnz,
        Aarch64Mnemonic::Cbnz => Aarch64Mnemonic::Cbz,
        Aarch64Mnemonic::Tbz => Aarch64Mnemonic::Tbnz,
        Aarch64Mnemonic::Tbnz => Aarch64Mnemonic::Tbz,
        _ => return None,
    })
}

impl InstructionInfo for DecodedInstruction {
    fn address(&self) -> u64 {
        self.address
    }

    fn size(&self) -> u64 {
        INSTRUCTION_BYTES
    }

    fn control_flow(&self) -> ControlFlow {
        let fallthrough = self.address.wrapping_add(INSTRUCTION_BYTES);
        match self.mnemonic {
            // Unconditional direct branch.
            Aarch64Mnemonic::B => match direct_branch_target(&self.operands) {
                Some(target) => ControlFlow::Jump { target },
                None => ControlFlow::Fall,
            },

            // Direct call.
            Aarch64Mnemonic::Bl => match direct_branch_target(&self.operands) {
                Some(target) => ControlFlow::Call { target, fallthrough },
                None => ControlFlow::Fall,
            },

            // Conditional branches: B.cond + Cbz/Cbnz + Tbz/Tbnz.
            Aarch64Mnemonic::Beq
            | Aarch64Mnemonic::Bne
            | Aarch64Mnemonic::Bcs
            | Aarch64Mnemonic::Bcc
            | Aarch64Mnemonic::Bmi
            | Aarch64Mnemonic::Bpl
            | Aarch64Mnemonic::Bvs
            | Aarch64Mnemonic::Bvc
            | Aarch64Mnemonic::Bhi
            | Aarch64Mnemonic::Bls
            | Aarch64Mnemonic::Bge
            | Aarch64Mnemonic::Blt
            | Aarch64Mnemonic::Bgt
            | Aarch64Mnemonic::Ble
            | Aarch64Mnemonic::Cbz
            | Aarch64Mnemonic::Cbnz
            | Aarch64Mnemonic::Tbz
            | Aarch64Mnemonic::Tbnz => match direct_branch_target(&self.operands) {
                Some(target) => ControlFlow::ConditionalJump { target, fallthrough },
                None => ControlFlow::Fall,
            },

            // Indirect branches and calls.
            Aarch64Mnemonic::Br => ControlFlow::IndirectJump,
            Aarch64Mnemonic::Blr => ControlFlow::IndirectCall { fallthrough },

            // Returns. `eret` and `drps` are exception-level returns; from a
            // CFG perspective they behave like ordinary returns.
            Aarch64Mnemonic::Ret | Aarch64Mnemonic::Eret | Aarch64Mnemonic::Drps => {
                ControlFlow::Return
            }

            // Exception generators. Resumption is up to the handler.
            Aarch64Mnemonic::Svc
            | Aarch64Mnemonic::Hvc
            | Aarch64Mnemonic::Smc
            | Aarch64Mnemonic::Brk
            | Aarch64Mnemonic::Hlt
            | Aarch64Mnemonic::Dcps1
            | Aarch64Mnemonic::Dcps2
            | Aarch64Mnemonic::Dcps3 => ControlFlow::Trap,

            _ => ControlFlow::Fall,
        }
    }
}

/// Find the first decoded operand that carries a PC-relative branch target.
fn direct_branch_target(operands: &[DecodedOperand]) -> Option<u64> {
    operands.iter().find_map(|operand| match operand {
        DecodedOperand::BranchTarget(target) => Some(*target),
        _ => None,
    })
}
