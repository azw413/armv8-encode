use armv8_encode::container::Container;
use armv8_encode::container::dynamic_relocs::{collect_skip_spans, SkipMap};

fn main() {
    let path = std::env::args().nth(1).expect("path to .so");
    let bytes = std::fs::read(&path).expect("read");
    let container = Container::from_bytes(&bytes).expect("parse");
    let report = collect_skip_spans(&container, &bytes);
    println!("spans: {}", report.spans.len());
    println!("unhandled: {} (sample: {:?})",
        report.unhandled.len(),
        report.unhandled.first());
    let mut by_len: std::collections::HashMap<u32, usize> = Default::default();
    let mut min_off = u64::MAX; let mut max_off = 0u64; let mut total = 0u64;
    for s in &report.spans {
        *by_len.entry(s.len).or_default() += 1;
        min_off = min_off.min(s.file_offset);
        max_off = max_off.max(s.file_offset + s.len as u64);
        total += s.len as u64;
    }
    println!("byte coverage: {} bytes across [{:#x}, {:#x})", total, min_off, max_off);
    println!("by len: {:?}", by_len);
    if !report.spans.is_empty() {
        println!("first 5: {:?}", &report.spans[..report.spans.len().min(5)]);
    }
    let map = SkipMap::new(&report.spans);
    let chunks: Vec<_> = map.encrypt_chunks(0, bytes.len() as u64).take(3).collect();
    println!("first 3 encrypt chunks: {:?}", chunks);
}
