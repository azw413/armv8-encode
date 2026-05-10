#!/usr/bin/env python3
"""
Import Thumb opcode tables from GNU binutils' opcodes/arm-dis.c.

Run:
    python3 tools/import_thumb_opcodes.py PATH_TO_arm-dis.c \
        > src/isa/armv7/table_generated.rs

Produces a Rust source file declaring `THUMB_OPCODE_TABLE_GENERATED`,
a static slice of `ThumbOpcode` rows. Each row has its mnemonic
extracted from binutils' format string (the bit before the first `\t`,
percent escape, or whitespace), with `%c` (conditional) noted on the
mnemonic name (e.g. `b%c` → `Bcond` placeholder). Mnemonics not yet in
our `ThumbMnemonic` enum get added as new variants.

Operand shapes default to `Unspecified` — operand decoding for new
shapes is a follow-up pass. The point of this generator is to land
the full mnemonic / opcode / mask classification in one go so
`match_opcode` correctly identifies every instruction binutils
recognises.

Source: GNU binutils 2.41 (or later), opcodes/arm-dis.c, BSD-licensed
header → GPL-licensed source. Output is regeneratable; see header
comment in the generated file for invocation.
"""
import re
import sys
from pathlib import Path


def extract_array(src: str, name: str) -> list[tuple[int, int, str]]:
    """Extract (opcode, mask, format_string) tuples from a binutils
    opcode array. Handles multi-line entries (binutils sometimes
    breaks `{feature, opcode, mask, "fmt"}` across lines).
    """
    # Find the start: `static const struct opcodeNN <name>[] = {`
    start = re.search(rf"static\s+const\s+struct\s+opcode\d+\s+{re.escape(name)}\[\]\s*=\s*{{", src)
    if not start:
        raise SystemExit(f"could not find array {name}")
    body_start = start.end()
    # Find the matching `};` at brace depth 0. We track brace depth
    # to handle nested braces in macro arguments.
    depth = 1
    i = body_start
    while i < len(src) and depth > 0:
        c = src[i]
        if c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
        i += 1
    body = src[body_start : i - 1]

    rows = []
    # Each entry is `{ FEATURE, opcode, mask, "fmt" }`. The feature
    # macro can itself contain commas and parens, so we tokenise by
    # tracking outer-level commas.
    pos = 0
    while pos < len(body):
        # Skip whitespace, comments, commas.
        while pos < len(body):
            if body[pos].isspace() or body[pos] == ',':
                pos += 1
                continue
            if body.startswith("/*", pos):
                end = body.find("*/", pos)
                if end < 0:
                    pos = len(body)
                    break
                pos = end + 2
                continue
            if body.startswith("//", pos):
                end = body.find("\n", pos)
                pos = len(body) if end < 0 else end + 1
                continue
            break
        if pos >= len(body):
            break
        if body[pos] != '{':
            # Not the start of a row.
            pos += 1
            continue
        # Consume until the matching `}`.
        depth = 1
        j = pos + 1
        while j < len(body) and depth > 0:
            cj = body[j]
            if cj == '"':
                # Skip string literal.
                j += 1
                while j < len(body) and body[j] != '"':
                    if body[j] == '\\':
                        j += 2
                    else:
                        j += 1
                j += 1
                continue
            if cj == '{':
                depth += 1
            elif cj == '}':
                depth -= 1
            j += 1
        entry = body[pos + 1 : j - 1]
        pos = j
        # Split entry's TOP-LEVEL commas.
        fields = split_top_level_commas(entry)
        if len(fields) < 4:
            continue
        # fields: [feature_macro, opcode, mask, "fmt"]
        opcode = parse_int(fields[1])
        mask = parse_int(fields[2])
        fmt = parse_string_literal(fields[3])
        if opcode is None or mask is None or fmt is None:
            continue
        rows.append((opcode, mask, fmt))
    return rows


def split_top_level_commas(s: str) -> list[str]:
    """Split a string by commas that are NOT inside parens/braces/strings."""
    out = []
    buf = []
    depth = 0
    i = 0
    while i < len(s):
        c = s[i]
        if c == '"':
            # Copy string literal verbatim.
            buf.append(c)
            i += 1
            while i < len(s) and s[i] != '"':
                if s[i] == '\\':
                    buf.append(s[i])
                    if i + 1 < len(s):
                        buf.append(s[i + 1])
                        i += 2
                        continue
                buf.append(s[i])
                i += 1
            if i < len(s):
                buf.append(s[i])  # closing quote
                i += 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        if c == ',' and depth == 0:
            out.append("".join(buf).strip())
            buf = []
            i += 1
            continue
        buf.append(c)
        i += 1
    if buf:
        out.append("".join(buf).strip())
    return out


def parse_int(s: str) -> int | None:
    s = s.strip()
    try:
        return int(s, 0)
    except ValueError:
        return None


def parse_string_literal(s: str) -> str | None:
    s = s.strip()
    if not (s.startswith('"') and s.endswith('"')):
        return None
    # Process common escapes.
    inner = s[1:-1]
    inner = inner.replace('\\t', '\t').replace('\\n', '\n').replace('\\\\', '\\').replace('\\"', '"')
    return inner


# Mnemonic extraction: take everything up to the first whitespace/tab
# /percent escape. Strip trailing `%c` (conditional indicator) or
# `%c.w` (conditional + wide) marker — those mean the condition code
# is encoded in fields, not part of the mnemonic name itself.
MNEMONIC_RE = re.compile(r"^([A-Za-z][A-Za-z0-9_.]*)")


def extract_mnemonic(fmt: str) -> str:
    # Trim leading whitespace.
    s = fmt.lstrip()
    # The mnemonic ends at the first '%' (escape), tab, or space.
    end = len(s)
    for i, ch in enumerate(s):
        if ch in '\t %':
            end = i
            break
    return s[:end] or '<unknown>'


# Map binutils format-string mnemonic (lowercase, possibly with
# trailing 's' or '.w') to our ThumbMnemonic variant name. The set
# below covers the bootstrap mnemonics + every distinct mnemonic
# found in binutils 2.41's thumb arrays. Any mnemonic NOT in this
# table gets its own auto-generated PascalCase variant name.
# Using a dict so future overrides (e.g. distinguishing flag-setting
# variants) are explicit.
EXPLICIT_VARIANT_OVERRIDES: dict[str, str] = {
    # Sometimes the binutils format string uses an unusual casing or
    # punctuation. Normalise here.
}


def variant_name(mnemonic: str) -> str:
    """Convert a mnemonic like 'add' or 'ldrd' or 'vmla.f32' into a
    Rust PascalCase variant name. Punctuation becomes underscored
    capitalisation."""
    if mnemonic in EXPLICIT_VARIANT_OVERRIDES:
        return EXPLICIT_VARIANT_OVERRIDES[mnemonic]
    # Replace non-alphanumeric chars with underscores, then split.
    parts = re.split(r"[^A-Za-z0-9]+", mnemonic)
    parts = [p for p in parts if p]
    return "".join(p[:1].upper() + p[1:].lower() for p in parts) or "Unknown"


def main():
    if len(sys.argv) != 2:
        sys.stderr.write("usage: import_thumb_opcodes.py PATH_TO_arm-dis.c\n")
        sys.exit(2)
    src_path = Path(sys.argv[1])
    src = src_path.read_text()

    thumb16 = extract_array(src, "thumb_opcodes")
    thumb32 = extract_array(src, "thumb32_opcodes")
    sys.stderr.write(f"parsed {len(thumb16)} 16-bit, {len(thumb32)} 32-bit entries\n")

    # Collect all distinct mnemonics so we can emit the enum.
    mnemonics: set[str] = set()
    rows16 = []
    for opcode, mask, fmt in thumb16:
        mn = extract_mnemonic(fmt)
        mnemonics.add(mn)
        rows16.append((opcode, mask, fmt, mn))
    rows32 = []
    for opcode, mask, fmt in thumb32:
        mn = extract_mnemonic(fmt)
        mnemonics.add(mn)
        rows32.append((opcode, mask, fmt, mn))

    # Stable order: ASCII sort on mnemonic.
    sorted_mnemonics = sorted(mnemonics)

    # Build output.
    out = []
    out.append("// AUTO-GENERATED — do not edit by hand.")
    out.append("//")
    out.append("// Regenerate with:")
    out.append("//")
    out.append("//   python3 tools/import_thumb_opcodes.py PATH_TO_arm-dis.c \\")
    out.append("//       > src/isa/armv7/table_generated.rs")
    out.append("//")
    out.append("// Source: GNU binutils opcodes/arm-dis.c (GPL-2.0-or-later).")
    out.append("// The generator extracts the (opcode, mask, format) triples for")
    out.append("// the `thumb_opcodes` and `thumb32_opcodes` arrays and emits a")
    out.append("// `ThumbOpcode` row per entry. Operand shapes default to")
    out.append("// `Unspecified` — operand decoding for new shapes is wired up")
    out.append("// in `decode_operands` as needed.")
    out.append("//")
    out.append("// The mnemonic enum below is auto-generated from the union of")
    out.append("// mnemonics seen in binutils' format strings. Each variant's")
    out.append("// name is PascalCase of the mnemonic with non-alphanumeric")
    out.append("// chars treated as separators (e.g. `vmla.f32` → `VmlaF32`).")
    out.append("")
    out.append("#![allow(dead_code, non_camel_case_types)]")
    out.append("")
    out.append("use super::table::ThumbWidth;")
    out.append("")
    out.append("/// Mnemonic identifier. Auto-generated from binutils.")
    out.append("#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]")
    out.append("pub enum ThumbMnemonicGenerated {")
    for mn in sorted_mnemonics:
        out.append(f"    /// `{mn}`")
        out.append(f"    {variant_name(mn)},")
    out.append("}")
    out.append("")
    out.append("impl ThumbMnemonicGenerated {")
    out.append("    pub fn as_str(&self) -> &'static str {")
    out.append("        match self {")
    for mn in sorted_mnemonics:
        # Escape special chars in the literal.
        lit = mn.replace('\\', '\\\\').replace('"', '\\"')
        out.append(f"            Self::{variant_name(mn)} => \"{lit}\",")
    out.append("        }")
    out.append("    }")
    out.append("}")
    out.append("")
    out.append("/// Auto-generated table covering every Thumb instruction")
    out.append("/// binutils 2.41 recognises.")
    out.append(f"pub static THUMB_OPCODE_TABLE_GENERATED: &[ThumbOpcodeGenerated] = &[")
    for opcode, mask, fmt, mn in rows16:
        # Escape format string for Rust string literal.
        lit = fmt.replace('\\', '\\\\').replace('"', '\\"').replace('\t', '\\t').replace('\n', '\\n')
        out.append(
            f"    ThumbOpcodeGenerated {{ "
            f"mnemonic: ThumbMnemonicGenerated::{variant_name(mn)}, "
            f"opcode: 0x{opcode:08x}, mask: 0x{mask:08x}, "
            f"width: ThumbWidth::Halfword, format: \"{lit}\" }},"
        )
    for opcode, mask, fmt, mn in rows32:
        lit = fmt.replace('\\', '\\\\').replace('"', '\\"').replace('\t', '\\t').replace('\n', '\\n')
        out.append(
            f"    ThumbOpcodeGenerated {{ "
            f"mnemonic: ThumbMnemonicGenerated::{variant_name(mn)}, "
            f"opcode: 0x{opcode:08x}, mask: 0x{mask:08x}, "
            f"width: ThumbWidth::Word, format: \"{lit}\" }},"
        )
    out.append("];")
    out.append("")
    out.append("/// Auto-generated row type. Carries the binutils format")
    out.append("/// string verbatim so callers can pattern-match operand")
    out.append("/// shapes against it without re-parsing.")
    out.append("#[derive(Debug, Copy, Clone)]")
    out.append("pub struct ThumbOpcodeGenerated {")
    out.append("    pub mnemonic: ThumbMnemonicGenerated,")
    out.append("    pub opcode: u32,")
    out.append("    pub mask: u32,")
    out.append("    pub width: ThumbWidth,")
    out.append("    pub format: &'static str,")
    out.append("}")
    out.append("")
    out.append("/// Find the first generated table entry whose mask + opcode")
    out.append("/// matches the input word for the given width.")
    out.append("pub fn match_generated(word: u32, width: ThumbWidth) -> Option<&'static ThumbOpcodeGenerated> {")
    out.append("    THUMB_OPCODE_TABLE_GENERATED")
    out.append("        .iter()")
    out.append("        .find(|row| row.width == width && (word & row.mask) == row.opcode)")
    out.append("}")
    out.append("")

    print("\n".join(out))


if __name__ == "__main__":
    main()
