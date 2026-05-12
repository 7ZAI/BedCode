//! Markdown Parser

use regex::Regex;

/// Markdown block types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MarkdownBlock {
    Heading { level: u8, text: String },
    Paragraph(String),
    CodeBlock { language: String, code: String },
    CodeInline(String),
    List { ordered: bool, items: Vec<String> },
    Blockquote(String),
    HorizontalRule,
    Link { text: String, url: String },
    Image { alt: String, url: String },
}

/// Markdown Parser
pub struct MarkdownParser {
    code_block_start: Regex,
    heading_regex: Regex,
    list_regex: Regex,
    blockquote_regex: Regex,
    link_regex: Regex,
}

impl MarkdownParser {
    pub fn new() -> Self {
        Self {
            code_block_start: Regex::new(r"^```(\w*)").unwrap(),
            heading_regex: Regex::new(r"^(#{1,6})\s+(.+)$").unwrap(),
            list_regex: Regex::new(r"^(\s*)[-*+]\s+(.+)$|^(\s*)(\d+)\.\s+(.+)$").unwrap(),
            blockquote_regex: Regex::new(r"^>\s+(.+)$").unwrap(),
            link_regex: Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap(),
        }
    }

    /// Detect if output contains Markdown
    pub fn detect_markdown(text: &str) -> bool {
        let indicators = [
            "```",  // Code block
            "#",    // Heading (will match #, ##, ###, etc.)
            "**",   // Bold
            "__",   // Bold
            "- ",   // List
            "* ",   // List
            "1. ",  // Numbered list
            "[",    // Link
            "> ",   // Blockquote
        ];

        for indicator in indicators {
            if text.contains(indicator) {
                return true;
            }
        }

        false
    }

    /// Extract code blocks from text
    pub fn extract_code_blocks(text: &str) -> Vec<(String, String)> {
        let mut blocks = Vec::new();
        let mut in_block = false;
        let mut language = String::new();
        let mut code = String::new();

        for line in text.lines() {
            if line.starts_with("```") {
                if in_block {
                    // End of block
                    blocks.push((language.clone(), code.trim_end().to_string()));
                    language.clear();
                    code.clear();
                    in_block = false;
                } else {
                    // Start of block
                    language = line[3..].trim().to_string();
                    in_block = true;
                }
            } else if in_block {
                code.push_str(line);
                code.push('\n');
            }
        }

        blocks
    }

    /// Parse text into markdown blocks
    pub fn parse(&self, text: &str) -> Vec<MarkdownBlock> {
        let mut blocks = Vec::new();
        let mut in_code_block = false;
        let mut code_language = String::new();
        let mut code_content = String::new();
        let mut current_paragraph = String::new();

        for line in text.lines() {
            // Handle code blocks
            if line.starts_with("```") {
                if in_code_block {
                    // End code block
                    blocks.push(MarkdownBlock::CodeBlock {
                        language: code_language.clone(),
                        code: code_content.trim_end().to_string(),
                    });
                    code_language.clear();
                    code_content.clear();
                    in_code_block = false;
                } else {
                    // Start code block
                    self.flush_paragraph(&mut current_paragraph, &mut blocks);
                    code_language = line[3..].trim().to_string();
                    in_code_block = true;
                }
                continue;
            }

            if in_code_block {
                code_content.push_str(line);
                code_content.push('\n');
                continue;
            }

            // Check for heading
            if let Some(cap) = self.heading_regex.captures(line) {
                self.flush_paragraph(&mut current_paragraph, &mut blocks);
                let level = cap[1].len() as u8;
                let text = cap[2].to_string();
                blocks.push(MarkdownBlock::Heading { level, text });
                continue;
            }

            // Check for list
            if let Some(cap) = self.list_regex.captures(line) {
                self.flush_paragraph(&mut current_paragraph, &mut blocks);
                let item = cap.get(2)
                    .or_else(|| cap.get(5))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let ordered = cap.get(4).is_some();
                blocks.push(MarkdownBlock::List {
                    ordered,
                    items: vec![item],
                });
                continue;
            }

            // Check for blockquote
            if let Some(cap) = self.blockquote_regex.captures(line) {
                self.flush_paragraph(&mut current_paragraph, &mut blocks);
                blocks.push(MarkdownBlock::Blockquote(cap[1].to_string()));
                continue;
            }

            // Check for horizontal rule
            if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
                self.flush_paragraph(&mut current_paragraph, &mut blocks);
                blocks.push(MarkdownBlock::HorizontalRule);
                continue;
            }

            // Accumulate as paragraph
            if !line.trim().is_empty() {
                if !current_paragraph.is_empty() {
                    current_paragraph.push('\n');
                }
                current_paragraph.push_str(line);
            } else if !current_paragraph.is_empty() {
                self.flush_paragraph(&mut current_paragraph, &mut blocks);
            }
        }

        // Flush remaining content
        self.flush_paragraph(&mut current_paragraph, &mut blocks);

        if in_code_block {
            blocks.push(MarkdownBlock::CodeBlock {
                language: code_language,
                code: code_content,
            });
        }

        blocks
    }

    /// Flush accumulated paragraph
    fn flush_paragraph(&self, paragraph: &mut String, blocks: &mut Vec<MarkdownBlock>) {
        if !paragraph.is_empty() {
            blocks.push(MarkdownBlock::Paragraph(paragraph.trim().to_string()));
            paragraph.clear();
        }
    }

    /// Extract links from text
    pub fn extract_links(&self, text: &str) -> Vec<(String, String)> {
        self.link_regex
            .captures_iter(text)
            .map(|cap| (cap[1].to_string(), cap[2].to_string()))
            .collect()
    }
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_markdown() {
        assert!(MarkdownParser::detect_markdown("# Heading"));
        assert!(MarkdownParser::detect_markdown("```\ncode\n```"));
        assert!(MarkdownParser::detect_markdown("**bold**"));
        assert!(!MarkdownParser::detect_markdown("plain text"));
    }

    #[test]
    fn test_extract_code_blocks() {
        let text = "```rust\nfn main() {}\n```\n\nSome text\n```python\nprint('hi')\n```";
        let blocks = MarkdownParser::extract_code_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "rust");
        assert_eq!(blocks[1].0, "python");
    }

    #[test]
    fn test_parse_heading() {
        let parser = MarkdownParser::new();
        let blocks = parser.parse("# Title\n## Subtitle");
        assert_eq!(blocks.len(), 2);
        if let MarkdownBlock::Heading { level, text } = &blocks[0] {
            assert_eq!(*level, 1);
            assert_eq!(text, "Title");
        }
    }
}
