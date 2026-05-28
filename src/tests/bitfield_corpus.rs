//! Corpus roundtrip test for the bitfield-family alias decoder.
//!
//! Sweeps a grid of (sf, immr, imms) values across UBFM / SBFM / BFM,
//! constructing each instruction word independently of the decoder and then:
//!
//! 1. Predicts which alias the disassembler should report (per the
//!    ARM ARM precedence rules: exact-match aliases > extract/insert
//!    based on `imms` vs `immr`).
//! 2. Decodes the word via `decode_instruction` and asserts the reported
//!    mnemonic and the (lsb, width) immediates match the prediction.
//! 3. Re-encodes via the canonical mnemonic + decoded operands and asserts
//!    the resulting 32-bit word is byte-identical to the input.
//!
//! This kind of "construct from spec, not from fixture" sweep is the
//! lowest-cost way to catch decoder defects that share the shape of the
//! ones fixed in `armv8_encode_bitfield_alias_decoder.md`: alias
//! disambiguation drift, raw-vs-unwrapped field reporting, and arithmetic
//! that depends on the alias mnemonic.

use crate::isa::aarch64::{decode_instruction, encode_instruction, DecodedOperand, InstructionTemplate};

#[derive(Debug, Clone, Copy)]
struct Expected {
    mnemonic: &'static str,
    /// Position of the `Imm` operand in `DecodedInstruction::operands`
    /// (None means the alias has no immediate operand).
    lsb: Option<i64>,
    /// Position of the `Width` operand (when applicable).
    width: Option<i64>,
}

/// Predict the disassembler's alias selection and the (lsb, width) the
/// disassembler should report, given the raw (sf, immr, imms) of a
/// UBFM/SBFM/BFM encoding.
///
/// Mirrors the ARM ARM's alias precedence: exact-match aliases
/// (`sxtb`/`sxth`/`sxtw`/`uxtb`/`uxth`, `lsl`/`lsr`/`asr` with the
/// fixed-bit patterns) win over the extract/insert siblings, which are
/// disambiguated by `imms >= immr` (extract) vs `imms < immr` (insert).
fn predict_alias(base: &str, sf: u32, immr: u32, imms: u32) -> Expected {
    let max = if sf == 0 { 31 } else { 63 };
    let regsize = max + 1;

    match base {
        "ubfm" => {
            // Exact-match: uxtb (sf=0, immr=0, imms=7), uxth (sf=0, immr=0, imms=15).
            if sf == 0 && immr == 0 && imms == 7 {
                return Expected { mnemonic: "uxtb", lsb: None, width: None };
            }
            if sf == 0 && immr == 0 && imms == 15 {
                return Expected { mnemonic: "uxth", lsb: None, width: None };
            }
            // LSL alias: imms != max && imms + 1 == immr.
            if imms != max && imms + 1 == immr {
                let shift = (max - imms) as i64;
                return Expected { mnemonic: "lsl", lsb: Some(shift), width: None };
            }
            // LSR alias: imms == max.
            if imms == max {
                return Expected { mnemonic: "lsr", lsb: Some(immr as i64), width: None };
            }
            if imms >= immr {
                Expected {
                    mnemonic: "ubfx",
                    lsb: Some(immr as i64),
                    width: Some((imms as i64) - (immr as i64) + 1),
                }
            } else {
                Expected {
                    mnemonic: "ubfiz",
                    lsb: Some(((regsize - immr) % regsize) as i64),
                    width: Some((imms as i64) + 1),
                }
            }
        }
        "sbfm" => {
            if sf == 1 && immr == 0 && imms == 7 {
                return Expected { mnemonic: "sxtb", lsb: None, width: None };
            }
            if sf == 1 && immr == 0 && imms == 15 {
                return Expected { mnemonic: "sxth", lsb: None, width: None };
            }
            if sf == 1 && immr == 0 && imms == 31 {
                return Expected { mnemonic: "sxtw", lsb: None, width: None };
            }
            if sf == 0 && immr == 0 && imms == 7 {
                return Expected { mnemonic: "sxtb", lsb: None, width: None };
            }
            if sf == 0 && immr == 0 && imms == 15 {
                return Expected { mnemonic: "sxth", lsb: None, width: None };
            }
            // ASR: imms == max.
            if imms == max {
                return Expected { mnemonic: "asr", lsb: Some(immr as i64), width: None };
            }
            if imms >= immr {
                Expected {
                    mnemonic: "sbfx",
                    lsb: Some(immr as i64),
                    width: Some((imms as i64) - (immr as i64) + 1),
                }
            } else {
                Expected {
                    mnemonic: "sbfiz",
                    lsb: Some(((regsize - immr) % regsize) as i64),
                    width: Some((imms as i64) + 1),
                }
            }
        }
        "bfm" => {
            if imms >= immr {
                Expected {
                    mnemonic: "bfxil",
                    lsb: Some(immr as i64),
                    width: Some((imms as i64) - (immr as i64) + 1),
                }
            } else {
                Expected {
                    mnemonic: "bfi",
                    lsb: Some(((regsize - immr) % regsize) as i64),
                    width: Some((imms as i64) + 1),
                }
            }
        }
        _ => unreachable!(),
    }
}

fn assemble(base: &str, sf: u32, rd: u32, rn: u32, immr: u32, imms: u32) -> u32 {
    let opc = match base {
        "sbfm" => 0b00,
        "bfm" => 0b01,
        "ubfm" => 0b10,
        _ => unreachable!(),
    };
    // Layout: sf | opc[1:0] | 100110 | N | immr[5:0] | imms[5:0] | Rn[4:0] | Rd[4:0]
    // N = sf for the 32/64-bit bitfield encodings.
    let n = sf;
    (sf << 31)
        | (opc << 29)
        | (0b100110 << 23)
        | (n << 22)
        | ((immr & 0x3f) << 16)
        | ((imms & 0x3f) << 10)
        | ((rn & 0x1f) << 5)
        | (rd & 0x1f)
}

fn check_one(base: &str, sf: u32, immr: u32, imms: u32) {
    // 32-bit forms have a structural constraint: immr[5] and imms[5] must be 0.
    // The decoder rejects those — skip combinations that don't satisfy it.
    if sf == 0 && (immr >= 32 || imms >= 32) {
        return;
    }

    let rd = 1;
    let rn = 2;
    let word = assemble(base, sf, rd, rn, immr, imms);

    let expected = predict_alias(base, sf, immr, imms);

    let decoded = decode_instruction(0, word).unwrap_or_else(|err| {
        panic!(
            "decode failed for {base} sf={sf} immr={immr} imms={imms} word={word:#010x}: {err:?}"
        )
    });

    let actual_mnemonic = decoded.format_mnemonic();
    assert_eq!(
        actual_mnemonic, expected.mnemonic,
        "mnemonic mismatch for {base} sf={sf} immr={immr} imms={imms} word={word:#010x} \
         (expected {}, got {actual_mnemonic} with operands {})",
        expected.mnemonic,
        decoded.format_operands(),
    );

    // Find the Imm operand among decoded.operands; not all aliases have one
    // (e.g. sxtb/uxtb).
    let mut imm_iter = decoded.operands.iter().filter_map(|op| match op {
        DecodedOperand::Immediate(v) => Some(*v),
        _ => None,
    });

    if let Some(want_lsb) = expected.lsb {
        let got = imm_iter
            .next()
            .unwrap_or_else(|| panic!("missing lsb for {base} sf={sf} immr={immr} imms={imms}"));
        assert_eq!(
            got, want_lsb,
            "lsb mismatch for {base} sf={sf} immr={immr} imms={imms} word={word:#010x} \
             ({} {})",
            actual_mnemonic,
            decoded.format_operands(),
        );
    }
    if let Some(want_width) = expected.width {
        let got = imm_iter.next().unwrap_or_else(|| {
            panic!("missing width for {base} sf={sf} immr={immr} imms={imms}")
        });
        assert_eq!(
            got, want_width,
            "width mismatch for {base} sf={sf} immr={immr} imms={imms} word={word:#010x} \
             ({} {})",
            actual_mnemonic,
            decoded.format_operands(),
        );
    }

    // Byte-equal re-encode roundtrip.
    let template = InstructionTemplate {
        address: 0,
        mnemonic: decoded.mnemonic,
        operands: decoded.operands.clone(),
    };
    let encoded = encode_instruction(&template).unwrap_or_else(|err| {
        panic!(
            "re-encode failed for {base} sf={sf} immr={immr} imms={imms} word={word:#010x} \
             ({} {}): {err:?}",
            actual_mnemonic,
            decoded.format_operands(),
        )
    });
    assert_eq!(
        encoded, word,
        "roundtrip word mismatch for {base} sf={sf} immr={immr} imms={imms} \
         (original {word:#010x}, re-encoded {encoded:#010x}, alias {actual_mnemonic} {})",
        decoded.format_operands(),
    );
}

#[test]
fn bitfield_family_full_sweep_roundtrips() {
    // Sweep every legal (sf, immr, imms) for the bitfield family.
    // 64-bit forms: immr/imms in 0..64. 32-bit forms: 0..32.
    // Total cases: 64*64 + 32*32 = 5120 per base mnemonic, ×3 bases = 15360.
    for base in ["sbfm", "ubfm", "bfm"] {
        for sf in [0u32, 1] {
            let cap = if sf == 0 { 32 } else { 64 };
            for immr in 0..cap {
                for imms in 0..cap {
                    check_one(base, sf, immr, imms);
                }
            }
        }
    }
}
