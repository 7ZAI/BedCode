//! host_storage_* — 插件键值存储（逻辑层）

use super::super::WasmPluginState;
use super::support::guarded_host_call;

/// 逻辑层：读取值（返回 JSON 字符串；值以 serde_json::Value 存储，
/// 组件契约 WIT `option<string>` 即承载 JSON 载荷）
pub(crate) fn storage_get(
    state: &WasmPluginState,
    key: &str,
) -> Result<Option<String>, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE)
    {
        return Err("permission denied: storage".to_string());
    }
    let storage = state.host_ctx.storage.clone();
    let value = guarded_host_call(&state.plugin_id, "host_storage_get",
        Err(crate::AppError::Internal("host_storage_get panicked".to_string())), || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(storage.get(&state.plugin_id, key))
        })
    })
    .map_err(|e| format!("storage error: {}", e))?;
    value
        .map(|v| serde_json::to_string(&v).map_err(|e| format!("JSON serialize failed: {}", e)))
        .transpose()
}

/// 逻辑层：设置值（value 为 JSON 字符串，送入前解析；
/// 组件契约 WIT 与 desktop 同语义：set(key, value: string) 先 JSON 解析）
pub(crate) fn storage_set(state: &WasmPluginState, key: &str, value: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE)
    {
        return Err("permission denied: storage".to_string());
    }
    let json_value: serde_json::Value = serde_json::from_str(value)
        .map_err(|e| format!("invalid JSON value: {}", e))?;
    let storage = state.host_ctx.storage.clone();
    guarded_host_call(&state.plugin_id, "host_storage_set",
        Err(crate::AppError::Internal("host_storage_set panicked".to_string())), || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(storage.set(&state.plugin_id, key, json_value))
        })
    })
    .map_err(|e| format!("storage error: {}", e))
}

/// 逻辑层：删除值
pub(crate) fn storage_delete(state: &WasmPluginState, key: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE)
    {
        return Err("permission denied: storage".to_string());
    }
    let storage = state.host_ctx.storage.clone();
    guarded_host_call(&state.plugin_id, "host_storage_delete",
        Err(crate::AppError::Internal("host_storage_delete panicked".to_string())), || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(storage.delete(&state.plugin_id, key))
        })
    })
    .map_err(|e| format!("storage error: {}", e))
}
