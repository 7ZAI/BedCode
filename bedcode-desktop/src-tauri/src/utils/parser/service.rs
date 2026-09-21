//! Output Parser Service
//!
//! 输出解析器服务实现

use regex::Regex;
use std::collections::VecDeque;

pub use super::types::ParsedSegment;
pub use super::{AnsiParser, MarkdownParser, StyledSegment};

/// Output parser combining ANSI and Markdown parsing
pub struct OutputParser {
    ansi_parser: AnsiParser,
    progress_regex: Regex,
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            ansi_parser: AnsiParser::new(),
            // Match progress patterns like "50%", "[50/100]", "Loading... 50%"
            progress_regex: Regex::new(r"(\d+)%|\[(\d+)/(\d+)\]|progress[:\s]*(\d+)%?").unwrap(),
        }
    }

    /// Parse output and return segments
    pub fn parse(&mut self, text: &str) -> Vec<ParsedSegment> {
        let mut segments = Vec::new();

        // First, strip ANSI for markdown detection
        let clean_text = self.ansi_parser.strip_ansi(text);

        // Check for code blocks
        let code_blocks = MarkdownParser::extract_code_blocks(&clean_text);
        if !code_blocks.is_empty() {
            for (language, code) in code_blocks {
                segments.push(ParsedSegment::CodeBlock { language, code });
            }
        }

        // Parse styled text from ANSI codes
        let styled = self.ansi_parser.parse(text);
        for segment in styled {
            segments.push(ParsedSegment::StyledText {
                text: segment.text,
                style: segment.style,
            });
        }

        // Check for progress
        if let Some(caps) = self.progress_regex.captures(&clean_text) {
            let percent = caps
                .get(1)
                .or_else(|| caps.get(4))
                .and_then(|m| m.as_str().parse::<u8>().ok())
                .unwrap_or(0);

            segments.push(ParsedSegment::Progress {
                percent,
                message: clean_text.clone(),
            });
        }

        segments
    }

    /// Get clean text without ANSI codes
    pub fn clean_output(&self, text: &str) -> String {
        self.ansi_parser.strip_ansi(text)
    }

    /// Parse streaming output
    pub fn parse_streaming(&mut self, text: &str) -> Vec<ParsedSegment> {
        self.ansi_parser.parse_streaming(text);
        self.parse(text)
    }

    /// Get buffered output
    pub fn get_buffer(&self) -> &VecDeque<StyledSegment> {
        self.ansi_parser.get_buffer()
    }

    /// Clear buffer
    pub fn clear_buffer(&mut self, keep: Option<usize>) {
        self.ansi_parser.clear_buffer(keep);
    }
}

impl Default for OutputParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ansi() {
        let mut parser = OutputParser::new();
        let segments = parser.parse("\x1b[32mGreen text\x1b[0m");
        assert!(!segments.is_empty());
    }

    #[test]
    fn test_progress_detection() {
        let mut parser = OutputParser::new();
        let segments = parser.parse("Downloading... 50%");
        let has_progress = segments.iter().any(|s| matches!(s, ParsedSegment::Progress { .. }));
        assert!(has_progress);
    }
}
