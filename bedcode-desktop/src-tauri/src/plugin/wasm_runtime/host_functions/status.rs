//! 插件状态域 Host Functions
//!
//! 提供插件向宿主上报自身状态的能力。目前仅支持错误标记：
//! 插件检测到配置失败（如 hooks 脚本拷贝失败、settings.json 写入失败）时，
//! 调用 `host_mark_plugin_error` 通知宿主，宿主仅弹窗提示前端，不改插件状态。

use super::memory::read_wasm_string_consume;
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use wasmtime::Caller;

/// 标记插件为错误状态
///
/// 参数：(err_ptr, err_len) — 错误描述
/// 无返回值。宿主仅 emit `plugin:error` 事件通知前端弹窗提示，
/// 不改变插件激活状态（保持激活，会话照常运行）。
pub(super) fn host_mark_plugin_error(
    mut caller: Caller<'_, WasmPluginState>,
    err_ptr: u32,
    err_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let error = read_wasm_string_consume(&mut caller, err_ptr, err_len).unwrap_or_default();
    let host_ctx = caller.data().host_ctx.clone();

    block_on_async(async move {
        match host_ctx.services().await {
            Some(services) => services.mark_plugin_error(plugin_id, error),
            None => tracing::error!(
                "[PluginHost] host_mark_plugin_error: plugin services not initialized"
            ),
        }
    });
}
