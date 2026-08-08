//! 插件状态域宿主实现
//!
//! 提供插件向宿主上报自身状态的能力。目前仅支持错误标记：
//! 插件检测到配置失败（如 hooks 脚本拷贝失败、settings.json 写入失败）时，
//! 通知宿主，宿主仅弹窗提示前端，不改插件状态。

use crate::plugin::wasm_runtime::block_on_async;

/// 标记插件为错误状态
///
/// 宿主仅 emit `plugin:error` 事件通知前端弹窗提示，不改变插件激活状态。
pub(crate) fn mark_plugin_error(host_ctx: &crate::plugin::wasm_runtime::WasmHostContext, plugin_id: String, error: String) {
    block_on_async(async move {
        match host_ctx.services().await {
            Some(services) => services.mark_plugin_error(plugin_id, error),
            None => tracing::error!(
                "[PluginHost] mark_plugin_error: plugin services not initialized"
            ),
        }
    });
}
