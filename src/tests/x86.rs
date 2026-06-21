//! Tests for the x86 / x86_64 ISA layer and its container integration.
//!
//! Decode fixtures are hand-encoded instruction bytes (no external
//! assembler); container fixtures are synthesized in-memory via
//! `object::write`, matching the rest of the suite's hermetic style.

use crate::container::{Architecture, BinaryFormat, Container, RelocationKind};
use crate::isa::x86::{
    assemble, disassemble_bytes, encode_instruction, Bitness, X86DecodedInstruction,
};
use crate::mc::{build_cfg, ControlFlow, InstructionInfo};
use iced_x86::{Instruction, Mnemonic};

use object::write::{
    Object as WriteObject, Relocation as WriteRelocation, StandardSection, Symbol as WriteSymbol,
    SymbolSection as WriteSymbolSection,
};
use object::{
    Architecture as ObjArch, BinaryFormat as ObjFormat, Endianness, SymbolFlags,
    SymbolKind as ObjSymbolKind, SymbolScope,
};

// ---- decode / sweep ------------------------------------------------------

fn decode64(bytes: &[u8]) -> Vec<X86DecodedInstruction> {
    disassemble_bytes(0x1000, bytes, Bitness::Bits64).expect("x86_64 sweep")
}

#[test]
fn decodes_single_byte_nop_and_ret() {
    let insns = decode64(&[0x90, 0xc3]);
    assert_eq!(insns.len(), 2);
    assert_eq!(insns[0].address, 0x1000);
    assert_eq!(insns[0].size_bytes(), 1);
    assert_eq!(insns[0].mnemonic(), Mnemonic::Nop);
    assert_eq!(insns[1].address, 0x1001);
    assert_eq!(insns[1].mnemonic(), Mnemonic::Ret);
}

#[test]
fn variable_length_addresses_are_contiguous() {
    // add rax, rbx (3) ; nop (1) ; ret (1)
    let insns = decode64(&[0x48, 0x01, 0xd8, 0x90, 0xc3]);
    let addrs: Vec<u64> = insns.iter().map(|i| i.address).collect();
    assert_eq!(addrs, vec![0x1000, 0x1003, 0x1004]);
    assert_eq!(insns[0].size_bytes(), 3);
}

#[test]
fn rejects_invalid_bytes() {
    // 0x06 (push es) is invalid in 64-bit mode.
    let err = disassemble_bytes(0x2000, &[0x06], Bitness::Bits64).unwrap_err();
    match err {
        crate::isa::x86::DisassembleError::DecodeFailed { address, .. } => {
            assert_eq!(address, 0x2000);
        }
    }
}

#[test]
fn i386_decodes_in_32bit_mode() {
    // 0x06 (push es) is *valid* in 32-bit mode — confirms bitness routing.
    let insns = disassemble_bytes(0x0, &[0x06], Bitness::Bits32).expect("i386 sweep");
    assert_eq!(insns.len(), 1);
    assert_eq!(insns[0].mnemonic(), Mnemonic::Push);
}

// ---- control-flow classification -----------------------------------------

#[test]
fn classifies_direct_call_target_and_fallthrough() {
    // call rel32 (+0): e8 00 00 00 00 at 0x1000, len 5 -> target 0x1005.
    let insns = decode64(&[0xe8, 0x00, 0x00, 0x00, 0x00]);
    match insns[0].control_flow() {
        ControlFlow::Call {
            target,
            fallthrough,
        } => {
            assert_eq!(target, 0x1005);
            assert_eq!(fallthrough, 0x1005);
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn classifies_conditional_jump() {
    // je rel8 (+1): 74 01 at 0x1000, len 2 -> next ip 0x1002, target 0x1003.
    let insns = decode64(&[0x74, 0x01]);
    match insns[0].control_flow() {
        ControlFlow::ConditionalJump {
            target,
            fallthrough,
        } => {
            assert_eq!(target, 0x1003);
            assert_eq!(fallthrough, 0x1002);
        }
        other => panic!("expected ConditionalJump, got {other:?}"),
    }
}

#[test]
fn classifies_unconditional_jump_and_indirect_and_return_and_trap() {
    // jmp rel8: eb 00 -> Jump
    assert!(matches!(
        decode64(&[0xeb, 0x00])[0].control_flow(),
        ControlFlow::Jump { target } if target == 0x1002
    ));
    // jmp rax: ff e0 -> IndirectJump
    assert!(matches!(
        decode64(&[0xff, 0xe0])[0].control_flow(),
        ControlFlow::IndirectJump
    ));
    // ret: c3 -> Return
    assert!(matches!(
        decode64(&[0xc3])[0].control_flow(),
        ControlFlow::Return
    ));
    // int3: cc -> Trap
    assert!(matches!(
        decode64(&[0xcc])[0].control_flow(),
        ControlFlow::Trap
    ));
}

// ---- CFG construction ----------------------------------------------------

#[test]
fn builds_cfg_with_conditional_split() {
    // 0x1000: cmp edi, 0    83 ff 00      (3)
    // 0x1003: je  0x1006    74 01         (2)  taken -> 0x1006, fall -> 0x1005
    // 0x1005: nop           90            (1)  falls to 0x1006
    // 0x1006: ret           c3            (1)
    let bytes = [0x83, 0xff, 0x00, 0x74, 0x01, 0x90, 0xc3];
    let insns = decode64(&bytes);
    assert_eq!(insns.len(), 4);
    let cfg = build_cfg(&insns);
    // Three blocks: [cmp,je], [nop], [ret].
    assert_eq!(cfg.blocks.len(), 3, "expected a 3-block diamond tail");
    // The conditional-jump block has two successors.
    let head = cfg.blocks.iter().find(|b| b.start == 0x1000).unwrap();
    assert_eq!(head.successors.len(), 2);
}

// ---- encode / assemble ---------------------------------------------------

#[test]
fn single_instruction_round_trips_through_encoder() {
    // add rax, rbx (48 01 d8) re-encodes byte-identically at its address.
    let insns = decode64(&[0x48, 0x01, 0xd8]);
    let bytes = encode_instruction(&insns[0].instr, 0x1000, Bitness::Bits64).expect("encode");
    assert_eq!(bytes, vec![0x48, 0x01, 0xd8]);
}

#[test]
fn rip_relative_reencodes_to_same_target_when_moved() {
    // lea rax, [rip+0]  (48 8d 05 00 00 00 00) at 0x1000 references
    // absolute 0x1007. Re-encoding at a new address must keep the same
    // absolute target, so the displacement changes.
    let insns =
        disassemble_bytes(0x1000, &[0x48, 0x8d, 0x05, 0, 0, 0, 0], Bitness::Bits64).expect("sweep");
    let original_target = insns[0].instr.memory_displacement64();
    assert_eq!(original_target, 0x1007);

    let moved = encode_instruction(&insns[0].instr, 0x2000, Bitness::Bits64).expect("encode");
    let redecoded = disassemble_bytes(0x2000, &moved, Bitness::Bits64).expect("sweep");
    assert_eq!(
        redecoded[0].instr.memory_displacement64(),
        0x1007,
        "RIP-relative target must survive a move"
    );
}

#[test]
fn assemble_fixes_up_intra_block_branch() {
    // je +2 ; nop ; nop ; ret  — je targets the ret at 0x1004.
    let insns = decode64(&[0x74, 0x02, 0x90, 0x90, 0xc3]);
    let instrs: Vec<Instruction> = insns.iter().map(|i| i.instr).collect();
    let out = assemble(&instrs, 0x4000, Bitness::Bits64).expect("assemble");
    // Re-decode and confirm the je now targets the ret at its new address.
    let redecoded = disassemble_bytes(out.base_address, &out.bytes, Bitness::Bits64).unwrap();
    let je = &redecoded[0];
    assert_eq!(je.mnemonic(), Mnemonic::Je);
    // ret is the last instruction; its address is where je should point.
    let ret_addr = redecoded.last().unwrap().address;
    assert_eq!(je.instr.near_branch_target(), ret_addr);
}

#[test]
fn assemble_widens_out_of_range_short_branch() {
    // je (rel8) ; 200 nops ; ret. Retarget the je at the far ret so the
    // displacement (~202) exceeds rel8 range — BlockEncoder must promote
    // it to the 6-byte rel32 form. This is the case the fixed-width
    // widening machinery can't express for x86.
    let mut bytes = vec![0x74, 0x00]; // je +0 (placeholder target)
    bytes.extend(std::iter::repeat(0x90).take(200)); // 200 nops
    bytes.push(0xc3); // ret
    let insns = disassemble_bytes(0x1000, &bytes, Bitness::Bits64).expect("sweep");
    let ret_addr = insns.last().unwrap().address; // 0x1000 + 202

    let mut instrs: Vec<Instruction> = insns.iter().map(|i| i.instr).collect();
    instrs[0].set_near_branch64(ret_addr); // retarget je at the far ret

    let out = assemble(&instrs, 0x1000, Bitness::Bits64).expect("assemble");
    // The je grew from 2 to 6 bytes, so the first nop now sits at +6.
    assert_eq!(
        out.instruction_offsets[1], 6,
        "je should widen to the 6-byte rel32 form"
    );
    // And it must still reach the (now shifted) ret.
    let redecoded = disassemble_bytes(out.base_address, &out.bytes, Bitness::Bits64).unwrap();
    let new_ret_addr = redecoded.last().unwrap().address;
    assert_eq!(redecoded[0].instr.near_branch_target(), new_ret_addr);
    assert_eq!(redecoded[0].mnemonic(), Mnemonic::Je);
}

// ---- container read: x86_64 ELF ------------------------------------------

fn x86_64_text_ret_obj(format: ObjFormat) -> Vec<u8> {
    let mut obj = WriteObject::new(format, ObjArch::X86_64, Endianness::Little);
    let text_id = obj.section_id(StandardSection::Text);
    let offset = obj.append_section_data(text_id, &[0xc3], 1); // ret
    obj.add_symbol(WriteSymbol {
        name: b"main".to_vec(),
        value: offset,
        size: 1,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Section(text_id),
        flags: SymbolFlags::None,
    });
    obj.write().expect("write x86_64 object")
}

#[test]
fn parses_x86_64_elf_object() {
    let bytes = x86_64_text_ret_obj(ObjFormat::Elf);
    let container = Container::from_bytes(&bytes).expect("parse x86_64 elf");
    assert_eq!(container.format, BinaryFormat::Elf);
    assert_eq!(container.architecture, Architecture::X86_64);
    let text = container.text_sections().next().expect("text section");
    assert_eq!(text.bytes, vec![0xc3]);
}

#[test]
fn parses_x86_64_coff_object_as_pe() {
    let bytes = x86_64_text_ret_obj(ObjFormat::Coff);
    let container = Container::from_bytes(&bytes).expect("parse x86_64 coff");
    assert_eq!(container.format, BinaryFormat::Pe);
    assert_eq!(container.architecture, Architecture::X86_64);
    assert_eq!(container.kind, crate::container::ContainerKind::Relocatable);
}

#[test]
fn maps_x86_64_elf_pc32_relocation() {
    // A `call rel32` (e8 + 4 zero bytes) with a PC32 relocation at the
    // displacement, targeting an undefined extern.
    let mut obj = WriteObject::new(ObjFormat::Elf, ObjArch::X86_64, Endianness::Little);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &[0xe8, 0, 0, 0, 0], 1);
    let target = obj.add_symbol(WriteSymbol {
        name: b"extern_fn".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });
    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 1,
            symbol: target,
            addend: -4,
            flags: object::RelocationFlags::Elf {
                r_type: object::elf::R_X86_64_PLT32,
            },
        },
    )
    .unwrap();
    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).expect("parse");
    let kinds: Vec<RelocationKind> = container.relocations.iter().map(|r| r.kind).collect();
    assert!(
        kinds.contains(&RelocationKind::X86Plt32),
        "expected X86Plt32, got {kinds:?}"
    );
}

#[test]
fn x86_64_coff_relocation_round_trips_through_writer() {
    // Build an x86_64 COFF object with a REL32 relocation, parse it,
    // write it back (exercising coff_flags), and confirm the relocation
    // survives as X86Pc32.
    let mut obj = WriteObject::new(ObjFormat::Coff, ObjArch::X86_64, Endianness::Little);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &[0xe8, 0, 0, 0, 0], 1); // call rel32
    let target = obj.add_symbol(WriteSymbol {
        name: b"extern_fn".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });
    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 1,
            symbol: target,
            addend: 0,
            flags: object::RelocationFlags::Coff {
                typ: object::pe::IMAGE_REL_AMD64_REL32,
            },
        },
    )
    .unwrap();
    let bytes = obj.write().unwrap();

    let container = Container::from_bytes(&bytes).expect("parse coff");
    assert_eq!(container.format, BinaryFormat::Pe);
    let written = container.to_bytes().expect("write coff back");
    let reparsed = Container::from_bytes(&written).expect("reparse coff");
    let kinds: Vec<RelocationKind> = reparsed.relocations.iter().map(|r| r.kind).collect();
    assert!(
        kinds.contains(&RelocationKind::X86Pc32),
        "REL32 should survive round-trip, got {kinds:?}"
    );
}

// ---- editor integration --------------------------------------------------

#[test]
fn editor_redirects_x86_branch_and_round_trips() {
    use crate::rewrite::BinaryEditor;

    // .text: jmp +1 (eb 01) skips the int3 and lands on ret.
    //   0x0: eb 01   jmp 0x3
    //   0x2: cc      int3
    //   0x3: c3      ret
    let mut obj = WriteObject::new(ObjFormat::Elf, ObjArch::X86_64, Endianness::Little);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &[0xeb, 0x01, 0xcc, 0xc3], 1);
    obj.add_symbol(WriteSymbol {
        name: b"main".to_vec(),
        value: 0,
        size: 4,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Section(text_id),
        flags: SymbolFlags::None,
    });
    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).unwrap();

    // Sanity: the jmp at 0x0 targets 0x3 initially.
    let mut editor = BinaryEditor::new(&container).unwrap();
    editor.lift_text_section(".text").unwrap();
    {
        let x86 = editor.text.as_ref().unwrap().x86().unwrap();
        assert_eq!(x86.instructions()[0].near_branch_target(), 0x3);
    }

    // Redirect the jmp to target 0x0 (itself) instead.
    editor
        .text
        .as_mut()
        .unwrap()
        .x86_mut()
        .unwrap()
        .redirect_branch_at(0x0, 0x0)
        .unwrap();

    let new_bytes = editor.commit_to_bytes().unwrap();
    let reparsed = Container::from_bytes(&new_bytes).unwrap();
    let text = reparsed.text_sections().next().unwrap();
    let (base, code) = text.for_disassembly().unwrap();
    let insns = disassemble_bytes(base, code, Bitness::Bits64).unwrap();
    // The first instruction is still a jmp, now targeting the section base.
    assert_eq!(insns[0].mnemonic(), Mnemonic::Jmp);
    assert_eq!(insns[0].instr.near_branch_target(), base);
}

#[test]
fn relocates_x86_function_through_generic_pipeline() {
    use crate::isa::x86::{X86Isa, X86Operand};
    use crate::rewrite::{emit, lay_out, plan::RewritePlan};

    // .text @ base: je +1 (74 01) skips the nop and lands on ret.
    //   +0: 74 01   je +0x3
    //   +2: 90      nop
    //   +3: c3      ret
    let mut obj = WriteObject::new(ObjFormat::Elf, ObjArch::X86_64, Endianness::Little);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &[0x74, 0x01, 0x90, 0xc3], 1);
    obj.add_symbol(WriteSymbol {
        name: b"f".to_vec(),
        value: 0,
        size: 4,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Section(text_id),
        flags: SymbolFlags::None,
    });
    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).unwrap();
    let text = container.text_sections().next().unwrap();
    let (base, code) = text.for_disassembly().unwrap();

    let insns = disassemble_bytes(base, code, Bitness::Bits64).unwrap();
    // Item 1: the je's branch target is surfaced as a crate-neutral operand.
    assert_eq!(insns[0].mnemonic(), Mnemonic::Je);
    assert_eq!(insns[0].operands, vec![X86Operand::Branch(base + 3)]);

    // Lift → relocate the whole function to a fresh address → emit.
    let cfg = build_cfg(&insns);
    let plan = RewritePlan::<X86Isa>::lift_from_decoded_with_container(&cfg, &insns, &container);
    let new_base = base + 0x4_0000;
    let layout = lay_out::<X86Isa>(&plan, new_base, Some(&container)).unwrap();
    let out = emit::<X86Isa>(&plan, &layout, Some(&container)).unwrap();

    // Re-decode the relocated bytes and check the branch was retargeted to the
    // relocated `ret` (item 1 + generic lift/layout/emit + item 5 jcc rel32), and
    // the nop survived verbatim.
    let relocated = disassemble_bytes(new_base, &out.bytes, Bitness::Bits64).unwrap();
    assert_eq!(relocated[0].mnemonic(), Mnemonic::Je);
    let ret = relocated.last().unwrap();
    assert_eq!(ret.mnemonic(), Mnemonic::Ret);
    assert_eq!(relocated[0].instr.near_branch_target(), ret.address);
    assert!(relocated.iter().any(|i| i.mnemonic() == Mnemonic::Nop));
}

#[test]
fn maps_i386_elf_pc32_relocation_distinctly() {
    // R_386_PC32 (== 1 in some other ABI's space) must map to X86Pc32
    // on a 32-bit container, not collide with x86_64 codes.
    let mut obj = WriteObject::new(ObjFormat::Elf, ObjArch::I386, Endianness::Little);
    let text_id = obj.section_id(StandardSection::Text);
    obj.append_section_data(text_id, &[0xe8, 0, 0, 0, 0], 1);
    let target = obj.add_symbol(WriteSymbol {
        name: b"extern_fn".to_vec(),
        value: 0,
        size: 0,
        kind: ObjSymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: WriteSymbolSection::Undefined,
        flags: SymbolFlags::None,
    });
    obj.add_relocation(
        text_id,
        WriteRelocation {
            offset: 1,
            symbol: target,
            addend: -4,
            flags: object::RelocationFlags::Elf {
                r_type: object::elf::R_386_PC32,
            },
        },
    )
    .unwrap();
    let bytes = obj.write().unwrap();
    let container = Container::from_bytes(&bytes).expect("parse");
    assert_eq!(container.architecture, Architecture::X86);
    let kinds: Vec<RelocationKind> = container.relocations.iter().map(|r| r.kind).collect();
    assert!(
        kinds.contains(&RelocationKind::X86Pc32),
        "expected X86Pc32, got {kinds:?}"
    );
}
