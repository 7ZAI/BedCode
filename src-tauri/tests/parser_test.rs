//! Tests for parser modules

use bedcode_lib::parser::{AnsiParser, AnsiStyle, MarkdownParser, MarkdownBlock, OutputParser, detect_waiting_input};

mod ansi_tests {
    use super::*;

    #[test]
    fn test_ansi_strip_basic() {
        let parser = AnsiParser::new();

        let input = "\x1b[31mRed text\x1b[0m normal text";
        let stripped = parser.strip_ansi(input);

        assert_eq!(stripped, "Red text normal text");
    }

    #[test]
    fn test_ansi_strip_multiple_codes() {
        let parser = AnsiParser::new();

        let input = "\x1b[1;31;42mBold Red on Green\x1b[0m";
        let stripped = parser.strip_ansi(input);

        assert_eq!(stripped, "Bold Red on Green");
    }

    #[test]
    fn test_ansi_strip_cursor_codes() {
        let parser = AnsiParser::new();

        let input = "Hello\x1b[HWorld";
        let stripped = parser.strip_ansi(input);

        assert_eq!(stripped, "HelloWorld");
    }

    #[test]
    fn test_ansi_parse_basic_colors() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[31mRed\x1b[0mNormal");

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Red");
        assert_eq!(segments[0].style.fg_color, Some("#cd0000".to_string()));
        assert_eq!(segments[1].text, "Normal");
        assert_eq!(segments[1].style.fg_color, None);
    }

    #[test]
    fn test_ansi_parse_bold() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[1mBold Text\x1b[0m");

        assert!(segments[0].style.bold);
    }

    #[test]
    fn test_ansi_parse_italic() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[3mItalic Text\x1b[0m");

        assert!(segments[0].style.italic);
    }

    #[test]
    fn test_ansi_parse_underline() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[4mUnderlined\x1b[0m");

        assert!(segments[0].style.underline);
    }

    #[test]
    fn test_ansi_parse_combined_styles() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[1;3;4;31mStyled\x1b[0m");

        let style = &segments[0].style;
        assert!(style.bold);
        assert!(style.italic);
        assert!(style.underline);
        assert_eq!(style.fg_color, Some("#cd0000".to_string()));
    }

    #[test]
    fn test_ansi_parse_bright_colors() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[91mBright Red\x1b[0m");

        assert_eq!(segments[0].style.fg_color, Some("#ff0000".to_string()));
    }

    #[test]
    fn test_ansi_parse_background_colors() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[44mBlue BG\x1b[0m");

        assert_eq!(segments[0].style.bg_color, Some("#0000ee".to_string()));
    }

    #[test]
    fn test_ansi_parse_reset() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("\x1b[1mBold\x1b[0mNormal");

        assert!(segments[0].style.bold);
        assert!(!segments[1].style.bold);
    }

    #[test]
    fn test_ansi_parse_no_codes() {
        let mut parser = AnsiParser::new();

        let segments = parser.parse("Plain text without codes");

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Plain text without codes");
        assert_eq!(segments[0].style, AnsiStyle::default());
    }

    #[test]
    fn test_ansi_streaming_parse() {
        let mut parser = AnsiParser::new();

        parser.parse_streaming("\x1b[31mRed ");
        parser.parse_streaming("Text\x1b[0m");

        let buffer = parser.get_buffer();
        assert!(buffer.len() >= 2);
    }

    #[test]
    fn test_ansi_buffer_clear() {
        let mut parser = AnsiParser::new();

        parser.parse_streaming("Some text");
        assert!(!parser.get_buffer().is_empty());

        parser.clear_buffer(None);
        assert!(parser.get_buffer().is_empty());
    }

    #[test]
    fn test_ansi_buffer_keep() {
        let mut parser = AnsiParser::new();

        for i in 0..10 {
            parser.parse_streaming(&format!("Line {}\n", i));
        }

        parser.clear_buffer(Some(5));
        assert_eq!(parser.get_buffer().len(), 5);
    }
}

mod markdown_tests {
    use super::*;

    #[test]
    fn test_detect_markdown() {
        assert!(MarkdownParser::detect_markdown("# Heading"));
        assert!(MarkdownParser::detect_markdown("```\ncode\n```"));
        assert!(MarkdownParser::detect_markdown("**bold**"));
        assert!(MarkdownParser::detect_markdown("- list item"));
        assert!(MarkdownParser::detect_markdown("[link](url)"));
        assert!(!MarkdownParser::detect_markdown("plain text"));
    }

    #[test]
    fn test_extract_code_blocks() {
        let text = r#"Some text
```rust
fn main() {}
```
More text
```python
print("hello")
```
"#;

        let blocks = MarkdownParser::extract_code_blocks(text);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "rust");
        assert_eq!(blocks[0].1, "fn main() {}");
        assert_eq!(blocks[1].0, "python");
        assert_eq!(blocks[1].1, "print(\"hello\")");
    }

    #[test]
    fn test_extract_code_blocks_no_language() {
        let text = "```\ncode without language\n```";

        let blocks = MarkdownParser::extract_code_blocks(text);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "");
        assert_eq!(blocks[0].1, "code without language");
    }

    #[test]
    fn test_parse_heading() {
        let parser = MarkdownParser::new();
        let blocks = parser.parse("# Title\n## Subtitle\n### Sub-subtitle");

        assert_eq!(blocks.len(), 3);

        if let MarkdownBlock::Heading { level, text } = &blocks[0] {
            assert_eq!(*level, 1);
            assert_eq!(text, "Title");
        } else {
            panic!("Expected Heading block");
        }

        if let MarkdownBlock::Heading { level, text } = &blocks[1] {
            assert_eq!(*level, 2);
            assert_eq!(text, "Subtitle");
        } else {
            panic!("Expected Heading block");
        }
    }

    #[test]
    fn test_parse_paragraph() {
        let parser = MarkdownParser::new();
        let blocks = parser.parse("This is a paragraph.\n\nAnother paragraph.");

        assert_eq!(blocks.len(), 2);

        if let MarkdownBlock::Paragraph(text) = &blocks[0] {
            assert_eq!(text, "This is a paragraph.");
        } else {
            panic!("Expected Paragraph block");
        }
    }

    #[test]
    fn test_parse_code_block() {
        let parser = MarkdownParser::new();
        let blocks = parser.parse("```javascript\nconsole.log('test');\n```");

        assert_eq!(blocks.len(), 1);

        if let MarkdownBlock::CodeBlock { language, code } = &blocks[0] {
            assert_eq!(language, "javascript");
            assert_eq!(code, "console.log('test');");
        } else {
            panic!("Expected CodeBlock");
        }
    }

    #[test]
    fn test_parse_list() {
        let parser = MarkdownParser::new();
        let blocks = parser.parse("- Item 1\n- Item 2");

        for block in &blocks {
            if let MarkdownBlock::List { ordered, items } = block {
                assert!(!ordered);
                assert!(!items.is_empty());
                return;
            }
        }
    }

    #[test]
    fn test_parse_blockquote() {
        let parser = MarkdownParser::new();
        let blocks = parser.parse("> This is a quote");

        assert_eq!(blocks.len(), 1);

        if let MarkdownBlock::Blockquote(text) = &blocks[0] {
            assert_eq!(text, "This is a quote");
        } else {
            panic!("Expected Blockquote block");
        }
    }

    #[test]
    fn test_parse_horizontal_rule() {
        let parser = MarkdownParser::new();
        let blocks = parser.parse("Above\n---\nBelow");

        assert!(blocks.iter().any(|b| matches!(b, MarkdownBlock::HorizontalRule)));
    }

    #[test]
    fn test_extract_links() {
        let parser = MarkdownParser::new();
        let text = "Check out [Google](https://google.com) and [GitHub](https://github.com)";

        let links = parser.extract_links(text);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0], ("Google".to_string(), "https://google.com".to_string()));
        assert_eq!(links[1], ("GitHub".to_string(), "https://github.com".to_string()));
    }

    #[test]
    fn test_parse_mixed_content() {
        let parser = MarkdownParser::new();
        let text = r#"# Project Title

This is an introduction.

## Code Example

```rust
fn main() {
    println!("Hello");
}
```

> Note: This is important.

- Item 1
- Item 2
"#;

        let blocks = parser.parse(text);

        // Should have heading, paragraph, heading, code block, blockquote, list
        assert!(blocks.len() >= 5);

        // Check for code block
        assert!(blocks.iter().any(|b| matches!(b, MarkdownBlock::CodeBlock { .. })));
        // Check for blockquote
        assert!(blocks.iter().any(|b| matches!(b, MarkdownBlock::Blockquote(_))));
    }
}

mod output_parser_tests {
    use super::*;

    #[test]
    fn test_detect_waiting_input_prompt() {
        assert!(detect_waiting_input("Enter command: > "));
        assert!(detect_waiting_input("Some output\n❯ "));
        assert!(detect_waiting_input("Continue? [Y/n] "));
        assert!(detect_waiting_input("press any key to continue"));
        assert!(!detect_waiting_input("No prompt here"));
        assert!(!detect_waiting_input("Just some output"));
    }

    #[test]
    fn test_detect_waiting_input_with_ansi() {
        let parser = OutputParser::new();

        // With ANSI codes
        assert!(parser.detect_waiting_input("\x1b[32m> \x1b[0m"));
        assert!(parser.detect_waiting_input("\x1b[1mContinue?\x1b[0m [Y/n] "));
    }

    #[test]
    fn test_output_parser_clean_output() {
        let parser = OutputParser::new();

        let input = "\x1b[31mRed\x1b[0m text";
        let clean = parser.clean_output(input);

        assert_eq!(clean, "Red text");
    }

    #[test]
    fn test_output_parser_waiting_detection() {
        let parser = OutputParser::new();

        // Should detect waiting
        assert!(parser.detect_waiting_input("Claude is thinking...\n> "));
        assert!(parser.detect_waiting_input("❯ "));
        assert!(parser.detect_waiting_input("What would you like to do? [Y/n] "));

        // Should not detect
        assert!(!parser.detect_waiting_input("Claude is thinking..."));
        assert!(!parser.detect_waiting_input("Processing..."));
    }
}
