//! ANSI Escape Sequence Parser

use regex::Regex;
use std::collections::VecDeque;

/// ANSI color codes
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnsiStyle {
    pub fg_color: Option<String>,
    pub bg_color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub blink: bool,
    pub reverse: bool,
}

impl Default for AnsiStyle {
    fn default() -> Self {
        Self {
            fg_color: None,
            bg_color: None,
            bold: false,
            italic: false,
            underline: false,
            dim: false,
            blink: false,
            reverse: false,
        }
    }
}

/// Parsed text segment with style
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StyledSegment {
    pub text: String,
    pub style: AnsiStyle,
}

/// ANSI Parser
pub struct AnsiParser {
    escape_regex: Regex,
    cursor_regex: Regex,
    erase_regex: Regex,
    style: AnsiStyle,
    buffer: VecDeque<StyledSegment>,
}

impl AnsiParser {
    pub fn new() -> Self {
        Self {
            // Match SGR sequences: ESC[...m
            escape_regex: Regex::new(r"\x1b\[([0-9;]*)m").unwrap(),
            // Match cursor movement: ESC[H, ESC[<n>A/B/C/D, etc.
            cursor_regex: Regex::new(r"\x1b\[[0-9]*[A-HJKSTfmsu]").unwrap(),
            // Match erase sequences: ESC[J, ESC[K
            erase_regex: Regex::new(r"\x1b\[[0-9]*[JK]").unwrap(),
            style: AnsiStyle::default(),
            buffer: VecDeque::new(),
        }
    }

    /// Strip all ANSI codes from text
    pub fn strip_ansi(&self, text: &str) -> String {
        let text = self.escape_regex.replace_all(text, "");
        let text = self.cursor_regex.replace_all(&text, "");
        self.erase_regex.replace_all(&text, "").to_string()
    }

    /// Parse text and return styled segments
    pub fn parse(&mut self, text: &str) -> Vec<StyledSegment> {
        let mut segments = Vec::new();
        let mut last_end = 0;

        // Find all SGR sequences and collect them first to avoid borrow conflict
        let captures: Vec<_> = self.escape_regex.captures_iter(text).collect();

        for cap in &captures {
            let full_match = cap.get(0).unwrap();

            // Add text before this sequence
            if full_match.start() > last_end {
                let text_segment = &text[last_end..full_match.start()];
                if !text_segment.is_empty() {
                    segments.push(StyledSegment {
                        text: text_segment.to_string(),
                        style: self.style.clone(),
                    });
                }
            }

            // Update style based on SGR parameters
            if let Some(params) = cap.get(1) {
                self.apply_sgr(params.as_str());
            }

            last_end = full_match.end();
        }

        // Add remaining text
        if last_end < text.len() {
            let text_segment = &text[last_end..];
            if !text_segment.is_empty() {
                segments.push(StyledSegment {
                    text: text_segment.to_string(),
                    style: self.style.clone(),
                });
            }
        }

        segments
    }

    /// Parse and accumulate output (for streaming)
    pub fn parse_streaming(&mut self, text: &str) -> Vec<StyledSegment> {
        let segments = self.parse(text);
        for segment in &segments {
            self.buffer.push_back(segment.clone());
        }
        segments
    }

    /// Get all buffered segments
    pub fn get_buffer(&self) -> &VecDeque<StyledSegment> {
        &self.buffer
    }

    /// Clear buffer, optionally keeping last N segments
    pub fn clear_buffer(&mut self, keep: Option<usize>) {
        if let Some(n) = keep {
            while self.buffer.len() > n {
                self.buffer.pop_front();
            }
        } else {
            self.buffer.clear();
        }
    }

    /// Apply SGR (Select Graphic Rendition) parameters
    fn apply_sgr(&mut self, params: &str) {
        if params.is_empty() {
            // Reset all styles
            self.style = AnsiStyle::default();
            return;
        }

        let codes: Vec<u16> = params
            .split(';')
            .filter_map(|s| s.parse().ok())
            .collect();

        for code in codes {
            match code {
                0 => self.style = AnsiStyle::default(),
                1 => self.style.bold = true,
                2 => self.style.dim = true,
                3 => self.style.italic = true,
                4 => self.style.underline = true,
                5 | 6 => self.style.blink = true,
                7 => self.style.reverse = true,
                22 => {
                    self.style.bold = false;
                    self.style.dim = false;
                }
                23 => self.style.italic = false,
                24 => self.style.underline = false,
                25 => self.style.blink = false,
                27 => self.style.reverse = false,
                30..=37 => self.style.fg_color = Some(ansi_color_to_hex(code - 30)),
                38 => {
                    // Extended foreground color - simplified
                }
                39 => self.style.fg_color = None,
                40..=47 => self.style.bg_color = Some(ansi_color_to_hex(code - 40)),
                48 => {
                    // Extended background color - simplified
                }
                49 => self.style.bg_color = None,
                90..=97 => self.style.fg_color = Some(ansi_bright_color_to_hex(code - 90)),
                100..=107 => self.style.bg_color = Some(ansi_bright_color_to_hex(code - 100)),
                _ => {}
            }
        }
    }
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert ANSI color code (0-7) to hex color
fn ansi_color_to_hex(code: u16) -> String {
    let colors = [
        "#000000", // Black
        "#cd0000", // Red
        "#00cd00", // Green
        "#cdcd00", // Yellow
        "#0000ee", // Blue
        "#cd00cd", // Magenta
        "#00cdcd", // Cyan
        "#e5e5e5", // White
    ];
    colors.get(code as usize).unwrap_or(&"#ffffff").to_string()
}

/// Convert ANSI bright color code (0-7) to hex color
fn ansi_bright_color_to_hex(code: u16) -> String {
    let colors = [
        "#7f7f7f", // Bright Black
        "#ff0000", // Bright Red
        "#00ff00", // Bright Green
        "#ffff00", // Bright Yellow
        "#5c5cff", // Bright Blue
        "#ff00ff", // Bright Magenta
        "#00ffff", // Bright Cyan
        "#ffffff", // Bright White
    ];
    colors.get(code as usize).unwrap_or(&"#ffffff").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let parser = AnsiParser::new();
        let text = "\x1b[31mRed text\x1b[0m normal text";
        assert_eq!(parser.strip_ansi(text), "Red text normal text");
    }

    #[test]
    fn test_parse_basic() {
        let mut parser = AnsiParser::new();
        let segments = parser.parse("\x1b[31mRed\x1b[0m Normal");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Red");
        assert_eq!(segments[0].style.fg_color, Some("#cd0000".to_string()));
        assert_eq!(segments[1].text, " Normal");
        assert_eq!(segments[1].style.fg_color, None);
    }

    #[test]
    fn test_parse_bold() {
        let mut parser = AnsiParser::new();
        let segments = parser.parse("\x1b[1mBold\x1b[0m");
        assert_eq!(segments[0].style.bold, true);
    }
}
