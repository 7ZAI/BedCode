//! Special Key Types
//!
//! 特殊键类型定义

use serde::{Deserialize, Serialize};

/// 特殊键
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecialKey {
    Tab,
    Enter,
    Escape,
    CtrlC,
    CtrlD,
    CtrlL,
    CtrlZ,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

impl SpecialKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpecialKey::Tab => "tab",
            SpecialKey::Enter => "enter",
            SpecialKey::Escape => "escape",
            SpecialKey::CtrlC => "ctrl_c",
            SpecialKey::CtrlD => "ctrl_d",
            SpecialKey::CtrlL => "ctrl_l",
            SpecialKey::CtrlZ => "ctrl_z",
            SpecialKey::ArrowUp => "arrow_up",
            SpecialKey::ArrowDown => "arrow_down",
            SpecialKey::ArrowLeft => "arrow_left",
            SpecialKey::ArrowRight => "arrow_right",
            SpecialKey::Backspace => "backspace",
            SpecialKey::Delete => "delete",
            SpecialKey::Home => "home",
            SpecialKey::End => "end",
            SpecialKey::PageUp => "pageup",
            SpecialKey::PageDown => "pagedown",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "tab" => Some(SpecialKey::Tab),
            "enter" => Some(SpecialKey::Enter),
            "escape" | "esc" => Some(SpecialKey::Escape),
            "ctrl_c" | "ctrlc" => Some(SpecialKey::CtrlC),
            "ctrl_d" | "ctrld" => Some(SpecialKey::CtrlD),
            "ctrl_l" | "ctrll" => Some(SpecialKey::CtrlL),
            "ctrl_z" | "ctrlz" => Some(SpecialKey::CtrlZ),
            "arrow_up" | "up" => Some(SpecialKey::ArrowUp),
            "arrow_down" | "down" => Some(SpecialKey::ArrowDown),
            "arrow_left" | "left" => Some(SpecialKey::ArrowLeft),
            "arrow_right" | "right" => Some(SpecialKey::ArrowRight),
            "backspace" => Some(SpecialKey::Backspace),
            _ => None,
        }
    }
}