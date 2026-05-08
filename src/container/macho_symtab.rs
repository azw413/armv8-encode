//! Mach-O `LC_SYMTAB` extender.
//!
//! `LC_SYMTAB` references a symbol table (array of `nlist_64`,
//! 16 bytes each) and a string table. To add an export we need
//! to:
//!
//! 1. Append the new symbol's name (with leading NUL skipped —
//!    Mach-O strtab strings start at offset >= 1) to the
//!    string table.
//! 2. Build a new `nlist_64` entry pointing at the new strtab
//!    offset.
//! 3. Insert it into the symbol table at the right position
//!    (Mach-O wants symbols sorted by category: locals first,
//!    then external defined, then external undefined; matches
//!    `LC_DYSYMTAB`'s ilocalsym / iextdefsym / iundefsym
//!    ranges).
//!
//! Phase 5 only handles "external defined" exports. The new
//! entry inserts at index `iextdefsym + nextdefsym` (just after
//! existing externals, before undefineds).

use crate::container::ContainerWriteError;

/// Decoded `nlist_64` entry.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Nlist64 {
    pub n_strx: u32,
    pub n_type: u8,
    pub n_sect: u8,
    pub n_desc: u16,
    pub n_value: u64,
}

impl Nlist64 {
    pub fn to_le_bytes(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&self.n_strx.to_le_bytes());
        out[4] = self.n_type;
        out[5] = self.n_sect;
        out[6..8].copy_from_slice(&self.n_desc.to_le_bytes());
        out[8..16].copy_from_slice(&self.n_value.to_le_bytes());
        out
    }

    pub fn from_le_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            n_strx: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            n_type: bytes[4],
            n_sect: bytes[5],
            n_desc: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            n_value: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        }
    }
}

/// Parse a symbol table blob (Mach-O nlist_64 × N) into a
/// list of entries.
pub fn parse_symtab(bytes: &[u8]) -> Result<Vec<Nlist64>, ContainerWriteError> {
    if bytes.len() % 16 != 0 {
        return Err(ContainerWriteError::ObjectWrite(format!(
            "Mach-O symtab: byte length {} not a multiple of 16",
            bytes.len(),
        )));
    }
    let mut out = Vec::with_capacity(bytes.len() / 16);
    for chunk in bytes.chunks_exact(16) {
        let arr: [u8; 16] = chunk.try_into().unwrap();
        out.push(Nlist64::from_le_bytes(&arr));
    }
    Ok(out)
}

/// Encode a list of nlist_64 entries into bytes.
pub fn encode_symtab(entries: &[Nlist64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * 16);
    for entry in entries {
        out.extend_from_slice(&entry.to_le_bytes());
    }
    out
}

/// Append a NUL-terminated symbol name to the string table.
/// Returns the new strtab bytes plus the offset where the new
/// name was placed. Mach-O strtabs traditionally start with a
/// NUL byte (so n_strx=0 means "no name"); the appender
/// preserves whatever the input starts with.
pub fn append_strtab(source: &[u8], new_name: &str) -> (u32, Vec<u8>) {
    let offset = source.len() as u32;
    let mut bytes = source.to_vec();
    bytes.extend_from_slice(new_name.as_bytes());
    bytes.push(0);
    (offset, bytes)
}

/// Build an `nlist_64` for an external defined symbol pointing
/// at `value` in section `n_sect`. Used by Phase 5 for new
/// function exports.
///
/// Mach-O symbol type/category encoding for an external
/// defined symbol:
///   - n_type = N_EXT | N_SECT
///       N_EXT = 0x01 (external)
///       N_SECT = 0x0e (defined in some section, indicated by
///                      n_sect)
///   - n_sect = 1-indexed section number (the new __APPENDED
///              section)
///   - n_desc = 0 (no flags)
pub fn external_defined_nlist(strtab_offset: u32, n_sect: u8, value: u64) -> Nlist64 {
    const N_EXT: u8 = 0x01;
    const N_SECT: u8 = 0x0e;
    Nlist64 {
        n_strx: strtab_offset,
        n_type: N_EXT | N_SECT,
        n_sect,
        n_desc: 0,
        n_value: value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nlist64_roundtrips() {
        let entry = Nlist64 {
            n_strx: 0x42,
            n_type: 0x0f,
            n_sect: 7,
            n_desc: 0,
            n_value: 0x4000,
        };
        let bytes = entry.to_le_bytes();
        let decoded = Nlist64::from_le_bytes(&bytes);
        assert_eq!(decoded, entry);
    }

    #[test]
    fn append_strtab_appends_with_null() {
        let source = b"\0_first\0_second\0";
        let (offset, extended) = append_strtab(source, "_third");
        assert_eq!(offset, 16);
        assert_eq!(&extended[..16], source);
        assert_eq!(&extended[16..], b"_third\0");
    }

    #[test]
    fn external_defined_nlist_sets_n_ext_and_n_sect() {
        let nl = external_defined_nlist(0x10, 5, 0x1000);
        assert_eq!(nl.n_strx, 0x10);
        assert_eq!(nl.n_type & 0x01, 0x01); // N_EXT
        assert_eq!(nl.n_type & 0x0e, 0x0e); // N_SECT
        assert_eq!(nl.n_sect, 5);
        assert_eq!(nl.n_value, 0x1000);
    }
}
