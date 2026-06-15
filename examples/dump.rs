//! Print the header, sections, symbols, functions, relocations, and a
//! disassembly of every text section of a Mach-O or ELF object file.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example dump -- path/to/binary
//! ```
//!
//! Works on .o / .so / .dylib / linked executables. Non-AArch64 inputs
//! are still inspectable for headers and symbols; the disassembly section
//! is skipped with a notice.

use armv8_encode::container::{
    Architecture, Container, ContainerError, Function, FunctionProvenance, RelocationKind,
    SectionId, SectionKind, Symbol, SymbolKind,
};
use armv8_encode::isa::aarch64;
use armv8_encode::isa::aarch64::DecodedInstruction;
use armv8_encode::mc::{
    build_cfg, BasicBlock, ControlFlow, ControlFlowGraph, EdgeKind, EdgeTarget,
};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::ExitCode;

const USAGE: &str = "usage: dump [OPTIONS] <FILE>

Inspect a Mach-O or ELF object file. Prints headers, the section table,
defined and undefined symbols, derived functions, a summary of
relocations, and a disassembly of every text section.

Options:
  --cfg NAME         draw the control-flow graph of function NAME and
                     skip the rest of the output
  --no-disasm        skip the disassembly section
  --max-listing N    cap symbol/function listings at N rows (default 50)
  -h, --help         show this help and exit";

const DEFAULT_LISTING_LIMIT: usize = 50;

struct Args {
    path: String,
    disassemble: bool,
    listing_limit: usize,
    cfg_function: Option<String>,
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(ParseError::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(ParseError::Usage(message)) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let bytes = match fs::read(&args.path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", args.path);
            return ExitCode::FAILURE;
        }
    };

    // macOS system binaries (and most Apple-shipped libraries) are fat
    // Mach-O — multiple architecture slices in one file. The `object`
    // crate's autodetect doesn't pick a slice; do it ourselves so the
    // common "what's in /bin/ls" case works.
    let (parse_bytes, slice_note) = match select_macho_arm64_slice(&bytes) {
        Some((slice, note)) => (slice, Some(note)),
        None => (bytes.as_slice(), None),
    };

    let container = match Container::from_bytes(parse_bytes) {
        Ok(container) => container,
        Err(error) => {
            eprintln!("error: cannot parse {}: {error}", args.path);
            if let ContainerError::Parse(detail) = &error {
                eprintln!("  detail: {detail}");
            }
            return ExitCode::FAILURE;
        }
    };

    if let Some(note) = slice_note {
        println!("{note}");
        println!();
    }

    print_header(&container, &args.path, bytes.len());

    if let Some(name) = args.cfg_function.as_deref() {
        if let Err(message) = print_cfg(&container, name) {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    print_sections(&container);
    print_symbols(&container, args.listing_limit);
    print_functions(&container, args.listing_limit);
    print_dwarf(&container, args.listing_limit);
    print_relocation_summary(&container);

    if args.disassemble {
        print_disassembly(&container);
    }

    ExitCode::SUCCESS
}

#[derive(Debug)]
enum ParseError {
    Help,
    Usage(String),
}

fn parse_args() -> Result<Args, ParseError> {
    let mut path: Option<String> = None;
    let mut disassemble = true;
    let mut listing_limit = DEFAULT_LISTING_LIMIT;
    let mut cfg_function: Option<String> = None;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Err(ParseError::Help),
            "--no-disasm" => disassemble = false,
            "--max-listing" => {
                let value = iter
                    .next()
                    .ok_or_else(|| ParseError::Usage("--max-listing requires a value".into()))?;
                listing_limit = value.parse().map_err(|_| {
                    ParseError::Usage(format!("--max-listing expects a number, got {value:?}"))
                })?;
            }
            "--cfg" => {
                let value = iter
                    .next()
                    .ok_or_else(|| ParseError::Usage("--cfg requires a function name".into()))?;
                cfg_function = Some(value);
            }
            other if other.starts_with("--") => {
                return Err(ParseError::Usage(format!("unknown flag {other}")));
            }
            _ => {
                if path.is_some() {
                    return Err(ParseError::Usage(
                        "exactly one input file is supported".into(),
                    ));
                }
                path = Some(arg);
            }
        }
    }

    let path = path.ok_or_else(|| ParseError::Usage("missing <FILE>".into()))?;
    Ok(Args {
        path,
        disassemble,
        listing_limit,
        cfg_function,
    })
}

// ---- Output sections -----------------------------------------------------

fn print_header(container: &Container, path: &str, byte_size: usize) {
    println!("File:      {path}");
    println!("Size:      {byte_size} bytes");
    println!("Format:    {:?}", container.format);
    println!("Arch:      {:?}", container.architecture);
    if !matches!(
        container.architecture,
        Architecture::Aarch64 | Architecture::X86_64 | Architecture::X86
    ) {
        println!("Note:      disassembly only works for AArch64 and x86; other listings still apply.");
    }
    println!();
}

fn print_sections(container: &Container) {
    println!("Sections ({}):", container.sections.len());
    println!(
        "  {:>3}  {:<32}  {:>16}  {:>10}  {}",
        "id", "name", "address", "size", "kind"
    );
    for section in &container.sections {
        println!(
            "  {:>3}  {:<32}  {:>16x}  {:>10}  {:?}",
            section.id.0,
            truncate(&section.name, 32),
            section.address,
            section.size,
            section.kind,
        );
    }
    println!();
}

fn print_symbols(container: &Container, limit: usize) {
    let defined: Vec<&Symbol> = container.defined_symbols().collect();
    let undefined: Vec<&Symbol> = container
        .symbols
        .iter()
        .filter(|symbol| symbol.is_undefined)
        .collect();

    println!("Defined symbols ({}):", defined.len());
    print_symbol_table_rows(&defined, limit, true);
    println!();

    println!("Undefined symbols ({}):", undefined.len());
    print_symbol_table_rows(&undefined, limit, false);
    println!();
}

fn print_symbol_table_rows(symbols: &[&Symbol], limit: usize, show_address: bool) {
    if symbols.is_empty() {
        println!("  (none)");
        return;
    }

    if show_address {
        println!(
            "  {:>16}  {:>9}  {:>7}  {}",
            "address", "kind", "binding", "name"
        );
    } else {
        println!("  {:>9}  {:>7}  {}", "kind", "binding", "name");
    }

    for symbol in symbols.iter().take(limit) {
        if show_address {
            println!(
                "  {:>16x}  {:>9}  {:>7}  {}",
                symbol.address,
                short_symbol_kind(symbol.kind),
                short_binding(&format!("{:?}", symbol.binding)),
                symbol.name,
            );
        } else {
            println!(
                "  {:>9}  {:>7}  {}",
                short_symbol_kind(symbol.kind),
                short_binding(&format!("{:?}", symbol.binding)),
                symbol.name,
            );
        }
    }

    if symbols.len() > limit {
        println!("  ... and {} more", symbols.len() - limit);
    }
}

fn print_functions(container: &Container, limit: usize) {
    let functions = container.functions();
    println!("Functions ({}):", functions.len());

    if functions.is_empty() {
        println!("  (none)");
        println!();
        return;
    }

    println!(
        "  {:>16}  {:>10}  {:<8}  {}",
        "address", "size", "src", "name"
    );
    for function in functions.iter().take(limit) {
        let provenance = match function.provenance {
            FunctionProvenance::Symbol => "symbol",
            FunctionProvenance::Dwarf => "dwarf",
        };
        println!(
            "  {:>16x}  {:>10}  {:<8}  {}",
            function.address, function.size, provenance, function.name,
        );
    }
    if functions.len() > limit {
        println!("  ... and {} more", functions.len() - limit);
    }
    println!();
}

fn print_dwarf(container: &Container, limit: usize) {
    let Some(dwarf) = container.dwarf.as_ref() else {
        println!("DWARF:     (none)");
        println!();
        return;
    };

    println!("DWARF subprograms ({}):", dwarf.functions.len());
    println!(
        "  {:>16}  {:>10}  {:<24}  {}",
        "address", "size", "source", "name"
    );
    for func in dwarf.functions.iter().take(limit) {
        let source = match (&func.source_file, func.source_line) {
            (Some(file), Some(line)) => format!("{}:{}", file, line),
            (Some(file), None) => file.clone(),
            (None, Some(line)) => format!("?:{line}"),
            (None, None) => String::new(),
        };
        println!(
            "  {:>16x}  {:>10}  {:<24}  {}",
            func.address,
            func.size,
            truncate(&source, 24),
            func.name
        );
    }
    if dwarf.functions.len() > limit {
        println!("  ... and {} more", dwarf.functions.len() - limit);
    }
    println!();
}

fn print_relocation_summary(container: &Container) {
    println!("Relocations ({}):", container.relocations.len());
    if container.relocations.is_empty() {
        println!("  (none)");
        println!();
        return;
    }

    let mut by_kind: HashMap<String, usize> = HashMap::new();
    for relocation in &container.relocations {
        *by_kind
            .entry(relocation_kind_label(relocation.kind))
            .or_insert(0) += 1;
    }
    let mut counts: Vec<(String, usize)> = by_kind.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    println!("  {:>6}  {}", "count", "kind");
    for (kind, count) in counts {
        println!("  {count:>6}  {kind}");
    }
    println!();
}

fn print_disassembly(container: &Container) {
    match container.architecture {
        Architecture::Aarch64 => {}
        Architecture::X86_64 | Architecture::X86 => {
            print_disassembly_x86(container);
            return;
        }
        _ => {
            println!("Disassembly: skipped (architecture is not AArch64 or x86).");
            return;
        }
    }

    let symbol_map: HashMap<u64, String> = container
        .symbols
        .iter()
        .filter(|symbol| !symbol.is_undefined && !symbol.name.is_empty())
        .map(|symbol| (symbol.address, symbol.name.clone()))
        .collect();
    let symbol_for_address = |address: u64| symbol_map.get(&address).cloned();

    let text_sections: Vec<_> = container.text_sections().collect();
    if text_sections.is_empty() {
        println!("Disassembly: no text sections present.");
        return;
    }

    for section in text_sections {
        let Some((base, bytes)) = section.for_disassembly() else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }

        // Entry points: every defined function symbol that lives in this
        // section, plus the section's start address. The latter
        // catches stripped binaries (where there are no function symbols
        // but the section's first byte is the real entry) without
        // sacrificing accuracy on unstripped ones.
        let mut entry_points: Vec<u64> = container
            .symbols
            .iter()
            .filter(|symbol| {
                symbol.kind == SymbolKind::Function
                    && symbol.section == Some(section.id)
                    && !symbol.is_undefined
            })
            .map(|symbol| symbol.address)
            .collect();
        if !entry_points.contains(&base) {
            entry_points.push(base);
        }

        let disassembly = aarch64::disassemble_recursive(base, bytes, &entry_points);

        let instruction_count = disassembly.instructions.len();
        let data_byte_count: usize = disassembly
            .data_ranges
            .iter()
            .map(|range| range.bytes.len())
            .sum();

        println!(
            "Disassembly of {} ({} bytes at {:#x}, {} instructions, {} data bytes):",
            section.name,
            bytes.len(),
            base,
            instruction_count,
            data_byte_count,
        );

        for entry in disassembly.timeline() {
            match entry {
                aarch64::TimelineEntry::Instruction(decoded) => {
                    let address = decoded.address;
                    if let Some(label) = symbol_map.get(&address) {
                        println!("\n{label}:");
                    }
                    let mnemonic = decoded.format_mnemonic();
                    let operands = decoded.format_operands_with_symbols(&symbol_for_address);
                    let word_offset = (address - base) as usize;
                    let word = u32::from_le_bytes([
                        bytes[word_offset],
                        bytes[word_offset + 1],
                        bytes[word_offset + 2],
                        bytes[word_offset + 3],
                    ]);
                    println!(
                        "  {address:>10x}: {word:08x}    {mnemonic:<10} {operands}"
                    );
                }
                aarch64::TimelineEntry::Data(range) => {
                    print_data_range(range);
                }
            }
        }
        println!();
    }
}

/// x86 / x86_64 disassembly: linear sweep per text section, rendered
/// with iced's Intel-syntax formatter. Simpler than the AArch64 path
/// (no recursive-descent timeline) — the variable-length stream is
/// decoded straight through and labelled with any symbol at each
/// address.
fn print_disassembly_x86(container: &Container) {
    use armv8_encode::isa::x86;
    use iced_x86::{Formatter, IntelFormatter};

    let bitness = match x86::bitness_for_architecture(container.architecture) {
        Some(b) => b,
        None => return,
    };

    let symbol_map: HashMap<u64, String> = container
        .symbols
        .iter()
        .filter(|symbol| !symbol.is_undefined && !symbol.name.is_empty())
        .map(|symbol| (symbol.address, symbol.name.clone()))
        .collect();

    let text_sections: Vec<_> = container.text_sections().collect();
    if text_sections.is_empty() {
        println!("Disassembly: no text sections present.");
        return;
    }

    let mut formatter = IntelFormatter::new();
    for section in text_sections {
        let Some((base, bytes)) = section.for_disassembly() else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }

        match x86::disassemble_bytes(base, bytes, bitness) {
            Ok(instructions) => {
                println!(
                    "Disassembly of {} ({} bytes at {:#x}, {} instructions):",
                    section.name,
                    bytes.len(),
                    base,
                    instructions.len(),
                );
                for decoded in &instructions {
                    if let Some(label) = symbol_map.get(&decoded.address) {
                        println!("\n{label}:");
                    }
                    let mut text = String::new();
                    formatter.format(&decoded.instr, &mut text);
                    println!("  {:>10x}: {text}", decoded.address);
                }
            }
            Err(err) => {
                println!(
                    "Disassembly of {} ({} bytes at {:#x}): failed — {err}",
                    section.name,
                    bytes.len(),
                    base,
                );
            }
        }
        println!();
    }
}

fn print_data_range(range: &aarch64::DataRange) {
    let label = match range.reason {
        aarch64::DataReason::Unreachable => "unreachable",
        aarch64::DataReason::DecodeError => "undecoded",
        aarch64::DataReason::Padding => "padding",
    };
    println!(
        "\n  ; {} bytes of {} data at {:#x}",
        range.bytes.len(),
        label,
        range.address
    );
    // For aligned 4-byte chunks render as `.word`; trailing odd bytes as
    // `.byte` to keep things tidy when the section has padding.
    let chunks = range.bytes.chunks(4);
    let mut offset = 0u64;
    for chunk in chunks {
        let address = range.address + offset;
        if chunk.len() == 4 {
            let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            println!("  {address:>10x}: {word:08x}    .word");
        } else {
            let bytes_str = chunk
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  {address:>10x}: {bytes_str:<8}    .byte");
        }
        offset += chunk.len() as u64;
    }
}

// ---- Control-flow graph rendering ---------------------------------------

fn print_cfg(container: &Container, function_name: &str) -> Result<(), String> {
    if container.architecture != Architecture::Aarch64 {
        return Err(format!(
            "--cfg only works on AArch64 binaries; this is {:?}",
            container.architecture
        ));
    }

    let function = locate_function(container, function_name).ok_or_else(|| {
        format!(
            "function '{function_name}' not found (try the linker-prefixed form, e.g. '_{function_name}')"
        )
    })?;

    let bytes = function_bytes(container, &function)?;
    if bytes.is_empty() {
        return Err(format!(
            "function '{}' at {:#x} has no bytes (size 0 with no successor symbol to bound it)",
            function.name, function.address
        ));
    }

    let instructions = aarch64::disassemble_bytes(function.address, bytes)
        .map_err(|err| format!("disassembly failed: {err:?}"))?;
    let cfg = build_cfg(&instructions);

    let symbols: HashMap<u64, String> = container
        .symbols
        .iter()
        .filter(|symbol| !symbol.is_undefined && !symbol.name.is_empty())
        .map(|symbol| (symbol.address, symbol.name.clone()))
        .collect();

    println!(
        "Control-flow graph of {} ({:#x}, {} bytes, {} blocks):",
        function.name,
        function.address,
        function.size.max(bytes.len() as u64),
        cfg.blocks.len()
    );
    println!();

    for (index, block) in cfg.blocks.iter().enumerate() {
        render_block(block, &instructions, &symbols);
        render_edges(block, &cfg, &symbols);
        if index + 1 < cfg.blocks.len() {
            println!("    ▼");
        }
    }

    Ok(())
}

/// Find the function with the requested name, tolerating common linker
/// prefixes (`_name` for Mach-O, `name` for ELF).
fn locate_function(container: &Container, name: &str) -> Option<Function> {
    let alternatives = [name.to_string(), format!("_{name}")];
    for candidate in &alternatives {
        if let Some(function) = container
            .functions()
            .into_iter()
            .find(|function| function.name == *candidate)
        {
            return Some(function);
        }
    }
    None
}

/// Slice the section bytes for `function`, falling back to "next defined
/// symbol in the same section" when the function size is 0 (common for
/// Mach-O, where the standard symbol table has no size field).
fn function_bytes<'a>(
    container: &'a Container,
    function: &Function,
) -> Result<&'a [u8], String> {
    let section = container.section(function.section);
    if function.address < section.address {
        return Err(format!(
            "function {} at {:#x} is before section {} ({:#x})",
            function.name, function.address, section.name, section.address
        ));
    }
    let offset = (function.address - section.address) as usize;
    let size = if function.size > 0 {
        function.size as usize
    } else {
        infer_function_size(container, function, section.id) as usize
    };
    let end = offset.checked_add(size).ok_or("offset+size overflow")?;
    section
        .bytes
        .get(offset..end)
        .ok_or_else(|| {
            format!(
                "function bytes ({:#x}..{:#x}) outside section {} ({} bytes)",
                function.address,
                function.address + size as u64,
                section.name,
                section.bytes.len(),
            )
        })
}

/// Distance from `function.address` to the next defined symbol in the
/// same section, or to the end of the section if none follows.
fn infer_function_size(
    container: &Container,
    function: &Function,
    section: SectionId,
) -> u64 {
    let next = container
        .symbols
        .iter()
        .filter(|symbol| !symbol.is_undefined && symbol.section == Some(section))
        .map(|symbol| symbol.address)
        .filter(|&address| address > function.address)
        .min();

    let section_end = {
        let s = container.section(section);
        s.address + s.size
    };
    let end = next.unwrap_or(section_end);
    end.saturating_sub(function.address)
}

fn render_block(
    block: &BasicBlock,
    instructions: &[DecodedInstruction],
    symbols: &HashMap<u64, String>,
) {
    let symbol_for = |address: u64| symbols.get(&address).cloned();

    let header = format!(
        "[block {}] {:#x}..{:#x}  ({})",
        block.id.0,
        block.start,
        block.end,
        terminator_label(block.terminator)
    );

    let body: Vec<String> = instructions[block.instructions.clone()]
        .iter()
        .map(|insn| {
            let mnemonic = insn.format_mnemonic();
            let operands = insn.format_operands_with_symbols(&symbol_for);
            format!("{:>10x}  {:<10} {}", insn.address, mnemonic, operands)
        })
        .collect();

    let inner_width = body
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .max(header.chars().count())
        + 2;

    println!("┌─ {header}{}", dashes(inner_width.saturating_sub(header.chars().count() + 1)));
    for line in &body {
        let pad = inner_width.saturating_sub(line.chars().count() + 1);
        println!("│  {line}{}│", " ".repeat(pad));
    }
    println!("└{}┘", dashes(inner_width + 1));
}

fn render_edges(
    block: &BasicBlock,
    cfg: &ControlFlowGraph,
    symbols: &HashMap<u64, String>,
) {
    if block.successors.is_empty() {
        let trailer = match block.terminator {
            Some(ControlFlow::Return) => "└─▶ return",
            Some(ControlFlow::Trap) => "└─▶ trap (no successors recorded)",
            None => "└─▶ (falls off analyzed region)",
            _ => "└─▶ (no successors)",
        };
        println!("    {trailer}");
        return;
    }

    let mut sorted: Vec<_> = block.successors.iter().collect();
    sorted.sort_by_key(|edge| edge_kind_order(edge.kind));

    for edge in sorted {
        let kind = edge_kind_label(edge.kind);
        let mut target = match edge.target {
            EdgeTarget::Block(id) => {
                let dest = cfg.block(id);
                let arrow = if dest.start <= block.start {
                    "↑ back-edge"
                } else if dest.start == block.end {
                    "↓ next"
                } else {
                    "→"
                };
                format!("{arrow} block {} @ {:#x}", id.0, dest.start)
            }
            EdgeTarget::External(address) => match symbols.get(&address) {
                Some(name) => format!("→ external {name} ({:#x})", address),
                None => format!("→ external {:#x}", address),
            },
            EdgeTarget::Indirect => "→ indirect (runtime-computed)".to_string(),
        };
        if matches!(edge.kind, EdgeKind::Call) {
            target.push_str("  [returns to fallthrough]");
        }
        println!("    │  {kind:<11}  {target}");
    }
}

fn terminator_label(terminator: Option<ControlFlow>) -> &'static str {
    match terminator {
        None => "no terminator",
        Some(ControlFlow::Fall) => "fall",
        Some(ControlFlow::Jump { .. }) => "jump",
        Some(ControlFlow::ConditionalJump { .. }) => "cond jump",
        Some(ControlFlow::Call { .. }) => "call",
        Some(ControlFlow::Return) => "return",
        Some(ControlFlow::IndirectJump) => "indirect jump",
        Some(ControlFlow::IndirectCall { .. }) => "indirect call",
        Some(ControlFlow::Trap) => "trap",
    }
}

fn edge_kind_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Fallthrough => "fallthrough",
        EdgeKind::Jump => "jump",
        EdgeKind::BranchTaken => "taken",
        EdgeKind::Call => "call",
    }
}

/// Print order: taken before fallthrough so conditional branches read
/// "if condition: ... else: ...".
fn edge_kind_order(kind: EdgeKind) -> u8 {
    match kind {
        EdgeKind::Jump => 0,
        EdgeKind::Call => 1,
        EdgeKind::BranchTaken => 2,
        EdgeKind::Fallthrough => 3,
    }
}

fn dashes(n: usize) -> String {
    "─".repeat(n)
}

// ---- Tiny helpers -------------------------------------------------------

fn relocation_kind_label(kind: RelocationKind) -> String {
    match kind {
        RelocationKind::Branch26 => "Branch26".into(),
        RelocationKind::Branch19 => "Branch19".into(),
        RelocationKind::Branch14 => "Branch14".into(),
        RelocationKind::AdrpPage21 => "AdrpPage21".into(),
        RelocationKind::AddPageOffset12 => "AddPageOffset12".into(),
        RelocationKind::LoadStorePageOffset12 { access_width_bytes } => {
            format!("LoadStorePageOffset12({access_width_bytes}B)")
        }
        RelocationKind::Absolute => "Absolute".into(),
        RelocationKind::ArmCall => "ArmCall".into(),
        RelocationKind::ArmJump24 => "ArmJump24".into(),
        RelocationKind::ArmPc24 => "ArmPc24".into(),
        RelocationKind::ArmRelative => "ArmRelative".into(),
        RelocationKind::ArmGlobData => "ArmGlobData".into(),
        RelocationKind::ArmJumpSlot => "ArmJumpSlot".into(),
        RelocationKind::ArmAbs32 => "ArmAbs32".into(),
        RelocationKind::ArmMovwAbsNc => "ArmMovwAbsNc".into(),
        RelocationKind::ArmMovtAbs => "ArmMovtAbs".into(),
        RelocationKind::ThumbCall => "ThumbCall".into(),
        RelocationKind::ThumbJump24 => "ThumbJump24".into(),
        RelocationKind::ThumbJump19 => "ThumbJump19".into(),
        RelocationKind::ThumbMovwAbsNc => "ThumbMovwAbsNc".into(),
        RelocationKind::ThumbMovtAbs => "ThumbMovtAbs".into(),
        RelocationKind::X86Pc32 => "X86Pc32".into(),
        RelocationKind::X86Plt32 => "X86Plt32".into(),
        RelocationKind::X86GotPcRel => "X86GotPcRel".into(),
        RelocationKind::X86Abs32 => "X86Abs32".into(),
        RelocationKind::X86Abs64 => "X86Abs64".into(),
        RelocationKind::Other(code) => format!("Other(0x{code:x})"),
    }
}

fn short_symbol_kind(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Object => "object",
        SymbolKind::Section => "section",
        SymbolKind::File => "file",
        SymbolKind::Unknown => "unknown",
    }
}

fn short_binding(binding: &str) -> String {
    binding.to_lowercase()
}

#[allow(dead_code)]
fn section_kind_label(kind: SectionKind) -> &'static str {
    // Kept for convenience if the kind printing changes — currently the
    // `{:?}` form is short enough.
    match kind {
        SectionKind::Text => "text",
        SectionKind::Data => "data",
        SectionKind::Rodata => "rodata",
        SectionKind::Bss => "bss",
        SectionKind::Debug => "debug",
        SectionKind::Other => "other",
    }
}

/// If `bytes` is a fat Mach-O containing an arm64 slice, return that slice
/// plus a note describing the selection. Otherwise return `None` and the
/// caller proceeds with the original bytes.
fn select_macho_arm64_slice(bytes: &[u8]) -> Option<(&[u8], String)> {
    use object::macho::{CPU_TYPE_ARM64, FAT_MAGIC, FAT_MAGIC_64};
    use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};

    if bytes.len() < 4 {
        return None;
    }
    let magic = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

    if magic == FAT_MAGIC {
        let fat = MachOFatFile32::parse(bytes).ok()?;
        let arch = fat
            .arches()
            .iter()
            .find(|a| a.cputype() == CPU_TYPE_ARM64 as u32)?;
        let slice = arch.data(bytes).ok()?;
        let offset = arch.offset() as usize;
        return Some((
            slice,
            format!(
                "Note: fat Mach-O — selecting arm64 slice ({} bytes at offset {offset})",
                slice.len()
            ),
        ));
    }
    if magic == FAT_MAGIC_64 {
        let fat = MachOFatFile64::parse(bytes).ok()?;
        let arch = fat
            .arches()
            .iter()
            .find(|a| a.cputype() == CPU_TYPE_ARM64 as u32)?;
        let slice = arch.data(bytes).ok()?;
        let offset = arch.offset() as usize;
        return Some((
            slice,
            format!(
                "Note: fat Mach-O — selecting arm64 slice ({} bytes at offset {offset})",
                slice.len()
            ),
        ));
    }

    None
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
