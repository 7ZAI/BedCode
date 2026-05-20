//! Status Detector
//!
//! 状态检测服务 - 检测会话状态（如等待输入）

/// 状态检测 trait
pub trait StatusDetector: Send + Sync {
    fn detect_waiting_input(&self, output: &str) -> bool;
}

/// 默认状态检测实现
pub struct DefaultStatusDetector;

impl DefaultStatusDetector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultStatusDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusDetector for DefaultStatusDetector {
    fn detect_waiting_input(&self, output: &str) -> bool {
        crate::shared::parser::detect_waiting_input(output)
    }
}