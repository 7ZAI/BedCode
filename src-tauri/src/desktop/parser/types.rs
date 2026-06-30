//! Parser Types
//!
//! 解析器类型定义

use super::AnsiStyle;

/// Parsed output segment
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ParsedSegment {
    Text(String),
    StyledText { text: String, style: AnsiStyle },
    AnsiCode(String),
    Markdown(String),
    CodeBlock { language: String, code: String },
    Progress { percent: u8, message: String },
    WaitingInput,
}