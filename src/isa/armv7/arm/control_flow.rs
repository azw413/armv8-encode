//! ARM-mode control-flow classification.
//!
//! Implements [`InstructionInfo`] for [`ArmDecodedInstruction`].
//! Recognises:
//!
//! - **Direct branches**: `B` (always or conditional).
//! - **Direct calls**: `BL`, `BLX` (immediate form).
//! - **Indirect**: `BX <Rm>` (return when Rm=lr, indirect
//!   jump otherwise), `BLX <Rm>` (indirect call).
//! - **Pop-as-return**: `LDM Rn!, {…, pc}` and the
//!   single-register form `LDR pc, …` are common return
//!   idioms in ARM mode (especially in PLT stubs and
//!   compiler-generated epilogues).
//! - **Trap-shaped**: `SVC`, `BKPT`, `UDF`, `HVC`, `SMC`.
//!
//! Conditional execution: every ARM instruction has a
//! 4-bit cond field. We treat unconditional B/BL as plain
//! Jump/Call and conditional B/BL as ConditionalJump /
//! call-with-fallthrough. Distinguishing them requires
//! reading bits 28..31 from the encoded word.

use super::sweep::ArmDecodedInstruction;
use super::table_generated::ArmMnemonicGenerated as M;
use crate::isa::armv7::operand::{DecodedOperand, RegisterClass};
use crate::mc::{ControlFlow, InstructionInfo};

const ARM_AL: u32 = 0xe; // condition code "always".

impl InstructionInfo for ArmDecodedInstruction {
    fn address(&self) -> u64 {
        self.address
    }

    fn size(&self) -> u64 {
        4
    }

    fn control_flow(&self) -> ControlFlow {
        let fallthrough = self.address.wrapping_add(4);
        let cond = (self.word >> 28) & 0xf;
        let is_unconditional = cond == ARM_AL || cond == 0xf;

        match self.mnemonic {
            // Binutils rolls B and BL into one row whose
            // format string prints `l` iff bit 24 is set.
            // Disambiguate here using bit 24.
            M::B => {
                let is_link = (self.word >> 24) & 0x1 == 1;
                let target = direct_branch_target(&self.operands);
                match (target, is_link, is_unconditional) {
                    (Some(target), true, _) => ControlFlow::Call { target, fallthrough },
                    (Some(target), false, true) => ControlFlow::Jump { target },
                    (Some(target), false, false) => ControlFlow::ConditionalJump { target, fallthrough },
                    (None, _, _) => ControlFlow::Fall,
                }
            }
            M::Blx => match direct_branch_target(&self.operands) {
                Some(target) => ControlFlow::Call { target, fallthrough },
                None => ControlFlow::IndirectCall { fallthrough },
            },
            M::Bx => {
                if branches_via_lr(&self.operands) {
                    ControlFlow::Return
                } else {
                    ControlFlow::IndirectJump
                }
            }
            // ldm Rn!, {…, pc} — pop-shaped return idiom.
            // ldmfd is the "full-descending" pop variant
            // GCC uses for function epilogues.
            M::Ldm | M::Ldmfd => {
                if list_contains_pc(&self.operands) {
                    ControlFlow::Return
                } else {
                    ControlFlow::Fall
                }
            }
            // ldr pc, [Rn, #imm]: a single-register load to
            // PC. The decoder has Rt as the first register
            // operand; if it's r15 this is a return / jump.
            M::Ldr => {
                if first_register_is_pc(&self.operands) {
                    ControlFlow::IndirectJump
                } else {
                    ControlFlow::Fall
                }
            }
            // mov pc, lr — classic ARM return.
            M::Mov => {
                if first_register_is_pc(&self.operands) && branches_via_lr(&self.operands) {
                    ControlFlow::Return
                } else if first_register_is_pc(&self.operands) {
                    ControlFlow::IndirectJump
                } else {
                    ControlFlow::Fall
                }
            }
            M::Svc | M::Bkpt | M::Udf | M::Hvc | M::Smc => ControlFlow::Trap,
            _ => ControlFlow::Fall,
        }
    }
}

fn direct_branch_target(operands: &[DecodedOperand]) -> Option<u64> {
    operands.iter().find_map(|o| match o {
        DecodedOperand::BranchTarget(t) => Some(*t),
        _ => None,
    })
}

fn branches_via_lr(operands: &[DecodedOperand]) -> bool {
    operands.iter().any(|o| match o {
        DecodedOperand::Register(r) if r.class == RegisterClass::R => r.index == 14,
        _ => false,
    })
}

fn first_register_is_pc(operands: &[DecodedOperand]) -> bool {
    operands.iter().find_map(|o| match o {
        DecodedOperand::Register(r) if r.class == RegisterClass::R => Some(r.index),
        _ => None,
    }) == Some(15)
}

fn list_contains_pc(operands: &[DecodedOperand]) -> bool {
    operands.iter().any(|o| match o {
        DecodedOperand::RegisterList(mask) => *mask & 0x8000 != 0,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::arm::sweep::disassemble_bytes;

    #[test]
    fn classifies_arm_b() {
        // b 0x100 from address 0x0 → ea000040 (offset 0x40
        // words ahead = 0x108 because of PC+8).
        // Actually use: at addr 0, "b ." = ea_fffffe.
        // imm24 = 0xfffffe → signed = -2 → +8-8 = 0.
        let bytes = [0xfe, 0xff, 0xff, 0xea];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns.len(), 1);
        match insns[0].control_flow() {
            ControlFlow::Jump { target } => assert_eq!(target, 0),
            other => panic!("expected Jump, got {other:?}"),
        }
    }

    #[test]
    fn classifies_arm_bx_lr() {
        // bx lr → e12fff1e.
        let bytes = [0x1e, 0xff, 0x2f, 0xe1];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns[0].control_flow(), ControlFlow::Return);
    }

    #[test]
    fn classifies_arm_bl_as_call() {
        // bl 0 from 0x0 → eb_fffffe (similar to b above).
        let bytes = [0xfe, 0xff, 0xff, 0xeb];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        match insns[0].control_flow() {
            ControlFlow::Call { fallthrough, .. } => assert_eq!(fallthrough, 4),
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn classifies_plt_stub_terminator_as_indirect_jump() {
        // ldr pc, [lr, #8]! → e5bef008.
        let bytes = [0x08, 0xf0, 0xbe, 0xe5];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        // Rt of this load is pc (r15) → IndirectJump.
        assert_eq!(insns[0].control_flow(), ControlFlow::IndirectJump);
    }
}
