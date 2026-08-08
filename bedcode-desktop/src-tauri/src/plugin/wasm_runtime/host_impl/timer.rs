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
