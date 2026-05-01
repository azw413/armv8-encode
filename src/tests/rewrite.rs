//! Tests for the rewrite IR: lift round-trips, edit operations, layout
//! widening, and end-to-end emit cycles.

use super::common::*;
use crate::container::{
    Architecture, BinaryFormat, Container, Section, SectionId, SectionKind, Symbol, SymbolBinding,
    SymbolId, SymbolKind,
};
use crate::isa::aarch64;
use crate::isa::aarch64::{Aarch64Mnemonic, DecodedOperand};
use crate::mc::{build_cfg, BasicBlockId};
use crate::rewrite::{
    emit, lay_out, EditError, EmitStrategy, LayoutError, RewriteBlock, RewriteInstruction,
    RewriteOperand, RewritePlan, Target,
};

/// Encode a sequence of templates at consecutive 4-byte addresses starting
/// at `base` and return the decoded instructions plus the byte buffer.
fn encode_stream(
    base: u64,
    templates: Vec<aarch64::InstructionTemplate>,
) -> (Vec<aarch64::DecodedInstruction>, Vec<u8>) {
    let mut bytes = Vec::with_capacity(templates.len() * 4);
    for (index, mut template) in templates.into_iter().enumerate() {
        template.address = base + (index as u64) * 4;
        let word = aarch64::encode_instruction(&template)
            .unwrap_or_else(|err| panic!("encode {:?}: {err:?}", template.mnemonic));
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    let instructions = aarch64::disassemble_bytes(base, &bytes).expect("decode round-trip");
    (instructions, bytes)
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

fn ret() -> aarch64::InstructionTemplate {
    aarch64::InstructionTemplate {
        address: 0,
        mnemonic: Aarch64Mnemonic::Ret,
        operands: vec![DecodedOperand::Register(crate::isa::aarch64::Register {
            class: crate::isa::aarch64::RegisterClass::X,
            index: 30,
        })],
    }
}

fn lift(
    base: u64,
    templates: Vec<aarch64::InstructionTemplate>,
) -> (RewritePlan, Vec<u8>) {
    let (instructions, bytes) = encode_stream(base, templates);
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift(&cfg, &instructions);
    (plan, bytes)
}

#[test]
fn lift_then_emit_reproduces_source_bytes_for_branch_fixture() {
    // Round-trip the entire branch fixture: decode → CFG → lift → lay_out →
    // emit. With no edits, the output bytes must match the input exactly.
    let entries = parse_otool_fixture(BRANCH_OTOOL_FIXTURE);
    assert!(!entries.is_empty());
    let mut bytes = Vec::with_capacity(entries.len() * 4);
    for entry in &entries {
        bytes.extend_from_slice(&entry.word.to_le_bytes());
    }
    let base = entries[0].address;
    let instructions = aarch64::disassemble_bytes(base, &bytes).expect("fixture decodes");
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift(&cfg, &instructions);

    let layout = lay_out(&plan, base, None).expect("layout");
    let out = emit(&plan, &layout, None).expect("emit");

    assert_eq!(out, bytes, "round-trip should be byte-identical");
}

#[test]
fn lift_resolves_known_branch_targets_to_block_references() {
    // Two-instruction stream: `b 0x1004 ; nop`. The branch target is the
    // start of the next block, so lift should produce Branch(Block(_)).
    let (plan, _) = lift(0x1000, vec![b(0x1004), nop()]);
    let head = plan.instruction_at(0x1000).expect("head instr");

    let target = head
        .operands
        .iter()
        .find_map(|operand| match operand {
            RewriteOperand::Branch(target) => Some(*target),
            _ => None,
        })
        .expect("branch operand present");
    assert!(matches!(target, Target::Block(_)));
}

#[test]
fn lift_falls_back_to_absolute_for_external_targets() {
    // `b 0x2000` from a 1-instruction slice: target is outside the CFG.
    let (plan, _) = lift(0x1000, vec![b(0x2000)]);
    let head = plan.instruction_at(0x1000).expect("head");
    let target = head
        .operands
        .iter()
        .find_map(|operand| match operand {
            RewriteOperand::Branch(target) => Some(*target),
            _ => None,
        })
        .expect("branch");
    assert_eq!(target, Target::Absolute(0x2000));
}

#[test]
fn redirect_branch_changes_emitted_bytes() {
    // Stream: `b 0x1010 ; nop ; nop ; nop ; nop`. After redirecting to a
    // different absolute address, the encoded `b` displacement changes.
    let (mut plan, _) = lift(0x1000, vec![b(0x1010), nop(), nop(), nop(), nop()]);
    plan.redirect_branch(0x1000, Target::Absolute(0x2000)).unwrap();

    let layout = lay_out(&plan, 0x1000, None).unwrap();
    let bytes = emit(&plan, &layout, None).unwrap();

    // First word should now be `b 0x2000` from address 0x1000:
    // displacement = 0x1000 → encoding 0x14000400 (imm26 = 0x400).
    let first_word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let decoded = aarch64::decode_instruction(0x1000, first_word).unwrap();
    assert_eq!(decoded.mnemonic, Aarch64Mnemonic::B);
    assert_eq!(
        decoded.operands,
        vec![DecodedOperand::BranchTarget(0x2000)]
    );
}

#[test]
fn redirect_branch_to_unknown_address_errors() {
    let (mut plan, _) = lift(0x1000, vec![nop(), b(0x1004)]);
    let result = plan.redirect_branch(0x9999, Target::Absolute(0x2000));
    assert_eq!(result, Err(EditError::AddressNotFound(0x9999)));
}

#[test]
fn redirect_branch_on_non_branch_errors() {
    let (mut plan, _) = lift(0x1000, vec![nop(), b(0x1004)]);
    // 0x1000 is a `nop`, has no branch operand.
    let result = plan.redirect_branch(0x1000, Target::Absolute(0x2000));
    assert_eq!(result, Err(EditError::NoBranchOperand(0x1000)));
}

#[test]
fn replace_terminator_swaps_block_exit() {
    // Stream: `b.eq 0x1008 ; nop ; ret`. Replace the b.eq with b.ne via
    // replace_terminator on the head block.
    let (mut plan, _) = lift(0x1000, vec![beq(0x1008), nop(), ret()]);

    // The head block is BasicBlockId(0). Find its terminator's branch
    // target so we can preserve it.
    let head_block_id = BasicBlockId(0);
    let target = plan.blocks[0]
        .instructions
        .last()
        .unwrap()
        .pc_relative_target()
        .expect("head terminator should have a branch target");

    let new_terminator = RewriteInstruction {
        mnemonic: Aarch64Mnemonic::Bne,
        operands: vec![RewriteOperand::Branch(target)],
        original_address: None,
    };
    plan.replace_terminator(head_block_id, new_terminator).unwrap();

    let layout = lay_out(&plan, 0x1000, None).unwrap();
    let bytes = emit(&plan, &layout, None).unwrap();

    let first_word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let decoded = aarch64::decode_instruction(0x1000, first_word).unwrap();
    assert_eq!(decoded.mnemonic, Aarch64Mnemonic::Bne);
}

#[test]
fn insert_after_address_recomputes_downstream_branch_targets() {
    // Stream: `nop ; b 0x100c ; nop ; nop`.
    // After inserting two nops after the first nop, addresses shift by 8.
    // The branch's *symbolic* target (the third nop's block) doesn't change,
    // but its emitted displacement does.
    let (mut plan, _) = lift(0x1000, vec![nop(), b(0x100c), nop(), nop()]);
    plan.insert_after_address(0x1000, vec![
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Nop,
            operands: Vec::new(),
            original_address: None,
        },
        RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Nop,
            operands: Vec::new(),
            original_address: None,
        },
    ])
    .unwrap();

    let layout = lay_out(&plan, 0x1000, None).unwrap();
    let bytes = emit(&plan, &layout, None).unwrap();

    // Total size: 6 instructions × 4 = 24 bytes.
    assert_eq!(bytes.len(), 24);

    // The original `b 0x100c` was at 0x1004, emitted at 0x100c after the
    // shift. Its target (the originally-third instruction at 0x100c) is now
    // at 0x1014. Decode the third word to confirm.
    let third = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let decoded = aarch64::decode_instruction(0x100c, third).unwrap();
    assert_eq!(decoded.mnemonic, Aarch64Mnemonic::B);
    assert_eq!(
        decoded.operands,
        vec![DecodedOperand::BranchTarget(0x1014)]
    );
}

#[test]
fn remove_at_address_drops_an_instruction() {
    let (mut plan, _) = lift(0x1000, vec![nop(), nop(), nop(), ret()]);
    plan.remove_at_address(0x1004).unwrap();

    let layout = lay_out(&plan, 0x1000, None).unwrap();
    let bytes = emit(&plan, &layout, None).unwrap();
    assert_eq!(bytes.len(), 12, "3 remaining instructions × 4 bytes");
}

#[test]
fn layout_widens_conditional_branch_to_far_absolute_target() {
    // Single-block plan: `b.eq 0x1004 ; nop`. Redirect the b.eq to 0x40_1000
    // (≈ 4 MiB away), well outside the ±1 MiB pcrel19 range but inside the
    // ±128 MiB pcrel26 range that the widened `b` can reach. Layout must
    // expand to:
    //   b.ne +8 ; b 0x40_1000 ; nop
    let far_target = 0x40_1000;
    let (mut plan, _) = lift(0x1000, vec![beq(0x1004), nop()]);
    plan.redirect_branch(0x1000, Target::Absolute(far_target))
        .unwrap();

    let layout = lay_out(&plan, 0x1000, None).unwrap();
    assert_eq!(
        layout.instruction_layouts[0][0].strategy,
        EmitStrategy::InvertedConditional
    );
    assert_eq!(layout.instruction_layouts[0][0].size, 8);

    // Total = widened (8) + nop (4) = 12 bytes.
    assert_eq!(layout.total_size, 12);

    let bytes = emit(&plan, &layout, None).unwrap();
    assert_eq!(bytes.len(), 12);

    // Word 0: b.ne +8 (skip over the b)
    let w0 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let d0 = aarch64::decode_instruction(0x1000, w0).unwrap();
    assert_eq!(d0.mnemonic, Aarch64Mnemonic::Bne);
    assert_eq!(d0.operands, vec![DecodedOperand::BranchTarget(0x1008)]);

    // Word 1: b far_target
    let w1 = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let d1 = aarch64::decode_instruction(0x1004, w1).unwrap();
    assert_eq!(d1.mnemonic, Aarch64Mnemonic::B);
    assert_eq!(
        d1.operands,
        vec![DecodedOperand::BranchTarget(far_target)]
    );
}

#[test]
fn layout_widening_for_unwidenable_unconditional_errors() {
    // `b 0x1_0000_0000` from base 0x1000: pcrel26 only reaches ±128 MiB.
    // 4 GiB is well outside, and `b` has no widening strategy yet.
    let (mut plan, _) = lift(0x1000, vec![b(0x1004), nop()]);
    plan.redirect_branch(0x1000, Target::Absolute(0x1_0000_0000))
        .unwrap();

    let result = lay_out(&plan, 0x1000, None);
    assert!(matches!(
        result,
        Err(LayoutError::DisplacementTooLarge { .. })
    ));
}

#[test]
fn layout_errors_on_symbol_without_container() {
    // Build a minimal plan by hand and inject a Symbol target. Without a
    // container to resolve it against, layout errors loudly.
    let mut plan = RewritePlan::new();
    plan.blocks.push(RewriteBlock {
        id: BasicBlockId(0),
        instructions: vec![RewriteInstruction {
            mnemonic: Aarch64Mnemonic::B,
            operands: vec![RewriteOperand::Branch(Target::Symbol(
                crate::rewrite::SymbolId(0),
            ))],
            original_address: None,
        }],
    });

    let result = lay_out(&plan, 0x1000, None);
    assert_eq!(result, Err(LayoutError::SymbolWithoutContainer));
}

#[test]
fn end_to_end_redirect_then_decode_yields_expected_target() {
    // Build a small program, lift, redirect a branch, emit, decode the
    // emitted bytes, and verify the new target is what we set.
    let (mut plan, _) = lift(
        0x1000,
        vec![
            beq(0x1010), // 0x1000 — original target was 0x1010
            nop(),       // 0x1004
            nop(),       // 0x1008
            nop(),       // 0x100c
            nop(),       // 0x1010 — original target
            nop(),       // 0x1014
            ret(),       // 0x1018
        ],
    );

    // Redirect to the ret instead of the post-branch nops.
    plan.redirect_branch(0x1000, Target::Absolute(0x1018)).unwrap();
    let layout = lay_out(&plan, 0x1000, None).unwrap();
    let bytes = emit(&plan, &layout, None).unwrap();

    // Re-decode and verify.
    let decoded = aarch64::disassemble_bytes(0x1000, &bytes).unwrap();
    let head = &decoded[0];
    assert_eq!(head.mnemonic, Aarch64Mnemonic::Beq);
    assert_eq!(
        head.operands,
        vec![DecodedOperand::BranchTarget(0x1018)]
    );

    // The remaining instructions should be unchanged.
    for (i, instr) in decoded.iter().enumerate().skip(1) {
        if i < decoded.len() - 1 {
            assert_eq!(instr.mnemonic, Aarch64Mnemonic::Nop);
        } else {
            assert_eq!(instr.mnemonic, Aarch64Mnemonic::Ret);
        }
    }
}

// ---- Container-aware lift / layout / emit -------------------------------

/// Hand-craft a minimal `Container` containing a single defined function
/// `target_fn` at `address`. Used to test symbol resolution without the
/// `object` round-trip overhead.
fn container_with_function(name: &str, address: u64) -> Container {
    Container {
        format: BinaryFormat::Elf,
        architecture: Architecture::Aarch64,
        sections: vec![Section {
            id: SectionId(0),
            name: ".text".to_string(),
            address,
            size: 4,
            bytes: vec![0xc0, 0x03, 0x5f, 0xd6], // ret
            kind: SectionKind::Text,
        }],
        symbols: vec![Symbol {
            id: SymbolId(0),
            name: name.to_string(),
            address,
            size: 4,
            kind: SymbolKind::Function,
            binding: SymbolBinding::Global,
            section: Some(SectionId(0)),
            is_undefined: false,
        }],
        relocations: Vec::new(),
        dwarf: None,
    }
}

fn container_with_undefined_function(name: &str) -> Container {
    Container {
        format: BinaryFormat::Elf,
        architecture: Architecture::Aarch64,
        sections: Vec::new(),
        symbols: vec![Symbol {
            id: SymbolId(0),
            name: name.to_string(),
            address: 0,
            size: 0,
            kind: SymbolKind::Function,
            binding: SymbolBinding::Global,
            section: None,
            is_undefined: true,
        }],
        relocations: Vec::new(),
        dwarf: None,
    }
}

#[test]
fn lift_with_container_resolves_cross_function_call_to_symbol() {
    // Stream: `bl 0x4000 ; ret`. The container places `target_fn` at 0x4000.
    let target_address = 0x4000;
    let container = container_with_function("target_fn", target_address);

    let templates = vec![
        aarch64::InstructionTemplate {
            address: 0,
            mnemonic: Aarch64Mnemonic::Bl,
            operands: vec![DecodedOperand::BranchTarget(target_address)],
        },
        ret(),
    ];
    let (instructions, _) = encode_stream(0x1000, templates);
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

    let bl = plan.instruction_at(0x1000).expect("bl present");
    let target = bl.pc_relative_target().expect("branch operand");
    assert_eq!(target, Target::Symbol(SymbolId(0)));
}

#[test]
fn lift_without_container_keeps_targets_as_absolute() {
    // Same stream, no container — should fall back to Target::Absolute.
    let templates = vec![
        aarch64::InstructionTemplate {
            address: 0,
            mnemonic: Aarch64Mnemonic::Bl,
            operands: vec![DecodedOperand::BranchTarget(0x4000)],
        },
        ret(),
    ];
    let (instructions, _) = encode_stream(0x1000, templates);
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift(&cfg, &instructions);

    let target = plan
        .instruction_at(0x1000)
        .unwrap()
        .pc_relative_target()
        .unwrap();
    assert_eq!(target, Target::Absolute(0x4000));
}

#[test]
fn block_target_wins_over_symbol_with_same_address() {
    // Container has a symbol at 0x1004; CFG also has a block starting at
    // 0x1004 (the second instruction). Block should win.
    let container = container_with_function("intra_fn", 0x1004);
    let (instructions, _) = encode_stream(0x1000, vec![b(0x1004), nop()]);
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

    let target = plan
        .instruction_at(0x1000)
        .unwrap()
        .pc_relative_target()
        .unwrap();
    assert!(matches!(target, Target::Block(_)),
        "Block(local CFG) must win over Symbol(container)"
    );
}

#[test]
fn layout_resolves_defined_symbol_against_container() {
    let target_address = 0x4000;
    let container = container_with_function("target_fn", target_address);

    let templates = vec![
        aarch64::InstructionTemplate {
            address: 0,
            mnemonic: Aarch64Mnemonic::Bl,
            operands: vec![DecodedOperand::BranchTarget(target_address)],
        },
        ret(),
    ];
    let (instructions, _) = encode_stream(0x1000, templates);
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

    let layout = lay_out(&plan, 0x1000, Some(&container)).expect("layout");
    let bytes = emit(&plan, &layout, Some(&container)).expect("emit");

    // The first word is `bl 0x4000` from address 0x1000. Decode and check.
    let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let decoded = aarch64::decode_instruction(0x1000, word).unwrap();
    assert_eq!(decoded.mnemonic, Aarch64Mnemonic::Bl);
    assert_eq!(
        decoded.operands,
        vec![DecodedOperand::BranchTarget(target_address)]
    );
}

#[test]
fn layout_errors_on_undefined_symbol_target() {
    let container = container_with_undefined_function("printf");

    let mut plan = RewritePlan::new();
    plan.blocks.push(RewriteBlock {
        id: BasicBlockId(0),
        instructions: vec![RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Bl,
            operands: vec![RewriteOperand::Branch(Target::Symbol(SymbolId(0)))],
            original_address: None,
        }],
    });

    let result = lay_out(&plan, 0x1000, Some(&container));
    assert_eq!(result, Err(LayoutError::UndefinedSymbol { symbol_id: 0 }));
}

#[test]
fn redirect_branch_to_symbol_target_emits_symbol_address() {
    // Lift a basic stream with a placeholder absolute target, then
    // redirect to a container symbol.
    let container = container_with_function("hot_path", 0x3000);
    let symbol_id = container.symbols[0].id;

    let (mut plan, _) = lift(0x1000, vec![b(0x1004), nop()]);
    plan.redirect_branch(0x1000, Target::Symbol(symbol_id))
        .unwrap();

    let layout = lay_out(&plan, 0x1000, Some(&container)).unwrap();
    let bytes = emit(&plan, &layout, Some(&container)).unwrap();

    let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let decoded = aarch64::decode_instruction(0x1000, word).unwrap();
    assert_eq!(decoded.mnemonic, Aarch64Mnemonic::B);
    assert_eq!(
        decoded.operands,
        vec![DecodedOperand::BranchTarget(0x3000)]
    );
}

#[test]
fn round_trip_with_container_is_byte_identical_for_extern_call() {
    // `bl _printf ; ret` where `_printf` is at 0x4000 in the container.
    // No edits — emit should reproduce the input bytes exactly.
    let target_address = 0x4000;
    let container = container_with_function("_printf", target_address);

    let templates = vec![
        aarch64::InstructionTemplate {
            address: 0,
            mnemonic: Aarch64Mnemonic::Bl,
            operands: vec![DecodedOperand::BranchTarget(target_address)],
        },
        ret(),
    ];
    let (instructions, source_bytes) = encode_stream(0x1000, templates);
    let cfg = build_cfg(&instructions);
    let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

    let layout = lay_out(&plan, 0x1000, Some(&container)).unwrap();
    let out = emit(&plan, &layout, Some(&container)).unwrap();
    assert_eq!(
        out, source_bytes,
        "no-op rewrite with container must round-trip"
    );
}
