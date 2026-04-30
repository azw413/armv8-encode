use armv8_encode::isa::aarch64;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Eq, PartialEq)]
struct OtoolInstruction {
    address: u64,
    mnemonic: String,
    operands: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Comparison {
    address: u64,
    word: u32,
    otool: String,
    ours: String,
}

#[test]
#[ignore = "requires macOS otool and compares a host Mach-O binary; run with --ignored --nocapture"]
fn report_otool_comparison_for_current_test_binary() {
    let binary = std::env::current_exe().expect("current test executable path");
    report_binary_comparison(&binary, 512);
}

#[test]
#[ignore = "requires macOS otool and compares a host Mach-O binary; set ARMV8_COMPARE_BINARY or defaults to /bin/ls"]
fn report_otool_comparison_for_system_binary() {
    let binary = std::env::var_os("ARMV8_COMPARE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/ls"));
    report_binary_comparison(&binary, 512);
}

fn report_binary_comparison(binary: &Path, limit: usize) {
    assert!(
        binary.exists(),
        "binary does not exist: {}",
        binary.display()
    );

    let comparisons = compare_binary_with_otool(binary, limit);
    assert!(
        !comparisons.is_empty(),
        "no comparable __TEXT,__text instructions in {}",
        binary.display()
    );

    let mismatches = comparisons
        .iter()
        .filter(|comparison| comparison.otool != comparison.ours)
        .collect::<Vec<_>>();

    eprintln!("binary: {}", binary.display());
    eprintln!("compared: {}", comparisons.len());
    eprintln!("matched: {}", comparisons.len() - mismatches.len());
    eprintln!("mismatched: {}", mismatches.len());

    for mismatch in mismatches.iter().take(50) {
        eprintln!(
            "{:#018x} {:#010x} | otool: {:48} | ours: {}",
            mismatch.address, mismatch.word, mismatch.otool, mismatch.ours
        );
    }

    if std::env::var_os("ARMV8_COMPARE_STRICT").is_some() {
        assert_eq!(
            mismatches,
            Vec::<&Comparison>::new(),
            "strict otool comparison failed"
        );
    }
}

fn compare_binary_with_otool(binary: &Path, limit: usize) -> Vec<Comparison> {
    let words = parse_otool_text_words(&run_otool(binary, &["-s", "__TEXT", "__text"]));
    let instructions = parse_otool_disassembly(&run_otool(binary, &["-tvV"]));

    instructions
        .into_iter()
        .filter_map(|instruction| {
            let word = *words.get(&instruction.address)?;
            let ours = aarch64::decode_instruction(instruction.address, word)
                .map(|decoded| {
                    format_instruction(decoded.format_mnemonic(), decoded.format_operands())
                })
                .unwrap_or_else(|| "<no match>".to_string());
            let ours = strip_otool_comment(&ours).to_string();
            let otool = format_instruction(
                instruction.mnemonic,
                strip_otool_comment(&instruction.operands).to_string(),
            );

            Some(Comparison {
                address: instruction.address,
                word,
                otool,
                ours,
            })
        })
        .take(limit)
        .collect()
}

fn run_otool(binary: &Path, args: &[&str]) -> String {
    let output = Command::new("otool")
        .args(args)
        .arg(binary)
        .output()
        .expect("run otool");

    assert!(
        output.status.success(),
        "otool failed for {}: {}",
        binary.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("otool output should be utf-8")
}

fn parse_otool_text_words(text: &str) -> BTreeMap<u64, u32> {
    let mut words = BTreeMap::new();

    for line in text.lines().map(str::trim) {
        let mut fields = line.split_whitespace();
        let Some(address_text) = fields.next() else {
            continue;
        };
        let Ok(mut address) = u64::from_str_radix(address_text, 16) else {
            continue;
        };

        for word_text in fields {
            if word_text.len() != 8 {
                continue;
            }
            if let Ok(word) = u32::from_str_radix(word_text, 16) {
                words.insert(address, word);
                address += 4;
            }
        }
    }

    words
}

fn parse_otool_disassembly(text: &str) -> Vec<OtoolInstruction> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let mut fields = line.split_whitespace();
            let address = u64::from_str_radix(fields.next()?, 16).ok()?;
            let mnemonic = fields.next()?.to_string();
            let operands = fields.collect::<Vec<_>>().join(" ");

            Some(OtoolInstruction {
                address,
                mnemonic,
                operands,
            })
        })
        .collect()
}

fn format_instruction(mnemonic: String, operands: String) -> String {
    if operands.is_empty() {
        mnemonic
    } else {
        format!("{mnemonic} {operands}")
    }
}

fn strip_otool_comment(operands: &str) -> &str {
    operands
        .split_once(';')
        .map_or(operands, |(before, _)| before)
        .trim_end()
}
