//! Terminal Handler
//!
//! 终端扩展点 trait — Rust 插件可通过实现此 trait 拦截/修改终端输入输出

/// 终端处理器 trait
pub trait TerminalHandler: Send + Sync + 'static {
    /// 处理终端输入
    ///
    /// 返回 `Some(modified_text)` 替换原始输入，返回 `None` 不修改
    fn on_input(&self, session_id: &str, text: &str) -> Option<String> {
        let _ = (session_id, text);
        None
    }

    /// 处理终端输出
    ///
    /// 返回 `Some(modified_data)` 替换原始输出，返回 `None` 不修改
    fn on_output(&self, session_id: &str, data: &str) -> Option<String> {
        let _ = (session_id, data);
        None
    }
}
