//! End-to-end comparison test for the ARMv7 Thumb decoder.
//!
//! Runs `arm-linux-gnueabihf-objdump -d` against a real ELF
//! shared object (Android `libtool-checker.so` in `tests/`),
//! parses its output, and checks our table-driven decoder
//! agrees on every Thumb instruction's mnemonic.
//!
//! ## What's compared
//!
//! Just the mnemonic. The decoder produces typed operands
//! but no text formatter yet, so a full operand-string
//! comparison would require building one — out of scope
//! for the first cross-check pass. Mnemonic agreement is
//! still a strong signal: it validates the imported
//! opcode table, the 16/32-bit width discrimination, and
//! the row-matching logic over thousands of real
//! instructions.
//!
//! ## What's filtered out
//!
//! - **ARM-mode instructions** (8-hex-digit word column):
//!   the decoder is Thumb-only.
//! - **Data ranges / `<UNDEFINED>` lines**: objdump's
//!   placeholder for non-instruction bytes. Skipped on
//!   both sides.
//! - **Condition-code suffixes**: objdump prints
//!   `addeq`/`bne.n`/`pop.w`; our decoder reports the
//!   base mnemonic and carries the condition / width as
//!   format-string state. Strip them before comparing so
//!   conditional branches don't all show up as
//!   mismatches.
//!
//! ## Strict mode
//!
//! Like the otool comparison test, this is `#[ignore]` by
//! default. Set `ARMV7_COMPARE_STRICT=1` to fail the
//! test on any mismatch; otherwise it just prints a
//! summary + first 50 mismatches and passes. Run with
//!
//! ```text
//! cargo test --test objdump_compare -- --ignored --nocapture
//! ```
//!
//! Requires `arm-linux-gnueabihf-objdump` on `$PATH`.

use armv8_encode::isa::armv7::{
    self,
    arm::sweep::disassemble_bytes as disassemble_arm_bytes,
    arm::table_generated::ArmMnemonicGenerated,
    sweep::disassemble_bytes,
    table_generated::ThumbMnemonicGenerated,
};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Mode {
    Thumb,
    Arm,
}

#[derive(Debug, Clone)]
struct ObjdumpInstruction {
    address: u64,
    bytes: Vec<u8>,
    mnemonic: String,
    mode: Mode,
}

#[derive(Debug)]
struct Comparison {
    address: u64,
    bytes: Vec<u8>,
    objdump_mnemonic: String,
    ours: Result<String, String>,
    mode: Mode,
}

#[test]
#[ignore = "requires arm-linux-gnueabihf-objdump and the libtool-checker.so fixture"]
fn report_objdump_comparison_for_libtool_checker() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary = PathBuf::from(manifest_dir)
        .join("tests")
        .join("libtool-checker.so");
    report_binary_comparison(&binary);
}

/// Round-trip the entire Thumb-mode portion of
/// libtool-checker.so through decode → encode.
#[test]
#[ignore = "requires the libtool-checker.so fixture"]
fn report_thumb_encoder_round_trip_for_libtool_checker() {
    use armv8_encode::isa::armv7::encode::{encode_neon_row, encode_with_row};
    use armv8_encode::isa::armv7::sweep::disassemble_bytes;
    use armv8_encode::isa::armv7::table::ThumbWidth;
    use std::collections::BTreeMap;
    let mut error_formats: BTreeMap<String, usize> = BTreeMap::new();
    let mut approximate_formats: BTreeMap<String, usize> = BTreeMap::new();

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary = PathBuf::from(manifest_dir)
        .join("tests")
        .join("libtool-checker.so");
    let raw = run_objdump(&binary);
    let instructions = parse_objdump_lines(&raw);

    let thumb_insns: Vec<&ObjdumpInstruction> = instructions
        .iter()
        .filter(|i| i.mode == Mode::Thumb)
        .collect();

    let mut roundtripped = 0usize;
    let mut approximate = 0usize;
    let mut errored = 0usize;
    let mut neon = 0usize;

    for insn in &thumb_insns {
        let decoded = match disassemble_bytes(insn.address, &insn.bytes) {
            Ok(d) if d.len() == 1 => d.into_iter().next().unwrap(),
            _ => {
                errored += 1;
                continue;
            }
        };
        let result = match (decoded.row, decoded.neon_row) {
            (Some(row), _) => {
                encode_with_row(row, &decoded.operands, decoded.address)
                    .map(|(w, width)| (w, width))
            }
            (None, Some(neon_row)) => {
                neon += 1;
                encode_neon_row(neon_row, &decoded.operands)
                    .map(|w| (w, ThumbWidth::Word))
            }
            (None, None) => {
                errored += 1;
                continue;
            }
        };
        match result {
            Ok((word, width)) => {
                let bytes_out = match width {
                    ThumbWidth::Halfword => (word as u16).to_le_bytes().to_vec(),
                    ThumbWidth::Word => {
                        let hw1 = ((word >> 16) & 0xffff) as u16;
                        let hw2 = (word & 0xffff) as u16;
                        let mut v = Vec::with_capacity(4);
                        v.extend_from_slice(&hw1.to_le_bytes());
                        v.extend_from_slice(&hw2.to_le_bytes());
                        v
                    }
                };
                if bytes_out == insn.bytes {
                    roundtripped += 1;
                } else {
                    let fmt = decoded
                        .row
                        .map(|r| r.format.to_string())
                        .unwrap_or_else(|| "<neon>".to_string());
                    *approximate_formats.entry(fmt).or_insert(0) += 1;
                    approximate += 1;
                }
            }
            Err(_) => {
                let fmt = decoded
                    .row
                    .map(|r| r.format.to_string())
                    .unwrap_or_else(|| "<neon>".to_string());
                *error_formats.entry(fmt).or_insert(0) += 1;
                errored += 1;
            }
        }
    }
    eprintln!("--- top approximate formats ---");
    let mut approx_vec: Vec<_> = approximate_formats.iter().collect();
    approx_vec.sort_by(|a, b| b.1.cmp(a.1));
    for (fmt, count) in approx_vec.iter().take(15) {
        eprintln!("  {count:5}  {fmt}");
    }
    eprintln!("--- top error formats ---");
    let mut err_vec: Vec<_> = error_formats.iter().collect();
    err_vec.sort_by(|a, b| b.1.cmp(a.1));
    for (fmt, count) in err_vec.iter().take(15) {
        eprintln!("  {count:5}  {fmt}");
    }

    eprintln!("Thumb encoder round-trip: total={}", thumb_insns.len());
    eprintln!("  exact roundtrip: {roundtripped} (incl NEON: {neon})");
    eprintln!("  approximate: {approximate}");
    eprintln!("  encoder error: {errored}");
    assert!(
        roundtripped + approximate + errored == thumb_insns.len(),
        "accounting bug: {roundtripped} + {approximate} + {errored} != {}",
        thumb_insns.len(),
    );
}

/// Round-trip the entire ARM-mode portion of
/// libtool-checker.so through decode → encode and report the
/// match rate. Verifies the new ARM encoder against real
/// PLT bytes.
#[test]
#[ignore = "requires the libtool-checker.so fixture"]
fn report_arm_encoder_round_trip_for_libtool_checker() {
    use armv8_encode::isa::armv7::arm::{
        encode::{encode_neon_row, encode_with_row}, sweep::disassemble_bytes,
    };

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary = PathBuf::from(manifest_dir)
        .join("tests")
        .join("libtool-checker.so");
    let raw = run_objdump(&binary);
    let instructions = parse_objdump_lines(&raw);

    // Group ARM instructions by their objdump-recorded
    // address; reassemble the raw bytes for a round-trip
    // sweep + encode.
    let arm_insns: Vec<&ObjdumpInstruction> = instructions
        .iter()
        .filter(|i| i.mode == Mode::Arm)
        .collect();

    let mut roundtripped = 0usize;
    let mut approximate = 0usize;
    let mut errored = 0usize;
    let mut neon = 0usize;

    for insn in &arm_insns {
        let decoded = match disassemble_bytes(insn.address, &insn.bytes) {
            Ok(d) if d.len() == 1 => d.into_iter().next().unwrap(),
            _ => {
                errored += 1;
                continue;
            }
        };
        let result = match (decoded.row, decoded.neon_row) {
            (Some(row), _) => encode_with_row(row, &decoded.operands, decoded.address),
            (None, Some(neon_row)) => {
                neon += 1;
                encode_neon_row(neon_row, &decoded.operands)
            }
            (None, None) => {
                errored += 1;
                continue;
            }
        };
        match result {
            Ok(word) => {
                let mut out = Vec::with_capacity(4);
                out.extend_from_slice(&word.to_le_bytes());
                if out == insn.bytes {
                    roundtripped += 1;
                } else {
                    eprintln!(
                        "approximate at 0x{:x}: in={:02x?} out={:02x?}",
                        insn.address, insn.bytes, out,
                    );
                    approximate += 1;
                }
            }
            Err(_) => errored += 1,
        }
    }

    eprintln!("ARM encoder round-trip: total={}", arm_insns.len());
    eprintln!("  exact roundtrip: {roundtripped} (incl NEON: {neon})");
    eprintln!("  approximate (decoder/encoder loses bits): {approximate}");
    eprintln!("  encoder error: {errored}");
    // NEON instructions are counted in `roundtripped` once
    // they encode successfully (which they do, via the
    // OpaqueBits round-trip path). `neon` is a sub-tally
    // for visibility, not an independent bucket.
    assert!(
        roundtripped + approximate + errored == arm_insns.len(),
        "accounting bug: {roundtripped} + {approximate} + {errored} != {}",
        arm_insns.len(),
    );
}

fn report_binary_comparison(binary: &Path) {
    assert!(
        binary.exists(),
        "binary does not exist: {}",
        binary.display()
    );
    let raw = run_objdump(binary);
    let instructions = parse_objdump_lines(&raw);
    assert!(
        !instructions.is_empty(),
        "no instruction lines parsed from objdump output",
    );
    let thumb_count = instructions.iter().filter(|i| i.mode == Mode::Thumb).count();
    let arm_count = instructions.iter().filter(|i| i.mode == Mode::Arm).count();
    eprintln!("parsed instructions: {thumb_count} Thumb, {arm_count} ARM");

    let comparisons: Vec<Comparison> = instructions
        .into_iter()
        .map(|insn| compare_one(insn))
        .collect();

    let mut matched = 0usize;
    let mut mismatched: Vec<&Comparison> = Vec::new();
    let mut errored = 0usize;
    for c in &comparisons {
        match &c.ours {
            Ok(ours) if normalise_mnemonic(ours) == normalise_mnemonic(&c.objdump_mnemonic) => {
                matched += 1;
            }
            Ok(_) => mismatched.push(c),
            Err(_) => errored += 1,
        }
    }

    eprintln!("binary: {}", binary.display());
    eprintln!("compared: {}", comparisons.len());
    eprintln!("matched: {matched}");
    eprintln!("mismatched: {}", mismatched.len());
    eprintln!("decode errors: {errored}");

    // Per-mode breakdown so ARM and Thumb regressions stay
    // legible.
    for mode_label in [Mode::Thumb, Mode::Arm] {
        let mut m = 0;
        let mut mm = 0;
        let mut e = 0;
        for c in comparisons.iter().filter(|c| c.mode == mode_label) {
            match &c.ours {
                Ok(ours) if normalise_mnemonic(ours) == normalise_mnemonic(&c.objdump_mnemonic) => m += 1,
                Ok(_) => mm += 1,
                Err(_) => e += 1,
            }
        }
        eprintln!("  {mode_label:?}: matched={m} mismatched={mm} errors={e}");
    }

    // Tally mismatch types for a quick gap analysis.
    let mut tally: std::collections::BTreeMap<(String, String), usize> =
        std::collections::BTreeMap::new();
    for m in &mismatched {
        let key = (
            m.objdump_mnemonic.to_string(),
            m.ours.as_ref().map(|s| s.clone()).unwrap_or_default(),
        );
        *tally.entry(key).or_insert(0) += 1;
    }
    let mut tally_vec: Vec<_> = tally.into_iter().collect();
    tally_vec.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("--- mismatch tally (top 30 (objdump → ours) pairs) ---");
    for ((obj, ours), count) in tally_vec.iter().take(30) {
        eprintln!("  {count:5}  {obj:24} → {ours}");
    }

    for m in mismatched.iter().take(50) {
        let bytes_hex: String = m
            .bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "{:#010x} [{:11}] | objdump: {:24} | ours: {}",
            m.address,
            bytes_hex,
            m.objdump_mnemonic,
            m.ours.as_ref().map(|s| s.as_str()).unwrap_or("<error>"),
        );
    }
    for c in comparisons.iter().filter(|c| c.ours.is_err()).take(20) {
        let bytes_hex: String = c
            .bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "{:#010x} [{:11}] | objdump: {:24} | ours: ERR {}",
            c.address,
            bytes_hex,
            c.objdump_mnemonic,
            c.ours.as_ref().err().map(|s| s.as_str()).unwrap_or(""),
        );
    }

    if std::env::var_os("ARMV7_COMPARE_STRICT").is_some() {
        assert_eq!(
            mismatched.len(),
            0,
            "strict objdump comparison: {} mismatches, {} errors",
            mismatched.len(),
            errored,
        );
        assert_eq!(errored, 0, "strict objdump comparison: {} errors", errored);
    }
}

fn compare_one(insn: ObjdumpInstruction) -> Comparison {
    let ours = match insn.mode {
        Mode::Thumb => match disassemble_bytes(insn.address, &insn.bytes) {
            Ok(decoded) if decoded.len() == 1 => Ok(decoded[0].mnemonic_name().to_string()),
            Ok(decoded) => Err(format!("expected 1 instruction, decoded {}", decoded.len())),
            Err(e) => Err(format!("{e}")),
        },
        Mode::Arm => match disassemble_arm_bytes(insn.address, &insn.bytes) {
            Ok(decoded) if decoded.len() == 1 => Ok(decoded[0].mnemonic_name().to_string()),
            Ok(decoded) => Err(format!("expected 1 instruction, decoded {}", decoded.len())),
            Err(e) => Err(format!("{e}")),
        },
    };
    Comparison {
        address: insn.address,
        bytes: insn.bytes,
        objdump_mnemonic: insn.mnemonic,
        ours,
        mode: insn.mode,
    }
}

fn run_objdump(binary: &Path) -> String {
    let output = Command::new("arm-linux-gnueabihf-objdump")
        .arg("-d")
        .arg(binary)
        .output()
        .expect("run arm-linux-gnueabihf-objdump");
    assert!(
        output.status.success(),
        "objdump failed for {}: {}",
        binary.display(),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).expect("objdump output utf-8")
}

/// Parse objdump output, returning instruction lines for
/// both ARM and Thumb modes (mode is inferred from the
/// word column format). Skips data lines, `<UNDEFINED>`,
/// and section headers.
fn parse_objdump_lines(text: &str) -> Vec<ObjdumpInstruction> {
    let mut out = Vec::new();
    for line in text.lines() {
        // objdump format: "  ADDR:\tBYTES\tMNEMONIC\tOPERANDS"
        // We want lines that start with whitespace + hex
        // address followed by ':'.
        let trimmed = line.trim_start();
        if !trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_hexdigit())
        {
            continue;
        }
        let mut parts = trimmed.splitn(2, '\t');
        let addr_part = parts.next().unwrap_or("").trim_end_matches(':');
        let rest = parts.next().unwrap_or("").trim_start();
        let Ok(address) = u64::from_str_radix(addr_part, 16) else {
            continue;
        };
        // Word column: split off at the first tab; everything
        // before is the bytes.
        let mut rest_parts = rest.splitn(2, '\t');
        let words_field = rest_parts.next().unwrap_or("").trim();
        let after_words = rest_parts.next().unwrap_or("").trim_start();
        // Word field looks like:
        //   "XXXX"             — Thumb 16-bit
        //   "XXXX YYYY"        — Thumb 32-bit
        //   "XXXXXXXX"         — ARM 32-bit  (skip)
        //   ""                 — section header, skip
        let words: Vec<&str> = words_field.split_whitespace().collect();
        let (bytes, mode) = match words.as_slice() {
            [w] if w.len() == 4 => match parse_hw(w) {
                Some(b) => (b.to_vec(), Mode::Thumb),
                None => continue,
            },
            [w1, w2] if w1.len() == 4 && w2.len() == 4 => {
                let Some(a) = parse_hw(w1) else { continue };
                let Some(b) = parse_hw(w2) else { continue };
                let mut v = a.to_vec();
                v.extend_from_slice(&b);
                (v, Mode::Thumb)
            }
            [w] if w.len() == 8 => match parse_arm_word(w) {
                Some(b) => (b.to_vec(), Mode::Arm),
                None => continue,
            },
            _ => continue, // data, empty, or unsupported.
        };
        // After-words field: "MNEMONIC\tOPERANDS" (or just
        // "MNEMONIC"). objdump may also emit `<UNDEFINED>`
        // for unallocated encodings — skip those.
        if after_words.is_empty() || after_words.starts_with('<') || after_words.contains("UNDEFINED") {
            continue;
        }
        let mnemonic = after_words
            .split(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_string();
        if mnemonic.is_empty() {
            continue;
        }
        out.push(ObjdumpInstruction {
            address,
            bytes,
            mnemonic,
            mode,
        });
    }
    out
}

/// Parse a 4-char hex string (one Thumb halfword) and return
/// the two little-endian bytes that would appear in the file.
/// objdump prints the halfword as the *value* (big-endian
/// human-readable), so "b500" → bytes [0x00, 0xb5].
fn parse_hw(s: &str) -> Option<[u8; 2]> {
    let v = u16::from_str_radix(s, 16).ok()?;
    Some([(v & 0xff) as u8, (v >> 8) as u8])
}

/// Parse an 8-char hex string (one ARM word) into 4
/// little-endian bytes.
fn parse_arm_word(s: &str) -> Option<[u8; 4]> {
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([
        (v & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        ((v >> 16) & 0xff) as u8,
        ((v >> 24) & 0xff) as u8,
    ])
}

/// Strip condition-code suffix and `.n`/`.w` width
/// modifiers, lowercase, so objdump's `bne.n`,
/// `addeq`, `pop.w` all collapse to the base
/// mnemonic the table reports.
fn normalise_mnemonic(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    // NEON / VFP data-type suffix (`.u16`, `.s32`, `.f64`,
    // `.i8`, `.16`, etc.) — objdump appends it; the table
    // carries it after `%c` in the format string. Strip
    // the first dotted segment that looks like a type
    // suffix.
    let lower = if let Some(dot_pos) = lower.find('.') {
        let (head, tail) = lower.split_at(dot_pos);
        // Tail starts with '.'. We strip if it looks like
        // a data-type marker: dot + optional letter +
        // digits.
        let body = &tail[1..];
        let looks_typed = !body.is_empty()
            && (body.chars().next().unwrap().is_ascii_alphabetic()
                || body.chars().next().unwrap().is_ascii_digit())
            && body.chars().all(|c| c.is_ascii_alphanumeric());
        if looks_typed && body != "n" && body != "w" {
            head.to_string()
        } else {
            lower.clone()
        }
    } else {
        lower.clone()
    };
    // Strip Thumb width suffix.
    let no_width = lower
        .strip_suffix(".n")
        .or_else(|| lower.strip_suffix(".w"))
        .unwrap_or(&lower)
        .to_string();
    // Strip the trailing `l` on coprocessor `ldcl`/`stcl`
    // (long form — binutils encodes via `%22'l`).
    if no_width == "ldcl" {
        return "ldc".to_string();
    }
    if no_width == "stcl" {
        return "stc".to_string();
    }
    // IT-block variants (`itt`, `ittt`, `itttt`, `ite`,
    // `itee`, etc.) — the trailing T/E sequence selects how
    // many follow-on instructions inherit the condition,
    // encoded in binutils as a format-string suffix rather
    // than a separate mnemonic. Our table only emits `it`.
    if no_width.starts_with("it") && no_width.len() <= 5
        && no_width[2..].chars().all(|c| c == 't' || c == 'e')
    {
        return "it".to_string();
    }
    // 32-bit Thumb-2 byte / halfword load-stores. Objdump
    // emits e.g. `ldrb.w`, `strh.w`; the table reports the
    // base `ldr`/`str` and carries the width-letter via the
    // `%w` format code. Treat them as the base.
    for prefix in &["ldr", "str"] {
        for suffix in &["b", "h", "sb", "sh"] {
            let combined = format!("{prefix}{suffix}");
            if no_width == combined {
                return prefix.to_string();
            }
        }
    }
    // Try the flag-setting `s` suffix first — `lsls`,
    // `movs`, etc. These would otherwise collide with the
    // cond-code stripper (which would see e.g. the `ls` in
    // `lsls` as a condition).
    let stripped_s = strip_flag_setting_s(&no_width);
    if stripped_s != no_width {
        return stripped_s;
    }
    // Strip condition-code suffix when the rest is a real
    // mnemonic. Conditions are 2-char ARM codes.
    const CONDS: &[&str] = &[
        "eq", "ne", "cs", "hs", "cc", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt",
        "le", "al",
    ];
    for cond in CONDS {
        if let Some(prefix) = no_width.strip_suffix(cond) {
            // Only strip when the prefix is a known
            // mnemonic and not itself "b" (for unconditional
            // `b`, the cond strip would empty it). The
            // simplest heuristic: require ≥2 prefix chars.
            if prefix.len() >= 2 {
                return strip_flag_setting_s(prefix);
            }
            // Special: "b" + cond is a conditional branch.
            // Our decoder reports `b` as the mnemonic for
            // both unconditional and conditional 16-bit
            // branches.
            if prefix == "b" {
                return "b".to_string();
            }
        }
    }
    strip_flag_setting_s(&no_width)
}

/// Remove the trailing `s` that objdump appends to
/// flag-setting Thumb-1 / Thumb-2 forms (`adds`, `subs`,
/// `movs`, `lsls`, `ands`, …). Our table reports the base
/// mnemonic — binutils encodes the flag-setting bit via the
/// format string's `%C` print specifier rather than a
/// distinct mnemonic. We use a small allow-list to avoid
/// stripping `s` from genuine names like `bx`, `bl`, `mls`.
fn strip_flag_setting_s(name: &str) -> String {
    // Mnemonics where a trailing `s` is the flag-setting
    // marker rather than part of the name. Objdump prints
    // the `s` form when the encoding sets condition flags;
    // our table reports the base form.
    const FLAG_SETTING_BASES: &[&str] = &[
        "add", "sub", "mov", "and", "orr", "eor", "bic", "lsl", "lsr", "asr", "ror", "rsb", "mul",
        "mvn", "mla", "adc", "sbc", "neg", "cmn", "tst",
    ];
    for base in FLAG_SETTING_BASES {
        if name == &format!("{base}s") {
            return base.to_string();
        }
    }
    name.to_string()
}

/// Lower-case the generated enum's variant name. The enum
/// uses PascalCase (e.g. `Push`, `Bx`, `Ldrh`) — strip the
/// case to match objdump's output. Prefer `as_str()` since
/// it preserves the punctuation form (e.g. `vmla.f32`)
/// rather than collapsing to `vmlaf32`.
fn thumb_mnemonic_name(m: ThumbMnemonicGenerated) -> String {
    m.as_str().to_string()
}

fn arm_mnemonic_name(m: ArmMnemonicGenerated) -> String {
    m.as_str().to_string()
}

// Pull armv7 into scope for modules used above.
#[allow(unused_imports)]
use armv7 as _ensure_module_in_scope;
