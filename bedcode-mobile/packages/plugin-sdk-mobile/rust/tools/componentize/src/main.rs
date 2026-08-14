//! 插件组件化工具（迁移阶段 B 构建链）
//!
//! 将 wit-bindgen 产出的 core module（含 `component-type` 自定义段）编码为
//! Component Model 组件，等价于 `wasm-tools component new`：
//! - 产物已是组件（魔法字节 `0d 00 01 00`）时直接复制（幂等，支持增量构建）
//! - 编码失败的输入（无组件元数据）给出明确错误
//!
//! 用法：`componentize <input.wasm> -o <output.wasm>`
//! 插件构建脚本（plugins/*/scripts/build.js）在 cargo build 后调用本工具。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 < args.len() {
                    output = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    eprintln!("Missing value after {}", args[i]);
                    return ExitCode::FAILURE;
                }
            }
            a if a.starts_with('-') => {
                eprintln!("Unknown option: {}", a);
                return ExitCode::FAILURE;
            }
            a => input = Some(a.to_string()),
        }
        i += 1;
    }

    let (Some(input), Some(output)) = (input, output) else {
        eprintln!("Usage: componentize <input.wasm> -o <output.wasm>");
        return ExitCode::FAILURE;
    };

    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read {}: {}", input, e);
            return ExitCode::FAILURE;
        }
    };

    // 幂等：产物已是组件（版本字 0d 00 01 00）则直接复制
    if bytes.len() >= 8 && &bytes[0..4] == b"\0asm" && &bytes[4..8] == [0x0d, 0x00, 0x01, 0x00] {
        return match std::fs::write(&output, &bytes) {
            Ok(()) => {
                println!("componentize: {} already a component, copied to {}", input, output);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Failed to write {}: {}", output, e);
                ExitCode::FAILURE
            }
        };
    }

    let mut encoder = match wit_component::ComponentEncoder::default().module(&bytes) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "componentize: {} is not a componentizable core module (missing WIT metadata): {}",
                input, e
            );
            return ExitCode::FAILURE;
        }
    };
    let encoded = match encoder.encode() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("componentize: encode {} failed: {}", input, e);
            return ExitCode::FAILURE;
        }
    };

    match std::fs::write(&output, &encoded) {
        Ok(()) => {
            println!("componentize: {} -> {} ({} bytes)", input, output, encoded.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Failed to write {}: {}", output, e);
            ExitCode::FAILURE
        }
    }
}
