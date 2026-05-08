//! GNU hash table emission for ELF `.gnu.hash` sections.
//!
//! `.gnu.hash` is the dynamic linker's accelerator for symbol
//! lookup. Layout (little-endian 64-bit ELF):
//!
//!   header (16 bytes):
//!     u32 nbuckets        — number of bucket entries
//!     u32 symbol_base     — first dynsym index that participates
//!                           in the hash (typically the first
//!                           non-LOCAL, non-UND entry).
//!     u32 bloom_size      — bloom filter length in 64-bit words
//!     u32 bloom_shift     — second-bit shift amount
//!
//!   bloom filter:
//!     bloom_size × u64 — Bloom filter bits, two per hashed name
//!
//!   buckets:
//!     nbuckets × u32 — each is the dynsym index of the first
//!                      chain entry for that bucket, or 0 for empty
//!
//!   chains:
//!     (dynsym_count - symbol_base) × u32 — each is the GNU hash
//!     of the corresponding dynsym entry's name, with bit 0 set
//!     iff this is the last entry in its bucket's chain
//!
//! Lookup walks bucket → chain, comparing `hash | 1 == chain[i] | 1`
//! before doing a string compare to confirm a name match. The bloom
//! filter prunes names that are definitely not in the table.
//!
//! ## Layout invariant
//!
//! For chains to be contiguous in dynsym, all hashable entries
//! sharing the same `hash % nbuckets` must appear consecutively in
//! dynsym, in source order. The simplest way to *guarantee* this
//! when regenerating is `nbuckets = 1`: every hashable symbol goes
//! in the single bucket and the chain reads dynsym in order. The
//! per-symbol cost is one chain comparison per lookup attempt;
//! still much better than O(n) iteration.
//!
//! For inputs that already use larger `nbuckets`, callers can
//! preserve the original bucket count when their changes are
//! purely *additive at the end of dynsym* and the new symbol's
//! `hash % nbuckets` happens to match the last existing chain.
//! When in doubt, [`build_gnu_hash`] takes an explicit
//! `nbuckets` so callers can pick.

/// Compute the GNU hash of a symbol name.
///
/// ```text
///   h = 5381
///   for c in name:
///       h = (h * 33 + c) mod 2^32
/// ```
///
/// Strings are hashed as their raw bytes (no decoding); the
/// dynamic linker uses the same hash on `.dynstr`-stored bytes.
pub fn gnu_hash(name: &[u8]) -> u32 {
    let mut h: u32 = 5381;
    for &c in name {
        h = h.wrapping_mul(33).wrapping_add(c as u32);
    }
    h
}

/// Input to [`build_gnu_hash`]: one hashable dynsym entry. The
/// caller passes these in dynsym order, starting from
/// `symbol_base`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HashableSymbol<'a> {
    /// Dynsym index of this symbol (>= symbol_base).
    pub dynsym_index: u32,
    /// Symbol name as it appears in `.dynstr`. Hashed verbatim.
    pub name: &'a [u8],
}

/// Build the bytes of a `.gnu.hash` section from the given
/// hashable dynsym entries.
///
/// `symbol_base` is the first dynsym index that participates;
/// dynsym entries below this (LOCAL, UND, etc.) are excluded
/// from the hash. `nbuckets` chooses the bucket count.
///
/// `bloom_size` must be a power of two. `bloom_shift` is the
/// shift used to derive the second bloom bit; common values are
/// `6` for 64-bit class (matches what the GNU linker emits for
/// small libraries).
///
/// The returned bytes are valid `.gnu.hash` content for any
/// dynsym whose hashable entries are exactly the ones passed in,
/// in the order given. Caller is responsible for ensuring
/// dynsym's actual layout matches that order — particularly that
/// chain contiguity holds (see module-level docs).
pub fn build_gnu_hash(
    symbols: &[HashableSymbol<'_>],
    symbol_base: u32,
    nbuckets: u32,
    bloom_size: u32,
    bloom_shift: u32,
) -> Vec<u8> {
    assert!(nbuckets > 0, "nbuckets must be at least 1");
    assert!(bloom_size > 0, "bloom_size must be at least 1");
    assert!(
        bloom_size.is_power_of_two(),
        "bloom_size must be a power of two; got {bloom_size}",
    );

    // Pre-compute hashes for each symbol.
    let hashes: Vec<u32> = symbols.iter().map(|s| gnu_hash(s.name)).collect();

    // Header.
    let mut bytes = Vec::with_capacity(
        16 + (bloom_size as usize) * 8 + (nbuckets as usize) * 4 + symbols.len() * 4,
    );
    bytes.extend_from_slice(&nbuckets.to_le_bytes());
    bytes.extend_from_slice(&symbol_base.to_le_bytes());
    bytes.extend_from_slice(&bloom_size.to_le_bytes());
    bytes.extend_from_slice(&bloom_shift.to_le_bytes());

    // Bloom filter.
    let mut bloom = vec![0u64; bloom_size as usize];
    let bloom_mask = (bloom_size as u64) - 1;
    // ELFCLASS64 uses 64-bit bloom words. The two bit positions
    // for a name are `hash % 64` and `(hash >> bloom_shift) % 64`,
    // both within the bloom word at `(hash / 64) % bloom_size`.
    for &h in &hashes {
        let word_index = ((h as u64) / 64) & bloom_mask;
        let bit_a = h & 63;
        let bit_b = (h >> bloom_shift) & 63;
        bloom[word_index as usize] |= (1u64 << bit_a) | (1u64 << bit_b);
    }
    for word in &bloom {
        bytes.extend_from_slice(&word.to_le_bytes());
    }

    // Bucket and chain construction. Group symbols by
    // `hash % nbuckets`. For correctness chains must be
    // contiguous in dynsym order, which the caller has already
    // ensured. We just discover where each bucket's chain
    // starts and which entry is its last.
    let mut buckets = vec![0u32; nbuckets as usize];
    let mut chain_words = vec![0u32; symbols.len()];

    let mut current_bucket: Option<u32> = None;
    for (i, sym) in symbols.iter().enumerate() {
        let bucket = hashes[i] % nbuckets;
        if Some(bucket) != current_bucket {
            // First symbol of a new bucket. The previous chain's
            // last entry needs its low bit set; we set it lazily
            // by handling "is this index one before the start of
            // a new bucket" below.
            buckets[bucket as usize] = sym.dynsym_index;
            current_bucket = Some(bucket);
        }
        chain_words[i] = hashes[i] & !1;
    }
    // Mark the last entry of each chain. A chain ends at the
    // last symbol whose bucket matches the next-symbol's bucket
    // change boundary, or at the very last symbol.
    for i in 0..symbols.len() {
        let this_bucket = hashes[i] % nbuckets;
        let next_bucket = symbols
            .get(i + 1)
            .map(|_| hashes[i + 1] % nbuckets);
        if next_bucket != Some(this_bucket) {
            chain_words[i] |= 1;
        }
    }

    for &b in &buckets {
        bytes.extend_from_slice(&b.to_le_bytes());
    }
    for &c in &chain_words {
        bytes.extend_from_slice(&c.to_le_bytes());
    }

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnu_hash_known_values() {
        // Empty string hashes to the seed.
        assert_eq!(gnu_hash(b""), 5381);
        // Reference values verified against `python3 -c` running
        // the same djb2-with-mul-33 algorithm. Useful to lock
        // these in case someone "optimises" the function and
        // accidentally changes its semantics.
        assert_eq!(gnu_hash(b"puts"), 0x7c9c_7b11);
        assert_eq!(gnu_hash(b"main"), 0x7c9a_7f6a);
        // Symbols from libgreet.so. Cross-checked against the
        // chain bytes the GNU linker emitted into its .gnu.hash
        // (see tests/elf_runtime/fixtures/lib_demo/libgreet.so).
        assert_eq!(gnu_hash(b"greet_base"), 0x5f90_74b6);
        assert_eq!(gnu_hash(b"greet_double"), 0x8b29_3cd6);
        assert_eq!(gnu_hash(b"greet_offset"), 0xa427_2d02);
    }

    #[test]
    fn build_single_bucket_round_trips_one_symbol() {
        let symbols = vec![HashableSymbol {
            dynsym_index: 4,
            name: b"only",
        }];
        let bytes = build_gnu_hash(&symbols, /*symbol_base=*/ 4, 1, 1, 6);
        // Header (16) + bloom (8) + buckets (4) + chains (4) = 32.
        assert_eq!(bytes.len(), 32);
        // nbuckets = 1.
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1);
        // symbol_base = 4.
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 4);
        // bucket[0] = dynsym index 4.
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 4);
        // chain[0] = hash with low bit set (last entry of chain).
        let chain0 = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        assert_eq!(chain0 & !1, gnu_hash(b"only") & !1);
        assert_eq!(chain0 & 1, 1);
    }

    #[test]
    fn build_for_libgreet_so_hashable_symbols() {
        // libgreet.so's hashable dynsym entries (indices 8..12)
        // are: greet_base, _greet_unused_puts_anchor, greet_double,
        // greet_offset. With our regenerated layout (nbuckets=1)
        // every entry chains in one bucket, in dynsym order.
        let symbols = vec![
            HashableSymbol { dynsym_index: 8, name: b"greet_base" },
            HashableSymbol { dynsym_index: 9, name: b"_greet_unused_puts_anchor" },
            HashableSymbol { dynsym_index: 10, name: b"greet_double" },
            HashableSymbol { dynsym_index: 11, name: b"greet_offset" },
        ];
        let bytes = build_gnu_hash(&symbols, 8, 1, 1, 6);

        // bucket[0] points at dynsym index 8.
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 8);

        // Chains: hashes with bit 0 set on the last entry.
        let c0 = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        let c1 = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        let c2 = u32::from_le_bytes(bytes[36..40].try_into().unwrap());
        let c3 = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
        assert_eq!(c0, gnu_hash(b"greet_base") & !1);
        assert_eq!(c1, gnu_hash(b"_greet_unused_puts_anchor") & !1);
        assert_eq!(c2, gnu_hash(b"greet_double") & !1);
        assert_eq!(c3, (gnu_hash(b"greet_offset") & !1) | 1);
    }

    #[test]
    fn build_with_two_symbols_in_single_bucket() {
        // Two symbols both hash to bucket 0 (only one bucket).
        // Chain should have first entry's bit 0 unset, second
        // entry's bit 0 set (chain terminator).
        let symbols = vec![
            HashableSymbol {
                dynsym_index: 4,
                name: b"alpha",
            },
            HashableSymbol {
                dynsym_index: 5,
                name: b"beta",
            },
        ];
        let bytes = build_gnu_hash(&symbols, 4, 1, 1, 6);
        // Bucket[0] points at first symbol's dynsym index.
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 4);
        // chain[0] = alpha hash, low bit 0 (chain continues).
        let c0 = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        assert_eq!(c0 & 1, 0);
        // chain[1] = beta hash, low bit 1 (chain terminator).
        let c1 = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        assert_eq!(c1 & 1, 1);
    }
}
