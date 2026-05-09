//! Dynamic-relocation byte-range extraction.
//!
//! Returns the file-offset spans that the dynamic loader will
//! overwrite at load time — `R_AARCH64_RELATIVE`, `JUMP_SLOT`,
//! `GLOB_DAT`, and friends. Callers (section encryption,
//! integrity hashing, anti-tamper) need this to leave the
//! loader's output bytes untouched at pack time and re-skip
//! the same ranges at runtime.
//!
//! Pure read-side — we don't write the relocations, just
//! report where they land. Built on top of [`ElfImage`]'s
//! already-parsed `dynamic` tag list and `program_headers`,
//! plus the section bytes for the actual `Elf64_Rela`
//! entries.
//!
//! Mach-O is not in scope: the runtime loader on Darwin
//! applies relocations very differently (LC_DYLD_INFO bind
//! opcodes), and Android — the consumer for the encryption
//! pass — is ELF-only. This module returns an empty list for
//! Mach-O containers.
//!
//! ## Reloc types we model
//!
//! Only the dynamic-reloc subset that actually appears in
//! AOSP-built `.so` files. Each yields a write of either 4 or
//! 8 bytes at `r_offset`:
//!
//! | Type | Width | Notes |
//! |------|-------|-------|
//! | `R_AARCH64_RELATIVE` | 8 | The bulk of `.rela.dyn` on PIC `.so`s |
//! | `R_AARCH64_GLOB_DAT` | 8 | Imported global variable slots |
//! | `R_AARCH64_JUMP_SLOT` | 8 | PLT entries |
//! | `R_AARCH64_ABS64` / `IRELATIVE` / `TLS_TPREL64` / `TLSDESC` | 8 | Less common but present |
//! | `R_AARCH64_ABS32` / `PREL32` | 4 | Rare in dynamic relocs |
//! | `R_AARCH64_COPY` | variable | Skipped — width comes from the dynsym entry's size, deferred to v2 |
//!
//! Unknown / unhandled types are silently skipped. That means
//! a section with such a reloc *would* be miscoded if
//! encrypted — log loudly when one shows up so the encrypt
//! pass can refuse the section.

use crate::container::{Container, ElfImage};

/// One contiguous range of bytes the dynamic loader will
/// rewrite. Sorted-by-offset, non-overlapping, contained
/// within the file image.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct RelocSpan {
    /// Byte offset within the ELF file image.
    pub file_offset: u64,
    /// Number of bytes the loader will write at that offset.
    /// Always 4 or 8 for the relocation types we handle.
    pub len: u32,
}

/// Anything we couldn't account for in the skip-list. Caller
/// can decide whether to refuse the encryption pass or
/// proceed (e.g. skipping `R_AARCH64_COPY` is safe for code
/// sections — they don't contain COPY relocs by construction).
#[derive(Debug, Clone)]
pub struct UnhandledReloc {
    pub r_type: u32,
    pub r_offset: u64,
}

/// Result of [`collect_skip_spans`]. Spans are sorted-by-
/// offset and merged; unhandled is non-empty when the input
/// has reloc types we don't model the width of.
#[derive(Debug, Clone, Default)]
pub struct SkipReport {
    pub spans: Vec<RelocSpan>,
    pub unhandled: Vec<UnhandledReloc>,
}

/// Extract the skip-list from `container`. `raw_bytes` is the
/// original ELF image the container was parsed from — used to
/// read the `Elf64_Rela` entries via the file offsets named in
/// `DT_RELA` / `DT_JMPREL`. Returns an empty report for Mach-O
/// or for ELFs without dynamic relocations.
pub fn collect_skip_spans(container: &Container, raw_bytes: &[u8]) -> SkipReport {
    let Some(image) = container.elf_image.as_ref() else {
        return SkipReport::default();
    };

    let mut report = SkipReport::default();

    // Locate `.rela.dyn` and `.rela.plt` blocks via the
    // already-parsed dynamic tags. Tag values are virtual
    // addresses; we resolve them through PT_LOAD.
    let mut dt_rela: Option<u64> = None;
    let mut dt_relasz: Option<u64> = None;
    let mut dt_jmprel: Option<u64> = None;
    let mut dt_pltrelsz: Option<u64> = None;
    let mut dt_pltrel_is_rela = true; // Default for aarch64.

    for entry in &image.dynamic {
        let tag = entry.tag as u32;
        match tag {
            object::elf::DT_RELA => dt_rela = Some(entry.value),
            object::elf::DT_RELASZ => dt_relasz = Some(entry.value),
            object::elf::DT_JMPREL => dt_jmprel = Some(entry.value),
            object::elf::DT_PLTRELSZ => dt_pltrelsz = Some(entry.value),
            object::elf::DT_PLTREL => {
                dt_pltrel_is_rela = (entry.value as u32) == object::elf::DT_RELA;
            }
            _ => {}
        }
    }

    if let (Some(rela_va), Some(rela_sz)) = (dt_rela, dt_relasz) {
        decode_block(image, raw_bytes, rela_va, rela_sz, &mut report);
    }
    if dt_pltrel_is_rela {
        if let (Some(plt_va), Some(plt_sz)) = (dt_jmprel, dt_pltrelsz) {
            decode_block(image, raw_bytes, plt_va, plt_sz, &mut report);
        }
    }

    report.spans.sort_by_key(|s| s.file_offset);
    report.spans = merge_adjacent(std::mem::take(&mut report.spans));
    report
}

/// Test whether `[file_offset, file_offset + len)` overlaps
/// any span. Linear scan; fine for the few-thousand spans an
/// Android `.so` produces. For per-byte queries use [`SkipMap`].
pub fn range_overlaps(spans: &[RelocSpan], file_offset: u64, len: u64) -> bool {
    let end = file_offset + len;
    spans.iter().any(|s| {
        let s_end = s.file_offset + s.len as u64;
        s.file_offset < end && file_offset < s_end
    })
}

/// O(log n) chunk iterator. Pack-time and runtime both walk
/// a section linearly; this emits the "encrypt these N bytes,
/// then jump to..." chunks that skip every reloc'd slot.
#[derive(Debug, Clone)]
pub struct SkipMap<'a> {
    spans: &'a [RelocSpan],
}

impl<'a> SkipMap<'a> {
    pub fn new(spans: &'a [RelocSpan]) -> Self {
        Self { spans }
    }

    /// Iterate `(start, len)` chunks of `[range_start, range_end)`
    /// that are NOT covered by any span. Useful for "encrypt
    /// every byte except these reloc'd bytes."
    pub fn encrypt_chunks(
        &self,
        range_start: u64,
        range_end: u64,
    ) -> impl Iterator<Item = (u64, u64)> + '_ {
        let spans = self.spans;
        let first = spans.partition_point(|s| s.file_offset + s.len as u64 <= range_start);
        let last = spans.partition_point(|s| s.file_offset < range_end);
        let relevant = &spans[first..last];

        let mut cursor = range_start;
        let mut iter = relevant.iter();
        std::iter::from_fn(move || loop {
            if cursor >= range_end {
                return None;
            }
            match iter.next() {
                Some(s) if s.file_offset > cursor => {
                    let chunk_end = s.file_offset.min(range_end);
                    let start = cursor;
                    cursor = (s.file_offset + s.len as u64).min(range_end).max(chunk_end);
                    return Some((start, chunk_end - start));
                }
                Some(s) => {
                    // Span starts at or before cursor — skip it.
                    cursor = (s.file_offset + s.len as u64).max(cursor);
                }
                None => {
                    let start = cursor;
                    cursor = range_end;
                    return Some((start, range_end - start));
                }
            }
        })
    }
}

fn decode_block(
    image: &ElfImage,
    raw_bytes: &[u8],
    block_vaddr: u64,
    block_size: u64,
    out: &mut SkipReport,
) {
    const RELA_ENTRY_SIZE: u64 = 24;
    if block_size == 0 || block_size % RELA_ENTRY_SIZE != 0 {
        return;
    }
    // Resolve block vaddr to a file offset via PT_LOAD, then
    // read the entries directly from the input image.
    let Some(block_off) = vaddr_to_file_offset(image, block_vaddr) else {
        return;
    };
    let start = block_off as usize;
    let end = start + block_size as usize;
    if end > raw_bytes.len() {
        return;
    }
    let bytes = &raw_bytes[start..end];

    let entries = (block_size / RELA_ENTRY_SIZE) as usize;
    for i in 0..entries {
        let off = i * RELA_ENTRY_SIZE as usize;
        let r_offset = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
        let r_info = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());
        let r_type = (r_info & 0xffff_ffff) as u32;
        let len = match reloc_write_size(r_type) {
            Some(n) => n,
            None => {
                out.unhandled.push(UnhandledReloc { r_type, r_offset });
                continue;
            }
        };
        let Some(file_off) = vaddr_to_file_offset(image, r_offset) else {
            continue;
        };
        out.spans.push(RelocSpan { file_offset: file_off, len });
    }
}

fn reloc_write_size(r_type: u32) -> Option<u32> {
    use object::elf;
    match r_type {
        elf::R_AARCH64_ABS64
        | elf::R_AARCH64_GLOB_DAT
        | elf::R_AARCH64_JUMP_SLOT
        | elf::R_AARCH64_RELATIVE
        | elf::R_AARCH64_TLS_TPREL
        | elf::R_AARCH64_TLSDESC
        | elf::R_AARCH64_IRELATIVE => Some(8u32),
        elf::R_AARCH64_ABS32 | elf::R_AARCH64_PREL32 => Some(4u32),
        // R_AARCH64_COPY: variable width from the dynsym
        // entry. v2 work — for now flag as unhandled.
        _ => None,
    }
}

fn merge_adjacent(mut spans: Vec<RelocSpan>) -> Vec<RelocSpan> {
    if spans.is_empty() {
        return spans;
    }
    let mut out: Vec<RelocSpan> = Vec::with_capacity(spans.len());
    let first = spans.remove(0);
    out.push(first);
    for s in spans {
        let last = out.last_mut().unwrap();
        let last_end = last.file_offset + last.len as u64;
        if s.file_offset <= last_end {
            let new_end = (s.file_offset + s.len as u64).max(last_end);
            last.len = (new_end - last.file_offset) as u32;
        } else {
            out.push(s);
        }
    }
    out
}

fn vaddr_to_file_offset(image: &ElfImage, vaddr: u64) -> Option<u64> {
    for p in &image.program_headers {
        if p.p_type != object::elf::PT_LOAD {
            continue;
        }
        if vaddr >= p.p_vaddr && vaddr < p.p_vaddr + p.p_filesz {
            return Some(p.p_offset + (vaddr - p.p_vaddr));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_adjacent_combines_touching() {
        let s = vec![
            RelocSpan { file_offset: 0x100, len: 8 },
            RelocSpan { file_offset: 0x108, len: 8 },
            RelocSpan { file_offset: 0x200, len: 4 },
        ];
        let m = merge_adjacent(s);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], RelocSpan { file_offset: 0x100, len: 16 });
        assert_eq!(m[1], RelocSpan { file_offset: 0x200, len: 4 });
    }

    #[test]
    fn merge_adjacent_combines_overlap() {
        let s = vec![
            RelocSpan { file_offset: 0x100, len: 8 },
            RelocSpan { file_offset: 0x104, len: 8 },
        ];
        assert_eq!(merge_adjacent(s), vec![RelocSpan { file_offset: 0x100, len: 12 }]);
    }

    #[test]
    fn skipmap_emits_chunks() {
        let spans = vec![
            RelocSpan { file_offset: 0x100, len: 8 },
            RelocSpan { file_offset: 0x200, len: 4 },
        ];
        let map = SkipMap::new(&spans);
        let chunks: Vec<(u64, u64)> = map.encrypt_chunks(0x80, 0x300).collect();
        assert_eq!(
            chunks,
            vec![(0x80, 0x80), (0x108, 0xf8), (0x204, 0xfc)]
        );
    }

    #[test]
    fn skipmap_handles_range_starting_inside_span() {
        let spans = vec![RelocSpan { file_offset: 0x100, len: 8 }];
        let map = SkipMap::new(&spans);
        let chunks: Vec<(u64, u64)> = map.encrypt_chunks(0x104, 0x200).collect();
        assert_eq!(chunks, vec![(0x108, 0xf8)]);
    }

    #[test]
    fn range_overlaps_basic() {
        let spans = vec![
            RelocSpan { file_offset: 0x100, len: 8 },
            RelocSpan { file_offset: 0x200, len: 8 },
        ];
        assert!(range_overlaps(&spans, 0x104, 1));
        assert!(range_overlaps(&spans, 0xf0, 0x20));
        assert!(!range_overlaps(&spans, 0x108, 0xf8));
        assert!(!range_overlaps(&spans, 0x80, 0x80));
    }
}
