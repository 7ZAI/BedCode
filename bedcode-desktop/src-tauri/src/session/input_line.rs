//! Session Input Extension Point
//!
//! Two responsibilities (design decisions and trade-offs see ADR 0001):
//!
//! 1. [`SessionInputListener`] — observer extension point for submitted input lines,
//!    mimicking the `SessionLifecycleListener` registration/callback mechanism
//! 2. [`SubmittedLineTracker`] — reconstructs complete submitted input lines from raw input byte streams
//!
//! Reconstruction rules:
//! - Printable Unicode characters and spaces accumulate into the line buffer
//! - `\r` / `\n` trigger submission (`\r\n` does not result in a duplicate submission); the host performs no semantic filtering, and empty submissions are likewise notified
//! - `\x7f` / `\x08` (backspace) pops the trailing character of the buffer (line editing)
//! - `\x03` (Ctrl+C) / `\x15` (Ctrl+U) clear the buffer but do not trigger submission
//! - Other C0 control characters (Tab, etc.) are discarded
//! - ESC escape sequences (CSI / SS3 / two-character sequences) are discarded as a whole;
//!   `\x1b\r` / `\x1b\n` (common Shift+Enter encodings) are restored as newline content
//! - Within a bracketed paste block (`\x1b[200~` … `\x1b[201~`) everything is content,
//!   and internal newlines do not trigger submission (`\r\n` is normalized to `\n`)
//!
//! Known lossy scenarios (accepted): TUI history-recall text does not pass through the input stream;
//! multi-line input under modifyOtherKeys mode may be split.

use crate::system::constants::terminal::MAX_SUBMITTED_LINE_BUFFER_BYTES;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Session input listener
///
/// External modules implement this trait and register with SessionManager to receive submitted input lines.
/// Registration: `session_manager.register_input_listener(Arc::new(MyListener))`
///
/// Call guarantees (pure observer semantics, consistent with ADR 0001):
/// - Asynchronous dispatch; never blocks the input path
/// - Callback errors / timeouts do not affect the input itself
/// - The host performs no semantic filtering; empty submissions (empty-line Enter) are likewise notified
pub trait SessionInputListener: Send + Sync + 'static {
    /// Handles a submitted-input-line event
    ///
    /// # Arguments
    /// * `session_id` — PTY session ID
    /// * `text` — reconstructed submitted input line (may be an empty string)
    fn on_input_submitted(&self, session_id: &str, text: &str);

    /// Returns the associated plugin ID (if any)
    ///
    /// Used for removing listeners by plugin ID; non-plugin listeners return None
    fn plugin_id(&self) -> Option<&str> {
        None
    }
}

/// Submitted input line reconstructor
///
/// Maintains one line buffer per session: [`SubmittedLineTracker::feed`] feeds in an input chunk
/// and returns all complete submitted input lines produced within that chunk.
///
/// Uses `std::sync::Mutex`: the critical section is a pure in-memory state-machine advance (no awaits),
/// called at keystroke-chunk frequency, so avoiding tokio lock scheduling overhead is preferable.
/// The lock is never held across dispatch, so it cannot deadlock with other locks.
///
/// Internally Arc-based, `Clone` shares the same state — used to inject into background tasks (e.g., PTY lifecycle handlers) for cleanup
#[derive(Clone, Default)]
pub struct SubmittedLineTracker {
    /// Per-session line buffers (session_id → state machine)
    buffers: Arc<Mutex<HashMap<String, LineBuffer>>>,
}

impl SubmittedLineTracker {
    /// Creates an empty reconstructor
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Feeds an input chunk, returning all submitted input lines produced within this chunk
    ///
    /// Multiple lines may be returned (if the chunk contains multiple submissions); empty lines are likewise returned,
    /// whether to ignore them is decided by the listener's own business logic.
    pub fn feed(&self, session_id: &str, data: &str) -> Vec<String> {
        let mut map = self
            .buffers
            .lock()
            .expect("SubmittedLineTracker mutex poisoned");
        let buffer = map.entry(session_id.to_string()).or_default();
        buffer.feed(data)
    }

    /// Removes the buffer for that session (called on session termination)
    ///
    /// Unsubmitted residual content is discarded directly without dispatch — if not submitted, it is not a submitted input line
    pub fn remove_session(&self, session_id: &str) {
        let mut map = self
            .buffers
            .lock()
            .expect("SubmittedLineTracker mutex poisoned");
        map.remove(session_id);
    }
}

/// ESC escape sequence parsing state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EscState {
    /// Normal state
    #[default]
    Ground,
    /// Received `\x1b`, waiting for the next byte to determine the sequence type
    Escape,
    /// Within a CSI / SS3 sequence, collecting parameter bytes, waiting for the final byte
    Csi { params: [u8; 16], count: usize },
    /// Within an OSC sequence (`\x1b]...`), discarding until BEL or ST —
    /// OSC 内容不做行重建（调色板/标题等应用控制序列），完整丢弃
    Osc,
    /// OSC 内收到 `\x1b`：等待 ST 终止符 `\x1b\\`（其余字符回 Osc 继续丢弃）
    OscSt,
}

/// Per-session line buffer (pure in-memory state machine)
#[derive(Default)]
struct LineBuffer {
    /// Accumulated normal input content
    text: String,
    /// ESC sequence parsing state
    esc: EscState,
    /// Whether we are currently inside a bracketed paste block
    in_paste: bool,
    /// The previous character was `\r` (suppresses duplicate submission from `\r\n`, normalizes paste newlines)
    prev_cr: bool,
}

impl LineBuffer {
    /// Feeds one chunk, returning the list of submitted input lines produced
    fn feed(&mut self, data: &str) -> Vec<String> {
        let mut submitted = Vec::new();
        for c in data.chars() {
            match self.esc {
                EscState::Ground => self.feed_ground(c, &mut submitted),
                EscState::Escape => self.feed_escape(c),
                EscState::Csi { .. } => {
                    // If the sequence is malformed and aborted, the current character is re-fed as a normal character
                    if let Some(reprocess) = self.feed_csi(c) {
                        self.feed_ground(reprocess, &mut submitted);
                    }
                }
                EscState::Osc => self.feed_osc(c),
                EscState::OscSt => self.feed_osc_st(c),
            }
        }
        submitted
    }

    /// Normal-state character classification (reconstruction core rules)
    fn feed_ground(&mut self, c: char, submitted: &mut Vec<String>) {
        if self.in_paste {
            // Within a paste block everything is content; only an ESC-prefixed CSI 201~ sequence can end the paste
            if c == '\x1b' {
                self.esc = EscState::Escape;
                return;
            }
            match c {
                '\r' => {
                    self.text.push('\n');
                    self.prev_cr = true;
                }
                // \r\n is normalized to a single newline
                '\n' => {
                    if !self.prev_cr {
                        self.text.push('\n');
                    }
                    self.prev_cr = false;
                }
                _ => {
                    self.prev_cr = false;
                    self.push_printable(c);
                }
            }
            return;
        }

        match c {
            '\x1b' => self.esc = EscState::Escape,
            '\r' => {
                self.prev_cr = true;
                submitted.push(self.take_text());
            }
            // Second half of \r\n, no duplicate submission; a lone \n is treated as submission
            '\n' => {
                if self.prev_cr {
                    self.prev_cr = false;
                    return;
                }
                submitted.push(self.take_text());
            }
            // Backspace: line editing, pops the trailing character (String::pop is char-unit safe)
            '\x7f' | '\x08' => {
                self.prev_cr = false;
                self.text.pop();
            }
            // Ctrl+C / Ctrl+U: abandon the current line, clear but do not submit
            '\x03' | '\x15' => {
                self.prev_cr = false;
                self.text.clear();
            }
            _ => {
                self.prev_cr = false;
                // Discard residual C0 control characters (Tab, other Ctrl key combinations); accumulate only printable characters and spaces
                if !is_control_or_del(c) {
                    self.push_printable(c);
                }
            }
        }
    }

    /// Second byte of an ESC sequence: determine the sequence type
    fn feed_escape(&mut self, c: char) {
        match c {
            // CSI (\x1b[) and SS3 (\x1bO) share parameter parsing: SS3 is terminated immediately by the final byte
            '[' | 'O' => {
                self.esc = EscState::Csi {
                    params: [0; 16],
                    count: 0,
                };
            }
            // OSC (\x1b]): 丢弃直到 BEL (\x07) 或 ST (\x1b\\)
            // （opencode/pi 等 TUI 的调色板/标题序列；粘贴内容可能携带 raw 字节）
            ']' => {
                self.esc = EscState::Osc;
            }
            // Common encoding for Shift+Enter / Option+Enter: restored as newline content, does not trigger submission
            '\r' | '\n' => {
                self.esc = EscState::Ground;
                self.prev_cr = false;
                self.push_printable('\n');
            }
            // Other two-character escape sequences (Alt+key, ESC =, etc.) are discarded as a whole: shortcut keys are not content
            _ => {
                // 连续 ESC（独立 ESC 键 + 随后另一 ESC 序列，如 \x1b 后接 \x1b[A 方向键）：
                // 必须保持 Escape 状态继续丢弃，否则序列头被丢弃后回到 Ground，
                // 下一个序列的 [A 会被当作普通字符累积进提交行
                // （任务日志出现 [A/ 垃圾描述，实测：ESC 键 + 上箭头 + / + Enter）
                if c != '\x1b' {
                    self.esc = EscState::Ground;
                }
            }
        }
    }

    /// OSC 内容字节：全部丢弃，直到 BEL 或 ESC（ST 起始）终止
    fn feed_osc(&mut self, c: char) {
        match c {
            '\x07' => self.esc = EscState::Ground,
            '\x1b' => self.esc = EscState::OscSt,
            _ => {}
        }
    }

    /// OSC 内 ESC 后的字节：`\\` 为 ST 终止，其余回 Osc 继续丢弃
    fn feed_osc_st(&mut self, c: char) {
        self.esc = if c == '\\' { EscState::Ground } else { EscState::Osc };
    }

    /// CSI / SS3 sequence body: collect parameter bytes, terminate on the final byte
    ///
    /// Returns `Some(c)` if the sequence is malformed and aborted, and the caller needs to re-feed that character as a normal character
    fn feed_csi(&mut self, c: char) -> Option<char> {
        let code = c as u32;
        match code {
            // Final byte (0x40-0x7e): terminate the sequence; if '~', recognize the bracketed paste markers 200~/201~ based on parameters
            0x40..=0x7e => {
                let params = self.take_csi_params();
                self.esc = EscState::Ground;
                if c == '~' {
                    match params.as_str() {
                        "200" => self.in_paste = true,
                        "201" => self.in_paste = false,
                        _ => {}
                    }
                }
                None
            }
            // Parameter / intermediate byte (0x20-0x3f): record only up to 16 bytes (sufficient for paste recognition)
            0x20..=0x3f => {
                if let EscState::Csi { params, count } = &mut self.esc {
                    if *count < params.len() {
                        params[*count] = code as u8;
                        *count += 1;
                    }
                }
                None
            }
            // A control character appearing in the parameter region indicates a malformed sequence: abort parsing, re-feed the current character as a normal character
            _ => {
                self.esc = EscState::Ground;
                Some(c)
            }
        }
    }

    /// Extracts the parameter-byte string currently collected by CSI and resets the state reference
    fn take_csi_params(&self) -> String {
        match self.esc {
            EscState::Csi { params, count } => {
                String::from_utf8_lossy(&params[..count]).into_owned()
            }
            // Theoretically unreachable (only called within Csi state); returns empty string as fallback
            _ => String::new(),
        }
    }

    /// Extracts the current buffer content as a submitted input line
    fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text)
    }

    /// Accumulates a printable character (with capacity limit to prevent unbounded growth of memory from an anomalous input stream)
    ///
    /// When the limit is reached, the leading half is discarded (trailing content is more valuable for logging);
    /// the truncation position must fall on a UTF-8 character boundary, otherwise split_off will panic
    fn push_printable(&mut self, c: char) {
        if self.text.len() >= MAX_SUBMITTED_LINE_BUFFER_BYTES {
            let mut start = MAX_SUBMITTED_LINE_BUFFER_BYTES / 2;
            while !self.text.is_char_boundary(start) {
                start += 1;
            }
            self.text = self.text.split_off(start);
        }
        self.text.push(c);
    }
}

/// Determines whether a character is a C0 control character or DEL (i.e., "shortcut key content," not normal input)
fn is_control_or_del(c: char) -> bool {
    (c as u32) < 0x20 || c == '\x7f'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds multiple chunks sequentially via the tracker and collects all submission results
    fn feed_chunks(tracker: &SubmittedLineTracker, session: &str, chunks: &[&str]) -> Vec<String> {
        chunks
            .iter()
            .flat_map(|chunk| tracker.feed(session, chunk))
            .collect()
    }

    #[test]
    fn test_plain_text_submit_on_cr() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["claude fix the bug\r"]);
        assert_eq!(out, vec!["claude fix the bug"]);
    }

    #[test]
    fn test_accumulate_across_chunks() {
        // xterm onData is submitted in keystroke chunks, and the buffer accumulates across chunks
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["hel", "lo wor", "ld\r"]);
        assert_eq!(out, vec!["hello world"]);
    }

    #[test]
    fn test_backspace_edits_buffer() {
        let t = SubmittedLineTracker::new();
        // \x7f / \x08 均为退格：弹出末尾字符（"xy\x08z" → "xz"）
        let out = feed_chunks(&t, "s1", &["abc\x7f\r", "xy\x08z\r"]);
        assert_eq!(out, vec!["ab", "xz"]);
    }

    #[test]
    fn test_backspace_on_empty_is_noop() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\x7f\x7fhi\r"]);
        assert_eq!(out, vec!["hi"]);
    }

    #[test]
    fn test_ctrl_c_clears_without_submit() {
        // After typing half of it, Ctrl+C abandons the current line; subsequent submissions contain only new content
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["abandon this\x03kept\r"]);
        assert_eq!(out, vec!["kept"]);
    }

    #[test]
    fn test_ctrl_u_clears_without_submit() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["whole line\x15tail\r"]);
        assert_eq!(out, vec!["tail"]);
    }

    #[test]
    fn test_control_chars_dropped() {
        // Tab (autocomplete trigger) and other Ctrl key combinations are not content
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\tb\x01c\x0b\r"]);
        assert_eq!(out, vec!["abc"]);
    }

    #[test]
    fn test_esc_csi_sequences_dropped() {
        // Arrow keys, Home/End, and other CSI sequences are discarded as a whole
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b[Ab\x1b[1~c\x1b[Hd\r"]);
        assert_eq!(out, vec!["abcd"]);
    }

    #[test]
    fn test_esc_key_then_arrow_csi_does_not_leak() {
        // 独立 ESC 键（\x1b）后紧跟方向键序列（\x1b[A）：连续 ESC 不应让 [A 泄漏为内容
        // （实测回归：任务日志出现 [A/ 垃圾描述 = ESC 键 + 上箭头 + / + Enter）
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\x1b", "\x1b[A/", "\r"]);
        assert_eq!(out, vec!["/"]);
    }

    #[test]
    fn test_osc_sequence_bel_terminated_dropped() {
        // OSC 序列（\x1b]4;0;rgb:...\x07，BEL 终止）整体丢弃
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b]4;0;rgb:2828/2c2c/3434\x07b\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_osc_sequence_st_terminated_dropped() {
        // OSC 序列（\x1b]4;0;rgb:...\x1b\\，ST 终止）整体丢弃
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b]4;0;rgb:2828/2c2c/3434\x1b\\b\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_osc_sequence_split_across_chunks() {
        // OSC 序列跨 chunk 到达（BEL 在后续 chunk）：状态机跨 chunk 保持，全部丢弃
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b]4;0;rgb:2828/2c2c/3434", "\x07b\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_osc_within_paste_dropped() {
        // 粘贴块内的 OSC 序列同样丢弃，粘贴内容不受影响
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\x1b[200~pre\x1b]4;0;rgb:2828/2c2c/3434\x07post\x1b[201~\r"]);
        assert_eq!(out, vec!["prepost"]);
    }

    #[test]
    fn test_esc_sequence_split_across_chunks() {
        // ESC and the sequence body arrive in different chunks, and the state machine must track across chunks
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b", "[Ab\x1b[", "3~c\r"]);
        assert_eq!(out, vec!["abc"]);
    }

    #[test]
    fn test_ss3_sequence_dropped() {
        // SS3 sequences (application mode for \x1bOA, etc.) are likewise discarded
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1bOAb\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_shift_enter_restored_as_newline() {
        // Shift+Enter is encoded as \x1b\r: restored as newline content, does not trigger submission
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["line1\x1b\rline2\r"]);
        assert_eq!(out, vec!["line1\nline2"]);
    }

    #[test]
    fn test_bracketed_paste_content_not_submit() {
        // Internal newlines within a paste block are content; only \r after the paste ends triggers submission
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\x1b[200~paste line1\r\npaste line2\x1b[201~\r"]);
        assert_eq!(out, vec!["paste line1\npaste line2"]);
    }

    #[test]
    fn test_bracketed_paste_split_across_chunks() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(
            &t,
            "s1",
            &["pre \x1b[200", "~in\r\npaste\x1b[2", "01~ post\r"],
        );
        assert_eq!(out, vec!["pre in\npaste post"]);
    }

    #[test]
    fn test_crlf_no_duplicate_submit() {
        // A WS client may send \r\n: only one submission should be produced, and the next line should not be swallowed
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["first\r\nsecond\r\n"]);
        assert_eq!(out, vec!["first", "second"]);
    }

    #[test]
    fn test_lone_lf_submits() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["via lf\n"]);
        assert_eq!(out, vec!["via lf"]);
    }

    #[test]
    fn test_empty_submit_still_notified() {
        // The host performs no semantic filtering: an empty-line Enter likewise triggers (whether to ignore is the plugin's business)
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\r", "real\r"]);
        assert_eq!(out, vec!["", "real"]);
    }

    #[test]
    fn test_unicode_and_multibyte_backspace() {
        // Backspace is char-unit safe against multi-byte UTF-8
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["修复 bu\x7fug 漏洞\x7f\x7f题\r"]);
        assert_eq!(out, vec!["修复 bug 题"]);
    }

    #[test]
    fn test_sessions_isolated() {
        let t = SubmittedLineTracker::new();
        t.feed("s1", "aaa");
        let out2 = t.feed("s2", "bbb\r");
        let out1 = t.feed("s1", "ccc\r");
        assert_eq!(out2, vec!["bbb"]);
        assert_eq!(out1, vec!["aaaccc"]);
    }

    #[test]
    fn test_remove_session_discards_unsubmitted() {
        let t = SubmittedLineTracker::new();
        t.feed("s1", "never submitted");
        t.remove_session("s1");
        // Buffer has been cleared: the next \r yields an empty submission, with no residual content
        let out = t.feed("s1", "\r");
        assert_eq!(out, vec![""]);
    }

    #[test]
    fn test_buffer_cap_bounds_memory() {
        // An abnormal input stream that never submits: the buffer self-truncates when it hits the upper bound
        let t = SubmittedLineTracker::new();
        let chunk = "a".repeat(64 * 1024);
        for _ in 0..8 {
            t.feed("s1", &chunk);
        }
        let map = t.buffers.lock().unwrap();
        let buf = map.get("s1").unwrap();
        assert!(buf.text.len() <= MAX_SUBMITTED_LINE_BUFFER_BYTES + 1);
    }

    #[test]
    fn test_malformed_csi_control_aborts_and_reprocesses() {
        // A control character appearing in CSI parameter region: terminates the sequence, and the character itself is reprocessed as a normal character (\r still triggers submission)
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["abc\x1b[1\r"]);
        assert_eq!(out, vec!["abc"]);
    }

    #[test]
    fn test_ctrl_c_from_mobile_ws_path_neutralized() {
        // Mobile WS path turns special keys into control bytes and sends them via write_input (see ADR 0001 deferred item):
        // the classification rules defensively neutralize them — they neither become content nor trigger submission
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["typed\x03", "ok\r"]);
        assert_eq!(out, vec!["ok"]);
    }
}
