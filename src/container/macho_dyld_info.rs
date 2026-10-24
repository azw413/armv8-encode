//! Classic `LC_DYLD_INFO[_ONLY]` rebase information.
//!
//! Images that predate chained fixups (or are linked for an older deployment
//! target) describe their rebases as an opcode stream: each opcode names a
//! *location* (segment index + offset) holding a pointer whose value is the
//! target's unslid vmaddr; dyld just adds the slide. Unlike chained fixups the
//! target lives in the data itself, so when a `__TEXT` grow shifts everything
//! past the growth point up by `delta`, every stored pointer that targets the
//! shifted range must gain `delta` too — the locations are segment-relative and
//! move with their segment automatically.

use super::macho_image::MachOSegment;

const REBASE_OPCODE_MASK: u8 = 0xF0;
const REBASE_IMMEDIATE_MASK: u8 = 0x0F;
const REBASE_OPCODE_DONE: u8 = 0x00;
const REBASE_OPCODE_SET_TYPE_IMM: u8 = 0x10;
const REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB: u8 = 0x20;
const REBASE_OPCODE_ADD_ADDR_ULEB: u8 = 0x30;
const REBASE_OPCODE_ADD_ADDR_IMM_SCALED: u8 = 0x40;
const REBASE_OPCODE_DO_REBASE_IMM_TIMES: u8 = 0x50;
const REBASE_OPCODE_DO_REBASE_ULEB_TIMES: u8 = 0x60;
const REBASE_OPCODE_DO_REBASE_ADD_ADDR_ULEB: u8 = 0x70;
const REBASE_OPCODE_DO_REBASE_ULEB_TIMES_SKIPPING_ULEB: u8 = 0x80;
const REBASE_TYPE_POINTER: u8 = 1;
const POINTER_SIZE: u64 = 8;

/// One rebased pointer slot: `LC_SEGMENT_64` index (load-command order,
/// `__PAGEZERO` included) and byte offset within that segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebaseLocation {
    pub segment: usize,
    pub offset: u64,
}

/// The `LC_DYLD_INFO[_ONLY]` data ranges (file offset, size) we act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DyldInfo {
    /// File offset of the load command itself.
    pub command_offset: usize,
    pub rebase: (u64, u64),
    pub export: (u64, u64),
}

/// Byte offset of `export_off` inside `dyld_info_command` (`export_size`
/// follows at +44).
pub const EXPORT_FIELD_OFFSET: usize = 40;

/// Locate `LC_DYLD_INFO[_ONLY]` in a 64-bit Mach-O image.
pub fn find(bytes: &[u8]) -> Option<DyldInfo> {
    use object::macho;
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let sizeofcmds = u32_at(20)? as usize;
    let end = 32 + sizeofcmds;
    let mut cursor = 32;
    while cursor + 8 <= end {
        let cmd = u32_at(cursor)?;
        let cmdsize = u32_at(cursor + 4)? as usize;
        if cmdsize == 0 {
            return None;
        }
        if (cmd == macho::LC_DYLD_INFO || cmd == macho::LC_DYLD_INFO_ONLY) && cmdsize >= 48 {
            return Some(DyldInfo {
                command_offset: cursor,
                rebase: (u32_at(cursor + 8)? as u64, u32_at(cursor + 12)? as u64),
                export: (u32_at(cursor + 40)? as u64, u32_at(cursor + 44)? as u64),
            });
        }
        cursor += cmdsize;
    }
    None
}

fn uleb(opcodes: &[u8], at: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let byte = *opcodes.get(*at).ok_or("rebase opcodes: truncated ULEB")?;
        *at += 1;
        if shift < 64 {
            value |= u64::from(byte & 0x7f) << shift;
        }
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}

/// Decode a rebase opcode stream into the pointer locations it rebases.
/// Only `REBASE_TYPE_POINTER` is modelled (the only type arm64 linkers emit);
/// anything else is an error so callers refuse rather than mis-shift.
pub fn rebase_locations(opcodes: &[u8]) -> Result<Vec<RebaseLocation>, String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    let mut segment: Option<usize> = None;
    let mut offset = 0u64;
    let mut kind = REBASE_TYPE_POINTER;
    let mut emit = |segment: Option<usize>, offset: u64, kind: u8| -> Result<(), String> {
        let segment = segment.ok_or("rebase opcodes: rebase before a segment was set")?;
        if kind != REBASE_TYPE_POINTER {
            return Err(format!("rebase opcodes: unsupported rebase type {kind}"));
        }
        out.push(RebaseLocation { segment, offset });
        Ok(())
    };
    while at < opcodes.len() {
        let byte = opcodes[at];
        at += 1;
        let imm = byte & REBASE_IMMEDIATE_MASK;
        match byte & REBASE_OPCODE_MASK {
            REBASE_OPCODE_DONE => break,
            REBASE_OPCODE_SET_TYPE_IMM => kind = imm,
            REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB => {
                segment = Some(imm as usize);
                offset = uleb(opcodes, &mut at)?;
            }
            REBASE_OPCODE_ADD_ADDR_ULEB => offset = offset.wrapping_add(uleb(opcodes, &mut at)?),
            REBASE_OPCODE_ADD_ADDR_IMM_SCALED => offset = offset.wrapping_add(imm as u64 * POINTER_SIZE),
            REBASE_OPCODE_DO_REBASE_IMM_TIMES => {
                for _ in 0..imm {
                    emit(segment, offset, kind)?;
                    offset = offset.wrapping_add(POINTER_SIZE);
                }
            }
            REBASE_OPCODE_DO_REBASE_ULEB_TIMES => {
                for _ in 0..uleb(opcodes, &mut at)? {
                    emit(segment, offset, kind)?;
                    offset = offset.wrapping_add(POINTER_SIZE);
                }
            }
            REBASE_OPCODE_DO_REBASE_ADD_ADDR_ULEB => {
                emit(segment, offset, kind)?;
                offset = offset.wrapping_add(uleb(opcodes, &mut at)?).wrapping_add(POINTER_SIZE);
            }
            REBASE_OPCODE_DO_REBASE_ULEB_TIMES_SKIPPING_ULEB => {
                let count = uleb(opcodes, &mut at)?;
                let skip = uleb(opcodes, &mut at)?;
                for _ in 0..count {
                    emit(segment, offset, kind)?;
                    offset = offset.wrapping_add(skip).wrapping_add(POINTER_SIZE);
                }
            }
            other => return Err(format!("rebase opcodes: unknown opcode 0x{other:02x}")),
        }
    }
    Ok(out)
}

/// After a `__TEXT` grow of `delta` bytes inserted at file offset
/// `growth_point`, add `delta` to every rebased pointer (in `bytes`, the grown
/// image) whose target lies at or past the growth point. `segments` is the
/// ORIGINAL (pre-grow) segment list. Returns how many pointers were shifted.
pub fn shift_rebased_pointers(
    bytes: &mut [u8],
    segments: &[MachOSegment],
    locations: &[RebaseLocation],
    growth_point: u64,
    delta: u64,
) -> Result<usize, String> {
    // The growth point is a file offset inside __TEXT; targets are vmaddrs.
    let text = segments
        .iter()
        .find(|s| s.filesize > 0 && s.fileoff <= growth_point && growth_point < s.fileoff + s.filesize)
        .ok_or("rebase shift: growth point is not inside a file-backed segment")?;
    let growth_va = text.vmaddr + (growth_point - text.fileoff);
    let mut shifted = 0;
    for loc in locations {
        let seg = segments
            .get(loc.segment)
            .ok_or_else(|| format!("rebase shift: segment index {} out of range", loc.segment))?;
        if loc.offset + POINTER_SIZE > seg.filesize {
            return Err(format!(
                "rebase shift: location {}+0x{:x} is outside the file-backed part of {}",
                loc.segment, loc.offset, seg.name
            ));
        }
        let original = seg.fileoff + loc.offset;
        let at = if original >= growth_point { original + delta } else { original } as usize;
        let slot = bytes
            .get_mut(at..at + POINTER_SIZE as usize)
            .ok_or("rebase shift: location past end of image")?;
        let value = u64::from_le_bytes(slot.as_ref().try_into().unwrap());
        if value >= growth_va {
            slot.copy_from_slice(&(value + delta).to_le_bytes());
            shifted += 1;
        }
    }
    Ok(shifted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(name: &str, vmaddr: u64, fileoff: u64, filesize: u64) -> MachOSegment {
        MachOSegment {
            name: name.into(),
            vmaddr,
            vmsize: filesize,
            fileoff,
            filesize,
            maxprot: 0,
            initprot: 0,
            flags: 0,
        }
    }

    #[test]
    fn decodes_every_rebase_opcode_form() {
        let ops = [
            REBASE_OPCODE_SET_TYPE_IMM | REBASE_TYPE_POINTER,
            REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB | 2, 0x10, // seg 2 @ 0x10
            REBASE_OPCODE_DO_REBASE_IMM_TIMES | 2,                // 0x10, 0x18
            REBASE_OPCODE_ADD_ADDR_IMM_SCALED | 1,                // skip 8 → 0x28
            REBASE_OPCODE_DO_REBASE_ADD_ADDR_ULEB, 0x08,          // 0x28, then +16 → 0x38
            REBASE_OPCODE_DO_REBASE_ULEB_TIMES_SKIPPING_ULEB, 2, 8, // 0x38, 0x48 → 0x58
            REBASE_OPCODE_ADD_ADDR_ULEB, 0x08,                    // 0x60
            REBASE_OPCODE_DO_REBASE_ULEB_TIMES, 1,                // 0x60
            REBASE_OPCODE_DONE,
            REBASE_OPCODE_DO_REBASE_IMM_TIMES | 1,                // after DONE: ignored
        ];
        let offsets: Vec<u64> = rebase_locations(&ops).unwrap().iter().map(|l| l.offset).collect();
        assert_eq!(offsets, vec![0x10, 0x18, 0x28, 0x38, 0x48, 0x60]);
        assert!(rebase_locations(&ops).unwrap().iter().all(|l| l.segment == 2));
    }

    #[test]
    fn rejects_streams_it_cannot_model() {
        assert!(rebase_locations(&[REBASE_OPCODE_DO_REBASE_IMM_TIMES | 1]).is_err());
        assert!(rebase_locations(&[REBASE_OPCODE_SET_TYPE_IMM | 2,
            REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB | 1, 0, REBASE_OPCODE_DO_REBASE_IMM_TIMES | 1]).is_err());
        assert!(rebase_locations(&[REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB | 1, 0x80]).is_err());
    }

    #[test]
    fn shifts_only_pointers_into_the_moved_range() {
        // __TEXT: file 0..0x1000 @ va 0x1_0000_0000, grown by 0x4000 at 0x800.
        // __DATA: file 0x1000..0x1100 (shifted to 0x5000 in the grown image).
        let segments = vec![
            seg("__PAGEZERO", 0, 0, 0),
            seg("__TEXT", 0x1_0000_0000, 0, 0x1000),
            seg("__DATA", 0x1_0000_1000, 0x1000, 0x100),
        ];
        let mut grown = vec![0u8; 0x5100];
        let put = |b: &mut Vec<u8>, at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        put(&mut grown, 0x5000, 0x1_0000_0100); // header region: before the growth point
        put(&mut grown, 0x5008, 0x1_0000_0900); // __text: moved
        put(&mut grown, 0x5010, 0x1_0000_1040); // __DATA itself: moved
        let locs: Vec<_> = (0..3).map(|i| RebaseLocation { segment: 2, offset: i * 8 }).collect();
        let n = shift_rebased_pointers(&mut grown, &segments, &locs, 0x800, 0x4000).unwrap();
        assert_eq!(n, 2);
        let get = |at: usize| u64::from_le_bytes(grown[at..at + 8].try_into().unwrap());
        assert_eq!(get(0x5000), 0x1_0000_0100);
        assert_eq!(get(0x5008), 0x1_0000_4900);
        assert_eq!(get(0x5010), 0x1_0000_5040);
    }
}
