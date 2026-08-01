//! Terminal Handler
//!
//! 终端扩展点 trait — Rust 插件可通过实现此 trait 拦截/修改终端输入输出
//!
//! 职责分界（见 ADR 0001）：
//! - `on_input` / `on_output`：同步修改管道（逐字节块，PTY 写入前）
//! - `on_input_submitted`：异步观察通知（提交输入行，不影响输入）

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

    /// 观察提交输入行（可选，默认忽略）
    ///
    /// 用户提交（回车触发）时收到由宿主重建的完整输入行。
    /// 纯观察回调：异步调用、出错不影响输入本身，返回值无语义。
    /// 与 `on_input`（逐块同步修改）互补，见 ADR 0001 的职责分界
    fn on_input_submitted(&self, session_id: &str, text: &str) {
        let _ = (session_id, text);
    }
}
