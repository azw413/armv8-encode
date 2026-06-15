//! Control-flow classification for x86, bridging `iced-x86`'s
//! `FlowControl` onto the crate-neutral [`ControlFlow`].

use crate::isa::x86::sweep::X86DecodedInstruction;
use crate::mc::{ControlFlow, InstructionInfo};
use iced_x86::FlowControl;

impl InstructionInfo for X86DecodedInstruction {
    fn address(&self) -> u64 {
        self.address
    }

    fn size(&self) -> u64 {
        self.size_bytes()
    }

    fn control_flow(&self) -> ControlFlow {
        let fallthrough = self.address.wrapping_add(self.size_bytes());
        // `near_branch_target()` resolves rel8/rel16/rel32 targets
        // against the instruction's own IP; valid only for the direct
        // branch/call flavours below.
        let target = self.instr.near_branch_target();

        match self.instr.flow_control() {
            FlowControl::Next => ControlFlow::Fall,
            FlowControl::UnconditionalBranch => ControlFlow::Jump { target },
            FlowControl::ConditionalBranch => ControlFlow::ConditionalJump {
                target,
                fallthrough,
            },
            FlowControl::Call => ControlFlow::Call {
                target,
                fallthrough,
            },
            FlowControl::IndirectBranch => ControlFlow::IndirectJump,
            FlowControl::IndirectCall => ControlFlow::IndirectCall { fallthrough },
            FlowControl::Return => ControlFlow::Return,
            // Interrupts, exceptions, and transactional-memory fences
            // are control-flow barriers for analysis purposes.
            FlowControl::Interrupt | FlowControl::Exception | FlowControl::XbeginXabortXend => {
                ControlFlow::Trap
            }
        }
    }
}
