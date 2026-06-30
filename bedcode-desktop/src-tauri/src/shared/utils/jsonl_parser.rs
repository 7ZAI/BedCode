//! JSONL 解析工具
//!
//! 提供 Claude Code JSONL 日志文件的解析能力，将每行 JSON 解析为结构化数据。
//! 本模块为独立工具类，不依赖任何业务逻辑。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ==================== Data Structures ====================

/// JSONL 日志条目
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeEntry {
    pub r#type: String,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub content: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<i64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(default)]
    pub server_tool_use: Option<ServerToolUse>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub is_error: Option<bool>,
}

/// 服务端工具使用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolUse {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
}

/// 消息内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// 内容块
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: Option<serde_json::Value>,
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

/// 格式化输出 — 将 ClaudeEntry 转为终端可显示的文本
#[derive(Debug, Clone)]
pub struct FormattedOutput {
    pub text: String,
    pub entry_type: String,
}

impl ClaudeEntry {
    /// 将条目格式化为终端输出文本
    pub fn to_formatted_output(&self) -> FormattedOutput {
        let text = match self.r#type.as_str() {
            "assistant" => self.format_assistant(),
            "user" => self.format_user(),
            "system" => self.format_system(),
            "result" => self.format_result(),
            "summary" => self.content.as_ref()
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        };

        FormattedOutput {
            text,
            entry_type: self.r#type.clone(),
        }
    }

    fn format_assistant(&self) -> String {
        let mut parts = Vec::new();

        if let Some(content) = &self.content {
            if let Ok(blocks) = serde_json::from_value::<Vec<ContentBlock>>(content.clone()) {
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => parts.push(text),
                        ContentBlock::ToolUse { name, input, .. } => {
                            parts.push(format!("[Tool: {}] {}", name, input));
                        }
                        ContentBlock::ToolResult { content, .. } => {
                            if let Some(c) = content {
                                parts.push(format!("[Result] {}", c));
                            }
                        }
                        ContentBlock::Thinking { thinking } => {
                            parts.push(format!("[Thinking] {}", thinking));
                        }
                    }
                }
            } else if let Some(s) = content.as_str() {
                parts.push(s.to_string());
            }
        }

        parts.join("\n")
    }

    fn format_user(&self) -> String {
        if let Some(content) = &self.content {
            if let Some(s) = content.as_str() {
                return format!("> {}\n", s);
            }
            if let Ok(blocks) = serde_json::from_value::<Vec<ContentBlock>>(content.clone()) {
                let texts: Vec<String> = blocks
                    .into_iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(format!("> {}", text)),
                        _ => None,
                    })
                    .collect();
                if !texts.is_empty() {
                    return texts.join("\n") + "\n";
                }
            }
        }
        String::new()
    }

    fn format_system(&self) -> String {
        self.content.as_ref()
            .and_then(|c| c.as_str())
            .map(|s| format!("[System] {}\n", s))
            .unwrap_or_default()
    }

    fn format_result(&self) -> String {
        self.content.as_ref()
            .and_then(|c| c.as_str())
            .map(|s| format!("{}\n", s))
            .unwrap_or_default()
    }
}

// ==================== Parsing ====================

/// 解析 JSONL 文件内容，返回所有有效条目
pub fn parse_jsonl(content: &str) -> Vec<ClaudeEntry> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ClaudeEntry>(line).ok())
        .collect()
}

/// 从指定偏移量开始读取并解析 JSONL 文件的新增行
///
/// 返回 (新条目列表, 新文件偏移量)
pub fn read_new_lines(content: &str, start_offset: usize) -> (Vec<ClaudeEntry>, usize) {
    let bytes = content.as_bytes();
    if start_offset >= bytes.len() {
        return (Vec::new(), bytes.len());
    }

    let remaining = &content[start_offset..];
    let entries: Vec<ClaudeEntry> = remaining
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ClaudeEntry>(line).ok())
        .collect();

    (entries, bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jsonl_single_entry() {
        let content = r#"{"type":"assistant","content":"hello"}"#;
        let entries = parse_jsonl(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].r#type, "assistant");
    }

    #[test]
    fn test_parse_jsonl_multiple_entries() {
        let content = r#"{"type":"assistant","content":"hello"}
{"type":"user","content":"world"}"#;
        let entries = parse_jsonl(content);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_parse_jsonl_skips_invalid() {
        let content = r#"{"type":"assistant","content":"hello"}
invalid json
{"type":"user","content":"world"}"#;
        let entries = parse_jsonl(content);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_read_new_lines() {
        let content = r#"{"type":"assistant","content":"hello"}
{"type":"user","content":"world"}"#;
        let (entries, new_offset) = read_new_lines(content, 0);
        assert_eq!(entries.len(), 2);
        assert_eq!(new_offset, content.len());
    }

    #[test]
    fn test_read_new_lines_with_offset() {
        let content = r#"{"type":"assistant","content":"hello"}
{"type":"user","content":"world"}"#;
        let (_, offset) = read_new_lines(content, 0);
        let (entries, _) = read_new_lines(content, offset);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_formatted_output_assistant() {
        let entry = ClaudeEntry {
            r#type: "assistant".to_string(),
            content: Some(serde_json::json!("hello world")),
            ..Default::default()
        };
        let output = entry.to_formatted_output();
        assert_eq!(output.entry_type, "assistant");
    }
}
