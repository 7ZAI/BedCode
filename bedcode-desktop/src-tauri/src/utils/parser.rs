//! Output Parser
//!
//! 终端输出解析 - ANSI 序列和 Markdown 检测

pub mod ansi;
pub mod markdown;
pub mod service;
pub mod types;

pub use ansi::{AnsiParser, AnsiStyle, StyledSegment};
pub use markdown::{MarkdownBlock, MarkdownParser};
pub use service::{detect_waiting_input, OutputParser, ParsedSegment};
