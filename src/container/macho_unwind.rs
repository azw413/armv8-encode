//! `__TEXT,__unwind_info` (compact unwind) fix-up for a `__TEXT` grow.
//!
//! Compact unwind addresses functions, LSDAs and personality pointers by
//! *image offset* (from the mach header). A grow that inserts `delta` bytes at
//! `growth_point` moves everything past it, so those absolute offsets must gain
//! `delta` — otherwise the unwinder finds no (or the wrong) entry for a moved
//! function and a panic/exception aborts instead of unwinding. Offsets that are
//! relative (compressed second-level entries, DWARF-mode `__eh_frame` offsets,
//! intra-section page/array offsets) are left alone.

const UNWIND_SECTION_VERSION: u32 = 1;
const UNWIND_SECOND_LEVEL_REGULAR: u32 = 2;
const UNWIND_SECOND_LEVEL_COMPRESSED: u32 = 3;

fn rd(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| format!("unwind_info: read past end at 0x{at:x}"))
}

fn shift_at(b: &mut [u8], at: usize, growth_point: u64, delta: u64) -> Result<(), String> {
    let v = rd(b, at)? as u64;
    if v >= growth_point {
        let shifted = u32::try_from(v + delta).map_err(|_| "unwind_info: offset overflow")?;
        b[at..at + 4].copy_from_slice(&shifted.to_le_bytes());
    }
    Ok(())
}

/// Shift the image-offset fields of an `__unwind_info` section in place.
pub fn shift_unwind_info(section: &mut [u8], growth_point: u64, delta: u64) -> Result<(), String> {
    if rd(section, 0)? != UNWIND_SECTION_VERSION {
        return Err("unwind_info: unsupported version".into());
    }
    let personality_off = rd(section, 12)? as usize;
    let personality_count = rd(section, 16)? as usize;
    let index_off = rd(section, 20)? as usize;
    let index_count = rd(section, 24)? as usize;

    // Personality array: image offsets of the personality-pointer slots.
    for i in 0..personality_count {
        shift_at(section, personality_off + i * 4, growth_point, delta)?;
    }

    // Read every index entry before shifting anything: the LSDA array span of
    // entry i is [lsda_i, lsda_{i+1}).
    let entries: Vec<(u32, u32)> = (0..index_count)
        .map(|i| {
            let e = index_off + i * 12;
            Ok((rd(section, e + 4)?, rd(section, e + 8)?))
        })
        .collect::<Result<_, String>>()?;

    for i in 0..index_count {
        let e = index_off + i * 12;
        shift_at(section, e, growth_point, delta)?; // functionOffset
        let (second_level, _) = entries[i];
        if second_level != 0 {
            let page = second_level as usize;
            match rd(section, page)? {
                UNWIND_SECOND_LEVEL_REGULAR => {
                    let entry_page_off = u16::from_le_bytes(
                        section.get(page + 4..page + 6).ok_or("unwind_info: page")?.try_into().unwrap(),
                    ) as usize;
                    let count = u16::from_le_bytes(
                        section.get(page + 6..page + 8).ok_or("unwind_info: page")?.try_into().unwrap(),
                    ) as usize;
                    for j in 0..count {
                        shift_at(section, page + entry_page_off + j * 8, growth_point, delta)?;
                    }
                }
                // Compressed entries are 24-bit offsets relative to the index
                // entry's functionOffset (shifted above) — unchanged.
                UNWIND_SECOND_LEVEL_COMPRESSED => {}
                k => return Err(format!("unwind_info: unknown second-level page kind {k}")),
            }
        }
    }

    // LSDA index: the array runs from the first entry's lsda offset to the
    // last (sentinel) entry's; each record is (functionOffset, lsdaOffset).
    if let (Some(first), Some(last)) = (entries.first(), entries.last()) {
        let (start, end) = (first.1 as usize, last.1 as usize);
        if end < start || (end - start) % 8 != 0 {
            return Err("unwind_info: malformed LSDA index".into());
        }
        for at in (start..end).step_by(8) {
            shift_at(section, at, growth_point, delta)?;
            shift_at(section, at + 4, growth_point, delta)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(b: &mut Vec<u8>, at: usize, v: u32) {
        if b.len() < at + 4 {
            b.resize(at + 4, 0);
        }
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Header, 1 personality, 2 index entries (+ sentinel), a regular and a
    /// compressed second-level page, 1 LSDA record.
    fn sample() -> Vec<u8> {
        let mut b = vec![0u8; 0x100];
        put(&mut b, 0, 1); // version
        put(&mut b, 12, 0x40); // personality array off
        put(&mut b, 16, 1); // count
        put(&mut b, 20, 0x50); // index off
        put(&mut b, 24, 3); // index count (incl. sentinel)
        put(&mut b, 0x40, 0x9000); // personality slot (past growth point)
        // index entries: (functionOffset, secondLevel, lsdaIndex)
        for (i, (f, p, l)) in [(0x0100, 0x80, 0xc0), (0x2000, 0xa0, 0xc8), (0x3000, 0, 0xc8)]
            .iter()
            .enumerate()
        {
            put(&mut b, 0x50 + i * 12, *f);
            put(&mut b, 0x54 + i * 12, *p);
            put(&mut b, 0x58 + i * 12, *l);
        }
        // regular page @0x80: kind 2, entryPageOffset 8, count 2
        put(&mut b, 0x80, 2);
        b[0x84..0x86].copy_from_slice(&8u16.to_le_bytes());
        b[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
        put(&mut b, 0x88, 0x0100); // before growth point
        put(&mut b, 0x90, 0x1800); // after
        // compressed page @0xa0: entry 0x0000_0010 (relative) must not change
        put(&mut b, 0xa0, 3);
        put(&mut b, 0xac, 0x0000_0010);
        // LSDA record @0xc0
        put(&mut b, 0xc0, 0x2000);
        put(&mut b, 0xc4, 0x5000);
        b
    }

    #[test]
    fn shifts_absolute_image_offsets_only() {
        let mut b = sample();
        shift_unwind_info(&mut b, 0x1000, 0x4000).unwrap();
        let g = |at: usize| rd(&b, at).unwrap();
        assert_eq!(g(0x40), 0xd000, "personality slot");
        assert_eq!([g(0x50), g(0x5c), g(0x68)], [0x0100, 0x6000, 0x7000], "index functionOffsets");
        assert_eq!([g(0x54), g(0x58)], [0x80, 0xc0], "intra-section offsets untouched");
        assert_eq!([g(0x88), g(0x90)], [0x0100, 0x5800], "regular entries");
        assert_eq!(g(0xac), 0x10, "compressed entry is relative");
        assert_eq!([g(0xc0), g(0xc4)], [0x6000, 0x9000], "LSDA record");
    }

    #[test]
    fn rejects_unknown_layouts() {
        let mut b = sample();
        put(&mut b, 0, 2);
        assert!(shift_unwind_info(&mut b, 0x1000, 0x4000).is_err());
        let mut b = sample();
        put(&mut b, 0x80, 7);
        assert!(shift_unwind_info(&mut b, 0x1000, 0x4000).is_err());
    }
}
