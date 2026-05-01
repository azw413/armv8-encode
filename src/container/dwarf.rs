//! DWARF lifting on top of the container layer.
//!
//! Walks `.debug_info` (or `__debug_info` for Mach-O) and extracts a
//! `DwarfFunction` for every `DW_TAG_subprogram`, with optional source
//! file/line info pulled from `DW_AT_decl_file` / `DW_AT_decl_line` and the
//! containing CU's line program.
//!
//! The parser is best-effort: a malformed DWARF section silently produces
//! `None`, since DWARF is metadata that callers should still be able to
//! work without. Parse-time failures are deliberately swallowed rather
//! than surfaced as `ContainerError` — the container is still usable.

use crate::container::types::{DwarfFunction, DwarfInfo};
use gimli::{AttributeValue, Dwarf, EndianSlice, Reader, RunTimeEndian, SectionId, Unit};
use object::read::{File, Object, ObjectSection};

pub fn parse(file: &File<'_>) -> Option<DwarfInfo> {
    let data = DwarfData::load(file);
    if data.debug_info.is_empty() {
        return None;
    }

    let endian = if file.is_little_endian() {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    };
    let dwarf = data.build(endian);

    let mut functions = Vec::new();
    let mut units = dwarf.units();
    while let Ok(Some(header)) = units.next() {
        let Ok(unit) = dwarf.unit(header) else {
            continue;
        };
        walk_unit(&dwarf, &unit, &mut functions);
    }

    if functions.is_empty() {
        // No usable subprograms found — better to surface that as "no
        // DWARF" than to attach an empty record.
        None
    } else {
        Some(DwarfInfo { functions })
    }
}

fn walk_unit<R: Reader>(dwarf: &Dwarf<R>, unit: &Unit<R>, out: &mut Vec<DwarfFunction>) {
    let mut entries = unit.entries();
    while let Ok(Some((_, entry))) = entries.next_dfs() {
        if entry.tag() != gimli::DW_TAG_subprogram {
            continue;
        }
        if let Some(function) = extract_subprogram(dwarf, unit, entry) {
            out.push(function);
        }
    }
}

fn extract_subprogram<R: Reader>(
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &gimli::DebuggingInformationEntry<'_, '_, R>,
) -> Option<DwarfFunction> {
    let name = read_string_attribute(dwarf, unit, entry, gimli::DW_AT_name).unwrap_or_default();

    // `low_pc` is required — without it we don't know where the function
    // lives. `high_pc` may be either an absolute address or a length offset
    // (DWARF 4 introduced the latter form).
    let low_pc = match entry.attr_value(gimli::DW_AT_low_pc).ok().flatten()? {
        AttributeValue::Addr(addr) => addr,
        _ => return None,
    };
    let size = match entry.attr_value(gimli::DW_AT_high_pc).ok().flatten() {
        Some(AttributeValue::Udata(length)) => length,
        Some(AttributeValue::Addr(addr)) => addr.saturating_sub(low_pc),
        _ => 0,
    };

    let source_file = read_decl_file(dwarf, unit, entry);
    let source_line = match entry.attr_value(gimli::DW_AT_decl_line).ok().flatten() {
        Some(AttributeValue::Udata(line)) => Some(line as u32),
        _ => None,
    };

    Some(DwarfFunction {
        name,
        address: low_pc,
        size,
        source_file,
        source_line,
    })
}

fn read_string_attribute<R: Reader>(
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &gimli::DebuggingInformationEntry<'_, '_, R>,
    attr: gimli::DwAt,
) -> Option<String> {
    let value = entry.attr_value(attr).ok().flatten()?;
    let slice = dwarf.attr_string(unit, value).ok()?;
    slice.to_string().ok().map(|cow| cow.into_owned())
}

fn read_decl_file<R: Reader>(
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &gimli::DebuggingInformationEntry<'_, '_, R>,
) -> Option<String> {
    let value = entry.attr_value(gimli::DW_AT_decl_file).ok().flatten()?;
    let file_index = match value {
        AttributeValue::FileIndex(index) => index,
        AttributeValue::Udata(index) => index,
        _ => return None,
    };
    let line_program = unit.line_program.as_ref()?;
    let header = line_program.header();
    let file = header.file(file_index)?;
    let path_value = file.path_name();
    let slice = dwarf.attr_string(unit, path_value).ok()?;
    slice.to_string().ok().map(|cow| cow.into_owned())
}

/// Owned copies of every DWARF section we might consult, loaded once from
/// the host object file. Owned rather than borrowed so we don't have to
/// thread lifetimes through every caller.
struct DwarfData {
    debug_abbrev: Vec<u8>,
    debug_addr: Vec<u8>,
    debug_aranges: Vec<u8>,
    debug_info: Vec<u8>,
    debug_line: Vec<u8>,
    debug_line_str: Vec<u8>,
    debug_loc: Vec<u8>,
    debug_loclists: Vec<u8>,
    debug_ranges: Vec<u8>,
    debug_rnglists: Vec<u8>,
    debug_str: Vec<u8>,
    debug_str_offsets: Vec<u8>,
    debug_types: Vec<u8>,
}

impl DwarfData {
    fn load(file: &File<'_>) -> Self {
        let load = |name: &str| load_section(file, name);
        Self {
            debug_abbrev: load(".debug_abbrev"),
            debug_addr: load(".debug_addr"),
            debug_aranges: load(".debug_aranges"),
            debug_info: load(".debug_info"),
            debug_line: load(".debug_line"),
            debug_line_str: load(".debug_line_str"),
            debug_loc: load(".debug_loc"),
            debug_loclists: load(".debug_loclists"),
            debug_ranges: load(".debug_ranges"),
            debug_rnglists: load(".debug_rnglists"),
            debug_str: load(".debug_str"),
            debug_str_offsets: load(".debug_str_offsets"),
            debug_types: load(".debug_types"),
        }
    }

    fn build<'a>(
        &'a self,
        endian: RunTimeEndian,
    ) -> Dwarf<EndianSlice<'a, RunTimeEndian>> {
        Dwarf::load(|id| -> Result<EndianSlice<'a, RunTimeEndian>, gimli::Error> {
            let bytes: &[u8] = match id {
                SectionId::DebugAbbrev => &self.debug_abbrev,
                SectionId::DebugAddr => &self.debug_addr,
                SectionId::DebugAranges => &self.debug_aranges,
                SectionId::DebugInfo => &self.debug_info,
                SectionId::DebugLine => &self.debug_line,
                SectionId::DebugLineStr => &self.debug_line_str,
                SectionId::DebugLoc => &self.debug_loc,
                SectionId::DebugLocLists => &self.debug_loclists,
                SectionId::DebugRanges => &self.debug_ranges,
                SectionId::DebugRngLists => &self.debug_rnglists,
                SectionId::DebugStr => &self.debug_str,
                SectionId::DebugStrOffsets => &self.debug_str_offsets,
                SectionId::DebugTypes => &self.debug_types,
                _ => &[],
            };
            Ok(EndianSlice::new(bytes, endian))
        })
        .expect("Dwarf::load with infallible loader")
    }
}

/// Look up a section by ELF name, falling back to the Mach-O equivalent.
fn load_section(file: &File<'_>, elf_name: &str) -> Vec<u8> {
    let macho_name = if let Some(stripped) = elf_name.strip_prefix('.') {
        format!("__{stripped}")
    } else {
        String::new()
    };
    for candidate in [elf_name, macho_name.as_str()] {
        if candidate.is_empty() {
            continue;
        }
        if let Some(section) = file.section_by_name(candidate) {
            return section
                .uncompressed_data()
                .map(|cow| cow.into_owned())
                .unwrap_or_default();
        }
    }
    Vec::new()
}
