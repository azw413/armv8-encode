//! Thumb control-flow classification.
//!
//! Implements [`InstructionInfo`] for [`ThumbDecodedInstruction`]
//! so the architecture-neutral CFG builder works on Thumb
//! without modification.
//!
//! ## What's recognised
//!
//! - **Direct branches**: `B`, `B<cond>` (both 16- and 32-bit
//!   forms), `BL` and `BLX` (immediate). Targets come from
//!   the decoded `BranchTarget` operand.
//! - **Compare-and-branch**: `CBZ`, `CBNZ` — conditional jumps
//!   with a register operand, encoded as 16-bit Thumb.
//! - **Indirect branches and calls**: `BX`, `BLX` (register
//!   form). `BX LR` is the canonical Thumb function return,
//!   so we classify it as `Return` if the register operand is
//!   visibly LR, and `IndirectJump` otherwise. Misclassifying
//!   `BX <other>` as a return would break the CFG; the
//!   conservative read is "it's an indirect jump unless we
//!   can see LR."
//! - **Pop with PC**: `pop {…, pc}` is also a function
//!   return idiom in Thumb. We detect it by inspecting the
//!   register-list operand for the PC bit.
//! - **Software interrupt**: `SVC`, `UDF`, `BKPT`, `HVC`,
//!   `SMC` — classified as `Trap`.
//!
//! ## What's deferred
//!
//! - **IT-block predicates**: instructions inside an IT block
//!   are conditional even if their mnemonic doesn't say so.
//!   The CFG currently models them as unconditional; a future
//!   pass can collapse an IT + body into a richer block-level
//!   construct.
//! - **Tail-call recognition**: `B <external>` is a tail call
//!   on Thumb just like AArch64. The CFG marks it as a plain
//!   `Jump`; the rewriter / analysis layer above can promote
//!   it.

use super::operand::{DecodedOperand, RegisterClass};
use super::sweep::ThumbDecodedInstruction;
use super::table::ThumbWidth;
use super::table_generated::ThumbMnemonicGenerated as M;
use crate::mc::{ControlFlow, InstructionInfo};

impl InstructionInfo for ThumbDecodedInstruction {
    fn address(&self) -> u64 {
        self.address
    }

    fn size(&self) -> u64 {
        self.size_bytes()
    }

    fn control_flow(&self) -> ControlFlow {
        let fallthrough = self.address.wrapping_add(self.size_bytes());

        match self.mnemonic {
            // Unconditional direct branch. Binutils labels
            // both unconditional and conditional 16-bit
            // branches as `B` (the condition lives in the
            // format string's `%c`). We disambiguate using
            // the row's format string.
            M::B => match direct_branch_target(&self.operands) {
                Some(target) => {
                    let format = self.row.map(|r| r.format).unwrap_or("");
                    if is_conditional_branch_format(format) {
                        ControlFlow::ConditionalJump { target, fallthrough }
                    } else {
                        ControlFlow::Jump { target }
                    }
                }
                None => ControlFlow::Fall,
            },

            // Direct call.
            M::Bl | M::Blx => match direct_branch_target(&self.operands) {
                Some(target) => ControlFlow::Call { target, fallthrough },
                None => {
                    // BLX <Rm> (register form) — indirect call.
                    ControlFlow::IndirectCall { fallthrough }
                }
            },

            // Compare-and-branch. The operand list contains a
            // register (the test) and a BranchTarget.
            M::Cbz | M::Cbnz => match direct_branch_target(&self.operands) {
                Some(target) => ControlFlow::ConditionalJump { target, fallthrough },
                None => ControlFlow::Fall,
            },

            // Indirect branch through a register. `BX LR` =
            // function return; anything else = indirect jump.
            M::Bx => {
                if branches_via_lr(&self.operands) {
                    ControlFlow::Return
                } else {
                    ControlFlow::IndirectJump
                }
            }

            // Pop is a Return when its reglist includes PC,
            // otherwise a plain instruction.
            M::Pop => {
                if pops_pc(&self.operands) {
                    ControlFlow::Return
                } else {
                    ControlFlow::Fall
                }
            }

            // LDM-variant returns. `ldm Rn!, {…, pc}` is
            // another return idiom. Same predicate as pop.
            M::Ldmia | M::Ldmdb => {
                if pops_pc(&self.operands) {
                    ControlFlow::Return
                } else {
                    ControlFlow::Fall
                }
            }

            // Trap-shaped instructions.
            M::Svc | M::Hvc | M::Smc | M::Udf | M::Bkpt => ControlFlow::Trap,

            _ => ControlFlow::Fall,
        }
    }
}

/// Find the first decoded operand carrying a PC-relative
/// branch target.
fn direct_branch_target(operands: &[DecodedOperand]) -> Option<u64> {
    operands.iter().find_map(|operand| match operand {
        DecodedOperand::BranchTarget(target) => Some(*target),
        _ => None,
    })
}

/// True iff the register operand on a Bx-shaped instruction
/// is r14 (LR).
fn branches_via_lr(operands: &[DecodedOperand]) -> bool {
    operands
        .iter()
        .find_map(|o| match o {
            DecodedOperand::Register(r) if r.class == RegisterClass::R => Some(r.index),
            _ => None,
        })
        .map_or(false, |idx| idx == 14)
}

/// True iff any register-list operand has bit 15 (PC) set.
fn pops_pc(operands: &[DecodedOperand]) -> bool {
    operands.iter().any(|o| match o {
        DecodedOperand::RegisterList(mask) => *mask & 0x8000 != 0,
        _ => false,
    })
}

/// Heuristic: a binutils format string represents a
/// conditional branch when it contains `%<bf>c` (a
/// bitfield-prefixed condition specifier). The plain `%c`
/// without a bitfield prints the *current* IT-state condition
/// for any instruction — it doesn't make the instruction
/// conditional.
fn is_conditional_branch_format(format: &str) -> bool {
    // Looking for "%<digits>-<digits>c" or "%<digit>c".
    // The simplest correct check: scan for '%' followed by a
    // digit, then walk past digits/'-' to find a `c`.
    let bytes = format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() || !bytes[i].is_ascii_digit() {
            continue;
        }
        // Skip digits and '-'.
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'c' {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::armv7::sweep::disassemble_bytes;

    #[test]
    fn classifies_unconditional_b() {
        // b . (encoded as e7fe) at address 0x100.
        let bytes = [0xfe, 0xe7];
        let insns = disassemble_bytes(0x100, &bytes).unwrap();
        assert_eq!(insns.len(), 1);
        match insns[0].control_flow() {
            ControlFlow::Jump { target } => assert_eq!(target, 0x100),
            other => panic!("expected Jump, got {other:?}"),
        }
    }

    #[test]
    fn classifies_conditional_b() {
        // beq +0 (d0fe) at 0x100.
        let bytes = [0xfe, 0xd0];
        let insns = disassemble_bytes(0x100, &bytes).unwrap();
        match insns[0].control_flow() {
            ControlFlow::ConditionalJump { target, fallthrough } => {
                assert_eq!(target, 0x100);
                assert_eq!(fallthrough, 0x102);
            }
            other => panic!("expected ConditionalJump, got {other:?}"),
        }
    }

    #[test]
    fn classifies_bl_as_call() {
        // bl _start (ff f7 fe ff) at 0x14.
        let bytes = [0xff, 0xf7, 0xfe, 0xff];
        let insns = disassemble_bytes(0x14, &bytes).unwrap();
        match insns[0].control_flow() {
            ControlFlow::Call { fallthrough, .. } => {
                assert_eq!(fallthrough, 0x18);
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn classifies_bx_lr_as_return() {
        // bx lr (70 47).
        let bytes = [0x70, 0x47];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns[0].control_flow(), ControlFlow::Return);
    }

    #[test]
    fn classifies_bx_other_as_indirect_jump() {
        // bx r3 (18 47).
        let bytes = [0x18, 0x47];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns[0].control_flow(), ControlFlow::IndirectJump);
    }

    #[test]
    fn classifies_pop_with_pc_as_return() {
        // pop {r4-r7, pc} (f0 bd).
        let bytes = [0xf0, 0xbd];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns[0].control_flow(), ControlFlow::Return);
    }

    #[test]
    fn classifies_pop_without_pc_as_fall() {
        // pop {r0, r1} (03 bc).
        let bytes = [0x03, 0xbc];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns[0].control_flow(), ControlFlow::Fall);
    }

    #[test]
    fn classifies_udf_as_trap() {
        // udf #0 (00 de).
        let bytes = [0x00, 0xde];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns[0].control_flow(), ControlFlow::Trap);
    }

    #[test]
    fn classifies_svc_as_trap() {
        // svc #0 (00 df).
        let bytes = [0x00, 0xdf];
        let insns = disassemble_bytes(0, &bytes).unwrap();
        assert_eq!(insns[0].control_flow(), ControlFlow::Trap);
    }

    #[test]
    fn end_to_end_cfg_build_on_thumb_function() {
        // Tiny function:
        //   push {r4, lr}
        //   movs r4, r0
        //   cmp r4, #0
        //   beq .skip       ; conditional jump
        //   bl _other       ; call
        //   .skip:
        //   mov r0, r4
        //   pop {r4, pc}    ; return
        let bytes: &[u8] = &[
            0x10, 0xb5,             // push {r4, lr}
            0x04, 0x46,             // mov r4, r0
            0x00, 0x2c,             // cmp r4, #0
            0x01, 0xd0,             // beq +2
            0xff, 0xf7, 0xfe, 0xff, // bl ...
            0x20, 0x46,             // mov r0, r4
            0x10, 0xbd,             // pop {r4, pc}
        ];
        let insns = disassemble_bytes(0x1000, bytes).unwrap();
        // Build a CFG via the architecture-neutral builder.
        let cfg = crate::mc::build_cfg(&insns);
        // Expect at least 2 blocks (split at the conditional
        // beq).
        assert!(
            cfg.blocks.len() >= 2,
            "expected ≥2 blocks, got {} (blocks: {:?})",
            cfg.blocks.len(),
            cfg.blocks,
        );
    }
}
