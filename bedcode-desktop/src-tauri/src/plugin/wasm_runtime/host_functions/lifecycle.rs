//! 会话生命周期域 Host Functions（监听器注册）

use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_TERMINAL_OBSERVE;

/// 会话生命周期：注册监听器
///
/// 插件调用后，宿主为该插件创建一个 PluginLifecycleListener 并注册到 SessionManager。
/// 生命周期事件通过 `__bedcode_on_session_lifecycle` 导出函数回调，不走消息总线。
///
/// 参数：无（自动根据调用者的 plugin_id 注册）
/// 返回：0 成功，-1 失败
///
/// 通过 `PluginServices` trait 对象回调 PluginHost，避免 wasm_runtime → host 的模块循环依赖
pub(super) fn host_session_lifecycle_register(
    caller: wasmtime::Caller<'_, WasmPluginState>,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    // 两阶段初始化：PluginHost 构造完成后才注入 services，activate 可能早于注入
    let services = block_on_async(host_ctx.services());
    let Some(services) = services else {
        tracing::error!(
            "host_session_lifecycle_register: plugin services not initialized yet for '{}'",
            plugin_id
        );
        return -1;
    };

    let session_manager = host_ctx.session_manager_arc();
    services.register_session_lifecycle_listener(plugin_id.clone(), session_manager);

    tracing::info!("PluginLifecycleListener registered for '{}'", plugin_id);
    0
}

/// 会话输入：注册提交输入行监听器
///
/// 插件调用后，宿主为该插件创建一个 PluginInputListener 并注册到 SessionManager。
/// 用户提交输入（回车触发）时，宿主重建完整输入行后经
/// `__bedcode_on_input_submitted` 导出函数异步回调，不走消息总线。
///
/// 与生命周期注册不同，此注册有权限门禁：输入内容可能包含
/// 用户在终端键入的密码 / API key / token，观察能力需显式授权（见 ADR 0001）
///
/// 参数：无（自动根据调用者的 plugin_id 注册）
/// 返回：0 成功，-1 失败（含权限拒绝）
pub(super) fn host_session_input_register(
    caller: wasmtime::Caller<'_, WasmPluginState>,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    // 权限门禁：未声明 terminal:observe 的插件直接拒绝
    if !super::check_permission(
        &host_ctx,
        &plugin_id,
        PERMISSION_TERMINAL_OBSERVE,
        "host_session_input_register",
    ) {
        return -1;
    }

    let services = block_on_async(host_ctx.services());
    let Some(services) = services else {
        tracing::error!(
            "host_session_input_register: plugin services not initialized yet for '{}'",
            plugin_id
        );
        return -1;
    };

    let session_manager = host_ctx.session_manager_arc();
    services.register_session_input_listener(plugin_id.clone(), session_manager);

    tracing::info!("PluginInputListener registered for '{}'", plugin_id);
    0
}
