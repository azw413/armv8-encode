//! Tests for basic-block discovery and CFG construction.

use super::common::*;
use crate::isa::aarch64;
use crate::isa::aarch64::{Aarch64Mnemonic, DecodedOperand};
use crate::mc::{
    build_cfg, BasicBlockId, ControlFlow, ControlFlowGraph, Edge, EdgeKind, EdgeTarget,
};

/// Encode a sequence of templates at consecutive 4-byte addresses starting at
/// `base`, then linear-sweep them back to a `Vec<DecodedInstruction>`. The
/// CFG builder consumes the result.
fn cfg_from_templates(
    base: u64,
    templates: Vec<aarch64::InstructionTemplate>,
) -> ControlFlowGraph {
    let mut bytes = Vec::with_capacity(templates.len() * 4);
    for (index, mut template) in templates.into_iter().enumerate() {
        template.address = base + (index as u64) * 4;
        let word = aarch64::encode_instruction(&template)
            .unwrap_or_else(|err| panic!("encode {:?}: {err:?}", template.mnemonic));
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    let instructions =
        aarch64::disassemble_bytes(base, &bytes).expect("synthetic stream decodes");
    build_cfg(&instructions)
}

fn nop() -> aarch64::InstructionTemplate {
    aarch64::InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Nop,
        operands: Vec::new(),
    }
}

fn b(target: u64) -> aarch64::InstructionTemplate {
    aarch64::InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::B,
        operands: vec![DecodedOperand::BranchTarget(target)],
    }
}

fn beq(target: u64) -> aarch64::InstructionTemplate {
    aarch64::InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Beq,
        operands: vec![DecodedOperand::BranchTarget(target)],
    }
}

fn bl(target: u64) -> aarch64::InstructionTemplate {
    aarch64::InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Bl,
        operands: vec![DecodedOperand::BranchTarget(target)],
    }
}

fn ret() -> aarch64::InstructionTemplate {
    // Bare `ret` is `ret x30`; the encoder requires the register operand.
    aarch64::InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Ret,
        operands: vec![DecodedOperand::Register(crate::isa::aarch64::Register {
            class: crate::isa::aarch64::RegisterClass::X,
            index: 30,
        })],
    }
}

fn br_x0() -> aarch64::InstructionTemplate {
    aarch64::InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Br,
        operands: vec![DecodedOperand::Register(crate::isa::aarch64::Register {
            class: crate::isa::aarch64::RegisterClass::X,
            index: 0,
        })],
    }
}

#[test]
fn empty_input_produces_empty_cfg() {
    let cfg = build_cfg::<aarch64::DecodedInstruction>(&[]);
    assert!(cfg.blocks.is_empty());
    assert_eq!(cfg.entry(), None);
}

#[test]
fn straight_line_code_is_one_block() {
    let cfg = cfg_from_templates(0x1000, vec![nop(), nop(), nop()]);
    assert_eq!(cfg.blocks.len(), 1);
    let block = &cfg.blocks[0];
    assert_eq!(block.start, 0x1000);
    assert_eq!(block.end, 0x100c);
    assert_eq!(block.instructions, 0..3);
    assert_eq!(block.terminator, None);
    assert!(block.successors.is_empty());
    assert_eq!(cfg.entry(), Some(BasicBlockId(0)));
}

#[test]
fn unconditional_jump_creates_two_blocks_with_jump_edge() {
    // Block A: nop; b 0x1010
    // Block B (target): nop
    // Layout: 0x1000 nop ; 0x1004 b 0x100c ; 0x1008 nop (dead) ; 0x100c nop
    let cfg = cfg_from_templates(0x1000, vec![nop(), b(0x100c), nop(), nop()]);
    assert_eq!(cfg.blocks.len(), 3);

    let entry = &cfg.blocks[0];
    assert_eq!(entry.start, 0x1000);
    assert_eq!(entry.end, 0x1008);
    assert_eq!(entry.terminator, Some(ControlFlow::Jump { target: 0x100c }));
    assert_eq!(entry.successors.len(), 1);
    assert_eq!(
        entry.successors[0],
        Edge {
            kind: EdgeKind::Jump,
            target: EdgeTarget::Block(BasicBlockId(2)),
        }
    );

    // The dead nop in the middle becomes its own block (it follows a
    // terminator, so the next-instruction-leader rule applies).
    let middle = &cfg.blocks[1];
    assert_eq!(middle.start, 0x1008);
    assert_eq!(middle.terminator, None);
}

#[test]
fn conditional_jump_emits_branch_taken_and_fallthrough_edges() {
    // 0x1000 nop
    // 0x1004 b.eq 0x100c
    // 0x1008 nop  ; fallthrough
    // 0x100c nop  ; branch target
    let cfg = cfg_from_templates(0x1000, vec![nop(), beq(0x100c), nop(), nop()]);
    assert_eq!(cfg.blocks.len(), 3);

    let head = &cfg.blocks[0];
    assert_eq!(
        head.terminator,
        Some(ControlFlow::ConditionalJump {
            target: 0x100c,
            fallthrough: 0x1008,
        })
    );
    assert_eq!(head.successors.len(), 2);
    assert_eq!(head.successors[0].kind, EdgeKind::BranchTaken);
    assert_eq!(head.successors[1].kind, EdgeKind::Fallthrough);
    assert_eq!(
        head.successors[0].target,
        EdgeTarget::Block(cfg.block_at(0x100c).unwrap())
    );
    assert_eq!(
        head.successors[1].target,
        EdgeTarget::Block(cfg.block_at(0x1008).unwrap())
    );
}

#[test]
fn return_block_has_no_successors() {
    let cfg = cfg_from_templates(0x1000, vec![nop(), ret()]);
    assert_eq!(cfg.blocks.len(), 1);
    let block = &cfg.blocks[0];
    assert_eq!(block.terminator, Some(ControlFlow::Return));
    assert!(block.successors.is_empty());
}

#[test]
fn call_emits_call_and_fallthrough_edges() {
    // 0x1000 bl 0x1008 ; calls into the next block
    // 0x1004 nop       ; fallthrough on return
    // 0x1008 ret       ; callee
    let cfg = cfg_from_templates(0x1000, vec![bl(0x1008), nop(), ret()]);
    assert_eq!(cfg.blocks.len(), 3);

    let caller = &cfg.blocks[0];
    assert_eq!(
        caller.terminator,
        Some(ControlFlow::Call {
            target: 0x1008,
            fallthrough: 0x1004,
        })
    );
    assert_eq!(caller.successors.len(), 2);
    assert_eq!(caller.successors[0].kind, EdgeKind::Call);
    assert_eq!(caller.successors[1].kind, EdgeKind::Fallthrough);
}

#[test]
fn indirect_jump_marks_indirect_target() {
    let cfg = cfg_from_templates(0x1000, vec![nop(), br_x0()]);
    assert_eq!(cfg.blocks.len(), 1);
    let block = &cfg.blocks[0];
    assert_eq!(block.terminator, Some(ControlFlow::IndirectJump));
    assert_eq!(block.successors.len(), 1);
    assert_eq!(block.successors[0].target, EdgeTarget::Indirect);
    assert_eq!(block.successors[0].kind, EdgeKind::Jump);
}

#[test]
fn branch_to_address_outside_slice_is_external() {
    // Single block, jumps to a target outside the 8-byte slice. 0x2000 is
    // well within pcrel26 range from base 0x1000 but not in our input.
    let cfg = cfg_from_templates(0x1000, vec![nop(), b(0x2000)]);
    assert_eq!(cfg.blocks.len(), 1);
    let block = &cfg.blocks[0];
    assert_eq!(block.successors[0].target, EdgeTarget::External(0x2000));
}

#[test]
fn loop_back_edge_resolves_to_block_id() {
    // 0x1000 nop          ; loop top
    // 0x1004 b.eq 0x1000  ; back-edge
    // 0x1008 ret          ; exit
    let cfg = cfg_from_templates(0x1000, vec![nop(), beq(0x1000), ret()]);
    assert_eq!(cfg.blocks.len(), 2);

    let loop_block = &cfg.blocks[0];
    assert_eq!(loop_block.start, 0x1000);
    let cj = loop_block.successors.iter().find(|edge| edge.kind == EdgeKind::BranchTaken);
    assert_eq!(
        cj.unwrap().target,
        EdgeTarget::Block(BasicBlockId(0)),
        "back-edge should target the loop-top block"
    );

    // The exit block is the predecessor target of the fallthrough edge.
    let fall = loop_block
        .successors
        .iter()
        .find(|edge| edge.kind == EdgeKind::Fallthrough);
    assert_eq!(fall.unwrap().target, EdgeTarget::Block(BasicBlockId(1)));
}

#[test]
fn branch_into_middle_splits_a_would_be_block() {
    // Without the mid-jump, instructions at 0x1000-0x1010 would be one
    // block. The conditional branch at 0x101c to 0x1008 forces a split:
    // block A is 0x1000-0x1008, block B is 0x1008-0x1010, etc.
    let cfg = cfg_from_templates(
        0x1000,
        vec![
            nop(),         // 0x1000
            nop(),         // 0x1004
            nop(),         // 0x1008  <- branch target
            nop(),         // 0x100c
            nop(),         // 0x1010
            nop(),         // 0x1014
            nop(),         // 0x1018
            beq(0x1008),   // 0x101c
            ret(),         // 0x1020
        ],
    );
    // Expected blocks: 0x1000-0x1008 (head) ; 0x1008-0x1020 (loop body w/
    // beq terminator) ; 0x1020-0x1024 (ret).
    assert_eq!(cfg.blocks.len(), 3);

    let head = &cfg.blocks[0];
    assert_eq!(head.start, 0x1000);
    assert_eq!(head.end, 0x1008);
    assert_eq!(head.terminator, None);
    // Falls off the end into the next block in source order, but since the
    // builder doesn't synthesize fallthrough edges for non-terminated blocks,
    // there's no successor here.
    assert!(head.successors.is_empty());

    let body = &cfg.blocks[1];
    assert_eq!(body.start, 0x1008);
    assert_eq!(body.end, 0x1020);
    let taken = body
        .successors
        .iter()
        .find(|edge| edge.kind == EdgeKind::BranchTaken)
        .unwrap();
    assert_eq!(taken.target, EdgeTarget::Block(BasicBlockId(1)));

    let tail = &cfg.blocks[2];
    assert_eq!(tail.start, 0x1020);
    assert_eq!(tail.terminator, Some(ControlFlow::Return));
}

#[test]
fn predecessors_of_finds_all_incoming_edges() {
    // 0x1000 b.eq 0x1008
    // 0x1004 nop          (fallthrough block)
    // 0x1008 ret          (target block)
    let cfg = cfg_from_templates(0x1000, vec![beq(0x1008), nop(), ret()]);

    let target_block = cfg.block_at(0x1008).expect("0x1008 should be a block");
    let preds = cfg.predecessors_of(target_block);
    assert_eq!(preds, vec![BasicBlockId(0)]);

    // The fallthrough block is also reached only from block 0.
    let fall_block = cfg.block_at(0x1004).expect("0x1004 should be a block");
    let preds = cfg.predecessors_of(fall_block);
    assert_eq!(preds, vec![BasicBlockId(0)]);
}

#[test]
fn branch_fixture_produces_expected_cfg() {
    // Sanity-check: feed the entire branch fixture through the pipeline
    // (sweep → CFG) and assert structural invariants. We don't pin the exact
    // shape because any future fixture change would be brittle.
    let entries = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);
    assert!(!entries.is_empty());
    let mut bytes = Vec::with_capacity(entries.len() * 4);
    for entry in &entries {
        bytes.extend_from_slice(&entry.word.to_le_bytes());
    }
    let base = entries[0].address;
    let instructions = aarch64::disassemble_bytes(base, &bytes).expect("fixture decodes");
    let cfg = build_cfg(&instructions);

    // Every direct branch in the fixture targets 0x4c, and 0x4c is in the
    // fixture (it's the bare `ret`). So every direct-target edge should
    // resolve to a Block, never External.
    let mut direct_edges = 0;
    for block in &cfg.blocks {
        for edge in &block.successors {
            if matches!(
                edge.kind,
                EdgeKind::Jump | EdgeKind::BranchTaken | EdgeKind::Call
            ) {
                if let EdgeTarget::Block(_) = edge.target {
                    direct_edges += 1;
                }
            }
        }
    }
    // The fixture has 12 direct branches/calls, all targeting 0x4c, plus
    // 2 indirect (br/blr) which don't count as direct.
    assert_eq!(
        direct_edges, 12,
        "expected all 12 direct branches/calls to resolve internally"
    );

    // The block at 0x4c (bare ret) should have many predecessors and no
    // successors.
    let ret_block = cfg.block_at(0x4c).expect("0x4c should start a block");
    assert!(cfg.block(ret_block).successors.is_empty());
    let preds = cfg.predecessors_of(ret_block);
    assert!(
        preds.len() >= 12,
        "0x4c is the common target; expected many predecessors, got {}",
        preds.len()
    );
}
