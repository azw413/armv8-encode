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
    let out = emit(&plan, &layout, None).expect("emit").bytes;

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
    let bytes = emit(&plan, &layout, None).unwrap().bytes;

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
    let last = plan.blocks[0].ops.last().unwrap();
    let target = match last {
        crate::rewrite::RewriteOp::Instruction(insn) => insn
            .pc_relative_target()
            .expect("head terminator should have a branch target"),
        crate::rewrite::RewriteOp::Macro(_) => {
            panic!("head terminator should be a singleton instruction")
        }
    };

    let new_terminator = RewriteInstruction {
        mnemonic: Aarch64Mnemonic::Bne,
        operands: vec![RewriteOperand::Branch(target)],
        original_address: None,
    };
    plan.replace_terminator(head_block_id, new_terminator).unwrap();

    let layout = lay_out(&plan, 0x1000, None).unwrap();
    let bytes = emit(&plan, &layout, None).unwrap().bytes;

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
    let bytes = emit(&plan, &layout, None).unwrap().bytes;

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
    let bytes = emit(&plan, &layout, None).unwrap().bytes;
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

    let bytes = emit(&plan, &layout, None).unwrap().bytes;
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
        ops: vec![crate::rewrite::RewriteOp::Instruction(RewriteInstruction {
            mnemonic: Aarch64Mnemonic::B,
            operands: vec![RewriteOperand::Branch(Target::Symbol(
                crate::rewrite::SymbolId(0),
            ))],
            original_address: None,
        })],
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
    let bytes = emit(&plan, &layout, None).unwrap().bytes;

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
            align: 4,
            flags: None,
            raw_sh_type: None,
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
            flags: None,
        }],
        relocations: Vec::new(),
        file_flags: None,
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
            flags: None,
        }],
        relocations: Vec::new(),
        file_flags: None,
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
    let bytes = emit(&plan, &layout, Some(&container)).expect("emit").bytes;

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
fn undefined_symbol_target_emits_relocation_instead_of_erroring() {
    // `bl printf` where printf is an extern. Layout shouldn't error;
    // emit should produce a placeholder word + Branch26 relocation.
    use crate::container::RelocationKind;
    use crate::rewrite::EmittedRelocation;

    let container = container_with_undefined_function("printf");

    let mut plan = RewritePlan::new();
    plan.blocks.push(RewriteBlock {
        id: BasicBlockId(0),
        ops: vec![crate::rewrite::RewriteOp::Instruction(RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Bl,
            operands: vec![RewriteOperand::Branch(Target::Symbol(SymbolId(0)))],
            original_address: None,
        })],
    });

    let layout = lay_out(&plan, 0x1000, Some(&container)).expect("layout");
    let output = emit(&plan, &layout, Some(&container)).expect("emit");

    assert_eq!(output.bytes.len(), 4, "single instruction = 4 bytes");
    assert_eq!(
        output.relocations,
        vec![EmittedRelocation {
            offset: 0,
            kind: RelocationKind::Branch26,
            symbol: SymbolId(0),
            addend: 0,
        }]
    );

    // The placeholder word should encode `bl 0x1000` (displacement 0).
    let word = u32::from_le_bytes([
        output.bytes[0],
        output.bytes[1],
        output.bytes[2],
        output.bytes[3],
    ]);
    let decoded = aarch64::decode_instruction(0x1000, word).expect("decode placeholder");
    assert_eq!(decoded.mnemonic, Aarch64Mnemonic::Bl);
    assert_eq!(
        decoded.operands,
        vec![DecodedOperand::BranchTarget(0x1000)]
    );
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
    let bytes = emit(&plan, &layout, Some(&container)).unwrap().bytes;

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
    let out = emit(&plan, &layout, Some(&container)).unwrap().bytes;
    assert_eq!(
        out, source_bytes,
        "no-op rewrite with container must round-trip"
    );
}

#[test]
fn cbz_to_undefined_symbol_emits_branch19_relocation() {
    use crate::container::RelocationKind;
    use crate::rewrite::EmittedRelocation;

    let container = container_with_undefined_function("trampoline");

    let mut plan = RewritePlan::new();
    plan.blocks.push(RewriteBlock {
        id: BasicBlockId(0),
        ops: vec![crate::rewrite::RewriteOp::Instruction(RewriteInstruction {
            mnemonic: Aarch64Mnemonic::Cbz,
            operands: vec![
                RewriteOperand::Decoded(DecodedOperand::Register(
                    crate::isa::aarch64::Register {
                        class: crate::isa::aarch64::RegisterClass::W,
                        index: 0,
                    },
                )),
                RewriteOperand::Branch(Target::Symbol(SymbolId(0))),
            ],
            original_address: None,
        })],
    });

    let layout = lay_out(&plan, 0x2000, Some(&container)).unwrap();
    let output = emit(&plan, &layout, Some(&container)).unwrap();

    assert_eq!(
        output.relocations,
        vec![EmittedRelocation {
            offset: 0,
            kind: RelocationKind::Branch19,
            symbol: SymbolId(0),
            addend: 0,
        }]
    );
}

#[test]
fn defined_and_undefined_targets_coexist_in_one_emit() {
    // Two-instruction plan: `bl defined ; bl extern`.
    // First should fold to the defined address; second should produce a
    // relocation.
    use crate::container::{
        Architecture as ContArch, BinaryFormat as ContFormat, Container as Cont, Section,
        Symbol, SymbolBinding, SymbolKind,
    };
    use crate::container::RelocationKind;

    let container = Cont {
        format: ContFormat::Elf,
        architecture: ContArch::Aarch64,
        sections: vec![Section {
            id: SectionId(0),
            name: ".text".to_string(),
            address: 0,
            size: 4,
            bytes: vec![0; 4],
            kind: SectionKind::Text,
            align: 4,
            flags: None,
            raw_sh_type: None,
        }],
        symbols: vec![
            Symbol {
                id: SymbolId(0),
                name: "defined_fn".to_string(),
                address: 0x5000,
                size: 4,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: Some(SectionId(0)),
                is_undefined: false,
                flags: None,
            },
            Symbol {
                id: SymbolId(1),
                name: "extern_fn".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: None,
                is_undefined: true,
                flags: None,
            },
        ],
        relocations: Vec::new(),
        file_flags: None,
        dwarf: None,
    };

    let mut plan = RewritePlan::new();
    plan.blocks.push(RewriteBlock {
        id: BasicBlockId(0),
        ops: vec![
            crate::rewrite::RewriteOp::Instruction(RewriteInstruction {
                mnemonic: Aarch64Mnemonic::Bl,
                operands: vec![RewriteOperand::Branch(Target::Symbol(SymbolId(0)))],
                original_address: None,
            }),
            crate::rewrite::RewriteOp::Instruction(RewriteInstruction {
                mnemonic: Aarch64Mnemonic::Bl,
                operands: vec![RewriteOperand::Branch(Target::Symbol(SymbolId(1)))],
                original_address: None,
            }),
        ],
    });

    let layout = lay_out(&plan, 0x1000, Some(&container)).unwrap();
    let output = emit(&plan, &layout, Some(&container)).unwrap();

    assert_eq!(output.bytes.len(), 8);
    // Only the second instruction needs a relocation. Its byte offset is 4.
    assert_eq!(output.relocations.len(), 1);
    assert_eq!(output.relocations[0].offset, 4);
    assert_eq!(output.relocations[0].kind, RelocationKind::Branch26);
    assert_eq!(output.relocations[0].symbol, SymbolId(1));

    // The first word (defined target) encodes `bl 0x5000` from 0x1000:
    // displacement = 0x4000.
    let first = u32::from_le_bytes([
        output.bytes[0],
        output.bytes[1],
        output.bytes[2],
        output.bytes[3],
    ]);
    let decoded = aarch64::decode_instruction(0x1000, first).unwrap();
    assert_eq!(
        decoded.operands,
        vec![DecodedOperand::BranchTarget(0x5000)]
    );
}

#[test]
fn commit_to_container_replaces_bytes_and_relocations() {
    use crate::container::RelocationKind;
    use crate::rewrite::{commit_to_container, EmitOutput, EmittedRelocation};

    let container = container_with_function("target_fn", 0x4000);
    // Inject a stale relocation into the section so we can verify it gets
    // dropped.
    let mut container = container;
    let text_id = container.text_sections().next().unwrap().id;
    container.relocations.push(crate::container::Relocation {
        id: crate::container::RelocationId(0),
        section: text_id,
        offset: 999,
        kind: RelocationKind::Branch26,
        size: 26,
        addend: 0,
        symbol: Some(container.symbols[0].id),
    });

    let output = EmitOutput {
        bytes: vec![0xaa, 0xbb, 0xcc, 0xdd],
        relocations: vec![EmittedRelocation {
            offset: 0,
            kind: RelocationKind::Branch26,
            symbol: container.symbols[0].id,
            addend: 0,
        }],
    };

    let committed = commit_to_container(&container, text_id, output);

    // Bytes replaced.
    assert_eq!(committed.section(text_id).bytes, vec![0xaa, 0xbb, 0xcc, 0xdd]);
    // Stale relocation gone, fresh one in place.
    let section_relocations: Vec<_> = committed.relocations_for(text_id).collect();
    assert_eq!(section_relocations.len(), 1);
    assert_eq!(section_relocations[0].offset, 0);
    assert_eq!(section_relocations[0].kind, RelocationKind::Branch26);
}

#[test]
fn commit_to_container_preserves_relocations_on_other_sections() {
    // Stage 2 invariant: rewriting one section's bytes/relocations must
    // not disturb relocations on other sections (e.g. .rodata pointing
    // into .text via a vtable).
    use crate::container::RelocationKind;
    use crate::rewrite::{commit_to_container, EmitOutput};

    let mut container = container_with_function("target_fn", 0x4000);
    let text_id = container.text_sections().next().unwrap().id;
    let other_section_id = SectionId(99);

    // Stale relocation on .text (will be cleared) and one on a different
    // section (must survive).
    container.relocations.push(crate::container::Relocation {
        id: crate::container::RelocationId(0),
        section: text_id,
        offset: 0x100,
        kind: RelocationKind::Branch26,
        size: 26,
        addend: 0,
        symbol: Some(container.symbols[0].id),
    });
    container.relocations.push(crate::container::Relocation {
        id: crate::container::RelocationId(1),
        section: other_section_id,
        offset: 0x200,
        kind: RelocationKind::Absolute,
        size: 64,
        addend: 7,
        symbol: Some(container.symbols[0].id),
    });

    let output = EmitOutput {
        bytes: vec![0xaa, 0xbb, 0xcc, 0xdd],
        relocations: vec![],
    };

    let committed = commit_to_container(&container, text_id, output);

    // Other section's relocation untouched.
    let other_relocations: Vec<_> = committed
        .relocations
        .iter()
        .filter(|r| r.section == other_section_id)
        .collect();
    assert_eq!(other_relocations.len(), 1);
    assert_eq!(other_relocations[0].offset, 0x200);
    assert_eq!(other_relocations[0].addend, 7);

    // Targeted section cleared (no emit relocations were supplied).
    let text_relocations: Vec<_> = committed.relocations_for(text_id).collect();
    assert_eq!(text_relocations.len(), 0);
}

#[test]
fn full_pipeline_emits_extern_call_relocation_in_written_object() {
    use crate::container::RelocationKind;
    use crate::rewrite::commit_to_container;
    use object::write::{
        Object as WriteObject, StandardSection, Symbol as WriteSymbol,
        SymbolSection as WriteSymbolSection,
    };
    use object::{
        Architecture as ObjArch, BinaryFormat as ObjFormat, Endianness, SymbolFlags,
        SymbolKind as ObjSymbolKind, SymbolScope,
    };

    // Build an ELF with `nop ; bl 0 ; ret`. The `bl` originally targets 0
    // (placeholder) and we'll redirect it to the extern symbol.
    let mut obj = WriteObject::new(ObjFormat::Elf, ObjArch::Aarch64, Endianness::Little);
    let text_id = obj.section_id(StandardSection::Text);
    let mut text = Vec::with_capacity(12);
    text.extend_from_slice(&0xd503201fu32.to_le_bytes()); // nop
    text.extend_from_slice(&0x94000000u32.to_le_bytes()); // bl 0 (placeholder)
    text.extend_from_slice(&0xd65f03c0u32.to_le_bytes()); // ret
    obj.append_section_data(text_id, &text, 4);
    let printf_id = obj.add_symbol(WriteSymbol {
        name: b"printf".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });
    let _ = printf_id;
    obj.add_symbol(WriteSymbol {
        name: b"main".to_vec(),
        value: 0,
        size: 12,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Section(text_id),
        flags: SymbolFlags::None,
    });
    let initial = obj.write().unwrap();
    let container = Container::from_bytes(&initial).unwrap();

    // Find the printf symbol in the parsed container.
    let printf_id = container
        .symbols
        .iter()
        .find(|s| s.name.ends_with("printf"))
        .expect("printf in symbol table")
        .id;

    // Disassemble + lift + redirect the bl at offset 4 to printf.
    let text_section_id = container.text_sections().next().unwrap().id;
    let text_section = container.section(text_section_id);
    let (base, code) = text_section.for_disassembly().unwrap();
    let instructions = aarch64::disassemble_bytes(base, code).unwrap();
    let cfg = build_cfg(&instructions);
    let mut plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);
    plan.redirect_branch(base + 4, Target::Symbol(printf_id))
        .unwrap();

    let layout = lay_out(&plan, base, Some(&container)).unwrap();
    let output = emit(&plan, &layout, Some(&container)).unwrap();

    // The redirect produced a relocation at offset 4.
    assert_eq!(output.relocations.len(), 1);
    assert_eq!(output.relocations[0].offset, 4);
    assert_eq!(output.relocations[0].kind, RelocationKind::Branch26);
    assert_eq!(output.relocations[0].symbol, printf_id);

    // Commit to a fresh container, write, re-read.
    let committed = commit_to_container(&container, text_section_id, output);
    let written = committed.to_bytes().unwrap();
    let reparsed = Container::from_bytes(&written).unwrap();

    // Reparsed binary should have a Branch26 relocation pointing at
    // `printf` in its text section.
    let printf_in_reparsed = reparsed
        .symbols
        .iter()
        .find(|s| s.name.ends_with("printf"))
        .expect("printf survived");
    let printf_relocations: Vec<_> = reparsed
        .relocations
        .iter()
        .filter(|r| r.kind == RelocationKind::Branch26 && r.symbol == Some(printf_in_reparsed.id))
        .collect();
    assert_eq!(
        printf_relocations.len(),
        1,
        "exactly one branch26 reloc to printf in the rewritten binary"
    );
}

// ---- Macro fusion (adrp + add) ------------------------------------------

mod macros {
    use super::*;
    use crate::container::RelocationKind;
    use crate::rewrite::{commit_to_container, MacroKind, RewriteOp};

    /// Helpers to build adrp+add instruction templates pointing at an
    /// absolute target. The encoder takes care of splitting target into
    /// page + offset for adrp.
    fn adrp(rd: u8, target_page: u64) -> aarch64::InstructionTemplate {
        aarch64::InstructionTemplate {
            address: 0,
            mnemonic: Aarch64Mnemonic::Adrp,
            operands: vec![
                DecodedOperand::Register(crate::isa::aarch64::Register {
                    class: crate::isa::aarch64::RegisterClass::X,
                    index: rd,
                }),
                DecodedOperand::PageTarget(target_page),
            ],
        }
    }

    fn add_imm(rd: u8, rn: u8, imm: i64) -> aarch64::InstructionTemplate {
        aarch64::InstructionTemplate {
            address: 0,
            mnemonic: Aarch64Mnemonic::Add,
            operands: vec![
                DecodedOperand::Register(crate::isa::aarch64::Register {
                    class: crate::isa::aarch64::RegisterClass::X,
                    index: rd,
                }),
                DecodedOperand::Register(crate::isa::aarch64::Register {
                    class: crate::isa::aarch64::RegisterClass::X,
                    index: rn,
                }),
                DecodedOperand::Immediate(imm),
            ],
        }
    }

    /// Produce a one-block plan from the given templates, lifted with no
    /// container.
    fn plan_from(templates: Vec<aarch64::InstructionTemplate>, base: u64) -> RewritePlan {
        let (instructions, _bytes) = encode_stream(base, templates);
        let cfg = build_cfg(&instructions);
        RewritePlan::lift(&cfg, &instructions)
    }

    #[test]
    fn adrp_add_pair_fuses_into_load_address_macro() {
        // adrp x0, 0x10000 ; add x0, x0, #0x18 ; ret
        let plan = plan_from(
            vec![adrp(0, 0x10000), add_imm(0, 0, 0x18), ret()],
            0x1000,
        );
        let head = &plan.blocks[0];
        assert_eq!(
            head.ops.len(),
            2,
            "adrp+add should collapse into one macro op + ret"
        );
        match &head.ops[0] {
            RewriteOp::Macro(macro_op) => {
                assert_eq!(macro_op.kind, MacroKind::LoadAddress);
                assert_eq!(macro_op.register.index, 0);
                assert_eq!(macro_op.target, Target::Absolute(0x10018));
                assert_eq!(macro_op.original_addresses, vec![0x1000, 0x1004]);
            }
            other => panic!("expected Macro variant, got {other:?}"),
        }
    }

    #[test]
    fn fusion_skipped_when_destination_registers_differ() {
        // adrp x0, 0x10000 ; add x1, x0, #0 ← different Rd
        let plan = plan_from(vec![adrp(0, 0x10000), add_imm(1, 0, 0), ret()], 0x1000);
        assert_eq!(plan.blocks[0].ops.len(), 3);
        assert!(matches!(plan.blocks[0].ops[0], RewriteOp::Instruction(_)));
        assert!(matches!(plan.blocks[0].ops[1], RewriteOp::Instruction(_)));
    }

    #[test]
    fn fusion_skipped_when_add_uses_third_register() {
        // adrp x0, 0x10000 ; add x0, x1, #0 ← Rn != Rd
        let plan = plan_from(vec![adrp(0, 0x10000), add_imm(0, 1, 0), ret()], 0x1000);
        assert_eq!(plan.blocks[0].ops.len(), 3);
    }

    #[test]
    fn fusion_skipped_when_intervening_instruction_present() {
        // adrp x0, 0x10000 ; nop ; add x0, x0, #0
        // Strict adjacency means the nop blocks fusion.
        let plan = plan_from(
            vec![adrp(0, 0x10000), nop(), add_imm(0, 0, 0), ret()],
            0x1000,
        );
        assert_eq!(plan.blocks[0].ops.len(), 4);
        for op in &plan.blocks[0].ops {
            assert!(matches!(op, RewriteOp::Instruction(_)));
        }
    }

    #[test]
    fn macro_round_trips_byte_identical_when_no_edits() {
        // adrp x0, 0x10000 ; add x0, x0, #0x18 ; ret
        let templates = vec![adrp(0, 0x10000), add_imm(0, 0, 0x18), ret()];
        let (instructions, source_bytes) = encode_stream(0x1000, templates);
        let cfg = build_cfg(&instructions);
        let plan = RewritePlan::lift(&cfg, &instructions);

        let layout = lay_out(&plan, 0x1000, None).unwrap();
        let output = emit(&plan, &layout, None).unwrap();

        assert_eq!(
            output.bytes, source_bytes,
            "no-op rewrite of adrp+add should byte-round-trip"
        );
    }

    #[test]
    fn redirect_macro_target_changes_emitted_bytes() {
        // adrp x0, 0x10000 ; add x0, x0, #0x18 ; ret
        // Redirect the macro to target 0x40_0030 instead.
        let templates = vec![adrp(0, 0x10000), add_imm(0, 0, 0x18), ret()];
        let mut plan = plan_from(templates, 0x1000);

        plan.redirect_macro_target(0x1000, Target::Absolute(0x40_0030))
            .unwrap();

        let layout = lay_out(&plan, 0x1000, None).unwrap();
        let output = emit(&plan, &layout, None).unwrap();

        // First word is the redirected adrp.
        let adrp_word = u32::from_le_bytes([
            output.bytes[0],
            output.bytes[1],
            output.bytes[2],
            output.bytes[3],
        ]);
        let adrp_decoded = aarch64::decode_instruction(0x1000, adrp_word).unwrap();
        assert_eq!(adrp_decoded.mnemonic, Aarch64Mnemonic::Adrp);
        // adrp should target the page containing 0x40_0030, i.e. 0x40_0000.
        match &adrp_decoded.operands[1] {
            DecodedOperand::PageTarget(page) => assert_eq!(*page, 0x40_0000),
            other => panic!("expected PageTarget, got {other:?}"),
        }

        // Second word is the redirected add.
        let add_word = u32::from_le_bytes([
            output.bytes[4],
            output.bytes[5],
            output.bytes[6],
            output.bytes[7],
        ]);
        let add_decoded = aarch64::decode_instruction(0x1004, add_word).unwrap();
        assert_eq!(add_decoded.mnemonic, Aarch64Mnemonic::Add);
        match add_decoded.operands.last().unwrap() {
            DecodedOperand::Immediate(imm) => assert_eq!(*imm, 0x30),
            other => panic!("expected Immediate, got {other:?}"),
        }
    }

    #[test]
    fn redirect_macro_target_via_instruction_method_errors() {
        let templates = vec![adrp(0, 0x10000), add_imm(0, 0, 0x18), ret()];
        let mut plan = plan_from(templates, 0x1000);
        let result = plan.redirect_branch(0x1000, Target::Absolute(0x40_0000));
        assert_eq!(result, Err(EditError::NotAnInstruction(0x1000)));
    }

    #[test]
    fn redirect_macro_target_on_singleton_errors() {
        let plan_templates = vec![nop(), ret()];
        let mut plan = plan_from(plan_templates, 0x1000);
        let result = plan.redirect_macro_target(0x1000, Target::Absolute(0x2000));
        assert_eq!(result, Err(EditError::NotAMacro(0x1000)));
    }

    #[test]
    fn macro_locating_works_for_both_component_addresses() {
        // adrp at 0x1000, add at 0x1004 — both addresses should locate
        // the same macro.
        let templates = vec![adrp(0, 0x10000), add_imm(0, 0, 0x18), ret()];
        let plan = plan_from(templates, 0x1000);

        match plan.op_at(0x1000) {
            Some(RewriteOp::Macro(_)) => {}
            other => panic!("0x1000 should locate the macro, got {other:?}"),
        }
        match plan.op_at(0x1004) {
            Some(RewriteOp::Macro(_)) => {}
            other => panic!("0x1004 should also locate the macro, got {other:?}"),
        }
    }

    #[test]
    fn undefined_symbol_macro_emits_two_relocations() {
        // Build a minimal container with an undefined `data_sym` and a
        // plan with a LoadAddress macro pointing at it.
        let container = container_with_undefined_function("data_sym");
        let symbol_id = container.symbols[0].id;

        let mut plan = plan_from(
            vec![adrp(0, 0x10000), add_imm(0, 0, 0x18), ret()],
            0x1000,
        );
        plan.redirect_macro_target(0x1000, Target::Symbol(symbol_id))
            .unwrap();

        let layout = lay_out(&plan, 0x1000, Some(&container)).unwrap();
        let output = emit(&plan, &layout, Some(&container)).unwrap();

        // Two relocations at offsets 0 and 4: AdrpPage21 +
        // AddPageOffset12 (the add-form companion).
        assert_eq!(output.relocations.len(), 2);
        assert_eq!(output.relocations[0].offset, 0);
        assert_eq!(output.relocations[0].kind, RelocationKind::AdrpPage21);
        assert_eq!(output.relocations[0].symbol, symbol_id);
        assert_eq!(output.relocations[1].offset, 4);
        assert_eq!(output.relocations[1].kind, RelocationKind::AddPageOffset12);
        assert_eq!(output.relocations[1].symbol, symbol_id);
    }

    #[test]
    fn defined_symbol_macro_resolves_at_emit_time() {
        // Container places `target_data` at 0x40_1234. Macro should
        // resolve to adrp page 0x40_1000, add immediate 0x234.
        let container = container_with_function("target_data", 0x40_1234);
        let symbol_id = container.symbols[0].id;

        let mut plan = plan_from(
            vec![adrp(0, 0x10000), add_imm(0, 0, 0), ret()],
            0x1000,
        );
        plan.redirect_macro_target(0x1000, Target::Symbol(symbol_id))
            .unwrap();

        let layout = lay_out(&plan, 0x1000, Some(&container)).unwrap();
        let output = emit(&plan, &layout, Some(&container)).unwrap();

        assert!(output.relocations.is_empty());

        let adrp_word = u32::from_le_bytes([
            output.bytes[0],
            output.bytes[1],
            output.bytes[2],
            output.bytes[3],
        ]);
        let adrp_decoded = aarch64::decode_instruction(0x1000, adrp_word).unwrap();
        match &adrp_decoded.operands[1] {
            DecodedOperand::PageTarget(page) => assert_eq!(*page, 0x40_1000),
            other => panic!("expected PageTarget, got {other:?}"),
        }

        let add_word = u32::from_le_bytes([
            output.bytes[4],
            output.bytes[5],
            output.bytes[6],
            output.bytes[7],
        ]);
        let add_decoded = aarch64::decode_instruction(0x1004, add_word).unwrap();
        match add_decoded.operands.last().unwrap() {
            DecodedOperand::Immediate(imm) => assert_eq!(*imm, 0x234),
            other => panic!("expected Immediate, got {other:?}"),
        }
    }

    #[test]
    fn full_pipeline_macro_to_extern_data_via_commit() {
        // Build an ELF whose text contains adrp+add to a placeholder
        // page; redirect to an undefined `data_sym`; emit; commit;
        // re-read; verify two AArch64 relocations land in the file.
        use object::write::{
            Object as WriteObject, StandardSection, Symbol as WriteSymbol,
            SymbolSection as WriteSymbolSection,
        };
        use object::{
            Architecture as ObjArch, BinaryFormat as ObjFormat, Endianness, SymbolFlags,
            SymbolKind as ObjSymbolKind, SymbolScope,
        };

        let mut obj = WriteObject::new(ObjFormat::Elf, ObjArch::Aarch64, Endianness::Little);
        let text_id = obj.section_id(StandardSection::Text);
        // Encode adrp+add+ret pointing at an arbitrary placeholder page.
        let mut text = Vec::with_capacity(12);
        let template_adrp = aarch64::InstructionTemplate {
            address: 0,
            mnemonic: Aarch64Mnemonic::Adrp,
            operands: vec![
                DecodedOperand::Register(crate::isa::aarch64::Register {
                    class: crate::isa::aarch64::RegisterClass::X,
                    index: 0,
                }),
                DecodedOperand::PageTarget(0x10000),
            ],
        };
        let template_add = aarch64::InstructionTemplate {
            address: 4,
            mnemonic: Aarch64Mnemonic::Add,
            operands: vec![
                DecodedOperand::Register(crate::isa::aarch64::Register {
                    class: crate::isa::aarch64::RegisterClass::X,
                    index: 0,
                }),
                DecodedOperand::Register(crate::isa::aarch64::Register {
                    class: crate::isa::aarch64::RegisterClass::X,
                    index: 0,
                }),
                DecodedOperand::Immediate(0),
            ],
        };
        text.extend_from_slice(&aarch64::encode_instruction(&template_adrp).unwrap().to_le_bytes());
        text.extend_from_slice(&aarch64::encode_instruction(&template_add).unwrap().to_le_bytes());
        text.extend_from_slice(&0xd65f03c0u32.to_le_bytes()); // ret
        obj.append_section_data(text_id, &text, 4);

        obj.add_symbol(WriteSymbol {
            name: b"data_sym".to_vec(),
            value: 0,
            size: 0,
            kind: ObjSymbolKind::Data,
            scope: SymbolScope::Linkage,
            weak: false,
            section: WriteSymbolSection::Undefined,
            flags: SymbolFlags::None,
        });
        obj.add_symbol(WriteSymbol {
            name: b"main".to_vec(),
            value: 0,
            size: 12,
            kind: ObjSymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: WriteSymbolSection::Section(text_id),
            flags: SymbolFlags::None,
        });
        let initial = obj.write().unwrap();
        let container = Container::from_bytes(&initial).unwrap();

        let data_sym_id = container
            .symbols
            .iter()
            .find(|s| s.name.ends_with("data_sym"))
            .unwrap()
            .id;

        let text_section_id = container.text_sections().next().unwrap().id;
        let text_section = container.section(text_section_id);
        let (base, code) = text_section.for_disassembly().unwrap();
        let instructions = aarch64::disassemble_bytes(base, code).unwrap();
        let cfg = build_cfg(&instructions);
        let mut plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);
        plan.redirect_macro_target(base, Target::Symbol(data_sym_id))
            .unwrap();

        let layout = lay_out(&plan, base, Some(&container)).unwrap();
        let output = emit(&plan, &layout, Some(&container)).unwrap();
        assert_eq!(output.relocations.len(), 2);

        let committed = commit_to_container(&container, text_section_id, output);
        let written = committed.to_bytes().unwrap();
        let reparsed = Container::from_bytes(&written).unwrap();

        let data_sym_in_reparsed = reparsed
            .symbols
            .iter()
            .find(|s| s.name.ends_with("data_sym"))
            .expect("data_sym survived");

        let relevant: Vec<_> = reparsed
            .relocations
            .iter()
            .filter(|r| r.symbol == Some(data_sym_in_reparsed.id))
            .collect();
        let kinds: Vec<RelocationKind> = relevant.iter().map(|r| r.kind).collect();
        assert!(kinds.contains(&RelocationKind::AdrpPage21));
        assert!(kinds.contains(&RelocationKind::AddPageOffset12));
    }
}

// ---- Lift consults container relocations ------------------------------
//
// In an unlinked .o, `bl <symbol>` is encoded as `bl 0` (placeholder)
// plus an `R_AARCH64_CALL26` relocation that names the real target.
// Without consulting the relocation, lift would treat the placeholder
// zero as a real PC-relative branch into the start of the section,
// freezing it through layout and emit. The runtime harness caught this
// — the rewritten binary jumped into its own prologue and hung. Cover
// it here so it can't regress.

mod relocation_lift {
    use super::*;
    use crate::container::{Relocation, RelocationId, RelocationKind};

    /// Build a container with a single text section containing `.text`
    /// bytes, plus one undefined extern symbol "extern_fn" and a
    /// Branch26 relocation pointing at it. The relocation's offset is
    /// caller-supplied so we can target a specific instruction.
    fn container_with_extern_branch(
        text_bytes: Vec<u8>,
        relocation_offset: u64,
    ) -> Container {
        Container {
            format: BinaryFormat::Elf,
            architecture: Architecture::Aarch64,
            sections: vec![Section {
                id: SectionId(0),
                name: ".text".to_string(),
                address: 0,
                size: text_bytes.len() as u64,
                bytes: text_bytes,
                kind: SectionKind::Text,
                align: 4,
                flags: None,
                raw_sh_type: None,
            }],
            symbols: vec![Symbol {
                id: SymbolId(0),
                name: "extern_fn".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: None,
                is_undefined: true,
                flags: None,
            }],
            relocations: vec![Relocation {
                id: RelocationId(0),
                section: SectionId(0),
                offset: relocation_offset,
                kind: RelocationKind::Branch26,
                size: 32,
                addend: 0,
                symbol: Some(SymbolId(0)),
            }],
            file_flags: None,
            dwarf: None,
        }
    }

    #[test]
    fn lift_replaces_placeholder_branch_target_with_relocation_symbol() {
        // `bl 0` (the unlinked placeholder) at offset 0 of .text, plus a
        // Branch26 relocation naming `extern_fn`. After lift the
        // instruction's branch target must be `Target::Symbol`, not
        // `Target::Absolute(0)` — otherwise layout would emit a
        // PC-relative branch to address 0 (= start of .text = the
        // function itself).
        let bl_word = 0x94000000_u32; // bl +0
        let mut text_bytes = Vec::new();
        text_bytes.extend_from_slice(&bl_word.to_le_bytes());
        // Followed by a `ret` so the CFG terminates cleanly.
        text_bytes.extend_from_slice(&0xd65f03c0_u32.to_le_bytes());

        let container = container_with_extern_branch(text_bytes.clone(), /*offset*/ 0);

        let instructions = aarch64::disassemble_bytes(0, &text_bytes).expect("decode");
        let cfg = build_cfg(&instructions);
        let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

        // The `bl` instruction is the first op of the only block.
        let op = &plan.blocks[0].ops[0];
        let insn = match op {
            crate::rewrite::RewriteOp::Instruction(i) => i,
            other => panic!("expected singleton instruction, got {other:?}"),
        };
        assert_eq!(insn.mnemonic, Aarch64Mnemonic::Bl);
        let target = insn
            .pc_relative_target()
            .expect("bl has a PC-relative operand");
        assert_eq!(
            target,
            Target::Symbol(SymbolId(0)),
            "lift must consult the container relocation rather than \
             trusting the placeholder branch displacement",
        );
    }

    #[test]
    fn lift_fuses_adrp_add_via_relocations_into_load_address_macro() {
        // Mirror the clang-emitted shape for `adrp x0, sym ; add x0, x0,
        // #:lo12:sym`: both instructions encode placeholder zero, both
        // carry relocations naming the same symbol. Lift should fuse
        // them into a LoadAddress macro pointing at the symbol — not a
        // pair of singleton instructions resolving the placeholder
        // zeroes through `resolve_address`.
        let adrp_word = 0x90000000_u32; // adrp x0, +0
        let add_word = 0x91000000_u32;  // add x0, x0, #0
        let mut text_bytes = Vec::new();
        text_bytes.extend_from_slice(&adrp_word.to_le_bytes());
        text_bytes.extend_from_slice(&add_word.to_le_bytes());
        text_bytes.extend_from_slice(&0xd65f03c0_u32.to_le_bytes()); // ret

        let container = Container {
            format: BinaryFormat::Elf,
            architecture: Architecture::Aarch64,
            sections: vec![Section {
                id: SectionId(0),
                name: ".text".to_string(),
                address: 0,
                size: text_bytes.len() as u64,
                bytes: text_bytes.clone(),
                kind: SectionKind::Text,
                align: 4,
                flags: None,
                raw_sh_type: None,
            }],
            symbols: vec![Symbol {
                id: SymbolId(0),
                name: "format_str".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Object,
                binding: SymbolBinding::Local,
                section: None,
                is_undefined: true,
                flags: None,
            }],
            relocations: vec![
                Relocation {
                    id: RelocationId(0),
                    section: SectionId(0),
                    offset: 0,
                    kind: RelocationKind::AdrpPage21,
                    size: 32,
                    addend: 0,
                    symbol: Some(SymbolId(0)),
                },
                Relocation {
                    id: RelocationId(1),
                    section: SectionId(0),
                    offset: 4,
                    kind: RelocationKind::AddPageOffset12,
                    size: 32,
                    addend: 0,
                    symbol: Some(SymbolId(0)),
                },
            ],
            file_flags: None,
            dwarf: None,
        };

        let instructions = aarch64::disassemble_bytes(0, &text_bytes).unwrap();
        let cfg = build_cfg(&instructions);
        let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

        // First op of the only block must be a LoadAddress macro
        // pointing at the symbol.
        let op = &plan.blocks[0].ops[0];
        let macro_op = match op {
            crate::rewrite::RewriteOp::Macro(m) => m,
            other => panic!("expected fused macro, got {other:?}"),
        };
        assert_eq!(macro_op.kind, crate::rewrite::MacroKind::LoadAddress);
        assert_eq!(macro_op.target, Target::Symbol(SymbolId(0)));
        assert_eq!(
            macro_op.original_addresses,
            vec![0, 4],
            "macro must remember both component addresses",
        );
    }

    #[test]
    fn lift_fuses_adrp_ldr_via_relocations_into_access_value_macro() {
        // Mirror clang's `adrp x8, sym ; ldr w8, [x8, :lo12:sym]` for
        // loading a global. Both halves carry relocations naming the
        // same data symbol; lift must fuse them into an AccessValue
        // macro, not leave the adrp as a singleton (whose relocation
        // alone is insufficient — the ldr's LoadStorePageOffset12
        // reloc would disappear from emit).
        let adrp_word = 0x90000008_u32; // adrp x8, +0
        let ldr_word = 0xb9400108_u32;  // ldr w8, [x8, #0]
        let mut text_bytes = Vec::new();
        text_bytes.extend_from_slice(&adrp_word.to_le_bytes());
        text_bytes.extend_from_slice(&ldr_word.to_le_bytes());
        text_bytes.extend_from_slice(&0xd65f03c0_u32.to_le_bytes()); // ret

        let container = Container {
            format: BinaryFormat::Elf,
            architecture: Architecture::Aarch64,
            sections: vec![Section {
                id: SectionId(0),
                name: ".text".to_string(),
                address: 0,
                size: text_bytes.len() as u64,
                bytes: text_bytes.clone(),
                kind: SectionKind::Text,
                align: 4,
                flags: None,
                raw_sh_type: None,
            }],
            symbols: vec![Symbol {
                id: SymbolId(0),
                name: "global_var".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Object,
                binding: SymbolBinding::Global,
                section: None,
                is_undefined: true,
                flags: None,
            }],
            relocations: vec![
                Relocation {
                    id: RelocationId(0),
                    section: SectionId(0),
                    offset: 0,
                    kind: RelocationKind::AdrpPage21,
                    size: 32,
                    addend: 0,
                    symbol: Some(SymbolId(0)),
                },
                Relocation {
                    id: RelocationId(1),
                    section: SectionId(0),
                    offset: 4,
                    kind: RelocationKind::LoadStorePageOffset12 {
                        access_width_bytes: 4,
                    },
                    size: 32,
                    addend: 0,
                    symbol: Some(SymbolId(0)),
                },
            ],
            file_flags: None,
            dwarf: None,
        };

        let instructions = aarch64::disassemble_bytes(0, &text_bytes).unwrap();
        let cfg = build_cfg(&instructions);
        let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

        let op = &plan.blocks[0].ops[0];
        let macro_op = match op {
            crate::rewrite::RewriteOp::Macro(m) => m,
            other => panic!("expected fused AccessValue macro, got {other:?}"),
        };
        assert_eq!(macro_op.kind, crate::rewrite::MacroKind::AccessValue);
        assert_eq!(macro_op.target, Target::Symbol(SymbolId(0)));
        assert_eq!(
            macro_op.original_instructions[1].mnemonic,
            Aarch64Mnemonic::Ldr,
        );

        // No-op rewrite emits both relocations.
        use crate::rewrite::{emit, lay_out};
        let layout = lay_out(&plan, 0, Some(&container)).unwrap();
        let output = emit(&plan, &layout, Some(&container)).unwrap();
        let kinds: Vec<RelocationKind> =
            output.relocations.iter().map(|r| r.kind).collect();
        assert!(
            kinds.contains(&RelocationKind::AdrpPage21)
                && kinds.contains(&RelocationKind::LoadStorePageOffset12 {
                    access_width_bytes: 4,
                }),
            "AccessValue emit must produce AdrpPage21 + LoadStorePageOffset12 \
             with access_width=4 for ldr w8, got {kinds:?}",
        );
    }

    #[test]
    fn macro_with_section_symbol_target_emits_relocations_not_folded_address() {
        // Section symbols (STT_SECTION) have address=0 in unlinked input,
        // but their final address depends on linker placement. Emit must
        // produce relocations rather than fold the placeholder zero.
        // Without this, `adrp x0, .rodata.str1.1 ; add x0, x0, :lo12:...`
        // round-trips to "load the address 0," which made the runtime
        // fixture printf the ELF magic.
        use crate::rewrite::{emit, lay_out};

        let adrp_word = 0x90000000_u32;
        let add_word = 0x91000000_u32;
        let mut text_bytes = Vec::new();
        text_bytes.extend_from_slice(&adrp_word.to_le_bytes());
        text_bytes.extend_from_slice(&add_word.to_le_bytes());
        text_bytes.extend_from_slice(&0xd65f03c0_u32.to_le_bytes()); // ret

        // Section symbol "" pointing at section id 1 (.rodata). Defined
        // (`is_undefined: false`), kind=Section, address 0.
        let container = Container {
            format: BinaryFormat::Elf,
            architecture: Architecture::Aarch64,
            sections: vec![
                Section {
                    id: SectionId(0),
                    name: ".text".to_string(),
                    address: 0,
                    size: text_bytes.len() as u64,
                    bytes: text_bytes.clone(),
                    kind: SectionKind::Text,
                    align: 4,
                    flags: None,
                    raw_sh_type: None,
                },
                Section {
                    id: SectionId(1),
                    name: ".rodata.str1.1".to_string(),
                    address: 0,
                    size: 16,
                    bytes: vec![0; 16],
                    kind: SectionKind::Rodata,
                    align: 1,
                    flags: None,
                    raw_sh_type: None,
                },
            ],
            symbols: vec![Symbol {
                id: SymbolId(0),
                name: "".to_string(),
                address: 0,
                size: 0,
                kind: SymbolKind::Section,
                binding: SymbolBinding::Local,
                section: Some(SectionId(1)),
                is_undefined: false,
                flags: None,
            }],
            relocations: vec![
                Relocation {
                    id: RelocationId(0),
                    section: SectionId(0),
                    offset: 0,
                    kind: RelocationKind::AdrpPage21,
                    size: 32,
                    addend: 0,
                    symbol: Some(SymbolId(0)),
                },
                Relocation {
                    id: RelocationId(1),
                    section: SectionId(0),
                    offset: 4,
                    kind: RelocationKind::AddPageOffset12,
                    size: 32,
                    addend: 0,
                    symbol: Some(SymbolId(0)),
                },
            ],
            file_flags: None,
            dwarf: None,
        };

        let instructions = aarch64::disassemble_bytes(0, &text_bytes).unwrap();
        let cfg = build_cfg(&instructions);
        let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);
        let layout = lay_out(&plan, 0, Some(&container)).unwrap();
        let output = emit(&plan, &layout, Some(&container)).unwrap();

        // Both halves of the macro must produce relocations.
        let kinds: Vec<RelocationKind> = output
            .relocations
            .iter()
            .map(|r| r.kind)
            .collect();
        assert!(
            kinds.contains(&RelocationKind::AdrpPage21)
                && kinds.contains(&RelocationKind::AddPageOffset12),
            "section-symbol macro must emit both relocations, got {kinds:?}",
        );
    }

    #[test]
    fn lift_emits_relocation_when_no_op_rewrite_round_trips_extern_call() {
        // End-to-end: read → lift → layout → emit, no edits. The output
        // must carry a Branch26 relocation, not a literal displacement.
        // Mirrors what the ELF runtime harness does, minus Docker/QEMU.
        use crate::rewrite::{emit, lay_out};

        let bl_word = 0x94000000_u32;
        let mut text_bytes = Vec::new();
        text_bytes.extend_from_slice(&bl_word.to_le_bytes());
        text_bytes.extend_from_slice(&0xd65f03c0_u32.to_le_bytes());

        let container = container_with_extern_branch(text_bytes.clone(), 0);

        let instructions = aarch64::disassemble_bytes(0, &text_bytes).unwrap();
        let cfg = build_cfg(&instructions);
        let plan = RewritePlan::lift_with_container(&cfg, &instructions, &container);

        let layout = lay_out(&plan, 0, Some(&container)).unwrap();
        let output = emit(&plan, &layout, Some(&container)).unwrap();

        // Output must include a Branch26 reloc at offset 0 pointing at
        // the extern symbol — proves emit didn't fold the symbol into a
        // displacement.
        assert!(
            output
                .relocations
                .iter()
                .any(|r| r.offset == 0 && r.kind == RelocationKind::Branch26),
            "expected a Branch26 relocation at offset 0; got {:?}",
            output.relocations,
        );

        // The placeholder word emitted at offset 0 should encode `bl +0`
        // — the linker fills in the real displacement via the
        // relocation. If lift had frozen the placeholder zero into a
        // real branch, the encoded word would still be `bl +0` here too,
        // but no relocation would accompany it; the relocation
        // assertion above is what distinguishes the two cases.
        let word = u32::from_le_bytes([
            output.bytes[0],
            output.bytes[1],
            output.bytes[2],
            output.bytes[3],
        ]);
        assert_eq!(word, bl_word, "placeholder word must be bl +0");
    }
}
