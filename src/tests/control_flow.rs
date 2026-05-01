//! Classifier tests: each `ControlFlow` shape against synthetic instruction
//! words, plus a sweep over the branch fixture asserting that every entry is
//! a terminator.

use super::common::*;
use crate::isa::aarch64;
use crate::mc::{ControlFlow, InstructionInfo};

/// Decode a word at a given address and classify it. Panics if the word
/// doesn't decode — these tests use known-good encodings from the fixtures.
fn classify(address: u64, word: u32) -> ControlFlow {
    aarch64::decode_instruction(address, word)
        .unwrap_or_else(|err| panic!("decode failed for {word:#010x}: {err:?}"))
        .control_flow()
}

#[test]
fn unconditional_direct_branch_is_jump() {
    // `b 0x4c` at address 0
    assert_eq!(classify(0, 0x14000013), ControlFlow::Jump { target: 0x4c });
}

#[test]
fn direct_call_is_call_with_fallthrough() {
    // `bl 0x4c` at address 4 — return address is 8.
    assert_eq!(
        classify(4, 0x94000012),
        ControlFlow::Call {
            target: 0x4c,
            fallthrough: 8,
        }
    );
}

#[test]
fn b_cond_is_conditional_jump() {
    // `b.eq 0x4c` at address 8.
    assert_eq!(
        classify(8, 0x54000220),
        ControlFlow::ConditionalJump {
            target: 0x4c,
            fallthrough: 0xc,
        }
    );
    // `b.ne 0x4c` at address 0xc.
    assert_eq!(
        classify(0xc, 0x54000201),
        ControlFlow::ConditionalJump {
            target: 0x4c,
            fallthrough: 0x10,
        }
    );
}

#[test]
fn cbz_and_cbnz_are_conditional_jumps() {
    // `cbz w0, 0x4c` at 0x10
    assert_eq!(
        classify(0x10, 0x340001e0),
        ControlFlow::ConditionalJump {
            target: 0x4c,
            fallthrough: 0x14,
        }
    );
    // `cbnz w2, 0x4c` at 0x18
    assert_eq!(
        classify(0x18, 0x350001a2),
        ControlFlow::ConditionalJump {
            target: 0x4c,
            fallthrough: 0x1c,
        }
    );
}

#[test]
fn tbz_and_tbnz_are_conditional_jumps() {
    // `tbz w0, #3, 0x4c` at 0x20
    assert_eq!(
        classify(0x20, 0x36180160),
        ControlFlow::ConditionalJump {
            target: 0x4c,
            fallthrough: 0x24,
        }
    );
    // `tbnz w1, #4, 0x4c` at 0x24
    assert_eq!(
        classify(0x24, 0x37200141),
        ControlFlow::ConditionalJump {
            target: 0x4c,
            fallthrough: 0x28,
        }
    );
}

#[test]
fn br_is_indirect_jump() {
    // `br x2` at 0x30
    assert_eq!(classify(0x30, 0xd61f0040), ControlFlow::IndirectJump);
}

#[test]
fn blr_is_indirect_call() {
    // `blr x3` at 0x34
    assert_eq!(
        classify(0x34, 0xd63f0060),
        ControlFlow::IndirectCall { fallthrough: 0x38 }
    );
}

#[test]
fn ret_is_return() {
    // `ret x4` at 0x38
    assert_eq!(classify(0x38, 0xd65f0080), ControlFlow::Return);
    // bare `ret` at 0x4c
    assert_eq!(classify(0x4c, 0xd65f03c0), ControlFlow::Return);
}

#[test]
fn eret_and_drps_are_returns() {
    // `eret` at 0x50
    assert_eq!(classify(0x50, 0xd69f03e0), ControlFlow::Return);
    // `drps` at 0x54
    assert_eq!(classify(0x54, 0xd6bf03e0), ControlFlow::Return);
}

#[test]
fn exception_generators_are_traps() {
    // From exception.otool.txt: `svc #0x1234` at 0x4 -> 0xd4024681
    let svc = aarch64::encode_instruction(&template(
        0,
        aarch64::Aarch64Mnemonic::Svc,
        vec![aarch64::DecodedOperand::Immediate(0x1234)],
    ))
    .expect("encode svc");
    assert_eq!(classify(0, svc), ControlFlow::Trap);

    let brk = aarch64::encode_instruction(&template(
        0,
        aarch64::Aarch64Mnemonic::Brk,
        vec![aarch64::DecodedOperand::Immediate(0x4567)],
    ))
    .expect("encode brk");
    assert_eq!(classify(0, brk), ControlFlow::Trap);

    let hlt = aarch64::encode_instruction(&template(
        0,
        aarch64::Aarch64Mnemonic::Hlt,
        vec![aarch64::DecodedOperand::Immediate(0x5678)],
    ))
    .expect("encode hlt");
    assert_eq!(classify(0, hlt), ControlFlow::Trap);
}

#[test]
fn arithmetic_falls_through() {
    // `add x0, x0, #1` at 0x8 — pure data processing.
    assert_eq!(classify(0x8, 0x91000400), ControlFlow::Fall);
    // `nop`
    assert_eq!(classify(0, 0xd503201f), ControlFlow::Fall);
}

#[test]
fn csel_falls_through_despite_condition_operand() {
    // `csel x5, x6, x7, eq` at 0x3c — no PC effect.
    assert_eq!(classify(0x3c, 0x9a8700c5), ControlFlow::Fall);
}

#[test]
fn every_branch_fixture_entry_is_a_terminator() {
    let fixture = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);
    assert!(!fixture.is_empty());

    for entry in fixture {
        let cf = classify(entry.address, entry.word);
        // The branch fixture is intentionally a control-flow grab bag, but it
        // does include a few non-branch instructions (csel, cinc, ccmp). Those
        // should be Fall, everything else should be a terminator. We assert
        // that *every entry that the disassembler renders with a branch-like
        // mnemonic* is a terminator, and skip the data-processing ones.
        let is_data_proc = matches!(
            entry.mnemonic.as_str(),
            "csel" | "cinc" | "ccmp" | "ccmn" | "cset" | "cneg" | "csneg"
        );
        if is_data_proc {
            assert_eq!(
                cf,
                ControlFlow::Fall,
                "{} {} should be Fall",
                entry.mnemonic,
                entry.operands
            );
        } else {
            assert!(
                cf.is_terminator(),
                "{} {} at {:#x} should be a terminator, got {:?}",
                entry.mnemonic,
                entry.operands,
                entry.address,
                cf
            );
        }
    }
}

#[test]
fn control_flow_helpers() {
    // `Jump` is a terminator with no fallthrough.
    let j = ControlFlow::Jump { target: 0x100 };
    assert!(j.is_terminator());
    assert!(!j.has_fallthrough());
    assert_eq!(j.direct_target(), Some(0x100));

    // `ConditionalJump` is a terminator that does fall through.
    let cj = ControlFlow::ConditionalJump {
        target: 0x100,
        fallthrough: 0x104,
    };
    assert!(cj.is_terminator());
    assert!(cj.has_fallthrough());
    assert_eq!(cj.direct_target(), Some(0x100));

    // `Call` falls through (after the callee returns).
    let call = ControlFlow::Call {
        target: 0x100,
        fallthrough: 0x104,
    };
    assert!(call.is_terminator());
    assert!(call.has_fallthrough());
    assert_eq!(call.direct_target(), Some(0x100));

    // `Return` is a terminator with no fallthrough and no static target.
    assert!(ControlFlow::Return.is_terminator());
    assert!(!ControlFlow::Return.has_fallthrough());
    assert_eq!(ControlFlow::Return.direct_target(), None);

    // `Fall` is not a terminator.
    assert!(!ControlFlow::Fall.is_terminator());
    assert!(ControlFlow::Fall.has_fallthrough());
    assert_eq!(ControlFlow::Fall.direct_target(), None);

    // `IndirectJump` and `IndirectCall` carry no static target.
    assert_eq!(ControlFlow::IndirectJump.direct_target(), None);
    assert_eq!(
        ControlFlow::IndirectCall { fallthrough: 0x104 }.direct_target(),
        None
    );
}
