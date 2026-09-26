//! 小工具：打印组件 / 核心模块的 WAT（取证用，替代未安装的 wasm-tools CLI）
//!
//! 用法：`cargo run --bin wat -- <file.wasm> [关键词过滤]`

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: wat <file.wasm> [filter]");
    let filter = args.next();
    let bytes = std::fs::read(&path).expect("read wasm");
    let text = wasmprinter::print_bytes(&bytes).expect("print wasm");
    match filter {
        None => println!("{text}"),
        Some(f) => {
            for (idx, line) in text.lines().enumerate() {
                if line.contains(&f) {
                    println!("{:>6}: {}", idx + 1, line);
                }
            }
        }
    }
}
