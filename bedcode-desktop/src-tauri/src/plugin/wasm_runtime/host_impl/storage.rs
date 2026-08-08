//! 存储域宿主实现（插件键值存储，按 plugin_id 隔离）
//!
//! `storage_get/set/delete`（权限校验 + 服务调用）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。

use crate::plugin::permission::PERMISSION_STORAGE;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 获取值（权限校验 + 服务调用）
pub(crate) fn storage_get(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
) -> Result<Option<serde_json::Value>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_storage_get") {
        return Err("permission denied".to_string());
    }
    let storage = host_ctx.storage.clone();
    block_on_async(storage.get(plugin_id, key))
        .map_err(|e| format!("storage error: {}", e))
}

/// 设置值（权限校验 + 服务调用）
pub(crate) fn storage_set(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_storage_set") {
        return Err("permission denied".to_string());
    }
    let storage = host_ctx.storage.clone();
    block_on_async(storage.set(plugin_id, key, value))
        .map_err(|e| format!("storage error: {}", e))
}

/// 删除值（权限校验 + 服务调用）
pub(crate) fn storage_delete(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_storage_delete") {
        return Err("permission denied".to_string());
    }
    let storage = host_ctx.storage.clone();
    block_on_async(storage.delete(plugin_id, key))
        .map_err(|e| format!("storage error: {}", e))
}
