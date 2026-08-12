//! 终端域宿主实现（PTY 输入注入）
//!
//! `terminal_send`（权限校验 + 写入）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。

use crate::plugin::permission::PERMISSION_TERMINAL_INPUT;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 向指定会话注入终端输入（权限校验 + 写入）
pub(crate) fn terminal_send(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
    data: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TERMINAL_INPUT, "host_terminal_send") {
        return Err("permission denied".to_string());
    }

    let sm = host_ctx.session_manager.clone();
    block_on_async(sm.write_input(session_id, data))
        .map_err(|e| format!("write failed: {}", e))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::build_host_ctx;

    /// 无 terminal:input 权限：终端输入注入被权限门禁拒绝
    #[test]
    fn terminal_send_permission_denied() {
        let ctx = build_host_ctx();
        let err = terminal_send(&ctx, "test-plugin", "session-1", "echo hi").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    // 成功路径（write_input → PTY 写入）依赖 AppContext 全局单例与真实 PTY 会话：
    // AppContext 仅应用启动时初始化（测试环境未初始化会 panic），且无 PTY 运行时
    // 会话不可用 —— 交由集成/手动测试覆盖，此处只测可独立验证的权限门禁
}
