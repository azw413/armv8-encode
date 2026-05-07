//! Deep inspection of an ELF file's surface area beyond what `dump`
//! shows. This is the "what does Stage 5 have to model" reference: it
//! prints program headers, `.dynamic` tags, dynsym, GNU version
//! sections, GNU hash header fields, build-ID note, `.eh_frame_hdr`
//! header, and `.interp`.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example elf_inspect -- path/to/binary
//! ```
//!
//! Works on ELF inputs only (any e_type / architecture). Mach-O / PE
//! inputs are rejected with a clear error.

use object::elf;
use object::read::elf::{
    Dyn, ElfFile64, NoteIterator, ProgramHeader, SectionHeader, Sym,
};
use object::read::{Object, ObjectSection};
use object::Endianness;
use std::env;
use std::fs;
use std::process::ExitCode;

const USAGE: &str = "usage: elf_inspect <FILE>

Print the deep ELF surface — program headers, .dynamic, .dynsym,
.gnu.version*, GNU hash, build-ID, .eh_frame_hdr, .interp — that the
mid-level Container model abstracts over. Used to scope the work for
ET_DYN/ET_EXEC writer support (Stage 5).

This tool only handles ELF inputs.";

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(p) if p == "-h" || p == "--help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(p) => p,
        None => {
            eprintln!("error: missing FILE argument\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("error: cannot read {path}: {err}");
            return ExitCode::from(1);
        }
    };

    let file = match ElfFile64::<Endianness>::parse(bytes.as_slice()) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("error: not an ELF64 file: {err}");
            return ExitCode::from(1);
        }
    };

    println!("# {path}\n");
    print_file_header(&file);
    print_program_headers(&file);
    print_section_headers(&file, bytes.as_slice());
    print_interp(&file);
    print_dynamic(&file, bytes.as_slice());
    print_dynsym(&file);
    print_gnu_versions(&file, bytes.as_slice());
    print_gnu_hash(&file, bytes.as_slice());
    print_build_id(&file);
    print_eh_frame_hdr(&file);

    ExitCode::SUCCESS
}

// ---- file header --------------------------------------------------------

fn print_file_header(file: &ElfFile64<Endianness>) {
    let endian = file.endian();
    let header = file.elf_header();

    println!("## File header");
    println!(
        "  e_ident      class={} data={} version={} os_abi={} abi_version={}",
        elf_class_name(header.e_ident.class),
        elf_data_name(header.e_ident.data),
        header.e_ident.version,
        elf_osabi_name(header.e_ident.os_abi),
        header.e_ident.abi_version,
    );
    println!(
        "  e_type       0x{:04x}  {}",
        header.e_type.get(endian),
        elf_type_name(header.e_type.get(endian)),
    );
    println!(
        "  e_machine    0x{:04x}  {}",
        header.e_machine.get(endian),
        elf_machine_name(header.e_machine.get(endian)),
    );
    println!("  e_entry      0x{:x}", header.e_entry.get(endian));
    println!("  e_flags      0x{:08x}", header.e_flags.get(endian));
    println!();
}

fn elf_class_name(class: u8) -> &'static str {
    match class {
        elf::ELFCLASSNONE => "ELFCLASSNONE",
        elf::ELFCLASS32 => "ELFCLASS32",
        elf::ELFCLASS64 => "ELFCLASS64",
        _ => "?",
    }
}

fn elf_data_name(data: u8) -> &'static str {
    match data {
        elf::ELFDATANONE => "ELFDATANONE",
        elf::ELFDATA2LSB => "ELFDATA2LSB",
        elf::ELFDATA2MSB => "ELFDATA2MSB",
        _ => "?",
    }
}

fn elf_osabi_name(abi: u8) -> &'static str {
    match abi {
        elf::ELFOSABI_NONE => "NONE",
        elf::ELFOSABI_LINUX => "LINUX",
        elf::ELFOSABI_FREEBSD => "FREEBSD",
        elf::ELFOSABI_SOLARIS => "SOLARIS",
        elf::ELFOSABI_NETBSD => "NETBSD",
        elf::ELFOSABI_HPUX => "HPUX",
        _ => "?",
    }
}

fn elf_type_name(t: u16) -> &'static str {
    match t {
        elf::ET_NONE => "ET_NONE",
        elf::ET_REL => "ET_REL",
        elf::ET_EXEC => "ET_EXEC",
        elf::ET_DYN => "ET_DYN",
        elf::ET_CORE => "ET_CORE",
        _ => "ET_?",
    }
}

fn elf_machine_name(m: u16) -> &'static str {
    match m {
        elf::EM_AARCH64 => "EM_AARCH64",
        elf::EM_X86_64 => "EM_X86_64",
        elf::EM_ARM => "EM_ARM",
        elf::EM_RISCV => "EM_RISCV",
        _ => "EM_?",
    }
}

// ---- program headers ----------------------------------------------------

fn print_program_headers(file: &ElfFile64<Endianness>) {
    let endian = file.endian();
    let phdrs = file.elf_program_headers();
    println!("## Program headers ({} total)", phdrs.len());
    if phdrs.is_empty() {
        println!("  (none — typical for ET_REL .o files)\n");
        return;
    }

    println!(
        "  {:<18} {:<6} {:>10} {:>16} {:>16} {:>10} {:>10} {:>10}",
        "type", "flags", "offset", "vaddr", "paddr", "filesz", "memsz", "align",
    );
    for phdr in phdrs {
        println!(
            "  {:<18} {:<6} {:>#10x} {:>#16x} {:>#16x} {:>#10x} {:>#10x} {:>#10x}",
            phdr_type_name(phdr.p_type(endian)),
            phdr_flags_string(phdr.p_flags(endian)),
            phdr.p_offset(endian),
            phdr.p_vaddr(endian),
            phdr.p_paddr(endian),
            phdr.p_filesz(endian),
            phdr.p_memsz(endian),
            phdr.p_align(endian),
        );
    }
    println!();
}

fn phdr_type_name(t: u32) -> String {
    let name = match t {
        elf::PT_NULL => "PT_NULL",
        elf::PT_LOAD => "PT_LOAD",
        elf::PT_DYNAMIC => "PT_DYNAMIC",
        elf::PT_INTERP => "PT_INTERP",
        elf::PT_NOTE => "PT_NOTE",
        elf::PT_SHLIB => "PT_SHLIB",
        elf::PT_PHDR => "PT_PHDR",
        elf::PT_TLS => "PT_TLS",
        elf::PT_GNU_EH_FRAME => "PT_GNU_EH_FRAME",
        elf::PT_GNU_STACK => "PT_GNU_STACK",
        elf::PT_GNU_RELRO => "PT_GNU_RELRO",
        elf::PT_GNU_PROPERTY => "PT_GNU_PROPERTY",
        _ => return format!("PT_?({:#x})", t),
    };
    name.into()
}

fn phdr_flags_string(flags: u32) -> String {
    let mut s = String::with_capacity(3);
    s.push(if flags & elf::PF_R != 0 { 'R' } else { '-' });
    s.push(if flags & elf::PF_W != 0 { 'W' } else { '-' });
    s.push(if flags & elf::PF_X != 0 { 'X' } else { '-' });
    s
}

// ---- section headers ----------------------------------------------------

fn print_section_headers(file: &ElfFile64<Endianness>, data: &[u8]) {
    let endian = file.endian();
    let sections = file.elf_section_table();
    let total = sections.len();
    println!("## Section headers ({total} total, including null at index 0)");

    println!(
        "  {:>3}  {:<22} {:<16} {:>10} {:>10} {:>10} {:>4} {:>4} {:>10} {:>6}",
        "idx", "name", "type", "addr", "offset", "size", "link", "info", "addralign", "entsz",
    );
    for (idx, header) in sections.enumerate() {
        let name = sections
            .section_name(endian, header)
            .ok()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or("?");
        println!(
            "  {:>3}  {:<22} {:<16} {:>#10x} {:>#10x} {:>#10x} {:>4} {:>4} {:>#10x} {:>#6x}",
            idx.0,
            truncate(name, 22),
            sh_type_name(header.sh_type(endian)),
            header.sh_addr(endian),
            header.sh_offset(endian),
            header.sh_size(endian),
            header.sh_link(endian),
            header.sh_info(endian),
            header.sh_addralign(endian),
            header.sh_entsize(endian),
        );
        // sh_flags is wide; print on a follow-up line only if non-zero
        // so the table stays readable.
        let flags = header.sh_flags(endian);
        if flags != 0 {
            println!(
                "        flags=0x{:x}  {}",
                flags,
                sh_flags_string(flags),
            );
        }
    }
    println!();

    let _ = data; // unused — sections.enumerate() doesn't need the byte slice
}

fn sh_type_name(t: u32) -> String {
    let name = match t {
        elf::SHT_NULL => "SHT_NULL",
        elf::SHT_PROGBITS => "SHT_PROGBITS",
        elf::SHT_SYMTAB => "SHT_SYMTAB",
        elf::SHT_STRTAB => "SHT_STRTAB",
        elf::SHT_RELA => "SHT_RELA",
        elf::SHT_HASH => "SHT_HASH",
        elf::SHT_DYNAMIC => "SHT_DYNAMIC",
        elf::SHT_NOTE => "SHT_NOTE",
        elf::SHT_NOBITS => "SHT_NOBITS",
        elf::SHT_REL => "SHT_REL",
        elf::SHT_DYNSYM => "SHT_DYNSYM",
        elf::SHT_INIT_ARRAY => "SHT_INIT_ARRAY",
        elf::SHT_FINI_ARRAY => "SHT_FINI_ARRAY",
        elf::SHT_PREINIT_ARRAY => "SHT_PREINIT_ARRAY",
        elf::SHT_GROUP => "SHT_GROUP",
        elf::SHT_SYMTAB_SHNDX => "SHT_SYMTAB_SHNDX",
        elf::SHT_GNU_HASH => "SHT_GNU_HASH",
        elf::SHT_GNU_VERDEF => "SHT_GNU_VERDEF",
        elf::SHT_GNU_VERNEED => "SHT_GNU_VERNEED",
        elf::SHT_GNU_VERSYM => "SHT_GNU_VERSYM",
        _ => return format!("SHT_?({:#x})", t),
    };
    name.into()
}

fn sh_flags_string(flags: u64) -> String {
    let mut parts: Vec<&'static str> = Vec::new();
    if flags & u64::from(elf::SHF_WRITE) != 0 {
        parts.push("W");
    }
    if flags & u64::from(elf::SHF_ALLOC) != 0 {
        parts.push("A");
    }
    if flags & u64::from(elf::SHF_EXECINSTR) != 0 {
        parts.push("X");
    }
    if flags & u64::from(elf::SHF_MERGE) != 0 {
        parts.push("MERGE");
    }
    if flags & u64::from(elf::SHF_STRINGS) != 0 {
        parts.push("STRINGS");
    }
    if flags & u64::from(elf::SHF_INFO_LINK) != 0 {
        parts.push("INFO_LINK");
    }
    if flags & u64::from(elf::SHF_LINK_ORDER) != 0 {
        parts.push("LINK_ORDER");
    }
    if flags & u64::from(elf::SHF_GROUP) != 0 {
        parts.push("GROUP");
    }
    if flags & u64::from(elf::SHF_TLS) != 0 {
        parts.push("TLS");
    }
    if flags & u64::from(elf::SHF_COMPRESSED) != 0 {
        parts.push("COMPRESSED");
    }
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join("|")
    }
}

// ---- .interp ------------------------------------------------------------

fn print_interp(file: &ElfFile64<Endianness>) {
    println!("## .interp (PT_INTERP / `.interp`)");
    let interp = file
        .section_by_name(".interp")
        .and_then(|s| s.data().ok())
        .map(|d| String::from_utf8_lossy(d.split(|&b| b == 0).next().unwrap_or(&[])).into_owned());
    match interp {
        Some(s) if !s.is_empty() => println!("  {s}"),
        _ => println!("  (none — typical for ET_REL .o, ET_DYN libraries without an interp)"),
    }
    println!();
}

// ---- .dynamic -----------------------------------------------------------

fn print_dynamic(file: &ElfFile64<Endianness>, data: &[u8]) {
    let endian = file.endian();
    let table = file.elf_section_table();

    let dyn_result = table.dynamic(endian, data);
    let entries = match dyn_result {
        Ok(Some((entries, _link))) => entries,
        Ok(None) => {
            println!("## .dynamic\n  (none — input is statically linked or ET_REL)\n");
            return;
        }
        Err(err) => {
            println!("## .dynamic\n  parse error: {err}\n");
            return;
        }
    };

    println!("## .dynamic ({} entries)", entries.len());
    for entry in entries {
        let tag = entry.d_tag(endian);
        let val = entry.d_val(endian);
        println!("  {:<22} 0x{:x}", dt_tag_name(tag), val);
    }
    println!();
}

fn dt_tag_name(tag: u64) -> String {
    let t = tag as u32;
    let name = match t {
        elf::DT_NULL => "DT_NULL",
        elf::DT_NEEDED => "DT_NEEDED",
        elf::DT_PLTRELSZ => "DT_PLTRELSZ",
        elf::DT_PLTGOT => "DT_PLTGOT",
        elf::DT_HASH => "DT_HASH",
        elf::DT_STRTAB => "DT_STRTAB",
        elf::DT_SYMTAB => "DT_SYMTAB",
        elf::DT_RELA => "DT_RELA",
        elf::DT_RELASZ => "DT_RELASZ",
        elf::DT_RELAENT => "DT_RELAENT",
        elf::DT_STRSZ => "DT_STRSZ",
        elf::DT_SYMENT => "DT_SYMENT",
        elf::DT_INIT => "DT_INIT",
        elf::DT_FINI => "DT_FINI",
        elf::DT_SONAME => "DT_SONAME",
        elf::DT_RPATH => "DT_RPATH",
        elf::DT_SYMBOLIC => "DT_SYMBOLIC",
        elf::DT_REL => "DT_REL",
        elf::DT_RELSZ => "DT_RELSZ",
        elf::DT_RELENT => "DT_RELENT",
        elf::DT_PLTREL => "DT_PLTREL",
        elf::DT_DEBUG => "DT_DEBUG",
        elf::DT_TEXTREL => "DT_TEXTREL",
        elf::DT_JMPREL => "DT_JMPREL",
        elf::DT_BIND_NOW => "DT_BIND_NOW",
        elf::DT_INIT_ARRAY => "DT_INIT_ARRAY",
        elf::DT_FINI_ARRAY => "DT_FINI_ARRAY",
        elf::DT_INIT_ARRAYSZ => "DT_INIT_ARRAYSZ",
        elf::DT_FINI_ARRAYSZ => "DT_FINI_ARRAYSZ",
        elf::DT_RUNPATH => "DT_RUNPATH",
        elf::DT_FLAGS => "DT_FLAGS",
        elf::DT_PREINIT_ARRAY => "DT_PREINIT_ARRAY",
        elf::DT_PREINIT_ARRAYSZ => "DT_PREINIT_ARRAYSZ",
        elf::DT_GNU_HASH => "DT_GNU_HASH",
        elf::DT_VERSYM => "DT_VERSYM",
        elf::DT_VERDEF => "DT_VERDEF",
        elf::DT_VERDEFNUM => "DT_VERDEFNUM",
        elf::DT_VERNEED => "DT_VERNEED",
        elf::DT_VERNEEDNUM => "DT_VERNEEDNUM",
        elf::DT_FLAGS_1 => "DT_FLAGS_1",
        elf::DT_RELACOUNT => "DT_RELACOUNT",
        elf::DT_RELCOUNT => "DT_RELCOUNT",
        _ => return format!("DT_?({:#x})", tag),
    };
    name.into()
}

// ---- .dynsym ------------------------------------------------------------

fn print_dynsym(file: &ElfFile64<Endianness>) {
    let endian = file.endian();
    let dynsym = file.elf_dynamic_symbol_table();
    let count = dynsym.len();
    println!("## .dynsym ({count} entries, including null)");
    if count == 0 {
        println!("  (none — input has no dynamic symbol table)\n");
        return;
    }

    let strings = dynsym.strings();
    println!(
        "  {:>4}  {:>16} {:>10}  bind/type/vis            shndx  name",
        "idx", "value", "size",
    );
    for (idx, sym) in dynsym.symbols().iter().enumerate() {
        if idx == 0 {
            continue; // null symbol
        }
        let name = sym
            .name(endian, strings)
            .ok()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or("?");
        println!(
            "  {:>4}  {:>#16x} {:>#10x}  {:<8} {:<8} {:<8} {:>5}  {}",
            idx,
            sym.st_value(endian),
            sym.st_size(endian),
            st_bind_name(sym.st_bind()),
            st_type_name(sym.st_type()),
            st_visibility_name(sym.st_visibility()),
            sym.st_shndx(endian),
            truncate(name, 60),
        );
    }
    println!();
}

fn st_bind_name(bind: u8) -> &'static str {
    match bind {
        elf::STB_LOCAL => "LOCAL",
        elf::STB_GLOBAL => "GLOBAL",
        elf::STB_WEAK => "WEAK",
        _ => "?",
    }
}

fn st_type_name(t: u8) -> &'static str {
    match t {
        elf::STT_NOTYPE => "NOTYPE",
        elf::STT_OBJECT => "OBJECT",
        elf::STT_FUNC => "FUNC",
        elf::STT_SECTION => "SECTION",
        elf::STT_FILE => "FILE",
        elf::STT_COMMON => "COMMON",
        elf::STT_TLS => "TLS",
        elf::STT_GNU_IFUNC => "GNU_IFUNC",
        _ => "?",
    }
}

fn st_visibility_name(v: u8) -> &'static str {
    match v {
        elf::STV_DEFAULT => "DEFAULT",
        elf::STV_INTERNAL => "INTERNAL",
        elf::STV_HIDDEN => "HIDDEN",
        elf::STV_PROTECTED => "PROTECTED",
        _ => "?",
    }
}

// ---- .gnu.version* ------------------------------------------------------

fn print_gnu_versions(file: &ElfFile64<Endianness>, data: &[u8]) {
    let endian = file.endian();
    let table = file.elf_section_table();

    println!("## GNU symbol versioning");

    // Verdef: versions defined by this object (typical of .so files).
    match table.gnu_verdef(endian, data) {
        Ok(Some((mut iter, _link))) => {
            println!("  ### .gnu.version_d (defined versions):");
            while let Ok(Some((verdef, mut aux_iter))) = iter.next() {
                println!(
                    "    vd_ndx={:>4} vd_flags=0x{:x} vd_hash=0x{:x}",
                    verdef.vd_ndx.get(endian),
                    verdef.vd_flags.get(endian),
                    verdef.vd_hash.get(endian),
                );
                while let Ok(Some(aux)) = aux_iter.next() {
                    // Resolving aux name requires the verdef's link
                    // section's string table; we skip name resolution
                    // here to keep the example code small. The hash
                    // and indices are enough to confirm the section
                    // round-trips structurally.
                    println!(
                        "      verdaux vda_name=0x{:x} vda_next=0x{:x}",
                        aux.vda_name.get(endian),
                        aux.vda_next.get(endian),
                    );
                }
            }
        }
        Ok(None) => println!("  ### .gnu.version_d: (none)"),
        Err(err) => println!("  ### .gnu.version_d: parse error: {err}"),
    }

    // Verneed: versions this object requires (typical of executables
    // and libraries linked against versioned .sos like libc).
    match table.gnu_verneed(endian, data) {
        Ok(Some((mut iter, _link))) => {
            println!("  ### .gnu.version_r (required versions):");
            while let Ok(Some((verneed, mut aux_iter))) = iter.next() {
                println!(
                    "    vn_version={} vn_cnt={} vn_file=0x{:x}",
                    verneed.vn_version.get(endian),
                    verneed.vn_cnt.get(endian),
                    verneed.vn_file.get(endian),
                );
                while let Ok(Some(aux)) = aux_iter.next() {
                    println!(
                        "      vernaux vna_hash=0x{:x} vna_flags=0x{:x} \
                         vna_other={} vna_name=0x{:x}",
                        aux.vna_hash.get(endian),
                        aux.vna_flags.get(endian),
                        aux.vna_other.get(endian),
                        aux.vna_name.get(endian),
                    );
                }
            }
        }
        Ok(None) => println!("  ### .gnu.version_r: (none)"),
        Err(err) => println!("  ### .gnu.version_r: parse error: {err}"),
    }

    // Versym: parallel array indexed by dynsym index. We dump the count
    // and the first few entries; full per-symbol output is too noisy
    // for this overview tool.
    match table.gnu_versym(endian, data) {
        Ok(Some((versym, _link))) => {
            println!(
                "  ### .gnu.version (versym): {} entries (one per dynsym)",
                versym.len(),
            );
        }
        Ok(None) => println!("  ### .gnu.version: (none)"),
        Err(err) => println!("  ### .gnu.version: parse error: {err}"),
    }
    println!();
}

// ---- GNU hash -----------------------------------------------------------

fn print_gnu_hash(file: &ElfFile64<Endianness>, data: &[u8]) {
    let endian = file.endian();
    let table = file.elf_section_table();
    println!("## .gnu.hash");
    match table.gnu_hash(endian, data) {
        Ok(Some((hash, _link))) => {
            println!("  symbol_base = {}", hash.symbol_base());
            if let Some(len) = hash.symbol_table_length(endian) {
                println!("  derived dynsym count from hash = {len}");
            } else {
                println!("  (could not derive dynsym count from hash)");
            }
        }
        Ok(None) => println!("  (none — input lacks a .gnu.hash section)"),
        Err(err) => println!("  parse error: {err}"),
    }

    // Plain SysV hash (.hash) — older ELFs use this.
    match table.hash(endian, data) {
        Ok(Some((_hash, _link))) => println!("  (plain SHT_HASH section also present)"),
        Ok(None) => {}
        Err(err) => println!("  SHT_HASH parse error: {err}"),
    }
    println!();
}

// ---- build-ID -----------------------------------------------------------

fn print_build_id(file: &ElfFile64<Endianness>) {
    println!("## Build-ID");
    match file.build_id() {
        Ok(Some(id)) => {
            print!("  ");
            for byte in id {
                print!("{byte:02x}");
            }
            println!();
        }
        Ok(None) => println!("  (none — no .note.gnu.build-id present)"),
        Err(err) => println!("  parse error: {err}"),
    }

    // Iterate any other notes for completeness — useful for spotting
    // GNU property notes (BTI/PAC markers) that Stage 5 may need to
    // preserve.
    let endian = file.endian();
    let table = file.elf_section_table();
    let mut other_notes = Vec::new();
    for (_idx, header) in table.enumerate() {
        if header.sh_type(endian) != elf::SHT_NOTE {
            continue;
        }
        let Ok(data) = header.data(endian, file.data()) else { continue };
        let Ok(mut iter) = NoteIterator::<elf::FileHeader64<Endianness>>::new(
            endian,
            header.sh_addralign(endian),
            data,
        ) else {
            continue;
        };
        while let Ok(Some(note)) = iter.next() {
            let n_type = note.n_type(endian);
            if note.name() == elf::ELF_NOTE_GNU && n_type == elf::NT_GNU_BUILD_ID {
                continue; // already shown above
            }
            other_notes.push(format!(
                "  note name={:?} type=0x{:x} size={}",
                String::from_utf8_lossy(note.name()),
                n_type,
                note.desc().len(),
            ));
        }
    }
    if !other_notes.is_empty() {
        println!("  Other notes:");
        for line in other_notes {
            println!("  {line}");
        }
    }
    println!();
}

// ---- .eh_frame_hdr ------------------------------------------------------

fn print_eh_frame_hdr(file: &ElfFile64<Endianness>) {
    println!("## .eh_frame_hdr");
    let Some(section) = file.section_by_name(".eh_frame_hdr") else {
        println!("  (none — input has no .eh_frame_hdr; ET_REL outputs from the ");
        println!("   compiler frequently lack one)\n");
        return;
    };
    let Ok(data) = section.data() else {
        println!("  (cannot read .eh_frame_hdr bytes)\n");
        return;
    };
    if data.len() < 4 {
        println!("  malformed: section is {} bytes (header is 4 bytes)\n", data.len());
        return;
    }

    let version = data[0];
    let eh_frame_ptr_enc = data[1];
    let fde_count_enc = data[2];
    let table_enc = data[3];
    println!(
        "  version={version} eh_frame_ptr_enc=0x{:02x} fde_count_enc=0x{:02x} table_enc=0x{:02x}",
        eh_frame_ptr_enc, fde_count_enc, table_enc,
    );
    println!("  body length = {} bytes", data.len() - 4);
    println!(
        "  (full FDE-table parsing requires DW_EH_PE_* encoding decode — \
         out of scope for this overview)",
    );
    println!();
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut t = s[..max_len.saturating_sub(1)].to_string();
        t.push('…');
        t
    }
}
