//! JSONL Parser for Claude Code message log
//!
//! Parses Claude Code's JSONL conversation log and formats for terminal display

use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use tracing::{debug, warn};

/// Maximum read size to prevent unbounded memory allocation (1MB)
const MAX_READ_SIZE: u64 = 1024 * 1024;

/// Maximum text display limits
const MAX_TOOL_INPUT_DISPLAY: usize = 200;
const MAX_TOOL_RESULT_DISPLAY: usize = 500;
const MAX_THINKING_DISPLAY: usize = 300;

/// Root entry types from Claude Code JSONL
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEntry {
    #[serde(rename = "user")]
    User {
        message: MessageContent,
        #[serde(default, alias = "isMeta")]
        is_meta: Option<bool>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        /// Assistant message can have various fields, use Value for flexibility
        #[serde(flatten)]
        message: serde_json::Value,
    },
    #[serde(rename = "attachment")]
    Attachment {
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot {
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "meta")]
    Meta {
        is_meta: bool,
    },
    #[serde(rename = "task_reminder")]
    TaskReminder {
        #[serde(default)]
        message: Option<String>,
    },
    #[serde(rename = "create")]
    Create {},
    #[serde(rename = "command_permissions")]
    CommandPermissions {},
    #[serde(rename = "skill_listing")]
    SkillListing {},
    #[serde(rename = "mcp_instructions_delta")]
    McpInstructions {},
    #[serde(rename = "hook_success")]
    HookSuccess {
        #[serde(default)]
        hook_name: Option<String>,
    },
    #[serde(rename = "hook_additional_context")]
    HookAdditionalContext {},
    /// Catch-all for unknown types
    #[serde(other)]
    Unknown,
}

/// Message content from user/assistant entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    pub role: String,
    /// Content can be string (simple) or array of content blocks (complex)
    /// Using serde_json::Value to handle both cases flexibly
    #[serde(default)]
    pub content: serde_json::Value,
}

impl MessageContent {
    /// Extract text from content (handles both string and array formats)
    pub fn extract_text(&self) -> String {
        match &self.content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        }
    }

    /// Extract content blocks from content (for assistant messages)
    pub fn as_blocks(&self) -> Vec<ContentBlock> {
        match &self.content {
            serde_json::Value::Array(arr) => {
                arr.iter()
                    .filter_map(|v| serde_json::from_str::<ContentBlock>(&v.to_string()).ok())
                    .collect()
            }
            _ => vec![],
        }
    }
}

/// Content block within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type", alias = "type")]
    pub block_type: String,
    /// Text content
    #[serde(default)]
    pub text: Option<String>,
    /// Tool use name (for tool_use type)
    #[serde(default)]
    pub name: Option<String>,
    /// Tool use input (for tool_use type)
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    /// Tool use ID (for tool_use type)
    #[serde(default)]
    pub id: Option<String>,
    /// Tool result content (for tool_result type)
    #[serde(default)]
    pub content: Option<String>,
    /// Tool use ID reference (for tool_result type)
    #[serde(default)]
    pub tool_use_id: Option<String>,
    /// Is error (for tool_result type)
    #[serde(default)]
    pub is_error: Option<bool>,
    /// Thinking content (for thinking type)
    #[serde(default)]
    pub thinking: Option<String>,
    /// Source info (for text type with source)
    #[serde(default)]
    pub source: Option<SourceInfo>,
    /// Signature (for thinking type)
    #[serde(default)]
    pub signature: Option<String>,
}

/// Source information for tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    #[serde(rename = "type")]
    pub source_type: String,
    pub id: String,
}

/// Attachment info (for hook outputs, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInfo {
    #[serde(rename = "type")]
    pub attachment_type: String,
    #[serde(default)]
    pub hook_name: Option<String>,
    #[serde(default)]
    pub hook_event: Option<String>,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// Formatted output for terminal display
#[derive(Debug, Clone)]
pub struct FormattedOutput {
    /// Display text
    pub text: String,
    /// Whether waiting for user input
    pub is_waiting: bool,
    /// Message type for styling
    pub output_type: OutputType,
}

impl FormattedOutput {
    /// Remove ANSI escape sequences for clean terminal output
    fn strip_ansi(text: &str) -> String {
        // Remove common ANSI escape sequences
        text.replace("\u{1b}[1m", "")  // Bold
            .replace("\u{1b}[22m", "") // Normal
            .replace("\u{1b}[31m", "") // Red
            .replace("\u{1b}[32m", "") // Green
            .replace("\u{1b}[33m", "") // Yellow
            .replace("\u{1b}[34m", "") // Blue
            .replace("\u{1b}[35m", "") // Magenta
            .replace("\u{1b}[36m", "") // Cyan
            .replace("\u{1b}[37m", "") // White
            .replace("\u{1b}[0m", "")  // Reset
            // Also handle bracket-style escapes like [1m, [22m
            .replace("[1m", "")
            .replace("[22m", "")
            .replace("[31m", "")
            .replace("[32m", "")
            .replace("[33m", "")
            .replace("[34m", "")
            .replace("[35m", "")
            .replace("[36m", "")
            .replace("[37m", "")
            .replace("[0m", "")
            // Handle xterm color format
            .replace("\u{1b}[38;5;", "")
            .replace("\u{1b}[48;5;", "")
    }

    /// Convert to pure terminal-style output string
    /// Like a real terminal: no emojis, simple prompts, clean formatting
    pub fn to_terminal_string(&self) -> String {
        // First strip ANSI sequences
        let clean_text = Self::strip_ansi(&self.text);

        match self.output_type {
            OutputType::User => {
                // User input: show with $ prompt
                if clean_text.starts_with("> ") {
                    format!("$ {}", clean_text.strip_prefix("> ").unwrap_or(&clean_text))
                } else {
                    format!("$ {}", clean_text)
                }
            }
            OutputType::Assistant => {
                // Assistant: plain text output
                clean_text
            }
            OutputType::Thinking => {
                // Thinking: show as comment (skip long content in terminal view)
                if clean_text.contains("[Thinking...]") {
                    let content = clean_text
                        .strip_prefix("[Thinking...]\n")
                        .unwrap_or(&clean_text);
                    // Only show first line
                    format!("# {}", content.lines().next().unwrap_or("..."))
                } else {
                    format!("# {}", clean_text.lines().next().unwrap_or(""))
                }
            }
            OutputType::ToolUse => {
                // Tool use: show as $ tool_name args
                if clean_text.starts_with("[Tool: ") {
                    let rest = clean_text.strip_prefix("[Tool: ").unwrap_or(&clean_text);
                    if let Some(end) = rest.find(']') {
                        let tool_name = &rest[..end];
                        let args = rest[end+1..].trim();
                        // Truncate long JSON args
                        let display_args = if args.len() > 80 {
                            format!("{}...", &args[..80])
                        } else {
                            args.to_string()
                        };
                        format!("$ {} {}", tool_name, display_args)
                    } else {
                        clean_text
                    }
                } else {
                    clean_text
                }
            }
            OutputType::ToolResult => {
                // Tool result: show as output
                let text = clean_text
                    .strip_prefix("[Result]: ")
                    .or_else(|| clean_text.strip_prefix("[Error]: "))
                    .unwrap_or(&clean_text);
                // Truncate long output
                if text.len() > 500 {
                    format!("{}\n...(truncated)", &text[..500])
                } else {
                    text.to_string()
                }
            }
            OutputType::Hook => {
                // Hook: show as is, but cleaner
                clean_text
            }
            _ => String::new(),
        }
    }
}

/// Output type for terminal styling
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputType {
    /// User input
    User,
    /// Assistant response
    Assistant,
    /// Tool use (function call)
    ToolUse,
    /// Tool result (function output)
    ToolResult,
    /// Thinking (internal reasoning)
    Thinking,
    /// System message
    System,
    /// Hook output
    Hook,
    /// File history snapshot
    FileHistory,
    /// Meta message (non-displayable)
    Meta,
    /// Unknown type
    Unknown,
}

impl ClaudeEntry {
    /// Parse a JSONL line and format for display
    pub fn parse_line(line: &str) -> Option<FormattedOutput> {
        match serde_json::from_str::<ClaudeEntry>(line) {
            Ok(entry) => Some(entry.format()),
            Err(e) => {
                // Check if it's a JSON parse error or just unknown structure
                if serde_json::from_str::<serde_json::Value>(line).is_ok() {
                    debug!("Unknown JSONL entry structure: {}", e);
                } else {
                    warn!("Invalid JSON in JSONL line: {}", e);
                }
                None
            }
        }
    }

    /// Format entry for terminal display
    pub fn format(&self) -> FormattedOutput {
        match self {
            ClaudeEntry::User { message, is_meta } => {
                // Skip meta messages
                if is_meta.map_or(false, |m| m) {
                    return FormattedOutput {
                        text: String::new(),
                        is_waiting: false,
                        output_type: OutputType::Meta,
                    };
                }

                // Check if content has tool_result blocks
                let (text, output_type) = Self::extract_text_with_type_from_content(&message.content);

                if text.is_empty() && output_type == OutputType::User {
                    FormattedOutput {
                        text: String::new(),
                        is_waiting: false,
                        output_type: OutputType::Meta,
                    }
                } else if output_type != OutputType::User {
                    FormattedOutput {
                        text,
                        is_waiting: false,
                        output_type,
                    }
                } else {
                    FormattedOutput {
                        text: format!("> {}", text),
                        is_waiting: false,
                        output_type: OutputType::User,
                    }
                }
            }

            ClaudeEntry::Assistant { message } => {
                // Extract content from the message value
                // Structure: message.message.content (flattened creates nested message)
                let content = message.get("message")
                    .and_then(|m| m.get("content"))
                    .unwrap_or(message);
                let outputs = Self::format_content_blocks(content);

                if outputs.is_empty() {
                    FormattedOutput {
                        text: String::new(),
                        is_waiting: false,
                        output_type: OutputType::Assistant,
                    }
                } else {
                    // Combine all outputs
                    let combined = outputs
                        .iter()
                        .map(|o| o.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n");

                    // Use the first non-Assistant output type (e.g., ToolUse, Thinking)
                    // Otherwise default to Assistant
                    let output_type = outputs
                        .iter()
                        .find(|o| o.output_type != OutputType::Assistant)
                        .map(|o| o.output_type)
                        .unwrap_or(OutputType::Assistant);

                    FormattedOutput {
                        text: combined,
                        is_waiting: false,
                        output_type,
                    }
                }
            }

            ClaudeEntry::Attachment { data } => {
                let text = Self::format_attachment(data);
                FormattedOutput {
                    text,
                    is_waiting: false,
                    output_type: OutputType::Hook,
                }
            }

            ClaudeEntry::FileHistorySnapshot { .. } => FormattedOutput {
                text: String::new(),
                is_waiting: false,
                output_type: OutputType::FileHistory,
            },

            ClaudeEntry::Meta { .. } => FormattedOutput {
                text: String::new(),
                is_waiting: false,
                output_type: OutputType::Meta,
            },

            ClaudeEntry::TaskReminder { message } => {
                let text = message.clone().unwrap_or_default();
                FormattedOutput {
                    text: format!("[Reminder] {}", text),
                    is_waiting: false,
                    output_type: OutputType::System,
                }
            }

            ClaudeEntry::CommandPermissions {} => FormattedOutput {
                text: String::new(),
                is_waiting: false,
                output_type: OutputType::System,
            },

            ClaudeEntry::SkillListing {} => FormattedOutput {
                text: String::new(),
                is_waiting: false,
                output_type: OutputType::System,
            },

            ClaudeEntry::McpInstructions {} => FormattedOutput {
                text: String::new(),
                is_waiting: false,
                output_type: OutputType::System,
            },

            ClaudeEntry::HookSuccess { hook_name } => {
                let name = hook_name.clone().unwrap_or_default();
                FormattedOutput {
                    text: format!("[Hook: {}] Success", name),
                    is_waiting: false,
                    output_type: OutputType::Hook,
                }
            }

            ClaudeEntry::HookAdditionalContext {} => FormattedOutput {
                text: String::new(),
                is_waiting: false,
                output_type: OutputType::Hook,
            },

            ClaudeEntry::Create { .. } | ClaudeEntry::Unknown => FormattedOutput {
                text: String::new(),
                is_waiting: false,
                output_type: OutputType::Unknown,
            },
        }
    }

    /// Extract text and output type from content (handles tool_result in user messages)
    fn extract_text_with_type_from_content(content: &serde_json::Value) -> (String, OutputType) {
        match content {
            serde_json::Value::String(s) => (s.clone(), OutputType::User),
            serde_json::Value::Array(arr) => {
                // Check if there's a tool_result block (for user messages with tool output)
                for block in arr {
                    if let Some(block_type) = block.get("type").and_then(|v| v.as_str()) {
                        if block_type == "tool_result" {
                            // Found tool_result, format it
                            let tool_content = block.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            let is_error = block.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                            let prefix = if is_error { "[Error]" } else { "[Result]" };
                            return (format!("{}: {}", prefix, tool_content), OutputType::ToolResult);
                        }
                    }
                }
                // No tool_result, extract text normally
                let text = arr
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()).map(String::from))
                    .collect::<Vec<_>>()
                    .join("");
                (text, OutputType::User)
            }
            _ => (String::new(), OutputType::User),
        }
    }

    /// Format content blocks (assistant message may have multiple blocks)
    fn format_content_blocks(content: &serde_json::Value) -> Vec<FormattedOutput> {
        match content {
            serde_json::Value::String(s) => {
                if s.is_empty() {
                    vec![]
                } else {
                    vec![FormattedOutput {
                        text: s.clone(),
                        is_waiting: false,
                        output_type: OutputType::Assistant,
                    }]
                }
            }
            serde_json::Value::Array(blocks) => {
                blocks
                    .iter()
                    .filter_map(|block| {
                        let block: ContentBlock = match serde_json::from_value(block.clone()) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::debug!("Failed to parse content block: {}", e);
                                return None;
                            }
                        };
                        Self::format_block(&block)
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    /// Format a single content block
    fn format_block(block: &ContentBlock) -> Option<FormattedOutput> {
        match block.block_type.as_str() {
            "text" => {
                let text = block.text.clone().unwrap_or_default();
                if text.is_empty() {
                    None
                } else {
                    Some(FormattedOutput {
                        text,
                        is_waiting: false,
                        output_type: OutputType::Assistant,
                    })
                }
            }

            "thinking" => {
                let thinking = block.thinking.clone().unwrap_or_default();
                if thinking.is_empty() {
                    None
                } else {
                    let truncated = truncate(&thinking, MAX_THINKING_DISPLAY);
                    let display = if thinking.chars().count() > MAX_THINKING_DISPLAY {
                        format!("{}...", truncated)
                    } else {
                        truncated
                    };
                    Some(FormattedOutput {
                        text: format!("[Thinking...]\n{}", display),
                        is_waiting: false,
                        output_type: OutputType::Thinking,
                    })
                }
            }

            "tool_use" => {
                let name = block.name.clone().unwrap_or_else(|| "Unknown".to_string());
                let input = block.input.as_ref().map(|i| serde_json::to_string(i).unwrap_or_default()).unwrap_or_default();
                let truncated = truncate(&input, MAX_TOOL_INPUT_DISPLAY);
                let display = if input.chars().count() > MAX_TOOL_INPUT_DISPLAY {
                    format!("{}...", truncated)
                } else {
                    truncated
                };

                Some(FormattedOutput {
                    text: format!("[Tool: {}] {}", name, display),
                    is_waiting: false,
                    output_type: OutputType::ToolUse,
                })
            }

            "tool_result" => {
                let content = block.content.clone().unwrap_or_default();
                let truncated = truncate(&content, MAX_TOOL_RESULT_DISPLAY);
                let display = if content.chars().count() > MAX_TOOL_RESULT_DISPLAY {
                    format!("{}...", truncated)
                } else {
                    truncated
                };

                let prefix = if block.is_error.unwrap_or(false) {
                    "[Error]"
                } else {
                    "[Result]"
                };

                Some(FormattedOutput {
                    text: format!("{}: {}", prefix, display),
                    is_waiting: false,
                    output_type: OutputType::ToolResult,
                })
            }

            _ => None,
        }
    }

    /// Format attachment info (hook outputs)
    /// Now handles serde_json::Value with flattened fields from JSONL
    fn format_attachment(data: &serde_json::Value) -> String {
        let mut parts = Vec::new();

        // Get nested "attachment" object if present
        let attachment = data.get("attachment").unwrap_or(data);

        // Hook info - check for hookEvent, hook_name, hookName
        if let Some(event) = attachment.get("hookEvent").or(attachment.get("hookName")).and_then(|v| v.as_str()) {
            parts.push(format!("[Hook: {}]", event));
        } else if let Some(atype) = attachment.get("type").and_then(|v| v.as_str()) {
            parts.push(format!("[Attachment: {}]", atype));
        }

        // Command if present
        if let Some(cmd) = attachment.get("command").and_then(|v| v.as_str()) {
            parts.push(format!("Command: {}", cmd));
        }

        // Output - check stdout
        if let Some(stdout) = attachment.get("stdout").and_then(|v| v.as_str()) {
            if !stdout.is_empty() {
                // Truncate long output
                let truncated = truncate(stdout, 500);
                let display = if stdout.chars().count() > 500 {
                    format!("{}...", truncated)
                } else {
                    truncated
                };
                parts.push(display);
            }
        }

        // Check stderr
        if let Some(stderr) = attachment.get("stderr").and_then(|v| v.as_str()) {
            if !stderr.is_empty() {
                parts.push(format!("[Stderr]: {}", truncate(stderr, 200)));
            }
        }

        // Exit code - check exitCode, exit_code
        if let Some(code) = attachment.get("exitCode").or(attachment.get("exit_code")).and_then(|v| v.as_i64()) {
            let status = if code == 0 { "Success" } else { "Failed" };
            parts.push(format!("Exit: {} ({})", code, status));
        }

        // Duration - check durationMs, duration_ms
        if let Some(duration) = attachment.get("durationMs").or(attachment.get("duration_ms")).and_then(|v| v.as_u64()) {
            parts.push(format!("Duration: {}ms", duration));
        }

        if parts.is_empty() {
            // Fallback: just show type if nothing else
            if let Some(atype) = attachment.get("type").and_then(|v| v.as_str()) {
                format!("[Attachment: {}]", atype)
            } else {
                String::new()
            }
        } else {
            parts.join("\n")
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
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("> Hello"));
        assert_eq!(output.output_type, OutputType::User);
    }

    #[test]
    fn test_parse_user_message_string_content() {
        let line = r#"{"type":"user","message":{"role":"user","content":"Hello world"}}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("Hello world"));
        assert_eq!(output.output_type, OutputType::User);
    }

    #[test]
    fn test_parse_assistant_message() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi there"}]}}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("Hi there"));
        assert_eq!(output.output_type, OutputType::Assistant);
    }

    #[test]
    fn test_parse_assistant_with_thinking() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me think about this problem carefully.","signature":"sig123"}]}}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("[Thinking"));
        assert!(output.text.contains("Let me think"));
        assert_eq!(output.output_type, OutputType::Thinking);
    }

    #[test]
    fn test_parse_tool_use() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.txt"},"id":"tool-1"}]}}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();

        assert!(output.text.contains("[Tool: Read]"));
        assert!(output.text.contains("file_path"));
        assert_eq!(output.output_type, OutputType::ToolUse);
    }

    #[test]
    fn test_parse_tool_result() {
        // Tool results are inside user message content array, matching actual JSONL format
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_123","content":"File content here"}]},"uuid":"test-123"}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("[Result]"));
        assert!(output.text.contains("File content here"));
        assert_eq!(output.output_type, OutputType::ToolResult);
    }

    #[test]
    fn test_parse_tool_result_error() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_123","content":"Error occurred","is_error":true}]},"uuid":"test-456"}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("[Error]"));
        assert_eq!(output.output_type, OutputType::ToolResult);
    }

    #[test]
    fn test_parse_meta_message_not_displayed() {
        // Meta has isMeta at root level
        let line = r#"{"type":"user","message":{"role":"user","content":"test"},"isMeta":true,"uuid":"test-789"}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.is_empty());
        assert_eq!(output.output_type, OutputType::Meta);
    }

    #[test]
    fn test_parse_file_history_snapshot() {
        let line = r#"{"type":"file-history-snapshot","messageId":"bb5d2298-831a-474a-bb0c-94c917b76a8d","snapshot":{"messageId":"bb5d2298-831a-474a-bb0c-94c917b76a8d","trackedFileBackups":{}},"isSnapshotUpdate":false}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.is_empty());
        assert_eq!(output.output_type, OutputType::FileHistory);
    }

    #[test]
    fn test_parse_attachment() {
        let line = r#"{"type":"attachment","attachment":{"type":"hook_success","hookName":"SessionStart:clear","hookEvent":"SessionStart","content":"","stdout":"test output","exitCode":0,"durationMs":100}}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("[Hook:"));
        assert!(output.text.contains("Exit:"));
        assert_eq!(output.output_type, OutputType::Hook);
    }

    #[test]
    fn test_parse_task_reminder() {
        let line = r#"{"type":"task_reminder","message":"Remember to check the build"}"#;
        let output = ClaudeEntry::parse_line(line).unwrap();
        assert!(output.text.contains("[Reminder]"));
        assert!(output.text.contains("Remember to check"));
    }

    #[test]
    fn test_parse_claude_code_jsonl_format() {
        // Test with actual Claude Code JSONL entries
        let lines = vec![
            // file-history-snapshot - should be skipped
            r#"{"type":"file-history-snapshot","messageId":"bb5d2298-831a-474a-bb0c-94c917b76a8d","snapshot":{"messageId":"bb5d2298-831a-474a-bb0c-94c917b76a8d","trackedFileBackups":{},"timestamp":"2026-05-11T03:00:53.458Z"},"isSnapshotUpdate":false}"#,
            // user message
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#,
            // assistant with text
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi there"}]}}"#,
            // assistant with thinking
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","text":"","thinking":"Let me solve this problem.","signature":"sig"}]}}"#,
            // assistant with tool_use
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.txt"},"id":"tool-1"}]}}"#,
            // user with tool_result
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":"file content","is_error":false}]}}"#,
            // attachment
            r#"{"type":"attachment","attachment":{"type":"hook_success","hookName":"test","stdout":"output","exitCode":0}}"#,
            // meta (isMeta)
            r#"{"type":"user","message":{"role":"user","content":"test"},"isMeta":true}"#,
        ];

        let expected = vec![
            ("file-history-snapshot", false, OutputType::FileHistory),
            ("user", true, OutputType::User),
            ("assistant", true, OutputType::Assistant),
            ("thinking", true, OutputType::Thinking),
            ("tool_use", true, OutputType::ToolUse),
            ("tool_result", true, OutputType::ToolResult),
            ("attachment", true, OutputType::Hook),
            ("meta", false, OutputType::Meta),
        ];

        for (i, line) in lines.iter().enumerate() {
            let output = ClaudeEntry::parse_line(line);
            let (name, should_have_text, expected_type) = &expected[i];

            match output {
                Some(out) => {
                    println!("[{}] {}: type={:?}, text={}", i, name, out.output_type,
                             if out.text.len() > 50 { format!("{}...", &out.text[..50]) } else { out.text.clone() });
                    assert_eq!(out.output_type, *expected_type, "Type mismatch for {}", name);
                    if *should_have_text {
                        assert!(!out.text.is_empty(), "Expected non-empty text for {}", name);
                    }
                }
                None => {
                    println!("[{}] {}: FAILED to parse", i, name);
                    // file-history-snapshot should return None (empty text)
                    if *name == "file-history-snapshot" {
                        // Expected to return Some with empty text
                    } else {
                        panic!("Failed to parse {}", name);
                    }
                }
            }
        }
    }

    /// Integration test: Parse actual Claude Code JSONL file
    /// This test uses the real conversation log file
    #[test]
    fn test_parse_real_claude_code_file() {
        let jsonl_path = r"C:\Users\binblink\.claude\projects\D--tauriProject-BedCode\0a771cd3-ac60-4638-9a60-6ca997349136.jsonl";

        // Skip if file doesn't exist (CI environments)
        let content = match std::fs::read_to_string(jsonl_path) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping test: JSONL file not found: {}", e);
                return;
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        assert!(lines.len() > 0, "JSONL file should have content");

        println!("\n=== Testing real Claude Code JSONL file ===");
        println!("Total lines: {}\n", lines.len());

        let mut parsed_count = 0;
        let mut user_count = 0;
        let mut assistant_count = 0;
        let mut tool_use_count = 0;
        let mut tool_result_count = 0;
        let mut thinking_count = 0;
        let mut hook_count = 0;

        // Parse all lines
        for line in &lines {
            if line.trim().is_empty() {
                continue;
            }

            if let Some(output) = ClaudeEntry::parse_line(line) {
                parsed_count += 1;

                match output.output_type {
                    OutputType::User => {
                        user_count += 1;
                        assert!(output.text.starts_with("> ") || output.text.contains("[Result]") || output.text.contains("[Error]"),
                            "User output should start with '>' or contain tool result");
                    }
                    OutputType::Assistant => {
                        assistant_count += 1;
                    }
                    OutputType::Thinking => {
                        thinking_count += 1;
                        assert!(output.text.contains("[Thinking"), "Thinking should contain [Thinking]");
                    }
                    OutputType::ToolUse => {
                        tool_use_count += 1;
                        assert!(output.text.contains("[Tool:"), "ToolUse should contain [Tool:]");
                    }
                    OutputType::ToolResult => {
                        tool_result_count += 1;
                        assert!(output.text.contains("[Result]") || output.text.contains("[Error]"),
                            "ToolResult should contain [Result] or [Error]");
                    }
                    OutputType::Hook => {
                        hook_count += 1;
                    }
                    _ => {}
                }
            }
        }

        // Print summary
        println!("Parsed: {}/{} lines", parsed_count, lines.len());
        println!("User messages: {}", user_count);
        println!("Assistant responses: {}", assistant_count);
        println!("Thinking: {}", thinking_count);
        println!("Tool uses: {}", tool_use_count);
        println!("Tool results: {}", tool_result_count);
        println!("Hooks: {}", hook_count);

        // Assertions
        assert!(parsed_count > 0, "Should parse at least some lines");
        assert!(user_count > 0, "Should have user messages");
        assert!(assistant_count > 0, "Should have assistant responses");
        assert!(tool_use_count > 0, "Should have tool uses");
        assert!(tool_result_count > 0, "Should have tool results");
    }
}