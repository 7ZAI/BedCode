//! Parser Model - Terminal output parsing types

use serde::{Deserialize, Serialize};

use crate::shared::parser::AnsiStyle;

/// Parsed output segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParsedSegment {
    Text(String),
    StyledText { text: String, style: AnsiStyle },
    AnsiCode(String),
    Markdown(String),
    CodeBlock { language: String, code: String },
    Progress { percent: u8, message: String },
    WaitingInput,
}