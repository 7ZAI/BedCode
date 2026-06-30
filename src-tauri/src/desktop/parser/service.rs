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
    md_parser: MarkdownParser,
    progress_regex: Regex,
    waiting_patterns: Vec<Regex>,
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            ansi_parser: AnsiParser::new(),
            md_parser: MarkdownParser::new(),
            // Match progress patterns like "50%", "[50/100]", "Loading... 50%"
            progress_regex: Regex::new(r"(\d+)%|\[(\d+)/(\d+)\]|progress[:\s]*(\d+)%?")
                .unwrap(),
            waiting_patterns: vec![
                Regex::new(r"> $").unwrap(),
                Regex::new(r"❯ $").unwrap(),
                Regex::new(r"\?\s*$").unwrap(),
                Regex::new(r"\[Y/n\]\s*$").unwrap(),
                Regex::new(r"\[y/N\]\s*$").unwrap(),
                Regex::new(r"press any key").unwrap(),
                Regex::new(r"Press any key").unwrap(),
                Regex::new(r"waiting for input").unwrap(),
                Regex::new(r"Enter your choice").unwrap(),
            ],
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
            let percent = caps.get(1)
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

    /// Detect if output indicates waiting for input
    pub fn detect_waiting_input(&self, output: &str) -> bool {
        let clean = self.ansi_parser.strip_ansi(output);

        // Check last few lines (without reversing)
        let lines: Vec<&str> = clean.lines().rev().take(5).collect();

        for line in lines {
            for pattern in &self.waiting_patterns {
                if pattern.is_match(line) {
                    return true;
                }
            }
        }

        false
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

/// Detect waiting input from output string (convenience function)
pub fn detect_waiting_input(output: &str) -> bool {
    let parser = OutputParser::new();
    parser.detect_waiting_input(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_waiting() {
        let parser = OutputParser::new();
        assert!(parser.detect_waiting_input("> "));
        assert!(parser.detect_waiting_input("Some text\n❯ "));
        assert!(parser.detect_waiting_input("Continue? [Y/n] "));
        assert!(!parser.detect_waiting_input("No prompt here"));
    }

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