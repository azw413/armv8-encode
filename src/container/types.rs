//! Neutral types for the binary-container layer.
//!
//! These are the format-agnostic shapes the rest of the crate sees. The
//! parser in [`super::reader`] lifts Mach-O / ELF specifics into them.

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SectionId(pub usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SymbolId(pub usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RelocationId(pub usize);

/// Container format detected at parse time.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum BinaryFormat {
    Macho,
    Elf,
}

/// Architecture exposed by the container's header. Analysis layers above
/// this (decoder, classifier, CFG) currently only handle [`Aarch64`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Architecture {
    Aarch64,
    /// Some other architecture — the container parses but the ISA layer
    /// can't decode the text sections.
    Other,
}

/// Coarse section role. Specific format-level kinds get collapsed into the
/// nearest neutral category so callers can ask "is this code?" without
/// knowing about `__TEXT,__text` vs. `.text`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SectionKind {
    /// Executable code.
    Text,
    /// Mutable initialized data.
    Data,
    /// Immutable initialized data (read-only constants, string literals).
    Rodata,
    /// Zero-initialized, no on-disk bytes.
    Bss,
    /// Debug info section (`.debug_*` / `__DWARF,__debug_*`).
    Debug,
    /// Anything else — symbol tables, link-edit metadata, …
    Other,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Section {
    pub id: SectionId,
    pub name: String,
    /// Virtual address at which this section is intended to load. Often
    /// zero in unlinked object files.
    pub address: u64,
    pub size: u64,
    /// On-disk bytes. Empty for `Bss`.
    pub bytes: Vec<u8>,
    pub kind: SectionKind,
}

impl Section {
    /// Convenience: tuple ready to feed into `aarch64::disassemble_bytes`.
    /// Returns `None` for non-text sections.
    pub fn for_disassembly(&self) -> Option<(u64, &[u8])> {
        if matches!(self.kind, SectionKind::Text) {
            Some((self.address, &self.bytes))
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SymbolKind {
    Function,
    Object,
    Section,
    File,
    Unknown,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SymbolBinding {
    Local,
    Global,
    Weak,
    Unknown,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub kind: SymbolKind,
    pub binding: SymbolBinding,
    /// Section the symbol is defined in. `None` for absolute, undefined,
    /// or section-less symbols.
    pub section: Option<SectionId>,
    /// True for imports — symbols referenced but not defined in this
    /// container.
    pub is_undefined: bool,
}

/// What kind of fix-up a relocation applies. Currently only the AArch64
/// shapes the rewriter wants to see are mapped explicitly; everything else
/// falls through to [`RelocationKind::Other`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum RelocationKind {
    /// 26-bit PC-relative branch (`b`, `bl`).
    Branch26,
    /// 19-bit PC-relative conditional branch (`b.cond`, `cbz`, `cbnz`).
    Branch19,
    /// 14-bit PC-relative test-branch (`tbz`, `tbnz`).
    Branch14,
    /// 21-bit PC-relative ADRP page reference.
    AdrpPage21,
    /// 12-bit page offset (the `add`/`ldr` companion to `adrp`).
    PageOffset12,
    /// Absolute pointer (data references, GOT entries).
    Absolute,
    /// Format-specific kind not yet mapped. Carries the raw type code so
    /// callers can still distinguish them.
    Other(u32),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Relocation {
    pub id: RelocationId,
    pub section: SectionId,
    /// Byte offset within `section.bytes` where the fix-up applies.
    pub offset: u64,
    pub kind: RelocationKind,
    /// Width of the encoded fix-up in bits, as reported by the format.
    pub size: u8,
    pub addend: i64,
    /// Target symbol, or `None` if the relocation is section-relative.
    pub symbol: Option<SymbolId>,
}

/// A function as seen by the container layer. Either derived from a
/// `Function`-kind symbol or lifted from DWARF debug info — `provenance`
/// records which.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Function {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub section: SectionId,
    pub provenance: FunctionProvenance,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum FunctionProvenance {
    /// Came from a function-kind symbol in the symbol table.
    Symbol,
    /// Came from a `DW_TAG_subprogram` in DWARF debug info. Used when
    /// symbols are stripped (common for release Mach-O).
    Dwarf,
}

/// DWARF-derived debug info attached to the container.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DwarfInfo {
    pub functions: Vec<DwarfFunction>,
}

/// A function as described by DWARF (`DW_TAG_subprogram`).
///
/// Only the fields we currently consume are lifted. `source_file` and
/// `source_line` are populated when the corresponding DIE attributes are
/// present and the line program resolves cleanly; otherwise they're `None`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DwarfFunction {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
}

/// Parsed binary container.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Container {
    pub format: BinaryFormat,
    pub architecture: Architecture,
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
    pub relocations: Vec<Relocation>,
    /// DWARF debug info, populated when the container has `.debug_info` /
    /// `__debug_info` sections that parse cleanly. `None` when no DWARF is
    /// present or parsing failed (best-effort — DWARF is metadata, not
    /// load-bearing).
    pub dwarf: Option<DwarfInfo>,
}

impl Container {
    pub fn section(&self, id: SectionId) -> &Section {
        &self.sections[id.0]
    }

    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0]
    }

    pub fn relocation(&self, id: RelocationId) -> &Relocation {
        &self.relocations[id.0]
    }

    pub fn text_sections(&self) -> impl Iterator<Item = &Section> {
        self.sections
            .iter()
            .filter(|section| matches!(section.kind, SectionKind::Text))
    }

    pub fn relocations_for(&self, section: SectionId) -> impl Iterator<Item = &Relocation> {
        self.relocations
            .iter()
            .filter(move |relocation| relocation.section == section)
    }

    /// Symbols defined within this container (not undefined imports).
    pub fn defined_symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter().filter(|symbol| !symbol.is_undefined)
    }

    /// Function-kind symbol whose address matches `address`, if any.
    pub fn function_symbol_at(&self, address: u64) -> Option<&Symbol> {
        self.symbols.iter().find(|symbol| {
            symbol.kind == SymbolKind::Function
                && symbol.address == address
                && !symbol.is_undefined
        })
    }

    /// Any defined symbol whose address matches `address`, regardless of
    /// kind. Useful for resolving rewrite-IR targets where we don't yet
    /// know whether the address points at code or data.
    pub fn symbol_at_address(&self, address: u64) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|symbol| !symbol.is_undefined && symbol.address == address)
    }

    /// Final address of a defined symbol, if any. Returns `None` for
    /// undefined externs (which still have an `id` but no address until
    /// the linker resolves them).
    pub fn address_of_symbol(&self, id: SymbolId) -> Option<u64> {
        let symbol = self.symbols.get(id.0)?;
        if symbol.is_undefined {
            None
        } else {
            Some(symbol.address)
        }
    }

    /// Merged view of all known functions: symbol-kind functions plus any
    /// `DW_TAG_subprogram` from DWARF that doesn't already appear in the
    /// symbol table at the same address.
    ///
    /// Symbol-derived entries take precedence — when a function appears in
    /// both, we keep the symbol entry (it carries linkage info DWARF
    /// doesn't). DWARF entries fill in the rest, including stripped
    /// binaries where `defined_symbols()` is empty.
    pub fn functions(&self) -> Vec<Function> {
        let mut functions: Vec<Function> = self
            .defined_symbols()
            .filter(|symbol| symbol.kind == SymbolKind::Function)
            .filter_map(|symbol| {
                Some(Function {
                    name: symbol.name.clone(),
                    address: symbol.address,
                    size: symbol.size,
                    section: symbol.section?,
                    provenance: FunctionProvenance::Symbol,
                })
            })
            .collect();

        if let Some(dwarf) = &self.dwarf {
            for entry in &dwarf.functions {
                if functions.iter().any(|f| f.address == entry.address) {
                    continue;
                }
                let Some(section) = self.section_for_address(entry.address) else {
                    // No matching loaded section — skip; we'd have nothing
                    // useful to point at for `Function::section`.
                    continue;
                };
                functions.push(Function {
                    name: entry.name.clone(),
                    address: entry.address,
                    size: entry.size,
                    section,
                    provenance: FunctionProvenance::Dwarf,
                });
            }
        }

        functions
    }

    /// Find the section whose address range contains `address`. Returns
    /// `None` for addresses that don't fall inside any loaded section.
    pub fn section_for_address(&self, address: u64) -> Option<SectionId> {
        self.sections
            .iter()
            .find(|section| {
                section.size > 0
                    && address >= section.address
                    && address < section.address + section.size
            })
            .map(|section| section.id)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ContainerError {
    /// Format isn't Mach-O or ELF (or wasn't recognised at all).
    UnsupportedFormat,
    /// File parsed, but the container architecture isn't one we have an
    /// ISA layer for. The container itself is still inspectable.
    UnsupportedArchitecture(String),
    /// Underlying parse error from the `object` crate.
    Parse(String),
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerError::UnsupportedFormat => write!(f, "unsupported binary format"),
            ContainerError::UnsupportedArchitecture(name) => {
                write!(f, "unsupported architecture: {name}")
            }
            ContainerError::Parse(reason) => write!(f, "container parse error: {reason}"),
        }
    }
}

impl std::error::Error for ContainerError {}
