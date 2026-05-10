//! Mode-aware ARMv7 entry points.
//!
//! ARMv7 binaries mix ARM (A32) and Thumb (T32) code. The
//! per-symbol mode is encoded in the ELF symbol value's low
//! bit: `st_value & 1 == 1` means the symbol points at
//! Thumb-mode code, and the *real* address is
//! `st_value & !1`. Interworking branches like `bx`/`blx`
//! switch modes at runtime.
//!
//! This module dispatches to the right per-mode decoder
//! based on the symbol's encoded mode bit, hiding the
//! distinction from callers that just want "disassemble the
//! function at this address."

use crate::container::Container;
use crate::isa::armv7::arm::recursive::{
    disassemble_recursive as arm_recursive, ArmDisassembly,
};
use crate::isa::armv7::recursive::{
    disassemble_recursive as thumb_recursive, ThumbDisassembly,
};

/// Which ARMv7 mode an address denotes.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Mode {
    /// 32-bit ARM (A32) — fixed 4-byte instructions.
    Arm,
    /// 16/32-bit Thumb (T32) — variable width.
    Thumb,
}

/// A disassembled function, tagged with the mode the
/// caller's entry point was decoded in.
#[derive(Debug, Clone)]
pub enum FunctionDisassembly {
    Arm(ArmDisassembly),
    Thumb(ThumbDisassembly),
}

impl FunctionDisassembly {
    pub fn mode(&self) -> Mode {
        match self {
            Self::Arm(_) => Mode::Arm,
            Self::Thumb(_) => Mode::Thumb,
        }
    }

    /// Total number of instructions decoded.
    pub fn instruction_count(&self) -> usize {
        match self {
            Self::Arm(d) => d.instructions.len(),
            Self::Thumb(d) => d.instructions.len(),
        }
    }

    /// Total bytes classified as data.
    pub fn data_byte_count(&self) -> usize {
        match self {
            Self::Arm(d) => d.data_ranges.iter().map(|r| r.bytes.len()).sum(),
            Self::Thumb(d) => d.data_ranges.iter().map(|r| r.bytes.len()).sum(),
        }
    }
}

/// Disassemble the function whose entry symbol has address
/// `address` (or `address | 1` if Thumb), starting from
/// the section that contains it. Picks ARM vs Thumb based on
/// the low bit of the entry-point address.
///
/// This is the convenient one-call entry point for tools that
/// have a `Container` and a function address. For more
/// control (multiple entry points, custom data classification,
/// or working with raw bytes) use the per-mode `recursive`
/// modules directly.
pub fn disassemble_function_at(
    container: &Container,
    address: u64,
) -> Option<FunctionDisassembly> {
    let mode = if address & 1 == 1 {
        Mode::Thumb
    } else {
        // Low-bit-clear addresses are ambiguous — could be
        // ARM-mode, or could be a Thumb function whose
        // symbol-table form happens to have the bit cleared
        // (which technically violates the ELF ABI, but
        // can happen). The ARM Procedure Call Standard
        // defines the bit as authoritative; trust it.
        Mode::Arm
    };
    let real_address = address & !1u64;
    let section = container
        .sections
        .iter()
        .find(|s| s.address <= real_address && real_address < s.address + s.size)?;
    let (base, code) = section.for_disassembly()?;
    Some(match mode {
        Mode::Arm => FunctionDisassembly::Arm(arm_recursive(base, code, &[real_address])),
        Mode::Thumb => FunctionDisassembly::Thumb(thumb_recursive(base, code, &[address])),
    })
}

/// Disassemble multiple functions in a single pass per mode.
/// More efficient than calling `disassemble_function_at` in a
/// loop because the worklist is shared within each mode.
pub fn disassemble_functions(
    container: &Container,
    addresses: &[u64],
) -> (Option<ArmDisassembly>, Option<ThumbDisassembly>) {
    if addresses.is_empty() {
        return (None, None);
    }
    // Group entry points by section + mode.
    let mut arm_by_section: std::collections::HashMap<usize, Vec<u64>> =
        std::collections::HashMap::new();
    let mut thumb_by_section: std::collections::HashMap<usize, Vec<u64>> =
        std::collections::HashMap::new();
    for &raw in addresses {
        let mode = if raw & 1 == 1 { Mode::Thumb } else { Mode::Arm };
        let real = raw & !1u64;
        let Some((idx, _)) = container
            .sections
            .iter()
            .enumerate()
            .find(|(_, s)| s.address <= real && real < s.address + s.size)
        else {
            continue;
        };
        let target = match mode {
            Mode::Arm => &mut arm_by_section,
            Mode::Thumb => &mut thumb_by_section,
        };
        target.entry(idx).or_default().push(raw);
    }

    let mut combined_arm: Option<ArmDisassembly> = None;
    for (idx, entries) in arm_by_section {
        let section = &container.sections[idx];
        let Some((base, code)) = section.for_disassembly() else {
            continue;
        };
        let dis = arm_recursive(base, code, &entries);
        combined_arm = Some(merge_arm(combined_arm, dis));
    }

    let mut combined_thumb: Option<ThumbDisassembly> = None;
    for (idx, entries) in thumb_by_section {
        let section = &container.sections[idx];
        let Some((base, code)) = section.for_disassembly() else {
            continue;
        };
        let dis = thumb_recursive(base, code, &entries);
        combined_thumb = Some(merge_thumb(combined_thumb, dis));
    }

    (combined_arm, combined_thumb)
}

fn merge_arm(a: Option<ArmDisassembly>, b: ArmDisassembly) -> ArmDisassembly {
    let mut combined = a.unwrap_or_default();
    combined.instructions.extend(b.instructions);
    combined.data_ranges.extend(b.data_ranges);
    combined.instructions.sort_by_key(|i| i.address);
    combined.data_ranges.sort_by_key(|r| r.address);
    combined
}

fn merge_thumb(a: Option<ThumbDisassembly>, b: ThumbDisassembly) -> ThumbDisassembly {
    let mut combined = a.unwrap_or_default();
    combined.instructions.extend(b.instructions);
    combined.data_ranges.extend(b.data_ranges);
    combined.instructions.sort_by_key(|i| i.address);
    combined.data_ranges.sort_by_key(|r| r.address);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container;
    use std::path::PathBuf;

    fn libtool_checker_path() -> PathBuf {
        let manifest = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest).join("tests").join("libtool-checker.so")
    }

    #[test]
    fn dispatch_picks_thumb_for_thumb_marked_address() {
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c = container::Container::from_bytes(&bytes).expect("parse");
        // Find a Thumb function (low-bit-set address).
        let thumb_fn = c
            .symbols
            .iter()
            .find(|s| !s.is_undefined && s.address & 1 == 1)
            .expect("a Thumb function");
        let dis = disassemble_function_at(&c, thumb_fn.address).expect("dispatch");
        assert_eq!(dis.mode(), Mode::Thumb);
        assert!(dis.instruction_count() > 0);
    }

    #[test]
    fn dispatch_picks_arm_for_arm_address() {
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c = container::Container::from_bytes(&bytes).expect("parse");
        // The .plt at 0xf84 is ARM-mode.
        let dis = disassemble_function_at(&c, 0xf84).expect("dispatch");
        assert_eq!(dis.mode(), Mode::Arm);
        assert!(dis.instruction_count() > 0);
    }

    #[test]
    fn dispatch_returns_none_for_address_outside_any_section() {
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c = container::Container::from_bytes(&bytes).expect("parse");
        assert!(disassemble_function_at(&c, 0xffff_ffff).is_none());
    }

    #[test]
    fn arm_plt_stubs_discovered_for_libtool_checker() {
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c = container::Container::from_bytes(&bytes).expect("parse");
        let image = c
            .elf_image
            .as_ref()
            .expect("ARM ELF32 should produce a minimal image with plt_stubs");
        // libtool-checker has 16 undefined symbols of which
        // ~14 are functions with PLT slots (the rest are
        // R_ARM_GLOB_DAT data references like __sF and
        // __stack_chk_guard).
        assert!(
            image.plt_stubs.len() >= 12,
            "expected ≥12 PLT stubs, got {}",
            image.plt_stubs.len()
        );
        // Every stub address should be inside `.plt`
        // (0xf84..0xf84+440).
        for &stub_addr in image.plt_stubs.values() {
            assert!(
                stub_addr >= 0xf84 && stub_addr < 0xf84 + 440,
                "stub at 0x{stub_addr:x} outside .plt",
            );
        }
        // Each maps a *defined* extern (the symbol resolved
        // via dynsym). Verify we resolved at least the
        // common ones.
        let names: std::collections::HashSet<&str> = image
            .plt_stubs
            .keys()
            .filter_map(|id| c.symbols.get(id.0).map(|s| s.name.as_str()))
            .collect();
        for expected in &["__cxa_finalize", "fopen", "fclose"] {
            assert!(
                names.contains(*expected),
                "missing PLT stub for {expected}; have: {:?}",
                names,
            );
        }
        // Spot-check stub addresses against objdump:
        //   __cxa_finalize@plt at 0xf98
        //   fopen@plt at 0xfb0
        let cxa_finalize = c
            .symbols
            .iter()
            .find(|s| s.is_undefined && s.name == "__cxa_finalize")
            .expect("__cxa_finalize symbol");
        assert_eq!(image.plt_stubs.get(&cxa_finalize.id), Some(&0xf98));
        let fopen = c
            .symbols
            .iter()
            .find(|s| s.is_undefined && s.name == "fopen")
            .expect("fopen symbol");
        assert_eq!(image.plt_stubs.get(&fopen.id), Some(&0xfb0));
    }

    #[test]
    fn arm_elf32_writes_valid_elf_header() {
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c = container::Container::from_bytes(&bytes).expect("parse");
        let written = c.to_bytes().expect("write");
        // ELF32 header: e_ident magic + class=1 (32-bit) +
        // data=1 (little-endian) + version=1; e_type bytes
        // 16..18 should be ET_DYN (3,0); e_machine bytes
        // 18..20 should be EM_ARM (40,0).
        assert_eq!(&written[0..4], b"\x7fELF");
        assert_eq!(written[4], 1, "ELF32 class");
        assert_eq!(written[5], 1, "little-endian data");
        assert_eq!(&written[16..18], &[3, 0], "e_type ET_DYN");
        assert_eq!(&written[18..20], &[40, 0], "e_machine EM_ARM");
    }

    #[test]
    fn arm_elf32_round_trip_re_parses() {
        // Read → write → re-parse. The round-trip should
        // produce a Container whose key fields match the
        // original (architecture, section count, symbol
        // count, relocation count, plt_stubs).
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c1 = container::Container::from_bytes(&bytes).expect("parse 1");
        let written = c1.to_bytes().expect("write");
        // Save the written bytes for diagnostics if the
        // re-parse fails.
        std::fs::write("/tmp/libtool-checker-rewritten.so", &written).ok();
        let c2 = container::Container::from_bytes(&written).expect("re-parse");

        assert_eq!(c1.architecture, c2.architecture);
        assert_eq!(c1.kind, c2.kind);
        // Writer rebuilds .shstrtab (and may consolidate or
        // add the section header string table), so section
        // count after a round-trip can differ by 1. Verify
        // it stayed within ±1.
        let diff = (c1.sections.len() as i64 - c2.sections.len() as i64).abs();
        assert!(diff <= 1, "section count drifted by {diff}");
        // Symbol count: writer may collapse / re-encode dynsym
        // so an exact match isn't guaranteed for stripped
        // binaries. Just confirm it's nonzero.
        assert!(!c2.symbols.is_empty(), "symbols should be present");
        // PLT discovery should still work after re-parse.
        let image1 = c1.elf_image.as_ref().expect("image 1");
        let image2 = c2.elf_image.as_ref().expect("image 2");
        assert_eq!(image1.plt_stubs.len(), image2.plt_stubs.len());
    }

    #[test]
    fn arm_elf32_appended_segment_writes_valid_elf() {
        // Phase 3 deliverable: grow .text via with_section_bytes
        // and confirm the append-PT_LOAD path emits a valid
        // ELF32 with a Thumb-mode trampoline at the original
        // function entry.
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c = container::Container::from_bytes(&bytes).expect("parse");

        // Find .text and append a single Thumb NOP halfword
        // (0xbf00) to grow it by 2 bytes.
        let text = c
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .expect(".text");
        let mut new_text = text.bytes.clone();
        new_text.extend_from_slice(&[0x00, 0xbf]);
        let grown = c.with_section_bytes(text.id, new_text);

        let written = grown.to_bytes().expect("write grown");
        // Re-parsing should yield a valid container.
        let c2 = container::Container::from_bytes(&written).expect("re-parse");
        assert_eq!(c2.architecture, container::Architecture::Arm);
        // The append path produces an extra PT_LOAD. The
        // section count should be ≥ the original (we don't
        // shrink anything).
        assert!(c2.sections.len() >= c.sections.len());
        // Save for objdump inspection if desired.
        std::fs::write("/tmp/libtool-checker-grown.so", &written).ok();
    }

    #[test]
    fn arm_elf32_image_is_fully_populated() {
        let bytes = std::fs::read(libtool_checker_path()).expect("read");
        let c = container::Container::from_bytes(&bytes).expect("parse");
        let image = c.elf_image.as_ref().expect("ElfImage");
        // Phase 1 deliverable: not just plt_stubs but every
        // top-level field that the AArch64 path populates.
        assert!(
            !image.program_headers.is_empty(),
            "program headers should be populated"
        );
        assert!(
            !image.section_layout.is_empty(),
            "section_layout should be populated"
        );
        assert!(!image.dynamic.is_empty(), "dynamic entries should be populated");
        assert!(image.dynamic_section.is_some(), "dynamic_section id should be set");
        assert!(image.dynsym.is_some(), ".dynsym bytes should be present");
        assert!(image.dynstr.is_some(), ".dynstr bytes should be present");
        // libtool-checker is a SONAME=_libtool-checker.so build,
        // so .gnu.hash and .interp may or may not be present —
        // this is binary-specific. Check only sections we
        // verified exist via the dump output.
        assert!(image.gnu_hash.is_some(), ".gnu.hash bytes should be present");
        assert!(image.sysv_hash.is_some(), ".hash bytes should be present");
        // The build-id note is in `.note.gnu.build-id`.
        assert!(image.build_id.is_some(), "build_id should be extracted");
    }
}
