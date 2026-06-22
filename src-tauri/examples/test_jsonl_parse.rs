//! Test JSONL parsing - 使用 shared/utils/jsonl_parser 工具类

use bedcode_lib::shared::utils::jsonl_parser::ClaudeEntry;

fn main() {
    let jsonl_path = r"C:\Users\binblink\.claude\projects\D--tauriProject-BedCode\0a771cd3-ac60-4638-9a60-6ca997349136.jsonl";

    let content = match std::fs::read_to_string(jsonl_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read JSONL file: {}", e);
            return;
        }
    };

    println!("========================================");
    println!(" Claude Code Conversation - Terminal View");
    println!("========================================\n");

    let entries = bedcode_lib::shared::utils::jsonl_parser::parse_jsonl(&content);

    for entry in entries.iter().take(100) {
        let output = entry.to_formatted_output();
        if output.text.is_empty() {
            continue;
        }
        println!("[{}] {}", output.entry_type, output.text);
    }

    println!("\n========================================");
    println!(" Total entries: {}", entries.len());
    println!("========================================");
}
