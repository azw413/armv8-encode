//! Helpers for *extending* an ELF input's `.dynsym`, `.dynstr`,
//! `.gnu.hash`, and `.gnu.version` sections to add a new dynamic
//! export.
//!
//! ## Why this lives outside the writer
//!
//! The four sections form a tightly coupled group: `.dynsym` entries
//! reference `.dynstr` byte offsets; `.gnu.hash` indexes into
//! `.dynsym`; `.gnu.version`'s array length must match `.dynsym`'s
//! entry count. Adding one symbol means rebuilding all four
//! together. Doing that here keeps the writer's section-bytes
//! override path simple — it just receives four pre-baked blobs
//! and a list of `.dynamic` tag updates.
//!
//! ## Strategy: orphan the originals, place new copies in the
//! appended segment
//!
//! The dynamic linker locates `.dynsym` / `.dynstr` / `.gnu.hash` /
//! `.gnu.version` via the `DT_SYMTAB` / `DT_STRTAB` / `DT_GNU_HASH`
//! / `DT_VERSYM` tags in `.dynamic` — by *virtual address*, not by
//! section header. We exploit that:
//!
//! 1. Build extended copies of the four sections.
//! 2. Place them at fresh virtual addresses in the appended PT_LOAD
//!    segment (alongside the new function and its data).
//! 3. Rewrite the captured `.dynamic` tag values so the loader
//!    follows the new copies.
//!
//! The original copies stay in place in the file but become
//! orphaned — the loader never reads them again. Tools that walk
//! section headers (objdump, readelf) still see them; they're
//! correct for the *original* set of dynsym entries even though
//! they're now stale relative to the runtime view.
//!
//! ## Layout invariant: nbuckets = 1
//!
//! [`crate::container::gnu_hash::build_gnu_hash`] requires the
//! hashable region of `.dynsym` to be sorted such that all entries
//! sharing a hash bucket are contiguous. With `nbuckets = 1` this
//! is trivially satisfied — every entry shares the one bucket. We
//! emit the new dynsym in source order plus the new entry at the
//! end, and `nbuckets = 1` gives a correct hash regardless of
//! order. Lookup is O(n) within the (still small) export set;
//! acceptable for the use cases this enables.

/// One dynsym entry in the format the GNU linker emits for ELF64.
///
/// Fields map directly to `Elf64_Sym`. The neutral
/// [`crate::container::Symbol`] type drops `st_name`/`st_other`
/// at parse time; this struct preserves them so we can reproduce
/// the original entries byte-for-byte.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DynsymEntry {
    /// Byte offset into `.dynstr` for the symbol's name.
    pub st_name: u32,
    /// Combined binding (high 4 bits) and type (low 4 bits).
    pub st_info: u8,
    /// Visibility (low 2 bits) plus padding bits.
    pub st_other: u8,
    /// Section header index, or `SHN_UNDEF` (0) for externs,
    /// `SHN_ABS` (0xfff1) for absolute, etc.
    pub st_shndx: u16,
    /// Symbol value — virtual address for defined entries, 0
    /// for undefined.
    pub st_value: u64,
    /// Symbol size in bytes.
    pub st_size: u64,
}

impl DynsymEntry {
    /// Encode as 24 bytes of little-endian Elf64_Sym.
    pub fn to_le_bytes(&self) -> [u8; 24] {
        let mut out = [0u8; 24];
        out[0..4].copy_from_slice(&self.st_name.to_le_bytes());
        out[4] = self.st_info;
        out[5] = self.st_other;
        out[6..8].copy_from_slice(&self.st_shndx.to_le_bytes());
        out[8..16].copy_from_slice(&self.st_value.to_le_bytes());
        out[16..24].copy_from_slice(&self.st_size.to_le_bytes());
        out
    }

    /// Decode 24 bytes of Elf64_Sym (LE).
    pub fn from_le_bytes(bytes: &[u8; 24]) -> Self {
        Self {
            st_name: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            st_info: bytes[4],
            st_other: bytes[5],
            st_shndx: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            st_value: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            st_size: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        }
    }
}

/// Malformed input surfaced while parsing a `.dynsym` or
/// `.dynamic` blob out of an untrusted binary.
///
/// These sections are fixed-width arrays; a byte length that isn't
/// a whole number of entries means the input is corrupt (or was
/// mis-sliced upstream). Parsing returns this instead of panicking
/// so a hostile file degrades to a caught error rather than
/// aborting the process.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DynParseError {
    /// `.dynsym` byte length is not a whole number of 24-byte
    /// `Elf64_Sym` entries.
    MisalignedDynsym { len: usize },
    /// `.dynamic` byte length is not a whole number of entries.
    /// `entry_size` is 16 for ELF64 (`Elf64_Dyn`), 8 for ELF32
    /// (`Elf32_Dyn`).
    MisalignedDynamic { len: usize, entry_size: usize },
}

impl std::fmt::Display for DynParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynParseError::MisalignedDynsym { len } => write!(
                f,
                ".dynsym byte length {len} is not a multiple of 24"
            ),
            DynParseError::MisalignedDynamic { len, entry_size } => write!(
                f,
                ".dynamic byte length {len} is not a multiple of {entry_size}"
            ),
        }
    }
}

impl std::error::Error for DynParseError {}

/// Parse `.dynsym` bytes into a list of [`DynsymEntry`]s. Returns
/// [`DynParseError::MisalignedDynsym`] if the byte length isn't a
/// multiple of 24 (`Elf64_Sym` width), so malformed input is a
/// caught error rather than a panic.
pub fn parse_dynsym(bytes: &[u8]) -> Result<Vec<DynsymEntry>, DynParseError> {
    if bytes.len() % 24 != 0 {
        return Err(DynParseError::MisalignedDynsym { len: bytes.len() });
    }
    let mut out = Vec::with_capacity(bytes.len() / 24);
    for chunk in bytes.chunks_exact(24) {
        let arr: [u8; 24] = chunk.try_into().unwrap();
        out.push(DynsymEntry::from_le_bytes(&arr));
    }
    Ok(out)
}

/// Encode a list of dynsym entries into bytes.
pub fn encode_dynsym(entries: &[DynsymEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * 24);
    for entry in entries {
        out.extend_from_slice(&entry.to_le_bytes());
    }
    out
}

/// Append a NUL-terminated string to a copy of the source
/// `.dynstr` and return the byte offset of the new string plus
/// the extended bytes.
pub fn append_dynstr(source: &[u8], new_name: &str) -> (u32, Vec<u8>) {
    let offset = source.len() as u32;
    let mut bytes = source.to_vec();
    bytes.extend_from_slice(new_name.as_bytes());
    bytes.push(0);
    (offset, bytes)
}

/// Build a fresh `.gnu.version` (versym) section by appending a
/// versym entry for a new symbol. The new entry is `1` (the
/// generic-base version), which the dynamic linker accepts for
/// unversioned symbols defined in this object.
///
/// Versym's wire format is one little-endian u16 per dynsym entry,
/// in dynsym index order.
pub fn append_gnu_versym(source: &[u8], new_versym: u16) -> Vec<u8> {
    let mut bytes = source.to_vec();
    bytes.extend_from_slice(&new_versym.to_le_bytes());
    bytes
}

/// Update `.dynamic` entries to point at new section locations.
/// Takes the source dynamic tag list and a set of (tag, new_value)
/// overrides; returns the rebuilt list. Tags not in the overrides
/// pass through verbatim.
pub fn update_dynamic_tags(
    source: &[crate::container::DynamicEntry],
    updates: &[(u64, u64)],
) -> Vec<crate::container::DynamicEntry> {
    use crate::container::DynamicEntry;
    let updates: std::collections::HashMap<u64, u64> = updates.iter().copied().collect();
    source
        .iter()
        .map(|entry| {
            if let Some(&new_value) = updates.get(&entry.tag) {
                DynamicEntry {
                    tag: entry.tag,
                    value: new_value,
                }
            } else {
                *entry
            }
        })
        .collect()
}

/// Insert new `.dynamic` entries by overwriting trailing
/// `DT_NULL` slots. Returns `Some(extended)` if there were
/// enough trailing nulls to absorb the additions in place,
/// `None` otherwise (caller must grow `.dynamic`).
///
/// The result preserves the section's byte size — important
/// because the writer keeps section offsets fixed for in-place
/// overrides.
///
/// Insertion happens *before* the first DT_NULL, so the
/// terminator stays at the end. Extra DT_NULLs (the padding
/// the linker emits) shrink to make room.
pub fn insert_dynamic_tags(
    source: &[crate::container::DynamicEntry],
    additions: &[(u64, u64)],
) -> Option<Vec<crate::container::DynamicEntry>> {
    use crate::container::DynamicEntry;
    use object::elf::DT_NULL;
    // Index of the first DT_NULL.
    let first_null = source.iter().position(|e| e.tag == DT_NULL as u64)?;
    let trailing_nulls = source.len() - first_null;
    if trailing_nulls < additions.len() + 1 {
        // Need at least one DT_NULL to remain as terminator.
        return None;
    }
    let mut out: Vec<DynamicEntry> = Vec::with_capacity(source.len());
    out.extend_from_slice(&source[..first_null]);
    for &(tag, value) in additions {
        out.push(DynamicEntry { tag, value });
    }
    // Refill remaining slots with DT_NULL so the byte length
    // stays the same.
    let remaining_slots = source.len() - out.len();
    for _ in 0..remaining_slots {
        out.push(DynamicEntry {
            tag: DT_NULL as u64,
            value: 0,
        });
    }
    Some(out)
}

/// Insert new `.dynamic` entries always, growing the tag list
/// past its original length if there isn't enough trailing
/// `DT_NULL` padding to absorb them. Companion to
/// [`insert_dynamic_tags`] for callers that don't need to
/// preserve byte length — typically because they're prepared
/// to relocate `.dynamic` to a fresh location if the result
/// no longer fits in the original section.
///
/// Always returns at least one trailing DT_NULL.
pub fn insert_dynamic_tags_growing(
    source: &[crate::container::DynamicEntry],
    additions: &[(u64, u64)],
) -> Vec<crate::container::DynamicEntry> {
    use crate::container::DynamicEntry;
    use object::elf::DT_NULL;

    // Strip trailing DT_NULLs so we can re-insert exactly one
    // terminator at the end after appending new entries.
    let mut prefix: Vec<DynamicEntry> = source
        .iter()
        .take_while(|e| e.tag != DT_NULL as u64)
        .copied()
        .collect();
    for &(tag, value) in additions {
        prefix.push(DynamicEntry { tag, value });
    }
    prefix.push(DynamicEntry {
        tag: DT_NULL as u64,
        value: 0,
    });
    prefix
}

/// Parse `.dynamic` bytes back into a tag list.
/// Inverse of [`encode_dynamic`]. `is_64` selects the entry
/// width: ELF64 uses `Elf64_Dyn` (16 bytes), ELF32 uses
/// `Elf32_Dyn` (8 bytes). Returns
/// [`DynParseError::MisalignedDynamic`] if the byte length isn't a
/// multiple of the entry width, so malformed input is a caught
/// error rather than a panic.
pub fn parse_dynamic(
    bytes: &[u8],
    is_64: bool,
) -> Result<Vec<crate::container::DynamicEntry>, DynParseError> {
    use crate::container::DynamicEntry;
    let entry_size = if is_64 { 16 } else { 8 };
    if bytes.len() % entry_size != 0 {
        return Err(DynParseError::MisalignedDynamic {
            len: bytes.len(),
            entry_size,
        });
    }
    let mut out = Vec::with_capacity(bytes.len() / entry_size);
    for chunk in bytes.chunks_exact(entry_size) {
        let (tag, value) = if is_64 {
            (
                u64::from_le_bytes(chunk[0..8].try_into().unwrap()),
                u64::from_le_bytes(chunk[8..16].try_into().unwrap()),
            )
        } else {
            (
                u32::from_le_bytes(chunk[0..4].try_into().unwrap()) as u64,
                u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as u64,
            )
        };
        out.push(DynamicEntry { tag, value });
    }
    Ok(out)
}

/// Read a NUL-terminated string out of a `.dynstr` blob at the
/// given byte offset. Returns `None` if the offset is past the
/// end of the blob or if no terminator is found before the blob
/// ends.
pub fn read_dynstr_at(dynstr: &[u8], offset: u32) -> Option<&str> {
    let start = offset as usize;
    if start >= dynstr.len() {
        return None;
    }
    let end = dynstr[start..].iter().position(|&b| b == 0)? + start;
    std::str::from_utf8(&dynstr[start..end]).ok()
}

/// Remove all `.dynamic` entries matching `(tag, predicate(value))`
/// and pad the section back to its original length with trailing
/// `DT_NULL` entries. Used to drop a `DT_NEEDED` tag for a
/// specific library without changing the section's byte length.
///
/// Returns the new entry list and the number of entries removed.
pub fn remove_dynamic_entries<F>(
    source: &[crate::container::DynamicEntry],
    tag: u64,
    mut predicate: F,
) -> (Vec<crate::container::DynamicEntry>, usize)
where
    F: FnMut(u64) -> bool,
{
    use crate::container::DynamicEntry;
    use object::elf::DT_NULL;

    let mut removed = 0usize;
    let mut out: Vec<DynamicEntry> = Vec::with_capacity(source.len());
    for entry in source {
        if entry.tag == tag && predicate(entry.value) {
            removed += 1;
            continue;
        }
        out.push(*entry);
    }
    // Re-pad to original length with DT_NULL so the section's
    // byte size is preserved (matches `insert_dynamic_tags`
    // discipline).
    while out.len() < source.len() {
        out.push(DynamicEntry {
            tag: DT_NULL as u64,
            value: 0,
        });
    }
    (out, removed)
}

/// Encode a `.dynamic` tag list into bytes. `is_64` selects
/// the entry layout: ELF64 emits `Elf64_Dyn` (16 bytes per
/// entry — u64 d_tag + u64 d_un), ELF32 emits `Elf32_Dyn`
/// (8 bytes per entry — u32 d_tag + u32 d_un). The DynamicEntry
/// tag and value fields are always stored as u64 in the
/// neutral representation; for ELF32 the writer truncates each
/// to u32 (which is safe — tags are well within u32 and the
/// values are either string-table offsets or vaddrs, also
/// always within the 32-bit address space for ELF32 inputs).
pub fn encode_dynamic(
    entries: &[crate::container::DynamicEntry],
    is_64: bool,
) -> Vec<u8> {
    let entry_size = if is_64 { 16 } else { 8 };
    let mut out = Vec::with_capacity(entries.len() * entry_size);
    for entry in entries {
        if is_64 {
            out.extend_from_slice(&entry.tag.to_le_bytes());
            out.extend_from_slice(&entry.value.to_le_bytes());
        } else {
            out.extend_from_slice(&(entry.tag as u32).to_le_bytes());
            out.extend_from_slice(&(entry.value as u32).to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynsym_entry_roundtrips_through_bytes() {
        let entry = DynsymEntry {
            st_name: 0x123,
            st_info: 0x12,
            st_other: 0x02,
            st_shndx: 0x000a,
            st_value: 0x6a8,
            st_size: 24,
        };
        let bytes = entry.to_le_bytes();
        let decoded = DynsymEntry::from_le_bytes(&bytes);
        assert_eq!(decoded, entry);
    }

    #[test]
    fn parse_dynsym_reads_whole_entries() {
        let entry = DynsymEntry {
            st_name: 7,
            st_info: 0x12,
            st_other: 0,
            st_shndx: 1,
            st_value: 0x400,
            st_size: 8,
        };
        let bytes = encode_dynsym(&[entry, entry]);
        let parsed = parse_dynsym(&bytes).unwrap();
        assert_eq!(parsed, vec![entry, entry]);
    }

    #[test]
    fn parse_dynsym_rejects_misaligned_length_without_panicking() {
        // 25 bytes is not a whole number of 24-byte Elf64_Sym
        // entries — a malformed/truncated .dynsym. Must be a caught
        // error, never a panic.
        let bytes = vec![0u8; 25];
        assert_eq!(
            parse_dynsym(&bytes),
            Err(DynParseError::MisalignedDynsym { len: 25 }),
        );
    }

    #[test]
    fn parse_dynamic_rejects_misaligned_length_without_panicking() {
        // 24 bytes is misaligned for the ELF64 16-byte entry width
        // (24 % 16 == 8) — a malformed/truncated .dynamic. Must be a
        // caught error, never a panic.
        let bytes = vec![0u8; 24];
        assert_eq!(
            parse_dynamic(&bytes, true),
            Err(DynParseError::MisalignedDynamic {
                len: 24,
                entry_size: 16,
            }),
        );
        // The same length is a whole number of ELF32 8-byte entries.
        assert!(parse_dynamic(&bytes, false).is_ok());
    }

    #[test]
    fn append_dynstr_appends_with_null_terminator() {
        let source = b"\0first\0second\0";
        let (offset, extended) = append_dynstr(source, "third");
        assert_eq!(offset, source.len() as u32);
        assert_eq!(&extended[offset as usize..], b"third\0");
    }

    #[test]
    fn append_gnu_versym_appends_one_u16() {
        // Two existing entries (4 bytes), append one (2 bytes).
        let source = vec![0x00, 0x00, 0x01, 0x00];
        let extended = append_gnu_versym(&source, 1);
        assert_eq!(extended.len(), 6);
        assert_eq!(&extended[4..6], &1u16.to_le_bytes());
    }

    #[test]
    fn read_dynstr_at_returns_name_at_offset() {
        let dynstr = b"\0libc.so.6\0libdep.so\0";
        assert_eq!(read_dynstr_at(dynstr, 1), Some("libc.so.6"));
        assert_eq!(read_dynstr_at(dynstr, 11), Some("libdep.so"));
        assert_eq!(read_dynstr_at(dynstr, 0), Some(""));
        assert_eq!(read_dynstr_at(dynstr, 999), None);
    }

    #[test]
    fn remove_dynamic_entries_drops_matching_and_pads_with_null() {
        use crate::container::DynamicEntry;
        use object::elf::{DT_NEEDED, DT_NULL, DT_STRTAB};
        let source = vec![
            DynamicEntry { tag: DT_NEEDED as u64, value: 1 },   // libc
            DynamicEntry { tag: DT_NEEDED as u64, value: 11 },  // libdep (to drop)
            DynamicEntry { tag: DT_STRTAB as u64, value: 0x100 },
            DynamicEntry { tag: DT_NULL as u64, value: 0 },
            DynamicEntry { tag: DT_NULL as u64, value: 0 },
        ];
        let (out, removed) =
            remove_dynamic_entries(&source, DT_NEEDED as u64, |v| v == 11);
        assert_eq!(removed, 1);
        // Preserved length (matches original).
        assert_eq!(out.len(), source.len());
        // libc DT_NEEDED still present; libdep gone.
        let needed: Vec<_> = out
            .iter()
            .filter(|e| e.tag == DT_NEEDED as u64)
            .collect();
        assert_eq!(needed.len(), 1);
        assert_eq!(needed[0].value, 1);
        // Last entry is DT_NULL (terminator preserved).
        assert_eq!(out.last().unwrap().tag, DT_NULL as u64);
    }

    #[test]
    fn remove_dynamic_entries_reports_zero_when_no_match() {
        use crate::container::DynamicEntry;
        use object::elf::{DT_NEEDED, DT_NULL};
        let source = vec![
            DynamicEntry { tag: DT_NEEDED as u64, value: 1 },
            DynamicEntry { tag: DT_NULL as u64, value: 0 },
        ];
        let (out, removed) =
            remove_dynamic_entries(&source, DT_NEEDED as u64, |v| v == 999);
        assert_eq!(removed, 0);
        assert_eq!(out, source);
    }
}
