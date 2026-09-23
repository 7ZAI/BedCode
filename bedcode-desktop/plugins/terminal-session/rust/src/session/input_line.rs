//! 提交行重建（会话引擎下沉 P2 核心，随 P1-b 一并迁入）
//!
//! 原宿主 `session/input_line.rs::SubmittedLineTracker`（2026-09-23 会话引擎下沉
//! P1-b 迁入本域）：从原始输入字节流重建「完整提交行」——任务域据此把用户敲
//! 回车的那一行当作任务记录。**纯状态机，无 OS 依赖**，native 单测全覆盖。
//!
//! 迁入原因：P1-b 起输入路径改为「前端 → 宿主窄转发层 → 本插件 `session-input`
//! → `host-pty.write`」，宿主 `SessionManager::write_input`（含提交行重建与
//! `on_input_submitted` 分发）不再是生产路径——提交行重建必须随写入管线归位
//! 插件，否则任务域静默断流。
//!
//! 重建规则（与宿主逐字一致，含 ADR 0001 的观察语义）：
//! - 可打印字符与空格累积入行缓冲；`\r` / `\n` 触发提交（`\r\n` 不重复提交；
//!   空提交同样通知——是否忽略是消费方的业务决策）
//! - `\x7f` / `\x08`（退格）弹出缓冲末尾字符（行编辑）
//! - `\x03`（Ctrl+C）/ `\x15`（Ctrl+U）清空缓冲但**不**触发提交
//! - 其余 C0 控制字符（Tab 等）丢弃
//! - ESC 转义序列（CSI / SS3 / 双字符序列）整体丢弃；`\x1b\r` / `\x1b\n`
//!   （常见 Shift+Enter 编码）还原为换行内容
//! - 括号粘贴块（`\x1b[200~` … `\x1b[201~`）内一切都是内容，内部换行不触发提交
//!   （`\r\n` 归一为 `\n`）
//!
//! 已知有损场景（接受）：TUI 历史回放文本不经过输入流；modifyOtherKeys 模式下
//! 多行输入可能被拆分。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 单会话行缓冲上限（字节）：异常输入流（粘贴大量文本）不无界增长内存。
/// 真源已从宿主 `MAX_SUBMITTED_LINE_BUFFER_BYTES` 迁移——本域是唯一消费方，
/// 常量随实现归位（宿主那份随 `session/` 整体退役，P4）。
const MAX_SUBMITTED_LINE_BUFFER_BYTES: usize = 256 * 1024;

/// 提交输入行重建器（宿主 `SubmittedLineTracker` 迁移版）
///
/// 每会话一条行缓冲：[`SubmittedLineTracker::feed`] 喂入输入块，返回该块内
/// 产生的全部完整提交行。`std::sync::Mutex`：临界区是纯内存状态机推进（无
/// await），按击键块频率调用；锁不跨分发持有，不会与其他锁死锁。
///
/// 内部 Arc 化、`Clone` 共享同一状态（任务域与停会话清理共用一份）。
#[derive(Clone, Default)]
pub struct SubmittedLineTracker {
    /// 每会话行缓冲（session_id → 状态机）
    buffers: Arc<Mutex<HashMap<String, LineBuffer>>>,
}

impl SubmittedLineTracker {
    /// 创建空重建器
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 喂入输入块，返回该块内产生的全部提交行（可能多条；空行同样返回，
    /// 是否忽略由消费方业务逻辑决定）
    pub fn feed(&self, session_id: &str, data: &str) -> Vec<String> {
        let mut map = self
            .buffers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let buffer = map.entry(session_id.to_string()).or_default();
        buffer.feed(data)
    }

    /// 移除该会话的缓冲（会话终止时调用）；未提交的残余内容直接丢弃
    /// （没提交就不是提交行，不补发）
    pub fn remove_session(&self, session_id: &str) {
        let mut map = self
            .buffers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.remove(session_id);
    }
}

/// ESC 转义序列解析状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EscState {
    /// 普通态
    #[default]
    Ground,
    /// 收到 `\x1b`，等待下一字节判定序列类型
    Escape,
    /// CSI / SS3 序列内：收集参数字节，等待终结字节
    Csi { params: [u8; 16], count: usize },
    /// OSC 序列内（`\x1b]...`）：丢弃直到 BEL 或 ST——OSC 内容不做行重建
    /// （调色板/标题等应用控制序列），完整丢弃
    Osc,
    /// OSC 内收到 `\x1b`：等待 ST 终止符 `\x1b\\`（其余字符回 Osc 继续丢弃）
    OscSt,
}

/// 每会话行缓冲（纯内存状态机）
#[derive(Default)]
struct LineBuffer {
    /// 已累积的普通输入内容
    text: String,
    /// ESC 序列解析状态
    esc: EscState,
    /// 是否处于括号粘贴块内
    in_paste: bool,
    /// 上一字符是 `\r`（抑制 `\r\n` 重复提交；粘贴内换行归一）
    prev_cr: bool,
}

impl LineBuffer {
    /// 喂入一块，返回产生的提交行列表
    fn feed(&mut self, data: &str) -> Vec<String> {
        let mut submitted = Vec::new();
        for c in data.chars() {
            match self.esc {
                EscState::Ground => self.feed_ground(c, &mut submitted),
                EscState::Escape => self.feed_escape(c),
                EscState::Csi { .. } => {
                    // 序列畸形中止时，当前字符按普通字符重喂
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

    /// 普通态字符分类（重建核心规则）
    fn feed_ground(&mut self, c: char, submitted: &mut Vec<String>) {
        if self.in_paste {
            // 粘贴块内一切都是内容；只有 ESC 前缀的 CSI 201~ 能结束粘贴
            if c == '\x1b' {
                self.esc = EscState::Escape;
                return;
            }
            match c {
                '\r' => {
                    self.text.push('\n');
                    self.prev_cr = true;
                }
                // \r\n 归一为单个换行
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
            // \r\n 后半段不重复提交；独立 \n 视作提交
            '\n' => {
                if self.prev_cr {
                    self.prev_cr = false;
                    return;
                }
                submitted.push(self.take_text());
            }
            // 退格：行编辑，弹出末尾字符（String::pop 按字符安全）
            '\x7f' | '\x08' => {
                self.prev_cr = false;
                self.text.pop();
            }
            // Ctrl+C / Ctrl+U：放弃当前行，清空但不提交
            '\x03' | '\x15' => {
                self.prev_cr = false;
                self.text.clear();
            }
            _ => {
                self.prev_cr = false;
                // 丢弃残余 C0 控制字符（Tab / 其它 Ctrl 组合）；只累积可打印字符与空格
                if !is_control_or_del(c) {
                    self.push_printable(c);
                }
            }
        }
    }

    /// ESC 序列第二字节：判定序列类型
    fn feed_escape(&mut self, c: char) {
        match c {
            // CSI（\x1b[）与 SS3（\x1bO）共享参数解析；SS3 同样以终结字节收尾
            '[' | 'O' => {
                self.esc = EscState::Csi {
                    params: [0; 16],
                    count: 0,
                };
            }
            // OSC（\x1b]）：丢弃直到 BEL（\x07）或 ST（\x1b\\）
            ']' => {
                self.esc = EscState::Osc;
            }
            // Shift+Enter / Option+Enter 常见编码：还原为换行内容，不触发提交
            '\r' | '\n' => {
                self.esc = EscState::Ground;
                self.prev_cr = false;
                self.push_printable('\n');
            }
            // 其它双字符转义序列（Alt+键、ESC = 等）整体丢弃：快捷键不是内容
            _ => {
                // 连续 ESC（独立 ESC 键 + 随后另一 ESC 序列，如 \x1b 后接 \x1b[A 方向键）：
                // 必须保持 Escape 状态继续丢弃，否则序列头被丢弃后回到 Ground，
                // 下一个序列的 [A 会被当作普通字符累积进提交行
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
        self.esc = if c == '\\' {
            EscState::Ground
        } else {
            EscState::Osc
        };
    }

    /// CSI / SS3 序列体：收集参数字节，终结字节收尾
    ///
    /// 返回 `Some(c)` = 序列畸形中止，调用方需把该字符按普通字符重喂
    fn feed_csi(&mut self, c: char) -> Option<char> {
        let code = c as u32;
        match code {
            // 终结字节（0x40-0x7e）：序列结束；'~' 时按参数识别括号粘贴标记 200~/201~
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
            // 参数 / 中间字节（0x20-0x3f）：只记录前 16 字节（粘贴识别足够）
            0x20..=0x3f => {
                if let EscState::Csi { params, count } = &mut self.esc {
                    if *count < params.len() {
                        params[*count] = code as u8;
                        *count += 1;
                    }
                }
                None
            }
            // 参数区出现控制字符 = 序列畸形：中止解析，当前字符按普通字符重喂
            _ => {
                self.esc = EscState::Ground;
                Some(c)
            }
        }
    }

    /// 取出 CSI 当前收集的参数串并复位状态引用
    fn take_csi_params(&self) -> String {
        match self.esc {
            EscState::Csi { params, count } => {
                String::from_utf8_lossy(&params[..count]).into_owned()
            }
            // 理论上不可达（只在 Csi 态内调用）；兜底空串
            _ => String::new(),
        }
    }

    /// 取当前缓冲内容为一条提交行
    fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text)
    }

    /// 累积可打印字符（带上限：异常输入流不无界增长内存）
    ///
    /// 达到上限时丢弃前段（尾段对日志更有价值）；截断位置必须落在 UTF-8
    /// 字符边界，否则 split_off 会 panic
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

/// 是否为 C0 控制字符或 DEL（「快捷键内容」，非普通输入）
fn is_control_or_del(c: char) -> bool {
    (c as u32) < 0x20 || c == '\x7f'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 通过 tracker 顺序喂多个块并收集全部提交结果
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
        // xterm onData 按击键块提交，缓冲跨块累积
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
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\tb\x01c\x0b\r"]);
        assert_eq!(out, vec!["abc"]);
    }

    #[test]
    fn test_esc_csi_sequences_dropped() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b[Ab\x1b[1~c\x1b[Hd\r"]);
        assert_eq!(out, vec!["abcd"]);
    }

    #[test]
    fn test_esc_key_then_arrow_csi_does_not_leak() {
        // 独立 ESC 键（\x1b）后紧跟方向键序列（\x1b[A）：连续 ESC 不应让 [A 泄漏为内容
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\x1b", "\x1b[A/", "\r"]);
        assert_eq!(out, vec!["/"]);
    }

    #[test]
    fn test_osc_sequence_bel_terminated_dropped() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b]4;0;rgb:2828/2c2c/3434\x07b\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_osc_sequence_st_terminated_dropped() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b]4;0;rgb:2828/2c2c/3434\x1b\\b\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_osc_sequence_split_across_chunks() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b]4;0;rgb:2828/2c2c/3434", "\x07b\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_osc_within_paste_dropped() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(
            &t,
            "s1",
            &["\x1b[200~pre\x1b]4;0;rgb:2828/2c2c/3434\x07post\x1b[201~\r"],
        );
        assert_eq!(out, vec!["prepost"]);
    }

    #[test]
    fn test_esc_sequence_split_across_chunks() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1b", "[Ab\x1b[", "3~c\r"]);
        assert_eq!(out, vec!["abc"]);
    }

    #[test]
    fn test_ss3_sequence_dropped() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["a\x1bOAb\r"]);
        assert_eq!(out, vec!["ab"]);
    }

    #[test]
    fn test_shift_enter_restored_as_newline() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["line1\x1b\rline2\r"]);
        assert_eq!(out, vec!["line1\nline2"]);
    }

    #[test]
    fn test_bracketed_paste_content_not_submit() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\x1b[200~paste line1\r\npaste line2\x1b[201~\r"]);
        assert_eq!(out, vec!["paste line1\npaste line2"]);
    }

    #[test]
    fn test_bracketed_paste_split_across_chunks() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["pre \x1b[200", "~in\r\npaste\x1b[2", "01~ post\r"]);
        assert_eq!(out, vec!["pre in\npaste post"]);
    }

    #[test]
    fn test_crlf_no_duplicate_submit() {
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
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["\r", "real\r"]);
        assert_eq!(out, vec!["", "real"]);
    }

    #[test]
    fn test_unicode_and_multibyte_backspace() {
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
        // 缓冲已清：下一次 \r 产生空提交，无残余内容
        let out = t.feed("s1", "\r");
        assert_eq!(out, vec![""]);
    }

    #[test]
    fn test_buffer_cap_bounds_memory() {
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
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["abc\x1b[1\r"]);
        assert_eq!(out, vec!["abc"]);
    }

    #[test]
    fn test_ctrl_c_from_mobile_ws_path_neutralized() {
        let t = SubmittedLineTracker::new();
        let out = feed_chunks(&t, "s1", &["typed\x03", "ok\r"]);
        assert_eq!(out, vec!["ok"]);
    }
}
