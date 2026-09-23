//! 会话生命周期域宿主实现（监听器注册）

use crate::wasm_core::manager::runtime::{block_on_async, WasmHostContext};
use bedcode_plugin_api::permission::{PERMISSION_SESSION_READ, PERMISSION_TERMINAL_OBSERVE};

/// 注册会话生命周期监听器（权限门禁：session:read；无参数，按调用者 plugin_id 注册）
///
/// 生命周期事件通过导出函数回调（组件形态为 `events.on-session-lifecycle`），
/// 不走消息总线。通过 `PluginServices` trait 对象回调 PluginHost，
/// 避免 wasm_runtime → host 的模块循环依赖。
///
/// 门禁理由（票 04，P0-3）：生命周期事件携带全部会话 id / 名称 / 工作目录，
/// 此前注册**零权限要求**——任意已激活插件装上监听器就能枚举全场会话，
/// 再配合 `terminal:input` 即为「向用户正在用的终端注入命令」的攻击前段。
/// 与输入监听的分档保持：生命周期是元数据（`session:read`），
/// 提交输入行是明文凭据面（`terminal:observe`，见 ADR 0001）。
pub(crate) fn session_lifecycle_register(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_SESSION_READ,
        "host_session_lifecycle_register",
    ) {
        return Err("permission denied".to_string());
    }
    // 两阶段初始化：PluginHost 构造完成后才注入 services，activate 可能早于注入
    let services = block_on_async(host_ctx.services())
        .ok_or_else(|| format!("session error: plugin services not initialized yet for '{}'", plugin_id))?;
    let session_manager = host_ctx.session_manager_arc();
    services.register_session_lifecycle_listener(plugin_id.to_string(), session_manager);
    Ok(())
}

/// 注册提交输入行监听器（权限门禁：terminal:observe）
///
/// 用户提交输入（回车触发）时，宿主重建完整输入行后经导出函数异步回调
/// （组件形态为 `events.on-input-submitted`），不走消息总线。
/// 输入内容可能包含用户在终端键入的密码 / API key / token，
/// 观察能力需显式授权（见 ADR 0001）。
pub(crate) fn session_input_register(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_TERMINAL_OBSERVE,
        "host_session_input_register",
    ) {
        return Err("permission denied".to_string());
    }
    let services = block_on_async(host_ctx.services())
        .ok_or_else(|| format!("session error: plugin services not initialized yet for '{}'", plugin_id))?;
    let session_manager = host_ctx.session_manager_arc();
    services.register_session_input_listener(plugin_id.to_string(), session_manager);
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    /// 无 terminal:observe 权限：输入监听注册被权限门禁拒绝
    #[test]
    fn session_input_register_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_input_register(&ctx, "test-plugin").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 session:read 权限：生命周期监听注册被拒（票 04 补的门）
    #[test]
    fn session_lifecycle_register_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_lifecycle_register(&ctx, "test-plugin").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 权限通过但 services 未注入（两阶段初始化完成前）：生命周期监听注册明确报错
    ///
    /// 成功路径需要 PluginHost 注入的 PluginServices（真实插件宿主），
    /// 测试环境无 PluginHost，此处验证两阶段初始化的降级行为
    #[tokio::test]
    async fn session_lifecycle_register_services_not_ready() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_SESSION_READ]);
        let err = session_lifecycle_register(&ctx, "test-plugin").unwrap_err();
        assert!(err.contains("not initialized yet"), "got: {}", err);
    }

    /// 权限通过但 services 未注入：输入监听注册同样报错（非静默忽略）
    #[tokio::test]
    async fn session_input_register_services_not_ready() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_TERMINAL_OBSERVE]);
        let err = session_input_register(&ctx, "test-plugin").unwrap_err();
        assert!(err.contains("not initialized yet"), "got: {}", err);
    }
}
