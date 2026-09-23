//! 终端按键组合 → 转义字节翻译（票 06 下沉自宿主 `enums/special_key.rs`）
//!
//! ADR 0022 D1 灰区收口：按键组合 → ANSI/ASCII 转义字节的翻译归**业务面**（本插件）。
//! 宿主 pty 引擎不再消费按键组合，只收插件算好的裸字节。本模块是宿主机
//! `to_pty_bytes` 的等价移植（语义逐字节一致），宿主侧已删除同逻辑。
//!
//! 翻译规则：
//! - Ctrl + 字母 → ASCII 控制符（Ctrl+A = 0x01）；Ctrl+2/3-7/8 → NUL/ESC/FS…/DEL
//! - Alt + 字母 → ESC + 字母；Alt+Shift+字母 → ESC + 大写字母
//! - 方向键 / 功能键 F1~F12 / Tab / 编辑键 → ANSI CSI 序列（xterm 修饰键协议）
//!
//! 与移动端 WS 线协议的合并语义：移动端仍按字符串发组合（如 `"ctrl+a"`），
//! 本插件自译自写，宿主不再代译。

/// 修饰键位标志
const MOD_CTRL: u8 = 0x01;
const MOD_SHIFT: u8 = 0x02;
const MOD_ALT: u8 = 0x04;

// ==================== KeyCode ====================

/// 键名（与宿主 `enums::special_key::KeyCode` 形状一致）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    /// 字母 a-z 或数字 0-9 或空格
    Char(char),
    /// 方向键
    Up,
    Down,
    Left,
    Right,
    /// 编辑键
    Tab,
    Enter,
    Escape,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    /// 功能键 F1~F12
    F(u8),
}

impl KeyCode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "up" | "arrow_up" => Some(KeyCode::Up),
            "down" | "arrow_down" => Some(KeyCode::Down),
            "left" | "arrow_left" => Some(KeyCode::Left),
            "right" | "arrow_right" => Some(KeyCode::Right),
            "tab" => Some(KeyCode::Tab),
            "enter" => Some(KeyCode::Enter),
            "escape" | "esc" => Some(KeyCode::Escape),
            "backspace" | "del" => Some(KeyCode::Backspace),
            "delete" => Some(KeyCode::Delete),
            "home" => Some(KeyCode::Home),
            "end" => Some(KeyCode::End),
            "pageup" | "page_up" => Some(KeyCode::PageUp),
            "pagedown" | "page_down" => Some(KeyCode::PageDown),
            "insert" => Some(KeyCode::Insert),
            "space" => Some(KeyCode::Char(' ')),
            _ => {
                // F1~F12
                if let Some(rest) = s.strip_prefix('f').or_else(|| s.strip_prefix('F')) {
                    if let Ok(n) = rest.parse::<u8>() {
                        if (1..=12).contains(&n) {
                            return Some(KeyCode::F(n));
                        }
                    }
                }
                // 单字母 / 数字
                let chars: Vec<char> = s.chars().collect();
                if chars.len() == 1 {
                    let c = chars[0];
                    if c.is_ascii_lowercase() || c.is_ascii_digit() {
                        return Some(KeyCode::Char(c));
                    }
                    if c.is_ascii_uppercase() {
                        return Some(KeyCode::Char(c.to_ascii_lowercase()));
                    }
                }
                None
            }
        }
    }
}

// ==================== KeyCombo ====================

/// 按键组合 = 修饰键 + 键名
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    modifiers: u8,
    key: KeyCode,
}

impl KeyCombo {
    fn ctrl(&self) -> bool {
        self.modifiers & MOD_CTRL != 0
    }
    fn shift(&self) -> bool {
        self.modifiers & MOD_SHIFT != 0
    }
    fn alt(&self) -> bool {
        self.modifiers & MOD_ALT != 0
    }

    /// 计算 xterm 修饰键编号：Shift=2, Alt=3, Alt+Shift=4, Ctrl=5, Ctrl+Shift=6,
    /// Ctrl+Alt=7, Ctrl+Alt+Shift=8；无修饰返回 None。
    fn modifier_number(&self) -> Option<u8> {
        match (self.ctrl(), self.shift(), self.alt()) {
            (false, true, false) => Some(2),
            (false, false, true) => Some(3),
            (false, true, true) => Some(4),
            (true, false, false) => Some(5),
            (true, true, false) => Some(6),
            (true, false, true) => Some(7),
            (true, true, true) => Some(8),
            (false, false, false) => None,
        }
    }

    fn to_pty_bytes(&self) -> Option<Vec<u8>> {
        match &self.key {
            // Ctrl + 字母/数字/空格
            KeyCode::Char(c) if self.ctrl() && !self.shift() && !self.alt() => ctrl_char_bytes(*c),
            // Alt + 字母 = ESC + 字母
            KeyCode::Char(c) if self.alt() && !self.ctrl() && !self.shift() => {
                let mut b = vec![0x1b];
                b.push(*c as u8);
                Some(b)
            }
            // Alt+Shift+字母 = ESC + 大写字母
            KeyCode::Char(c) if self.alt() && self.shift() && !self.ctrl() => {
                let mut b = vec![0x1b];
                b.push(c.to_ascii_uppercase() as u8);
                Some(b)
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => arrow_key_bytes(&self.key, self.modifier_number()),
            KeyCode::F(n) => function_key_bytes(*n, self.modifier_number()),
            KeyCode::Tab => tab_key_bytes(self, self.modifier_number()),
            // Delete/Insert/PageUp/PageDown/Home/End
            KeyCode::Delete
            | KeyCode::Insert
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => csi_edit_key_bytes(&self.key, self.modifier_number()),
            KeyCode::Enter if !self.ctrl() && !self.shift() && !self.alt() => Some(vec![0x0d]),
            KeyCode::Escape if !self.ctrl() && !self.shift() && !self.alt() => Some(vec![0x1b]),
            KeyCode::Backspace if !self.ctrl() && !self.shift() && !self.alt() => Some(vec![0x7f]),
            KeyCode::Char(c) if !self.ctrl() && !self.alt() && !self.shift() => Some(vec![*c as u8]),
            KeyCode::Char(c) if !self.ctrl() && !self.alt() && self.shift() => Some(vec![c.to_ascii_uppercase() as u8]),
            _ => None,
        }
    }
}

// ==================== 翻译实施 ====================

/// Ctrl + 字母/数字 → ASCII 控制字符
fn ctrl_char_bytes(c: char) -> Option<Vec<u8>> {
    if c.is_ascii_lowercase() {
        let byte = (c as u8) - b'a' + 1;
        Some(vec![byte])
    } else if c.is_ascii_digit() {
        let byte = match c {
            '2' => 0x00,
            '3' => 0x1b,
            '4' => 0x1c,
            '5' => 0x1d,
            '6' => 0x1e,
            '7' => 0x1f,
            '8' => 0x7f,
            _ => return None,
        };
        Some(vec![byte])
    } else if c == ' ' {
        Some(vec![0x00])
    } else {
        None
    }
}

/// 方向键 + 修饰键 → ANSI 转义序列
fn arrow_key_bytes(key: &KeyCode, modifier_number: Option<u8>) -> Option<Vec<u8>> {
    let dir = match key {
        KeyCode::Up => 'A',
        KeyCode::Down => 'B',
        KeyCode::Right => 'C',
        KeyCode::Left => 'D',
        _ => return None,
    };
    let bytes = match modifier_number {
        None => format!("\x1b[{}", dir),
        Some(m) => format!("\x1b[1;{}{}", m, dir),
    };
    Some(bytes.into_bytes())
}

/// Tab + 修饰键 → 0x09 / \x1b[Z / \x1b[1;mI
fn tab_key_bytes(combo: &KeyCombo, modifier_number: Option<u8>) -> Option<Vec<u8>> {
    if !combo.ctrl() && !combo.shift() && !combo.alt() {
        return Some(vec![0x09]);
    }
    if combo.shift() && !combo.ctrl() && !combo.alt() {
        return Some("\x1b[Z".as_bytes().to_vec());
    }
    let m = modifier_number?;
    Some(format!("\x1b[1;{}I", m).into_bytes())
}

/// CSI 编辑键 → \x1b[{n}~ / \x1b[{n};m~ / \x1b[H / \x1b[1;mH
fn csi_edit_key_bytes(key: &KeyCode, modifier_number: Option<u8>) -> Option<Vec<u8>> {
    match key {
        KeyCode::Delete => csi_tilde_key(3, modifier_number),
        KeyCode::Insert => csi_tilde_key(2, modifier_number),
        KeyCode::PageUp => csi_tilde_key(5, modifier_number),
        KeyCode::PageDown => csi_tilde_key(6, modifier_number),
        KeyCode::Home => csi_letter_key('H', modifier_number),
        KeyCode::End => csi_letter_key('F', modifier_number),
        _ => None,
    }
}

fn csi_tilde_key(n: u8, modifier_number: Option<u8>) -> Option<Vec<u8>> {
    let bytes = match modifier_number {
        None => format!("\x1b[{}~", n),
        Some(m) => format!("\x1b[{};{}~", n, m),
    };
    Some(bytes.into_bytes())
}

fn csi_letter_key(final_char: char, modifier_number: Option<u8>) -> Option<Vec<u8>> {
    let bytes = match modifier_number {
        None => format!("\x1b[{}", final_char),
        Some(m) => format!("\x1b[1;{}{}", m, final_char),
    };
    Some(bytes.into_bytes())
}

/// 功能键 F1~F12 → ANSI 转义序列
fn function_key_bytes(n: u8, modifier_number: Option<u8>) -> Option<Vec<u8>> {
    if !(1..=12).contains(&n) {
        return None;
    }
    if let Some(m) = modifier_number {
        if (1..=4).contains(&n) {
            let pp = match n {
                1 => 'P',
                2 => 'Q',
                3 => 'R',
                4 => 'S',
                _ => return None,
            };
            return Some(format!("\x1b[1;{}{}", m, pp).into_bytes());
        }
        let code = match n {
            5 => 15,
            6 => 17,
            7 => 18,
            8 => 19,
            9 => 20,
            10 => 21,
            11 => 23,
            12 => 24,
            _ => return None,
        };
        return Some(format!("\x1b[{};{}~", code, m).into_bytes());
    }
    let bytes = match n {
        1 => "\x1bOP".as_bytes().to_vec(),
        2 => "\x1bOQ".as_bytes().to_vec(),
        3 => "\x1bOR".as_bytes().to_vec(),
        4 => "\x1bOS".as_bytes().to_vec(),
        5 => "\x1b[15~".as_bytes().to_vec(),
        6 => "\x1b[17~".as_bytes().to_vec(),
        7 => "\x1b[18~".as_bytes().to_vec(),
        8 => "\x1b[19~".as_bytes().to_vec(),
        9 => "\x1b[20~".as_bytes().to_vec(),
        10 => "\x1b[21~".as_bytes().to_vec(),
        11 => "\x1b[23~".as_bytes().to_vec(),
        12 => "\x1b[24~".as_bytes().to_vec(),
        _ => return None,
    };
    Some(bytes)
}

// ==================== 组合解析 ====================

/// 解析按键组合字符串（新格式 `"ctrl+a"` / 旧格式 `"ctrl_a"` / 无修饰 `"enter"`）→ 转义字节。
/// 无法解析或不支持的组合返回 `None`（fail-visible，调用方决定丢弃或告警）。
pub fn special_key_to_pty_bytes(key: &str) -> Option<Vec<u8>> {
    combo_from_str(key)?.to_pty_bytes()
}

fn combo_from_str(s: &str) -> Option<KeyCombo> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split('+').collect();
    if parts.len() > 1 {
        let key_str = parts.last()?;
        let key = KeyCode::from_str(key_str)?;
        let mut modifiers: u8 = 0;
        for &part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= MOD_CTRL,
                "shift" => modifiers |= MOD_SHIFT,
                "alt" | "meta" => modifiers |= MOD_ALT,
                _ => return None,
            }
        }
        return Some(KeyCombo { modifiers, key });
    }
    // 旧格式：ctrl_c / ctrlz / arrow_up
    if let Some(rest) = s.strip_prefix("ctrl_").or_else(|| s.strip_prefix("ctrl")) {
        if rest.is_empty() {
            return None;
        }
        let key = KeyCode::from_str(rest)?;
        return Some(KeyCombo { modifiers: MOD_CTRL, key });
    }
    let key = KeyCode::from_str(s)?;
    Some(KeyCombo { modifiers: 0, key })
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// ASCII 控制字符（Ctrl 字母）
    #[test]
    fn ctrl_letter_control_codes() {
        let cases = [
            ("ctrl+a", 0x01),
            ("ctrl+c", 0x03),
            ("ctrl+d", 0x04),
            ("ctrl+z", 0x1a),
            ("ctrl+space", 0x00),
        ];
        for (s, expected) in cases {
            assert_eq!(special_key_to_pty_bytes(s), Some(vec![expected]), "case {s}");
        }
    }

    /// Ctrl 数字
    #[test]
    fn ctrl_digit_control_codes() {
        assert_eq!(special_key_to_pty_bytes("ctrl+2"), Some(vec![0x00]));
        assert_eq!(special_key_to_pty_bytes("ctrl+8"), Some(vec![0x7f]));
    }

    /// 旧格式兼容
    #[test]
    fn legacy_format_compat() {
        assert_eq!(special_key_to_pty_bytes("ctrl_c"), Some(vec![0x03]));
        assert_eq!(special_key_to_pty_bytes("ctrl+z"), Some(vec![0x1a]));
        assert_eq!(special_key_to_pty_bytes("arrow_up"), Some(vec![0x1b, b'[', b'A']));
    }

    /// Alt / Alt+Shift
    #[test]
    fn alt_combos() {
        assert_eq!(special_key_to_pty_bytes("alt+f"), Some(vec![0x1b, b'f']));
        assert_eq!(special_key_to_pty_bytes("alt+shift+f"), Some(vec![0x1b, b'F']));
    }

    /// 基础编辑键
    #[test]
    fn base_edit_keys() {
        assert_eq!(special_key_to_pty_bytes("enter"), Some(vec![0x0d]));
        assert_eq!(special_key_to_pty_bytes("tab"), Some(vec![0x09]));
        assert_eq!(special_key_to_pty_bytes("escape"), Some(vec![0x1b]));
        assert_eq!(special_key_to_pty_bytes("backspace"), Some(vec![0x7f]));
    }

    /// 方向键 + 修饰键
    #[test]
    fn arrow_keys_with_modifiers() {
        assert_eq!(special_key_to_pty_bytes("up"), Some(vec![0x1b, b'[', b'A']));
        assert_eq!(special_key_to_pty_bytes("shift+up"), Some(b"\x1b[1;2A".to_vec()));
        assert_eq!(special_key_to_pty_bytes("ctrl+up"), Some(b"\x1b[1;5A".to_vec()));
        assert_eq!(special_key_to_pty_bytes("alt+up"), Some(b"\x1b[1;3A".to_vec()));
    }

    /// Shift+Tab 与传统序列
    #[test]
    fn tab_with_shift() {
        assert_eq!(special_key_to_pty_bytes("shift+tab"), Some(b"\x1b[Z".to_vec()));
        assert_eq!(special_key_to_pty_bytes("ctrl+tab"), Some(b"\x1b[1;5I".to_vec()));
    }

    /// 功能键无修饰 / 带修饰
    #[test]
    fn function_keys() {
        assert_eq!(special_key_to_pty_bytes("f1"), Some(b"\x1bOP".to_vec()));
        assert_eq!(special_key_to_pty_bytes("f4"), Some(b"\x1bOS".to_vec()));
        assert_eq!(special_key_to_pty_bytes("f5"), Some(b"\x1b[15~".to_vec()));
        assert_eq!(special_key_to_pty_bytes("f12"), Some(b"\x1b[24~".to_vec()));
        assert_eq!(special_key_to_pty_bytes("ctrl+f5"), Some(b"\x1b[15;5~".to_vec()));
        assert_eq!(special_key_to_pty_bytes("shift+f1"), Some(b"\x1b[1;2P".to_vec()));
    }

    /// 编辑键 CSI 序列
    #[test]
    fn csi_edit_keys() {
        assert_eq!(special_key_to_pty_bytes("delete"), Some(b"\x1b[3~".to_vec()));
        assert_eq!(special_key_to_pty_bytes("home"), Some(b"\x1b[H".to_vec()));
        assert_eq!(special_key_to_pty_bytes("end"), Some(b"\x1b[F".to_vec()));
        assert_eq!(special_key_to_pty_bytes("ctrl+delete"), Some(b"\x1b[3;5~".to_vec()));
        assert_eq!(special_key_to_pty_bytes("shift+home"), Some(b"\x1b[1;2H".to_vec()));
        assert_eq!(special_key_to_pty_bytes("pageup"), Some(b"\x1b[5~".to_vec()));
    }

    /// 普通字符（无修饰 / Shift）
    #[test]
    fn plain_chars() {
        assert_eq!(special_key_to_pty_bytes("f"), Some(vec![b'f']));
        assert_eq!(special_key_to_pty_bytes("shift+f"), Some(vec![b'F']));
        assert_eq!(special_key_to_pty_bytes("F"), Some(vec![b'f']));
    }

    /// fail-visible：非法 / 未知组合 → None
    #[test]
    fn unsupported_or_invalid_rejected() {
        assert_eq!(special_key_to_pty_bytes(""), None);
        assert_eq!(special_key_to_pty_bytes("invalid_key"), None);
        assert_eq!(special_key_to_pty_bytes("ctrl+"), None);
        assert_eq!(special_key_to_pty_bytes("ctrl+13"), None);
        assert_eq!(special_key_to_pty_bytes("ctrl+bogus"), None);
    }
}