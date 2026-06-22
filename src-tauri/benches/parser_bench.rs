//! Benchmarks for parser modules

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, black_box, Throughput};
use bedcode_lib::desktop::parser::{AnsiParser, MarkdownParser, OutputParser};

fn ansi_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("ansi_parser");

    // Simple text without ANSI codes
    let simple_text = "Hello, World! This is a test output line.\n".repeat(100);
    group.throughput(Throughput::Bytes(simple_text.len() as u64));
    group.bench_with_input(BenchmarkId::new("strip", "simple"), &simple_text, |b, text| {
        let parser = AnsiParser::new();
        b.iter(|| parser.strip_ansi(black_box(text)));
    });

    // Text with ANSI color codes
    let colored_text = "\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[34mBlue\x1b[0m\n".repeat(100);
    group.throughput(Throughput::Bytes(colored_text.len() as u64));
    group.bench_with_input(BenchmarkId::new("strip", "colored"), &colored_text, |b, text| {
        let parser = AnsiParser::new();
        b.iter(|| parser.strip_ansi(black_box(text)));
    });

    // Parse with style extraction
    group.bench_with_input(BenchmarkId::new("parse", "colored"), &colored_text, |b, text| {
        let mut parser = AnsiParser::new();
        b.iter(|| parser.parse(black_box(text)));
    });

    // Complex ANSI sequences
    let complex_text = "\x1b[1;3;4;31;42mStyled\x1b[0m normal \x1b[2;37mDim\x1b[0m\n".repeat(100);
    group.throughput(Throughput::Bytes(complex_text.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "complex"), &complex_text, |b, text| {
        let mut parser = AnsiParser::new();
        b.iter(|| parser.parse(black_box(text)));
    });

    group.finish();
}

fn markdown_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_parser");

    // Plain text
    let plain_text = "This is a paragraph of text.\n\nAnother paragraph.\n".repeat(50);
    group.throughput(Throughput::Bytes(plain_text.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "plain"), &plain_text, |b, text| {
        let parser = MarkdownParser::new();
        b.iter(|| parser.parse(black_box(text)));
    });

    // Markdown with headings
    let markdown_headings = "# Heading 1\n## Heading 2\n### Heading 3\n\nContent here.\n".repeat(20);
    group.throughput(Throughput::Bytes(markdown_headings.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "headings"), &markdown_headings, |b, text| {
        let parser = MarkdownParser::new();
        b.iter(|| parser.parse(black_box(text)));
    });

    // Markdown with code blocks
    let code_block = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\n".to_string();
    let markdown_code = code_block.repeat(20);
    group.throughput(Throughput::Bytes(markdown_code.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "code_blocks"), &markdown_code, |b, text| {
        let parser = MarkdownParser::new();
        b.iter(|| parser.parse(black_box(text)));
    });

    // Mixed markdown
    let mixed = r#"# Title

This is **bold** and *italic* text.

## Code Example

```javascript
console.log("test");
```

- List item 1
- List item 2

> A quote

---
"#.repeat(10);
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "mixed"), &mixed, |b, text| {
        let parser = MarkdownParser::new();
        b.iter(|| parser.parse(black_box(text)));
    });

    // Code block extraction
    group.bench_with_input(BenchmarkId::new("extract_code", "code_blocks"), &markdown_code, |b, text| {
        b.iter(|| MarkdownParser::extract_code_blocks(black_box(text)));
    });

    group.finish();
}

fn output_parser_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("output_parser");

    // Waiting input detection
    let waiting_text = "Processing...\n> ";
    group.bench_function("detect_waiting", |b| {
        let parser = OutputParser::new();
        b.iter(|| parser.detect_waiting_input(black_box(waiting_text)));
    });

    // Large output with ANSI
    let large_output = "\x1b[32mSuccess\x1b[0m: Processing file 1\n".repeat(1000);
    group.throughput(Throughput::Bytes(large_output.len() as u64));
    group.bench_with_input(BenchmarkId::new("parse", "large"), &large_output, |b, text| {
        let mut parser = OutputParser::new();
        b.iter(|| parser.parse(black_box(text)));
    });

    // Clean output
    group.bench_with_input(BenchmarkId::new("clean", "large"), &large_output, |b, text| {
        let parser = OutputParser::new();
        b.iter(|| parser.clean_output(black_box(text)));
    });

    group.finish();
}

fn combined_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined");

    // Simulate typical terminal output
    let terminal_output = r#"\x1b[1;36m→\x1b[0m \x1b[1mClaude Code\x1b[0m
Analyzing project structure...

\x1b[32m✓\x1b[0m Found 15 TypeScript files
\x1b[32m✓\x1b[0m Found 3 configuration files

```typescript
interface Config {
  name: string;
  version: string;
}
```

\x1b[33m!\x1b[0m Warning: Large files detected

> "#.replace("\\x1b", "\x1b");

    let repeated_output = terminal_output.repeat(100);
    group.throughput(Throughput::Bytes(repeated_output.len() as u64));

    group.bench_function("full_parse", |b| {
        let mut parser = OutputParser::new();
        b.iter(|| {
            let output = parser.parse(black_box(&repeated_output));
            let waiting = parser.detect_waiting_input(black_box(&repeated_output));
            black_box((output, waiting));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    ansi_benchmark,
    markdown_benchmark,
    output_parser_benchmark,
    combined_benchmark,
);

criterion_main!(benches);
