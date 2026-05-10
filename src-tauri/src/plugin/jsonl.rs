//! JSONL Parser for Claude Code message log
//!
//! Parses Claude Code's JSONL conversation log and formats for display

use serde::{Deserialize, Serialize};
use std::path::Path;

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
        let msg: ClaudeMessage = serde_json::from_str(line).ok()?;
        Some(msg.format())
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
                let truncated = if input_str.len() > 200 {
                    format!("{}...", &input_str[..200])
                } else {
                    input_str
                };
                FormattedOutput {
                    text: format!("[Tool: {}]: {}", name, truncated),
                    is_waiting: false,
                }
            }
            ClaudeMessage::ToolResult { content, is_error, .. } => {
                let truncated = if content.len() > 500 {
                    format!("{}...", &content[..500])
                } else {
                    content.clone()
                };
                let prefix = if is_error.unwrap_or(false) {
                    "[Error]"
                } else {
                    "[Result]"
                };
                FormattedOutput {
                    text: format!("{}: {}", prefix, truncated),
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
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    if file_size <= last_pos {
        return Ok((vec![], last_pos));
    }

    file.seek(SeekFrom::Start(last_pos))?;
    let mut buffer = vec![0; (file_size - last_pos) as usize];
    file.read_exact(&mut buffer)?;

    let content = String::from_utf8_lossy(&buffer);
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    Ok((lines, file_size))
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
