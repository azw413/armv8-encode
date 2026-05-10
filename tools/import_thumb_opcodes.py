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


def extract_array_5field(src: str, name: str) -> list[tuple[str, int, int, str]]:
    """Extract (isa_kind, opcode, mask, format) tuples from an
    sopcode32 array (`coprocessor_opcodes`/`generic_coprocessor_opcodes`).
    Each entry has 5 fields: ISA_TAG, FEATURE, OPCODE, MASK, "FMT". The
    ISA_TAG (`ANY`/`T32`/`ARM`) tells us which mode the row applies to.
    Sentinel rows whose mask/value are non-numeric (`SENTINEL_IWMMXT_START`
    etc.) are skipped.
    """
    start = re.search(rf"static\s+const\s+struct\s+sopcode\d+\s+{re.escape(name)}\[\]\s*=\s*{{", src)
    if not start:
        raise SystemExit(f"could not find array {name}")
    body_start = start.end()
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
    pos = 0
    while pos < len(body):
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
            pos += 1
            continue
        depth = 1
        j = pos + 1
        while j < len(body) and depth > 0:
            cj = body[j]
            if cj == '"':
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
        fields = split_top_level_commas(entry)
        if len(fields) < 5:
            continue
        isa_tag = fields[0].strip()
        opcode = parse_int(fields[2])
        mask = parse_int(fields[3])
        fmt = parse_string_literal(fields[4])
        if opcode is None or mask is None or fmt is None:
            # Sentinels (SENTINEL_IWMMXT_START etc.) hit this
            # branch — they're not real instruction rows.
            continue
        rows.append((isa_tag, opcode, mask, fmt))
    return rows


def emit_neon_table(src: str) -> None:
    """Generate the unified NEON + coprocessor table.
    NEON rows apply to both ARM and Thumb modes (with the
    Thumb→ARM word normalisation handled at lookup time).
    Coprocessor rows have an ISA tag.
    """
    neon = extract_array(src, "neon_opcodes")
    cop = extract_array_5field(src, "coprocessor_opcodes")
    gen_cop = extract_array_5field(src, "generic_coprocessor_opcodes")
    sys.stderr.write(
        f"parsed {len(neon)} neon, {len(cop)} coprocessor, {len(gen_cop)} generic-coprocessor rows\n"
    )

    # Tag each row with applicability:
    #   "neon": both ARM and Thumb (with Thumb→ARM normalisation)
    #   "any":  both, no normalisation needed
    #   "arm":  ARM-only
    #   "thumb": Thumb-only
    tagged = []
    for opcode, mask, fmt in neon:
        tagged.append(("neon", opcode, mask, fmt))
    for isa, opcode, mask, fmt in cop + gen_cop:
        tag = {"ANY": "any", "T32": "thumb", "ARM": "arm"}.get(isa, "any")
        tagged.append((tag, opcode, mask, fmt))

    mnemonics: set[str] = set()
    enriched = []
    for tag, opcode, mask, fmt in tagged:
        mn = extract_mnemonic(fmt)
        mnemonics.add(mn)
        enriched.append((tag, opcode, mask, fmt, mn))
    sorted_mnemonics = sorted(mnemonics)

    out: list[str] = []
    out.append("// AUTO-GENERATED — do not edit by hand.")
    out.append("//")
    out.append("// Regenerate with:")
    out.append("//")
    out.append("//   python3 tools/import_thumb_opcodes.py PATH_TO_arm-dis.c neon \\")
    out.append("//       > src/isa/armv7/neon_table_generated.rs")
    out.append("//")
    out.append("// Source: GNU binutils opcodes/arm-dis.c — neon_opcodes,")
    out.append("// coprocessor_opcodes, generic_coprocessor_opcodes arrays")
    out.append("// (GPL-2.0-or-later).")
    out.append("//")
    out.append("// Each row carries an `IsaApplicability` tag controlling")
    out.append("// which decoder paths consider it: NEON rows apply to both")
    out.append("// modes (with a small Thumb-to-ARM word transform performed")
    out.append("// at match time); coprocessor rows are tagged ARM/Thumb/Any")
    out.append("// based on binutils' ANY/T32/ARM selector.")
    out.append("")
    out.append("#![allow(dead_code, non_camel_case_types)]")
    out.append("")
    out.append("/// Mode-applicability tag from binutils' table sources.")
    out.append("#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]")
    out.append("pub enum IsaApplicability {")
    out.append("    /// NEON row — applies to both ARM and Thumb encodings.")
    out.append("    /// The Thumb encoding requires a small word-level")
    out.append("    /// transform before matching (see `normalise_neon_thumb`).")
    out.append("    Neon,")
    out.append("    /// Coprocessor row applicable to both ARM and Thumb.")
    out.append("    Any,")
    out.append("    /// ARM-only.")
    out.append("    Arm,")
    out.append("    /// Thumb-only.")
    out.append("    Thumb,")
    out.append("}")
    out.append("")
    out.append("/// Auto-generated mnemonic enum for NEON / VFP / coprocessor")
    out.append("/// instructions.")
    out.append("#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]")
    out.append("pub enum NeonMnemonicGenerated {")
    for mn in sorted_mnemonics:
        out.append(f"    /// `{mn}`")
        out.append(f"    {variant_name(mn)},")
    out.append("}")
    out.append("")
    out.append("impl NeonMnemonicGenerated {")
    out.append("    pub fn as_str(&self) -> &'static str {")
    out.append("        match self {")
    for mn in sorted_mnemonics:
        lit = mn.replace('\\', '\\\\').replace('"', '\\"')
        out.append(f"            Self::{variant_name(mn)} => \"{lit}\",")
    out.append("        }")
    out.append("    }")
    out.append("}")
    out.append("")
    out.append("#[derive(Debug, Copy, Clone)]")
    out.append("pub struct NeonOpcodeGenerated {")
    out.append("    pub mnemonic: NeonMnemonicGenerated,")
    out.append("    pub opcode: u32,")
    out.append("    pub mask: u32,")
    out.append("    pub isa: IsaApplicability,")
    out.append("    pub format: &'static str,")
    out.append("}")
    out.append("")
    out.append("pub static NEON_OPCODE_TABLE_GENERATED: &[NeonOpcodeGenerated] = &[")
    for tag, opcode, mask, fmt, mn in enriched:
        lit = fmt.replace('\\', '\\\\').replace('"', '\\"').replace('\t', '\\t').replace('\n', '\\n')
        rust_isa = {
            "neon": "IsaApplicability::Neon",
            "any": "IsaApplicability::Any",
            "arm": "IsaApplicability::Arm",
            "thumb": "IsaApplicability::Thumb",
        }[tag]
        out.append(
            f"    NeonOpcodeGenerated {{ "
            f"mnemonic: NeonMnemonicGenerated::{variant_name(mn)}, "
            f"opcode: 0x{opcode:08x}, mask: 0x{mask:08x}, "
            f"isa: {rust_isa}, format: \"{lit}\" }},"
        )
    out.append("];")
    out.append("")
    out.append("/// Translate a Thumb-encoded NEON word so it can be matched")
    out.append("/// against the table directly. Mirrors the prologue of")
    out.append("/// binutils' `print_insn_neon` (when the `thumb` flag is set).")
    out.append("///")
    out.append("/// Returns `Some(normalised_word)` when the input is a Thumb")
    out.append("/// NEON encoding the table can match, `None` if it falls")
    out.append("/// outside the NEON encoding space and should be tried")
    out.append("/// against other tables.")
    out.append("pub fn normalise_neon_thumb(given: u32) -> Option<u32> {")
    out.append("    if (given & 0xef000000) == 0xef000000 {")
    out.append("        // Move bit 28 to bit 24 to translate Thumb-2 to ARM.")
    out.append("        let bit28 = given & (1 << 28);")
    out.append("        let mut g = given & 0x00ffffff;")
    out.append("        g |= if bit28 != 0 { 0xf3000000 } else { 0xf2000000 };")
    out.append("        Some(g)")
    out.append("    } else if (given & 0xff000000) == 0xf9000000 {")
    out.append("        Some(given ^ (0xf9000000 ^ 0xf4000000))")
    out.append("    } else if (given & 0xff000000) == 0xfe000000")
    out.append("        || (given & 0xff000000) == 0xfc000000")
    out.append("    {")
    out.append("        // BFloat16 NEON: no top-byte transform.")
    out.append("        Some(given)")
    out.append("    } else if (given & 0xff900f5f) == 0xee800b10 {")
    out.append("        // vdup.")
    out.append("        Some(given)")
    out.append("    } else {")
    out.append("        None")
    out.append("    }")
    out.append("}")
    out.append("")
    out.append("/// Match a Thumb-mode NEON / VFP / coprocessor word.")
    out.append("/// Returns the matched row plus the normalised word")
    out.append("/// (so callers can pass that to operand decoders).")
    out.append("pub fn match_thumb(word: u32) -> Option<(&'static NeonOpcodeGenerated, u32)> {")
    out.append("    // First try the NEON normalisation. If it produces a")
    out.append("    // Some, run only NEON rows; otherwise the word is a")
    out.append("    // coprocessor encoding and needs the cond-bit fix-up.")
    out.append("    if let Some(norm) = normalise_neon_thumb(word) {")
    out.append("        for row in NEON_OPCODE_TABLE_GENERATED.iter() {")
    out.append("            if !matches!(row.isa, IsaApplicability::Neon) {")
    out.append("                continue;")
    out.append("            }")
    out.append("            // For NEON rows whose mask leaves the high 4 bits")
    out.append("            // unspecified, the Thumb match requires those")
    out.append("            // bits == 0xe.")
    out.append("            let mut cond_mask = row.mask;")
    out.append("            let mut cond_value = row.opcode;")
    out.append("            if (cond_mask & 0xf0000000) == 0 {")
    out.append("                cond_mask |= 0xf0000000;")
    out.append("                cond_value |= 0xe0000000;")
    out.append("            }")
    out.append("            if (norm & cond_mask) == cond_value {")
    out.append("                return Some((row, norm));")
    out.append("            }")
    out.append("        }")
    out.append("    }")
    out.append("    // Coprocessor encodings: for Thumb the high 4 bits are")
    out.append("    // forced to 0xe before matching.")
    out.append("    let coproc_word = (word & 0x0fffffff) | 0xe0000000;")
    out.append("    for row in NEON_OPCODE_TABLE_GENERATED.iter() {")
    out.append("        if matches!(row.isa, IsaApplicability::Neon | IsaApplicability::Arm) {")
    out.append("            continue;")
    out.append("        }")
    out.append("        let mask = row.mask | 0xf0000000;")
    out.append("        let value = row.opcode | 0xe0000000;")
    out.append("        if (coproc_word & mask) == value {")
    out.append("            return Some((row, coproc_word));")
    out.append("        }")
    out.append("    }")
    out.append("    None")
    out.append("}")
    out.append("")
    out.append("/// Match an ARM-mode NEON / VFP / coprocessor word.")
    out.append("pub fn match_arm(word: u32) -> Option<&'static NeonOpcodeGenerated> {")
    out.append("    for row in NEON_OPCODE_TABLE_GENERATED.iter() {")
    out.append("        if matches!(row.isa, IsaApplicability::Thumb) {")
    out.append("            continue;")
    out.append("        }")
    out.append("        if (word & row.mask) == row.opcode {")
    out.append("            return Some(row);")
    out.append("        }")
    out.append("    }")
    out.append("    None")
    out.append("}")
    print("\n".join(out))


def emit_arm_table(src: str) -> None:
    """Generate src/isa/armv7/arm/table_generated.rs from arm_opcodes[]."""
    rows = extract_array(src, "arm_opcodes")
    sys.stderr.write(f"parsed {len(rows)} arm opcodes\n")

    mnemonics: set[str] = set()
    enriched = []
    for opcode, mask, fmt in rows:
        mn = extract_mnemonic(fmt)
        mnemonics.add(mn)
        enriched.append((opcode, mask, fmt, mn))
    sorted_mnemonics = sorted(mnemonics)

    out: list[str] = []
    out.append("// AUTO-GENERATED — do not edit by hand.")
    out.append("//")
    out.append("// Regenerate with:")
    out.append("//")
    out.append("//   python3 tools/import_thumb_opcodes.py PATH_TO_arm-dis.c arm \\")
    out.append("//       > src/isa/armv7/arm/table_generated.rs")
    out.append("//")
    out.append("// Source: GNU binutils opcodes/arm-dis.c, `arm_opcodes` array")
    out.append("// (32-bit ARM-mode instructions, GPL-2.0-or-later).")
    out.append("//")
    out.append("// Format strings are carried verbatim. The mnemonic enum is")
    out.append("// auto-generated from the union of distinct mnemonics in the")
    out.append("// table; PascalCase, with non-alphanumeric chars treated as")
    out.append("// separators (e.g. `vmla.f32` → `VmlaF32`).")
    out.append("")
    out.append("#![allow(dead_code, non_camel_case_types)]")
    out.append("")
    out.append("/// Mnemonic identifier. Auto-generated from binutils' arm_opcodes.")
    out.append("#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]")
    out.append("pub enum ArmMnemonicGenerated {")
    for mn in sorted_mnemonics:
        out.append(f"    /// `{mn}`")
        out.append(f"    {variant_name(mn)},")
    out.append("}")
    out.append("")
    out.append("impl ArmMnemonicGenerated {")
    out.append("    pub fn as_str(&self) -> &'static str {")
    out.append("        match self {")
    for mn in sorted_mnemonics:
        lit = mn.replace('\\', '\\\\').replace('"', '\\"')
        out.append(f"            Self::{variant_name(mn)} => \"{lit}\",")
    out.append("        }")
    out.append("    }")
    out.append("}")
    out.append("")
    out.append("/// Auto-generated table: every ARM-mode instruction binutils 2.41 recognises.")
    out.append("pub static ARM_OPCODE_TABLE_GENERATED: &[ArmOpcodeGenerated] = &[")
    for opcode, mask, fmt, mn in enriched:
        lit = fmt.replace('\\', '\\\\').replace('"', '\\"').replace('\t', '\\t').replace('\n', '\\n')
        out.append(
            f"    ArmOpcodeGenerated {{ "
            f"mnemonic: ArmMnemonicGenerated::{variant_name(mn)}, "
            f"opcode: 0x{opcode:08x}, mask: 0x{mask:08x}, "
            f"format: \"{lit}\" }},"
        )
    out.append("];")
    out.append("")
    out.append("/// Row type. Carries the binutils format string verbatim so")
    out.append("/// callers can pattern-match operand shapes without re-parsing.")
    out.append("#[derive(Debug, Copy, Clone)]")
    out.append("pub struct ArmOpcodeGenerated {")
    out.append("    pub mnemonic: ArmMnemonicGenerated,")
    out.append("    pub opcode: u32,")
    out.append("    pub mask: u32,")
    out.append("    pub format: &'static str,")
    out.append("}")
    out.append("")
    out.append("/// Find the first row whose mask + opcode matches the input word.")
    out.append("pub fn match_generated(word: u32) -> Option<&'static ArmOpcodeGenerated> {")
    out.append("    ARM_OPCODE_TABLE_GENERATED")
    out.append("        .iter()")
    out.append("        .find(|row| (word & row.mask) == row.opcode)")
    out.append("}")
    out.append("")
    print("\n".join(out))


def main():
    if len(sys.argv) < 2 or len(sys.argv) > 3:
        sys.stderr.write(
            "usage: import_thumb_opcodes.py PATH_TO_arm-dis.c [thumb|arm]\n"
            "   default mode is 'thumb' (back-compat).\n"
        )
        sys.exit(2)
    src_path = Path(sys.argv[1])
    mode = sys.argv[2] if len(sys.argv) == 3 else "thumb"
    src = src_path.read_text()
    if mode == "arm":
        return emit_arm_table(src)
    if mode == "neon":
        return emit_neon_table(src)
    if mode != "thumb":
        sys.stderr.write(f"unknown mode {mode!r}; expected 'thumb', 'arm', or 'neon'\n")
        sys.exit(2)

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
