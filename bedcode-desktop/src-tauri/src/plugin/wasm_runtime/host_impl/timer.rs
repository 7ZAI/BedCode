//! 定时器域宿主实现（v6，ADR 0003）
//!
//! 宿主侧只负责"到点调用插件 command"，具体到点做什么、幂等与否归插件。

use crate::plugin::permission::PERMISSION_TIMER;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 定时器最小间隔（秒）——防止插件误传 0 导致空转循环
const MIN_TIMER_INTERVAL_SECS: u64 = 1;

/// 注册周期回调（权限 + 参数校验 + services 注入）
///
/// 插件调用后，宿主以 tokio interval 按间隔调用插件指定 command，
/// 参数附带 `now_ms`（Unix 毫秒）与 `now_utc`（UTC "YYYY-MM-DD HH:MM:SS"，
/// 与 SQLite datetime('now') 同格式）。重复注册替换已有定时器。
pub(crate) fn timer_register(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    interval_secs: u64,
    command: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TIMER, "host_timer_register") {
        return Err("permission denied".to_string());
    }
    if command.is_empty() {
        return Err("timer error: empty command name".to_string());
    }
    let interval = interval_secs.max(MIN_TIMER_INTERVAL_SECS);
    // 两阶段初始化：PluginHost 构造完成后才注入 services
    let services = block_on_async(host_ctx.services()).ok_or_else(|| {
        format!("timer error: plugin services not initialized yet for '{}'", plugin_id)
    })?;
    services.register_plugin_timer(plugin_id.to_string(), interval, command.to_string());
    tracing::info!(
        "Plugin timer registered for '{}': interval={}s command={}",
        plugin_id, interval, command
    );
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};

    /// 无 timer:schedule 权限：注册被权限门禁拒绝
    #[test]
    fn timer_register_permission_denied() {
        let ctx = build_host_ctx();
        let err = timer_register(&ctx, "test-plugin", 5, "my.command").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 空 command 名：权限通过后参数校验拒绝（防注册无效定时器空转）
    #[test]
    fn timer_register_empty_command_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_TIMER]);
        let err = timer_register(&ctx, "test-plugin", 5, "").unwrap_err();
        assert_eq!(err, "timer error: empty command name");
    }

    /// 参数合法但 services 未注入（两阶段初始化完成前）：明确报错而非静默忽略
    #[tokio::test]
    async fn timer_register_services_not_ready() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_TIMER]);
        let err = timer_register(&ctx, "test-plugin", 5, "my.command").unwrap_err();
        assert!(err.contains("not initialized yet"), "got: {}", err);
    }

    // 间隔钳制（interval_secs.max(MIN_TIMER_INTERVAL_SECS)）生效于 services 注入后：
    // 注册的定时器句柄存于 PluginHost，测试环境无 PluginHost 无法观测最终间隔，
    // 交由集成/手动测试覆盖；0 秒钳制为 1 秒的语义由常量注释保证
}
