//! 宿主能力：终端写入

use super::HostError;

/// 终端输入注入
///
/// 向指定会话写入数据（移动端通过 WebSocket 转发到桌面端 PTY）。
/// 需要 `terminal:input` 权限。
pub trait HostTerminal {
    /// 向会话终端发送输入
    fn terminal_send(&self, session_id: &str, data: &str) -> Result<(), HostError>;
}
