//! JSONL Parser for Claude Code message log
//!
//! Parses Claude Code's JSONL conversation log and formats for display

use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Maximum read size to prevent unbounded memory allocation (1MB)
const MAX_READ_SIZE: u64 = 1024 * 1024;

/// JSONL entry types from Claude Code
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeMessage {
    #[serde(rename = "user")]
    User {
        message: UserMessage,
    },
    #[serde(rename = "assistant")]
    Assistant {
        message: AssistantMessage,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        input: serde_json::Value,
        id: String,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: Option<bool>,
    },
    #[serde(rename = "system")]
    System {
        message: SystemMessage,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Option<Vec<ContentBlock>>,
    pub thinking: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    pub source: Option<SourceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    #[serde(rename = "type")]
    pub source_type: String,
    pub id: String,
}

/// Formatted output for display
#[derive(Debug, Clone)]
pub struct FormattedOutput {
    pub text: String,
    pub is_waiting: bool,
}

impl ClaudeMessage {
    /// Parse a JSONL line and format for display
    pub fn parse_line(line: &str) -> Option<FormattedOutput> {
        match serde_json::from_str::<ClaudeMessage>(line) {
            Ok(msg) => Some(msg.format()),
            Err(e) => {
                tracing::debug!("Failed to parse JSONL line: {}", e);
                None
            }
        }
    }

    /// Format message for terminal display
    pub fn format(&self) -> FormattedOutput {
        match self {
            ClaudeMessage::User { message } => {
                let text = message
                    .content
                    .iter()
                    .filter_map(|b| b.text.clone())
                    .collect::<Vec<_>>()
                    .join("");
                FormattedOutput {
                    text: format!("[You]: {}", text),
                    is_waiting: false,
                }
            }
            ClaudeMessage::Assistant { message } => {
                let text = message
                    .content
                    .as_ref()
                    .map(|c| {
                        c.iter()
                            .filter_map(|b| b.text.clone())
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .unwrap_or_default();
                FormattedOutput {
                    text,
                    is_waiting: false,
                }
            }
            ClaudeMessage::ToolUse { name, input, .. } => {
                let input_str = serde_json::to_string(input).unwrap_or_default();
                let truncated = truncate(&input_str, 200);
                let display = if input_str.chars().count() > 200 {
                    format!("{}...", truncated)
                } else {
                    truncated
                };
                FormattedOutput {
                    text: format!("[Tool: {}]: {}", name, display),
                    is_waiting: false,
                }
            }
            ClaudeMessage::ToolResult { content, is_error, .. } => {
                let truncated = truncate(content, 500);
                let display = if content.chars().count() > 500 {
                    format!("{}...", truncated)
                } else {
                    truncated
                };
                let prefix = if is_error.unwrap_or(false) {
                    "[Error]"
                } else {
                    "[Result]"
                };
                FormattedOutput {
                    text: format!("{}: {}", prefix, display),
                    is_waiting: false,
                }
            }
            ClaudeMessage::System { .. } => FormattedOutput {
                text: String::new(),
                is_waiting: false,
            },
        }
    }
}

/// Read new lines from JSONL file (since last position)
pub fn read_new_lines(path: &Path, last_pos: u64) -> std::io::Result<(Vec<String>, u64)> {
    let mut file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size <= last_pos {
        return Ok((vec![], last_pos));
    }

    file.seek(SeekFrom::Start(last_pos))?;

    let read_size = std::cmp::min(file_size - last_pos, MAX_READ_SIZE);
    if read_size == 0 {
        return Ok((vec![], last_pos));
    }

    let mut buffer = vec![0u8; read_size as usize];
    let bytes_read = file.read(&mut buffer)?;
    if bytes_read == 0 {
        return Ok((vec![], last_pos));
    }
    // Shrink buffer to actual bytes read in case file shrank between metadata and read
    buffer.truncate(bytes_read);

    let content = String::from_utf8_lossy(&buffer);
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    let new_pos = last_pos + bytes_read as u64;
    Ok((lines, new_pos))
}

/// Safely truncate a string to a maximum number of characters without panicking
/// on multi-byte UTF-8 characters.
fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user_message() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#;
        let output = ClaudeMessage::parse_line(line).unwrap();
        assert!(output.text.contains("[You]: Hello"));
    }

    #[test]
    fn test_parse_assistant_message() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi there"}]}}"#;
        let output = ClaudeMessage::parse_line(line).unwrap();
        assert!(output.text.contains("Hi there"));
    }

    #[test]
    fn test_parse_tool_use() {
        let line = r#"{"type":"tool_use","name":"Read","input":{"file_path":"/test.txt"},"id":"tool-1"}"#;
        let output = ClaudeMessage::parse_line(line).unwrap();
        assert!(output.text.contains("[Tool: Read]"));
    }

    #[test]
    fn test_system_message_not_displayed() {
        let line =
            r#"{"type":"system","message":{"role":"system","content":"Welcome"}}"#;
        let output = ClaudeMessage::parse_line(line).unwrap();
        assert!(output.text.is_empty());
    }
}
