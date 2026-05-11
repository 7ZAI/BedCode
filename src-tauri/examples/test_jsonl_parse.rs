//! Test JSONL parsing - pure terminal style output
//! Shows raw terminal-like output similar to real terminal

use bedcode_lib::plugin::jsonl::{ClaudeEntry, OutputType};

fn main() {
    let jsonl_path = r"C:\Users\binblink\.claude\projects\D--tauriProject-BedCode\0a771cd3-ac60-4638-9a60-6ca997349136.jsonl";

    let content = std::fs::read_to_string(jsonl_path).expect("Failed to read JSONL file");
    let lines: Vec<&str> = content.lines().collect();

    println!("========================================");
    println!(" Claude Code Conversation - Terminal View");
    println!("========================================\n");

    let mut output_lines: Vec<String> = Vec::new();
    let mut in_thinking = false;
    let mut in_tool_use = false;

    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }

        if let Some(output) = ClaudeEntry::parse_line(line) {
            let terminal_text = output.to_terminal_string();

            if terminal_text.is_empty() {
                continue;
            }

            match output.output_type {
                OutputType::Thinking => {
                    if !in_thinking {
                        output_lines.push(String::new());
                        in_thinking = true;
                    }
                    output_lines.push(terminal_text);
                }
                OutputType::ToolUse => {
                    if in_thinking {
                        in_thinking = false;
                    }
                    if !in_tool_use {
                        output_lines.push(String::new());
                        in_tool_use = true;
                    }
                    output_lines.push(terminal_text);
                }
                OutputType::ToolResult => {
                    in_tool_use = false;
                    output_lines.push(terminal_text);
                    output_lines.push(String::new());
                }
                _ => {
                    if in_thinking {
                        in_thinking = false;
                        output_lines.push(String::new());
                    }
                    if in_tool_use {
                        in_tool_use = false;
                    }
                    output_lines.push(terminal_text);
                    if matches!(output.output_type, OutputType::User) {
                        output_lines.push(String::new());
                    }
                }
            }
        }
    }

    // Print all output lines
    for line in output_lines.iter().take(100) {
        println!("{}", line);
    }

    println!("\n========================================");
    println!(" End of conversation");
    println!("========================================");
}