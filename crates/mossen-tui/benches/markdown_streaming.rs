//! Benchmark: streaming markdown re-parse performance.
//!
//! Measures the cost of re-parsing the entire accumulated buffer on each
//! token delta, simulating a 10 KB final response delivered in 200 chunks.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mossen_tui::widgets::markdown::MarkdownWidget;

fn bench_streaming_reparse(c: &mut Criterion) {
    // Simulate 200 token deltas, each ~50 bytes, final ~10 KB
    let chunks: Vec<String> = (0..200)
        .map(|i| {
            format!(
                " word{} more text to parse and render for streaming markdown\n",
                i
            )
        })
        .collect();

    c.bench_function("markdown_reparse_per_chunk_10kb_final", |b| {
        b.iter(|| {
            let mut acc = String::new();
            for chunk in &chunks {
                acc.push_str(chunk);
                let widget = MarkdownWidget::new(black_box(&acc));
                let _lines = widget.parse_to_lines();
            }
        });
    });
}

criterion_group!(benches, bench_streaming_reparse);
criterion_main!(benches);
