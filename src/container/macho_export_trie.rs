//! Mach-O export trie parser + builder.
//!
//! The export trie (referenced by `LC_DYLD_EXPORTS_TRIE` or the
//! older `LC_DYLD_INFO_ONLY.export_off` field) is a radix tree
//! mapping symbol names to their export information. dyld walks
//! it during `dlsym(RTLD_DEFAULT, name)` lookups.
//!
//! ## Wire format
//!
//! Each node is encoded as:
//!
//! ```text
//!   terminal_size: ULEB128
//!   if terminal_size > 0:
//!     terminal_data:
//!       flags: ULEB128
//!       if flags & EXPORT_SYMBOL_FLAGS_REEXPORT:
//!         ordinal: ULEB128
//!         importedName: cstring
//!       elif flags & EXPORT_SYMBOL_FLAGS_STUB_AND_RESOLVER:
//!         stub: ULEB128
//!         resolver: ULEB128
//!       else:
//!         address: ULEB128
//!   child_count: u8
//!   for each child:
//!     edge_string: cstring (NUL-terminated)
//!     child_offset: ULEB128 (absolute byte offset within the trie)
//! ```
//!
//! Child offsets are absolute byte positions inside the trie
//! blob — that's why building the trie is a fixed-point
//! computation (offsets depend on encoded sizes which depend on
//! offsets).
//!
//! Phase 5 only emits "regular" exports
//! (flags = EXPORT_SYMBOL_FLAGS_KIND_REGULAR | _DEFAULT,
//! encoded as 0x0); reexports and stub-and-resolver are not
//! generated, but we preserve them in the parser when reading
//! existing tries so we can faithfully round-trip the export
//! list.

use crate::container::ContainerWriteError;

/// One exported symbol from the Mach-O export trie.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MachOExport {
    /// Full symbol name including any underscore prefix
    /// (e.g. `_greet_double`).
    pub name: String,
    /// Export flags. Phase 5 builders emit `0`
    /// (EXPORT_SYMBOL_FLAGS_KIND_REGULAR | DEFAULT).
    pub flags: u64,
    /// vmaddr-relative offset of the symbol's value, encoded
    /// as ULEB128 in regular exports. For our purposes this is
    /// the symbol's vmaddr minus the dylib's base vmaddr (=
    /// just the vmaddr for typical dylibs that load at vmaddr
    /// 0x0).
    pub address_offset: u64,
}

/// Parse an export trie blob into a flat list of exports.
///
/// The parser walks the trie depth-first, accumulating the
/// path string from root to each terminal node. Returns the
/// exports in trie-traversal order (which is alphabetical for
/// a well-formed trie).
pub fn parse(bytes: &[u8]) -> Result<Vec<MachOExport>, ContainerWriteError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut stack: Vec<(usize, String)> = vec![(0, String::new())];
    while let Some((node_off, prefix)) = stack.pop() {
        if node_off >= bytes.len() {
            return Err(ContainerWriteError::ObjectWrite(format!(
                "export trie: node offset {node_off} past trie size {}",
                bytes.len(),
            )));
        }
        let mut cursor = node_off;
        let terminal_size = read_uleb128(bytes, &mut cursor)?;
        let after_terminal = cursor + terminal_size as usize;
        if after_terminal > bytes.len() {
            return Err(ContainerWriteError::ObjectWrite(
                "export trie: terminal_size extends past trie end".into(),
            ));
        }
        if terminal_size > 0 {
            let flags = read_uleb128(bytes, &mut cursor)?;
            // Reexport / stub-and-resolver flags carry
            // additional ULEB128s — Phase 5 doesn't generate
            // them but the parser tolerates them so we can
            // round-trip existing tries.
            const EXPORT_SYMBOL_FLAGS_REEXPORT: u64 = 0x08;
            const EXPORT_SYMBOL_FLAGS_STUB_AND_RESOLVER: u64 = 0x10;
            let address_offset = if flags & EXPORT_SYMBOL_FLAGS_REEXPORT != 0 {
                // ordinal then C-string. For Phase 5 we just
                // skip — we don't care about reexport
                // semantics, only the symbol-name → address
                // mapping for "regular" exports.
                let _ordinal = read_uleb128(bytes, &mut cursor)?;
                let _name = read_cstring(bytes, &mut cursor)?;
                0
            } else if flags & EXPORT_SYMBOL_FLAGS_STUB_AND_RESOLVER != 0 {
                let stub = read_uleb128(bytes, &mut cursor)?;
                let _resolver = read_uleb128(bytes, &mut cursor)?;
                stub
            } else {
                read_uleb128(bytes, &mut cursor)?
            };
            out.push(MachOExport {
                name: prefix.clone(),
                flags,
                address_offset,
            });
        }
        // Skip any unread terminal bytes (shouldn't normally
        // happen but defensive).
        cursor = after_terminal;
        if cursor >= bytes.len() {
            return Err(ContainerWriteError::ObjectWrite(
                "export trie: missing child_count after terminal".into(),
            ));
        }
        let child_count = bytes[cursor];
        cursor += 1;
        // Push children in REVERSE order so the stack pops
        // them in original order.
        let mut children: Vec<(String, usize)> =
            Vec::with_capacity(child_count as usize);
        for _ in 0..child_count {
            let edge = read_cstring(bytes, &mut cursor)?;
            let child_off = read_uleb128(bytes, &mut cursor)?;
            children.push((edge, child_off as usize));
        }
        for (edge, child_off) in children.into_iter().rev() {
            let mut child_prefix = prefix.clone();
            child_prefix.push_str(&edge);
            stack.push((child_off, child_prefix));
        }
    }
    Ok(out)
}

/// Build an export trie blob from a list of exports.
///
/// The exports are sorted by name internally (the trie is
/// built as a radix tree on the names), so callers can pass
/// them in any order. Returns the byte-encoded trie suitable
/// for inclusion in the file with `LC_DYLD_EXPORTS_TRIE.dataoff`
/// pointing at the first byte.
pub fn build(exports: &[MachOExport]) -> Vec<u8> {
    if exports.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<&MachOExport> = exports.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));

    // Build the radix tree as a Vec<TrieNode>, where node[0]
    // is the root.
    let mut tree = Tree::default();
    let root = tree.new_node();
    for export in &sorted {
        tree.insert(root, &export.name, export);
    }

    // Two-pass encode: iterate to fixpoint on node offsets.
    // Each iteration encodes nodes assuming the previous
    // iteration's offsets, then re-computes offsets from the
    // encoded sizes.
    //
    // The fixpoint converges quickly (typically 2-3
    // iterations) because ULEB128 sizes change only when an
    // offset crosses a 7-bit boundary.
    let n = tree.nodes.len();
    let mut offsets: Vec<usize> = vec![0; n];
    // Initial guess: ULEB128 single-byte offsets.
    for _ in 0..16 {
        let mut new_offsets = vec![0usize; n];
        let mut cursor = 0usize;
        for i in 0..n {
            new_offsets[i] = cursor;
            cursor += encoded_node_size(&tree.nodes[i], &offsets);
        }
        if new_offsets == offsets {
            break;
        }
        offsets = new_offsets;
    }

    // Final encode using the converged offsets.
    let mut out = Vec::new();
    for i in 0..n {
        encode_node(&tree.nodes[i], &offsets, &mut out);
    }
    out
}

#[derive(Debug, Default)]
struct Tree {
    nodes: Vec<TrieNode>,
}

#[derive(Debug, Default, Clone)]
struct TrieNode {
    /// If `Some`, the path from root to this node is an
    /// exported symbol's full name; the export's flags +
    /// address_offset are recorded here.
    terminal: Option<TerminalInfo>,
    children: Vec<(String, usize)>,
}

#[derive(Debug, Clone)]
struct TerminalInfo {
    flags: u64,
    address_offset: u64,
}

impl Tree {
    fn new_node(&mut self) -> usize {
        let id = self.nodes.len();
        self.nodes.push(TrieNode::default());
        id
    }

    /// Insert a symbol into the trie rooted at `node_idx`.
    /// Splits existing edges as necessary.
    fn insert(&mut self, node_idx: usize, name: &str, export: &MachOExport) {
        if name.is_empty() {
            self.nodes[node_idx].terminal = Some(TerminalInfo {
                flags: export.flags,
                address_offset: export.address_offset,
            });
            return;
        }
        // Check for an existing child whose edge shares a
        // prefix with `name`.
        for ci in 0..self.nodes[node_idx].children.len() {
            let (edge, child_idx) = self.nodes[node_idx].children[ci].clone();
            let common = common_prefix_len(&edge, name);
            if common == 0 {
                continue;
            }
            if common == edge.len() {
                // Whole edge matches; descend into the child
                // and insert the remainder.
                self.insert(child_idx, &name[common..], export);
                return;
            }
            // Partial match: split the edge at `common`. Insert
            // a new intermediate node with the existing child
            // as one of its children, then continue inserting
            // `name[common..]` from there.
            let edge_head: String = edge[..common].to_string();
            let edge_tail: String = edge[common..].to_string();
            let intermediate = self.new_node();
            self.nodes[intermediate]
                .children
                .push((edge_tail, child_idx));
            // Replace the original (edge, child) with
            // (edge_head, intermediate).
            self.nodes[node_idx].children[ci] = (edge_head, intermediate);
            self.insert(intermediate, &name[common..], export);
            return;
        }
        // No prefix match — create a new leaf for the whole
        // remaining name.
        let leaf = self.new_node();
        self.nodes[leaf].terminal = Some(TerminalInfo {
            flags: export.flags,
            address_offset: export.address_offset,
        });
        self.nodes[node_idx].children.push((name.to_string(), leaf));
    }
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.bytes()
        .zip(b.bytes())
        .take_while(|(x, y)| x == y)
        .count()
}

/// Compute the encoded size of a single node given the current
/// offsets table (used during the fixpoint iteration).
fn encoded_node_size(node: &TrieNode, offsets: &[usize]) -> usize {
    let mut size = 0;
    let terminal_bytes = encode_terminal_bytes(node);
    size += uleb128_size(terminal_bytes.len() as u64);
    size += terminal_bytes.len();
    size += 1; // child_count u8
    for (edge, child_idx) in &node.children {
        size += edge.len() + 1; // edge string + NUL
        size += uleb128_size(offsets[*child_idx] as u64);
    }
    size
}

/// Encode a single node into `out`.
fn encode_node(node: &TrieNode, offsets: &[usize], out: &mut Vec<u8>) {
    let terminal_bytes = encode_terminal_bytes(node);
    write_uleb128(out, terminal_bytes.len() as u64);
    out.extend_from_slice(&terminal_bytes);
    out.push(node.children.len() as u8);
    for (edge, child_idx) in &node.children {
        out.extend_from_slice(edge.as_bytes());
        out.push(0);
        write_uleb128(out, offsets[*child_idx] as u64);
    }
}

fn encode_terminal_bytes(node: &TrieNode) -> Vec<u8> {
    let Some(terminal) = node.terminal.as_ref() else {
        return Vec::new();
    };
    // We only emit "regular" exports in the builder (no
    // reexport/stub-and-resolver). flags + address_offset.
    let mut buf = Vec::new();
    write_uleb128(&mut buf, terminal.flags);
    write_uleb128(&mut buf, terminal.address_offset);
    buf
}

// --- ULEB128 helpers -----------------------------------------

fn read_uleb128(bytes: &[u8], cursor: &mut usize) -> Result<u64, ContainerWriteError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        if *cursor >= bytes.len() {
            return Err(ContainerWriteError::ObjectWrite(
                "ULEB128: unexpected end of input".into(),
            ));
        }
        let b = bytes[*cursor];
        *cursor += 1;
        if shift >= 64 {
            return Err(ContainerWriteError::ObjectWrite(
                "ULEB128: shift overflow".into(),
            ));
        }
        result |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

fn write_uleb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
            out.push(byte);
        } else {
            out.push(byte);
            return;
        }
    }
}

fn uleb128_size(mut value: u64) -> usize {
    let mut size = 1;
    while value >= 0x80 {
        size += 1;
        value >>= 7;
    }
    size
}

fn read_cstring(bytes: &[u8], cursor: &mut usize) -> Result<String, ContainerWriteError> {
    let start = *cursor;
    while *cursor < bytes.len() && bytes[*cursor] != 0 {
        *cursor += 1;
    }
    if *cursor >= bytes.len() {
        return Err(ContainerWriteError::ObjectWrite(
            "C-string: missing NUL terminator".into(),
        ));
    }
    let s = std::str::from_utf8(&bytes[start..*cursor])
        .map_err(|_| {
            ContainerWriteError::ObjectWrite("C-string: non-UTF8 bytes".into())
        })?
        .to_string();
    *cursor += 1; // skip NUL
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn export(name: &str, address_offset: u64) -> MachOExport {
        MachOExport {
            name: name.to_string(),
            flags: 0,
            address_offset,
        }
    }

    #[test]
    fn roundtrip_single_symbol() {
        let exports = vec![export("_foo", 0x1000)];
        let bytes = build(&exports);
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed, exports);
    }

    #[test]
    fn roundtrip_two_symbols_no_common_prefix() {
        let exports = vec![export("_alpha", 0x1000), export("_beta", 0x2000)];
        let bytes = build(&exports);
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.len(), 2);
        // sorted alphabetically by build()
        assert_eq!(parsed[0].name, "_alpha");
        assert_eq!(parsed[0].address_offset, 0x1000);
        assert_eq!(parsed[1].name, "_beta");
        assert_eq!(parsed[1].address_offset, 0x2000);
    }

    #[test]
    fn roundtrip_common_prefix_radix() {
        let exports = vec![
            export("_greet_double", 0x13f0),
            export("_greet_offset", 0x1408),
            export("_greet_base", 0x4000),
            export("_greet_quintuple", 0x100000),
        ];
        let bytes = build(&exports);
        let mut parsed = parse(&bytes).expect("parse");
        parsed.sort_by(|a, b| a.name.cmp(&b.name));
        let mut expected = exports.clone();
        expected.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(parsed, expected);
    }

    #[test]
    fn roundtrip_libgreet_real_trie() {
        // Real trie bytes captured from libgreet.dylib (4
        // exports sharing prefix `_greet_`). Verifies the
        // parser accepts what ld emits in practice.
        let bytes = [
            // 0x00..=0x0a: root node (no terminal, 1 child
            // edge "_greet_" → 0x25).
            0x00, 0x01, b'_', b'g', b'r', b'e', b'e', b't', b'_', 0x00, 0x25,
            // 0x0b..=0x0e: 4 padding zero bytes.
            0x00, 0x00, 0x00, 0x00,
            // 0x0f..=0x14: leaf for `_greet_base`
            // (terminal_size=4, flags=0, address=0x4000).
            0x04, 0x00, 0x80, 0x80, 0x01, 0x00,
            // 0x15..=0x1a: leaf for `_greet_ctor_marker`
            // (terminal_size=4, flags=0, address=0x4004).
            0x04, 0x00, 0x84, 0x80, 0x01, 0x00,
            // 0x1b..=0x1f: leaf for `_greet_double`
            // (terminal_size=3, flags=0, address=0x13f0).
            0x03, 0x00, 0xf0, 0x27, 0x00,
            // 0x20..=0x24: leaf for `_greet_offset`
            // (terminal_size=3, flags=0, address=0x1408).
            0x03, 0x00, 0x88, 0x28, 0x00,
            // 0x25..: interior node `_greet_` with 4 children.
            0x00, 0x04,
            b'b', b'a', b's', b'e', 0x00, 0x0f,
            b'c', b't', b'o', b'r', b'_', b'm', b'a', b'r', b'k', b'e', b'r', 0x00, 0x15,
            b'd', b'o', b'u', b'b', b'l', b'e', 0x00, 0x1b,
            b'o', b'f', b'f', b's', b'e', b't', 0x00, 0x20,
            // padding to 80 bytes total.
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let parsed = parse(&bytes).expect("parse libgreet trie");
        let names: Vec<&str> = parsed.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"_greet_double"),
            "expected _greet_double, got names: {names:?}",
        );
        assert!(names.contains(&"_greet_base"));
        assert!(names.contains(&"_greet_ctor_marker"));
        assert!(names.contains(&"_greet_offset"));
    }

    #[test]
    fn parse_empty_returns_empty_list() {
        let parsed = parse(&[]).expect("parse");
        assert_eq!(parsed.len(), 0);
    }

    #[test]
    fn build_empty_returns_empty_blob() {
        let bytes = build(&[]);
        assert!(bytes.is_empty());
    }

    #[test]
    fn uleb128_roundtrip() {
        for v in [0u64, 1, 0x7f, 0x80, 0x3fff, 0x4000, 0xffff_ffff, 1 << 50] {
            let mut buf = Vec::new();
            write_uleb128(&mut buf, v);
            assert_eq!(buf.len(), uleb128_size(v));
            let mut cursor = 0;
            let read = read_uleb128(&buf, &mut cursor).unwrap();
            assert_eq!(read, v, "uleb128 roundtrip failed for {v:#x}");
            assert_eq!(cursor, buf.len());
        }
    }
}
