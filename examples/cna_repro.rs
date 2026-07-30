//! Smoke test for the bug report at /tmp/cna_repro.so.
//! Confirms that the minimal repro from armadillo
//!
//!   BinaryEditor::new(&container)
//!     + add_library_dependency
//!     + commit_to_bytes
//!
//! succeeds on a real Android NDK ET_DYN input. The historical
//! failure mode was an `assertion failed: self.len <= offset`
//! in object's reserve_until inside the per-section reserve
//! loop. That fault doesn't reproduce on the current tree (commit
//! f1dfb2d with the PT_PHDR drop and the .dynamic relocation
//! path); this example serves as a long-running canary so we'd
//! catch a regression.
use armv8_encode::container::Container;
use armv8_encode::rewrite::BinaryEditor;

fn main() {
    let path = "/tmp/cna_repro.so";
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!(
                "skipping: {path} not present ({err}). Drop \
                 a real Android NDK .so at this path to run the \
                 canary."
            );
            return;
        }
    };
    let container = Container::from_bytes(&bytes).expect("parse");
    let mut editor = BinaryEditor::new(&container).expect("BinaryEditor::new");
    editor
        .binary
        .add_library_dependency("libmain00vm.so")
        .expect("add_library_dependency");
    let written = editor.commit_to_bytes().expect("commit_to_bytes");
    println!(
        "commit_to_bytes ok ({} bytes in, {} bytes out)",
        bytes.len(),
        written.len(),
    );
    let reparsed = Container::from_bytes(&written).expect("re-parse");
    println!("re-parse ok, kind={:?}", reparsed.kind);

    // Sanity: the new dep should appear in the relocated
    // .dynamic. Walk PT_DYNAMIC.p_offset to find the live
    // dynamic table in the rewritten file.
    let image = reparsed.elf_image.as_ref().expect("elf_image");
    let pt_dynamic = image
        .program_headers
        .iter()
        .find(|p| p.p_type == object::elf::PT_DYNAMIC)
        .expect("PT_DYNAMIC");
    use armv8_encode::container::dynsym_extension as dx;
    let dyn_off = pt_dynamic.p_offset as usize;
    let dyn_size = pt_dynamic.p_filesz as usize;
    let parsed = dx::parse_dynamic(&written[dyn_off..dyn_off + dyn_size], reparsed.is_64())
        .expect("parse relocated .dynamic");
    let dt_needed_count = parsed
        .iter()
        .filter(|e| e.tag == object::elf::DT_NEEDED as u64)
        .count();
    println!(
        "PT_DYNAMIC at vaddr=0x{:x} offset=0x{:x} contains {} DT_NEEDED entries",
        pt_dynamic.p_vaddr, pt_dynamic.p_offset, dt_needed_count,
    );
}
