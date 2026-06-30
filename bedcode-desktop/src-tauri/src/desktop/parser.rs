//! Output Parser Module
//!
//! 输出解析模块 - 桌面端专用
//!
//! 模块划分:
//! - types.rs: 类型定义
//! - ansi.rs: ANSI 解析器
//! - markdown.rs: Markdown 解析器
//! - service.rs: 解析器服务实现

mod ansi;
mod markdown;
mod service;
mod types;

pub use ansi::{AnsiParser, AnsiStyle, StyledSegment};
pub use markdown::{MarkdownBlock, MarkdownParser};
pub use service::{detect_waiting_input, OutputParser, ParsedSegment};