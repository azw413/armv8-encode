//! Text-space allocator.
//!
//! Formalises "where does room for new code come from" into a small,
//! format-neutral bump allocator. New R-X code and read-only data are
//! carved out of a binary's *free space* — the pockets a linker leaves
//! behind:
//!
//! - **header pad** — the gap between the end of the load commands and
//!   the first section (Mach-O) / between the ELF/program headers and
//!   the first section. Opt-in, because it is the same pool load-command
//!   insertion draws on.
//! - **inter-section holes** — alignment padding between sections.
//! - **tail pad** — the slack between the last section's end and the
//!   segment's page-rounded end.
//!
//! When those are exhausted the caller may allow the segment to *grow*
//! at its tail (the expensive path that shifts following segments); that
//! decision is modelled here as [`Exhaustion::Grow`] but fulfilled by the
//! per-format writer, not by this module.
//!
//! This module is deliberately pure: it reasons over a list of
//! [`FreeExtent`]s and hands back a [`PlannedReservation`] plus a
//! [`Region`] bump allocator. It performs no I/O and knows nothing about
//! Mach-O / ELF / PE — the caller supplies the free extents and applies
//! the plan. That keeps the allocation logic exhaustively unit-testable
//! in isolation from the writers.

/// Where a chunk of reserved space came from. Reported back to the
/// caller so it can observe whether a reservation stayed free or forced
/// a segment grow, and enforce policies like "never touch header pad".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceSource {
    /// Alignment padding between two sections in the segment.
    InterSectionHole,
    /// Slack at the end of the segment, within its page-rounded size.
    TailPad,
    /// The gap between the load commands and the first section. Opt-in.
    HeaderPad,
    /// The segment was extended at its tail by `pages` pages, shifting
    /// everything after it. Never produced by [`plan_reservation`]; the
    /// writer records it when it fulfils an [`Exhaustion::Grow`] request.
    GrewText { pages: u64 },
}

impl SpaceSource {
    /// Preference rank — lower is cheaper / less disruptive, so it is
    /// consumed first. Inter-section holes and tail pad move nothing;
    /// header pad competes with load-command growth; a grow shifts
    /// following segments.
    fn rank(self) -> u8 {
        match self {
            SpaceSource::InterSectionHole => 0,
            SpaceSource::TailPad => 1,
            SpaceSource::HeaderPad => 2,
            SpaceSource::GrewText { .. } => 3,
        }
    }
}

/// One contiguous run of free bytes the allocator may hand out, within a
/// single segment. `address` and `file_offset` are the start of the run;
/// within one run they advance together (the writer maps the segment
/// with a fixed `vaddr − fileoff` skew), so a carve at `address + k`
/// lives at `file_offset + k`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeExtent {
    pub address: u64,
    pub file_offset: u64,
    pub len: u64,
    pub source: SpaceSource,
}

/// How a reservation may be fulfilled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReserveRequest {
    /// Bytes the caller needs available immediately.
    pub min_bytes: u64,
    /// Extra capacity to grab beyond `min_bytes` so later carves are
    /// free. Amortises the one-time cost of a grow: reserving generous
    /// head-room once avoids a second shift on the next placement.
    pub headroom: u64,
    /// Required alignment of the region base (bytes). Must be non-zero.
    pub align: u64,
    /// May header pad be harvested? Off by default because it is the
    /// same pool `add_library_dependency` draws on.
    pub allow_headerpad: bool,
    /// What to do when free space can't satisfy the request.
    pub on_exhaustion: Exhaustion,
}

impl ReserveRequest {
    /// A request for exactly `bytes`, 4-byte aligned, free-space only,
    /// no header pad. The conservative default: never triggers a grow.
    pub fn exact(bytes: u64) -> Self {
        ReserveRequest {
            min_bytes: bytes,
            headroom: 0,
            align: 4,
            allow_headerpad: false,
            on_exhaustion: Exhaustion::Fail,
        }
    }

    /// `min_bytes` now plus `headroom` slack for future carves.
    pub fn with_headroom(mut self, headroom: u64) -> Self {
        self.headroom = headroom;
        self
    }

    /// Total capacity the reservation asks for.
    fn needed(&self) -> u64 {
        self.min_bytes.saturating_add(self.headroom)
    }
}

/// What to do when free space is insufficient for a reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exhaustion {
    /// Extend the segment at its tail (shifts following segments).
    Grow,
    /// Fail without moving anything.
    Fail,
}

/// A reservation the caller should realise: a contiguous span at
/// `base_address` / `base_file_offset` of `capacity` bytes drawn from
/// `source`. Turn it into a live bump allocator with [`Region::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedReservation {
    pub base_address: u64,
    pub base_file_offset: u64,
    pub capacity: u64,
    pub align: u64,
    pub source: SpaceSource,
}

impl PlannedReservation {
    /// The bump allocator over this reservation's span.
    pub fn region(&self) -> Region {
        Region::new(
            self.base_address,
            self.base_file_offset,
            self.capacity,
            self.source,
        )
    }
}

/// Why a reservation could not be planned from free space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveError {
    /// `align` was zero.
    ZeroAlign,
    /// No single free extent could hold `needed` bytes after alignment,
    /// and the policy was [`Exhaustion::Fail`]. `largest_available` is
    /// the biggest aligned run found (0 if there was no free space).
    InsufficientFreeSpace { needed: u64, largest_available: u64 },
    /// Free space is insufficient and the policy is [`Exhaustion::Grow`]:
    /// the caller (writer) must extend the segment by at least `deficit`
    /// bytes. Not an error per se — a request for the grow path.
    GrowthRequired { deficit: u64 },
}

/// Plan a contiguous reservation from `free_extents`.
///
/// Picks the best-fitting allowed extent (smallest sufficient, then
/// cheapest source, then lowest address — fully deterministic), giving
/// the region a base aligned to `req.align` and a capacity of
/// `min_bytes + headroom`. Slack beyond the region stays free for later
/// reservations (the caller is expected to subtract handed-out ranges
/// before calling again).
///
/// `free_extents` need not be sorted and may overlap in `source` kind;
/// they must be genuinely free (the caller owns that invariant).
pub fn plan_reservation(
    free_extents: &[FreeExtent],
    req: &ReserveRequest,
) -> Result<PlannedReservation, ReserveError> {
    if req.align == 0 {
        return Err(ReserveError::ZeroAlign);
    }
    let needed = req.needed();

    // Evaluate every allowed extent for how much aligned room it offers.
    let mut best: Option<(u64, SpaceSource, u64, u64)> = None; // (usable, source, base_addr, base_off)
    let mut largest_available = 0u64;
    for ext in free_extents {
        if matches!(ext.source, SpaceSource::HeaderPad) && !req.allow_headerpad {
            continue;
        }
        // Growth extents are never offered as free space.
        if matches!(ext.source, SpaceSource::GrewText { .. }) {
            continue;
        }
        let Some(base_addr) = round_up(ext.address, req.align) else {
            continue;
        };
        let pad = base_addr - ext.address;
        if pad > ext.len {
            continue; // alignment ate the whole extent
        }
        let usable = ext.len - pad;
        if usable > largest_available {
            largest_available = usable;
        }
        if usable < needed {
            continue;
        }
        let base_off = ext.file_offset + pad;
        let candidate = (usable, ext.source, base_addr, base_off);
        best = Some(match best {
            None => candidate,
            Some(cur) => pick_better(cur, candidate),
        });
    }

    if let Some((_usable, source, base_address, base_file_offset)) = best {
        return Ok(PlannedReservation {
            base_address,
            base_file_offset,
            capacity: needed,
            align: req.align,
            source,
        });
    }

    match req.on_exhaustion {
        Exhaustion::Fail => Err(ReserveError::InsufficientFreeSpace {
            needed,
            largest_available,
        }),
        Exhaustion::Grow => Err(ReserveError::GrowthRequired {
            deficit: needed - largest_available,
        }),
    }
}

/// Best-fit tie-broken by cheaper source then lower address. Returns the
/// preferred of two candidates `(usable, source, base_addr, base_off)`.
fn pick_better(
    a: (u64, SpaceSource, u64, u64),
    b: (u64, SpaceSource, u64, u64),
) -> (u64, SpaceSource, u64, u64) {
    // Smaller usable wins (tighter fit → preserves large extents).
    match a.0.cmp(&b.0) {
        std::cmp::Ordering::Less => a,
        std::cmp::Ordering::Greater => b,
        std::cmp::Ordering::Equal => match a.1.rank().cmp(&b.1.rank()) {
            std::cmp::Ordering::Less => a,
            std::cmp::Ordering::Greater => b,
            std::cmp::Ordering::Equal => {
                if a.2 <= b.2 {
                    a
                } else {
                    b
                }
            }
        },
    }
}

/// A reserved contiguous span, sub-allocated front-to-back. Every
/// [`Region::carve`] hands out a fresh, non-overlapping, aligned slice;
/// overlap is impossible by construction (the cursor only advances).
#[derive(Debug, Clone)]
pub struct Region {
    base_address: u64,
    base_file_offset: u64,
    capacity: u64,
    cursor: u64, // bytes consumed from base (incl. alignment padding)
    source: SpaceSource,
}

/// One sub-allocation out of a [`Region`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carve {
    /// Virtual address of the carved slice.
    pub address: u64,
    /// File offset of the carved slice.
    pub file_offset: u64,
    /// Byte offset of the slice from the region base.
    pub offset_in_region: u64,
    /// Length of the slice.
    pub len: u64,
}

/// Why a carve failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarveError {
    /// `align` was zero.
    ZeroAlign,
    /// The region doesn't have `requested` bytes left after aligning the
    /// cursor. `available` is what remained before this carve.
    RegionFull { requested: u64, available: u64 },
    /// Address arithmetic overflowed `u64` (pathological inputs).
    Overflow,
}

impl Region {
    /// A fresh bump allocator over `[base_address, base_address+capacity)`.
    pub fn new(base_address: u64, base_file_offset: u64, capacity: u64, source: SpaceSource) -> Self {
        Region {
            base_address,
            base_file_offset,
            capacity,
            cursor: 0,
            source,
        }
    }

    pub fn base_address(&self) -> u64 {
        self.base_address
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn used(&self) -> u64 {
        self.cursor
    }

    pub fn remaining(&self) -> u64 {
        self.capacity - self.cursor
    }

    pub fn source(&self) -> SpaceSource {
        self.source
    }

    /// Carve `len` bytes aligned to `align` from the region. Advances the
    /// cursor past any alignment padding plus `len`. `len` may be 0 (a
    /// pure alignment marker). Fails without mutating the cursor if the
    /// slice wouldn't fit.
    pub fn carve(&mut self, len: u64, align: u64) -> Result<Carve, CarveError> {
        if align == 0 {
            return Err(CarveError::ZeroAlign);
        }
        let cur_addr = self
            .base_address
            .checked_add(self.cursor)
            .ok_or(CarveError::Overflow)?;
        let aligned_addr = round_up(cur_addr, align).ok_or(CarveError::Overflow)?;
        let pad = aligned_addr - cur_addr;
        // New cursor = old cursor + pad + len, guarding overflow.
        let new_cursor = self
            .cursor
            .checked_add(pad)
            .and_then(|c| c.checked_add(len))
            .ok_or(CarveError::Overflow)?;
        if new_cursor > self.capacity {
            return Err(CarveError::RegionFull {
                requested: len,
                available: self.capacity - self.cursor,
            });
        }
        let offset_in_region = self.cursor + pad;
        let carve = Carve {
            address: aligned_addr,
            file_offset: self.base_file_offset + offset_in_region,
            offset_in_region,
            len,
        };
        self.cursor = new_cursor;
        Ok(carve)
    }
}

/// Round `value` up to the next multiple of `align` (any `align >= 1`).
/// Returns `None` on overflow. `align` is assumed non-zero by callers.
fn round_up(value: u64, align: u64) -> Option<u64> {
    debug_assert!(align != 0);
    let rem = value % align;
    if rem == 0 {
        Some(value)
    } else {
        value.checked_add(align - rem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ext(address: u64, file_offset: u64, len: u64, source: SpaceSource) -> FreeExtent {
        FreeExtent {
            address,
            file_offset,
            len,
            source,
        }
    }

    // ---- round_up -----------------------------------------------------

    #[test]
    fn round_up_basics() {
        assert_eq!(round_up(0, 4), Some(0));
        assert_eq!(round_up(1, 4), Some(4));
        assert_eq!(round_up(4, 4), Some(4));
        assert_eq!(round_up(5, 4), Some(8));
        assert_eq!(round_up(4095, 4096), Some(4096));
        assert_eq!(round_up(4096, 4096), Some(4096));
        // Non-power-of-two alignment is still handled.
        assert_eq!(round_up(10, 3), Some(12));
    }

    #[test]
    fn round_up_overflow_is_none() {
        assert_eq!(round_up(u64::MAX, 4096), None);
        assert_eq!(round_up(u64::MAX - 1, 8), None);
    }

    // ---- Region::carve ------------------------------------------------

    #[test]
    fn carve_sequential_is_contiguous_and_nonoverlapping() {
        let mut r = Region::new(0x4000, 0x100, 0x400, SpaceSource::TailPad);
        let a = r.carve(0x40, 4).unwrap();
        let b = r.carve(0x30, 4).unwrap();
        let c = r.carve(0x10, 4).unwrap();
        assert_eq!(a.address, 0x4000);
        assert_eq!(a.file_offset, 0x100);
        assert_eq!(b.address, 0x4040); // right after a, 4-aligned
        assert_eq!(c.address, 0x4070);
        // No overlap: each start >= previous end.
        assert!(b.address >= a.address + a.len);
        assert!(c.address >= b.address + b.len);
        assert_eq!(r.used(), 0x70 + 0x10);
        assert_eq!(r.remaining(), 0x400 - (0x70 + 0x10));
    }

    #[test]
    fn carve_applies_alignment_padding() {
        let mut r = Region::new(0x4000, 0, 0x1000, SpaceSource::TailPad);
        let a = r.carve(1, 4).unwrap(); // 1 byte at 0x4000
        assert_eq!(a.address, 0x4000);
        let b = r.carve(8, 16).unwrap(); // must jump 0x4001 -> 0x4010
        assert_eq!(b.address, 0x4010);
        assert_eq!(b.offset_in_region, 0x10);
        // file_offset tracks the address skew.
        assert_eq!(b.file_offset, 0x10);
    }

    #[test]
    fn carve_file_offset_tracks_address_skew() {
        // A region whose file offset and vaddr differ by a fixed skew.
        let mut r = Region::new(0x1_0000, 0x8000, 0x200, SpaceSource::TailPad);
        let a = r.carve(0x20, 8).unwrap();
        assert_eq!(a.address - r.base_address(), a.file_offset - 0x8000);
        let b = r.carve(0x40, 8).unwrap();
        assert_eq!(b.address - 0x1_0000, b.file_offset - 0x8000);
    }

    #[test]
    fn carve_exact_capacity_succeeds_then_full() {
        let mut r = Region::new(0, 0, 0x40, SpaceSource::TailPad);
        assert!(r.carve(0x40, 1).is_ok());
        assert_eq!(r.remaining(), 0);
        assert_eq!(
            r.carve(1, 1),
            Err(CarveError::RegionFull {
                requested: 1,
                available: 0
            })
        );
    }

    #[test]
    fn carve_overflowing_request_fails_without_advancing_cursor() {
        let mut r = Region::new(0, 0, 0x40, SpaceSource::TailPad);
        r.carve(0x20, 4).unwrap();
        let before = r.used();
        assert_eq!(
            r.carve(0x30, 4),
            Err(CarveError::RegionFull {
                requested: 0x30,
                available: 0x20
            })
        );
        assert_eq!(r.used(), before, "failed carve must not consume space");
    }

    #[test]
    fn carve_alignment_pad_counted_against_capacity() {
        // Capacity 0x18: one byte at 0, then a 16-aligned 8-byte carve
        // needs offset 0x10..0x18 — exactly fits.
        let mut r = Region::new(0, 0, 0x18, SpaceSource::TailPad);
        r.carve(1, 1).unwrap();
        assert!(r.carve(8, 16).is_ok());
        assert_eq!(r.remaining(), 0);
        // With capacity one short, the aligned carve must fail.
        let mut r2 = Region::new(0, 0, 0x17, SpaceSource::TailPad);
        r2.carve(1, 1).unwrap();
        assert!(matches!(
            r2.carve(8, 16),
            Err(CarveError::RegionFull { .. })
        ));
    }

    #[test]
    fn carve_zero_length_is_an_alignment_marker() {
        let mut r = Region::new(0x4002, 0, 0x100, SpaceSource::TailPad);
        let a = r.carve(0, 8).unwrap(); // aligns to 0x4008, len 0
        assert_eq!(a.address, 0x4008);
        assert_eq!(a.len, 0);
        // Cursor advanced past the alignment padding only.
        assert_eq!(r.used(), 0x4008 - 0x4002);
    }

    #[test]
    fn carve_zero_align_rejected() {
        let mut r = Region::new(0, 0, 0x10, SpaceSource::TailPad);
        assert_eq!(r.carve(4, 0), Err(CarveError::ZeroAlign));
    }

    #[test]
    fn carve_overflow_is_caught_not_panicked() {
        let mut r = Region::new(u64::MAX - 3, 0, u64::MAX, SpaceSource::TailPad);
        // Aligning near u64::MAX overflows; must be an error, never a panic.
        assert_eq!(r.carve(1, 4096), Err(CarveError::Overflow));
    }

    // ---- plan_reservation --------------------------------------------

    #[test]
    fn plan_picks_only_extent_that_fits() {
        let extents = [
            ext(0x1000, 0x1000, 0x20, SpaceSource::InterSectionHole),
            ext(0x4000, 0x4000, 0x800, SpaceSource::TailPad),
        ];
        let plan = plan_reservation(&extents, &ReserveRequest::exact(0x100)).unwrap();
        assert_eq!(plan.base_address, 0x4000);
        assert_eq!(plan.base_file_offset, 0x4000);
        assert_eq!(plan.capacity, 0x100);
        assert_eq!(plan.source, SpaceSource::TailPad);
    }

    #[test]
    fn plan_best_fit_prefers_smallest_sufficient() {
        let extents = [
            ext(0x1000, 0x1000, 0x1000, SpaceSource::TailPad),
            ext(0x8000, 0x8000, 0x200, SpaceSource::TailPad), // tighter fit
            ext(0x9000, 0x9000, 0x400, SpaceSource::TailPad),
        ];
        let plan = plan_reservation(&extents, &ReserveRequest::exact(0x180)).unwrap();
        assert_eq!(plan.base_address, 0x8000, "should pick the tightest fit");
    }

    #[test]
    fn plan_tie_break_prefers_cheaper_source_then_lower_address() {
        // Two equal-size fits: an inter-section hole (rank 0) should win
        // over a tail pad (rank 1) of the same size.
        let extents = [
            ext(0x9000, 0x9000, 0x200, SpaceSource::TailPad),
            ext(0x2000, 0x2000, 0x200, SpaceSource::InterSectionHole),
        ];
        let plan = plan_reservation(&extents, &ReserveRequest::exact(0x100)).unwrap();
        assert_eq!(plan.source, SpaceSource::InterSectionHole);
        assert_eq!(plan.base_address, 0x2000);
    }

    #[test]
    fn plan_headroom_included_in_capacity() {
        let extents = [ext(0x4000, 0x4000, 0x1000, SpaceSource::TailPad)];
        let req = ReserveRequest::exact(0x100).with_headroom(0x300);
        let plan = plan_reservation(&extents, &req).unwrap();
        assert_eq!(plan.capacity, 0x400);
    }

    #[test]
    fn plan_respects_alignment_when_measuring_fit() {
        // Extent of 0x110 bytes at 0x4008; a 0x1000-aligned request wastes
        // 0xff8 to reach 0x5000, so it no longer fits.
        let extents = [ext(0x4008, 0x4008, 0x110, SpaceSource::TailPad)];
        let mut req = ReserveRequest::exact(0x100);
        req.align = 0x1000;
        assert!(matches!(
            plan_reservation(&extents, &req),
            Err(ReserveError::InsufficientFreeSpace { .. })
        ));
        // 4-byte aligned, the same extent fits fine.
        let plan = plan_reservation(&extents, &ReserveRequest::exact(0x100)).unwrap();
        assert_eq!(plan.base_address, 0x4008);
    }

    #[test]
    fn plan_headerpad_excluded_unless_opted_in() {
        let extents = [ext(0x200, 0x200, 0x400, SpaceSource::HeaderPad)];
        // Default: header pad off -> no fit.
        assert!(matches!(
            plan_reservation(&extents, &ReserveRequest::exact(0x100)),
            Err(ReserveError::InsufficientFreeSpace { .. })
        ));
        // Opt in -> header pad is usable.
        let mut req = ReserveRequest::exact(0x100);
        req.allow_headerpad = true;
        let plan = plan_reservation(&extents, &req).unwrap();
        assert_eq!(plan.source, SpaceSource::HeaderPad);
    }

    #[test]
    fn plan_insufficient_reports_largest_available() {
        let extents = [
            ext(0x1000, 0x1000, 0x40, SpaceSource::InterSectionHole),
            ext(0x4000, 0x4000, 0x80, SpaceSource::TailPad),
        ];
        assert_eq!(
            plan_reservation(&extents, &ReserveRequest::exact(0x100)),
            Err(ReserveError::InsufficientFreeSpace {
                needed: 0x100,
                largest_available: 0x80,
            })
        );
    }

    #[test]
    fn plan_grow_policy_reports_deficit() {
        let extents = [ext(0x4000, 0x4000, 0x80, SpaceSource::TailPad)];
        let req = ReserveRequest {
            min_bytes: 0x100,
            headroom: 0x100,
            align: 4,
            allow_headerpad: false,
            on_exhaustion: Exhaustion::Grow,
        };
        // needed = 0x200, largest free = 0x80 -> deficit 0x180.
        assert_eq!(
            plan_reservation(&extents, &req),
            Err(ReserveError::GrowthRequired { deficit: 0x180 })
        );
    }

    #[test]
    fn plan_grow_deficit_zero_free_space() {
        let req = ReserveRequest {
            min_bytes: 0x100,
            headroom: 0,
            align: 4,
            allow_headerpad: false,
            on_exhaustion: Exhaustion::Grow,
        };
        assert_eq!(
            plan_reservation(&[], &req),
            Err(ReserveError::GrowthRequired { deficit: 0x100 })
        );
    }

    #[test]
    fn plan_zero_align_rejected() {
        let extents = [ext(0x4000, 0x4000, 0x800, SpaceSource::TailPad)];
        let mut req = ReserveRequest::exact(0x100);
        req.align = 0;
        assert_eq!(plan_reservation(&extents, &req), Err(ReserveError::ZeroAlign));
    }

    #[test]
    fn plan_grow_extents_are_never_offered_as_free() {
        let extents = [ext(0x4000, 0x4000, 0x1000, SpaceSource::GrewText { pages: 1 })];
        assert!(matches!(
            plan_reservation(&extents, &ReserveRequest::exact(0x10)),
            Err(ReserveError::InsufficientFreeSpace { .. })
        ));
    }

    #[test]
    fn plan_then_region_carve_round_trips() {
        let extents = [ext(0x4000, 0x2000, 0x1000, SpaceSource::TailPad)];
        let req = ReserveRequest::exact(0x100).with_headroom(0x300);
        let plan = plan_reservation(&extents, &req).unwrap();
        let mut region = plan.region();
        assert_eq!(region.capacity(), 0x400);
        let f = region.carve(0x80, 16).unwrap();
        assert_eq!(f.address, 0x4000);
        assert_eq!(f.file_offset, 0x2000);
        let g = region.carve(0x40, 16).unwrap();
        assert_eq!(g.address, 0x4080);
        assert_eq!(g.file_offset, 0x2080);
        // 0x80 + 0x40 used = 0xc0; 0x340 remains.
        assert_eq!(region.remaining(), 0x340);
        assert!(region.carve(0x341, 1).is_err()); // one past capacity
        assert!(region.carve(0x340, 1).is_ok()); // exact fit
        assert_eq!(region.remaining(), 0);
    }

    #[test]
    fn plan_empty_free_list_fails_cleanly() {
        assert_eq!(
            plan_reservation(&[], &ReserveRequest::exact(0x100)),
            Err(ReserveError::InsufficientFreeSpace {
                needed: 0x100,
                largest_available: 0,
            })
        );
    }

    #[test]
    fn plan_zero_byte_reservation_is_allowed() {
        let extents = [ext(0x4000, 0x4000, 0x10, SpaceSource::TailPad)];
        let plan = plan_reservation(&extents, &ReserveRequest::exact(0)).unwrap();
        assert_eq!(plan.capacity, 0);
        assert_eq!(plan.base_address, 0x4000);
    }

}
