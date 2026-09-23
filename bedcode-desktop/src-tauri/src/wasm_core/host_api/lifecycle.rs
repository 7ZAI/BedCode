//! 会话生命周期域宿主实现（**票 03 起为退役占位**）
//!
//! 本模块原有两条注册面——`host-session.lifecycle-register`（会话生命周期回调）
//! 与 `host-session.input-register`（用户提交输入行回调，需 `terminal:observe`）。
//! 它们的宿主侧机制（`SessionManager` 的注册表 + 派发点、`manager/host/listeners.rs`
//! 的插件监听器、内核逐帧输入修饰链）随票 03 **整体退役**：P1-b 会话真源下沉后
//! 这两条通道的生产流量已归零（创建/终态由 `com.bedcode.terminal-session` 自驱，
//! 提交行重建在该插件内完成），留着「代码在、永远不触发」正是下一处断链的种子。
//!
//! **为什么这里还留着两个函数**：WIT 里 `host-session.lifecycle-register` /
//! `input-register` 的删除属 interface 级破坏性变更，按契约硬约束与 ABI bump
//! 统一在票 10 定稿（一次 bump 只给插件作者一次重建）。因此在票 10 之前，这两个
//! host function 退化为**显性失败的退役占位**：旧产物（≤ ABI v25）调用时拿到
//! 点明原因的错误（哪个能力没了 / 要按哪个版本重建），而不是静默成功。
//! 新产物不再调用它们（插件 activate 已摘除两条注册调用）。

use crate::wasm_core::manager::runtime::WasmHostContext;

/// 会话生命周期注册面（退役占位）——见模块文档
pub(crate) fn session_lifecycle_register(_host_ctx: &WasmHostContext, plugin_id: &str) -> Result<(), String> {
    tracing::warn!(
        plugin_id = %plugin_id,
        "host-session.lifecycle-register 已退役（票 03）：宿主不再派发会话生命周期回调"
    );
    Err(RETIRED_LIFECYCLE_REGISTER.to_string())
}

/// 提交输入行注册面（退役占位）——见模块文档
pub(crate) fn session_input_register(_host_ctx: &WasmHostContext, plugin_id: &str) -> Result<(), String> {
    tracing::warn!(
        plugin_id = %plugin_id,
        "host-session.input-register 已退役（票 03）：宿主不再派发提交输入行回调"
    );
    Err(RETIRED_INPUT_REGISTER.to_string())
}

/// 生命周期注册面的退役文案（调用方可能直接转述给用户，故写全「原因 + 动作」）
const RETIRED_LIFECYCLE_REGISTER: &str = "host-session.lifecycle-register is retired (session lifecycle \
     dispatch moved into the com.bedcode.terminal-session plugin); rebuild the plugin artifact with the \
     current plugin SDK";

/// 输入注册面的退役文案
const RETIRED_INPUT_REGISTER: &str = "host-session.input-register is retired (submitted-line observation \
     moved into the com.bedcode.terminal-session plugin); rebuild the plugin artifact with the current \
     plugin SDK";

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};
    use crate::wasm_core::permission::{PERMISSION_SESSION_READ, PERMISSION_TERMINAL_OBSERVE};

    /// 退役占位必须显性失败（而不是静默 `Ok`），且文案点明「重建产物」这一动作。
    ///
    /// 反向锁：若有人把占位改回 `Ok(())`（「先放个空实现以后再说」），本用例转红。
    #[test]
    fn retired_register_surfaces_fail_visibly_with_rebuild_hint() {
        let ctx = build_host_ctx();
        // 即便把历史权限位都授予，也不再有可注册的东西——失败原因不是权限
        grant_permissions(
            &ctx,
            "test-plugin",
            &[PERMISSION_SESSION_READ, PERMISSION_TERMINAL_OBSERVE],
        );

        let e1 = session_lifecycle_register(&ctx, "test-plugin").unwrap_err();
        assert!(e1.contains("retired"), "必须点明「已退役」, got: {e1}");
        assert!(
            e1.contains("rebuild the plugin artifact"),
            "必须给出重建产物的动作指引, got: {e1}"
        );

        let e2 = session_input_register(&ctx, "test-plugin").unwrap_err();
        assert!(e2.contains("retired"), "必须点明「已退役」, got: {e2}");
        assert!(
            e2.contains("rebuild the plugin artifact"),
            "必须给出重建产物的动作指引, got: {e2}"
        );
        // 两条面各自点名，不共用一句兜底文案（否则调用方分不清是哪条没了）
        assert_ne!(e1, e2, "两条退役面的错误必须各自点名");
    }
}
