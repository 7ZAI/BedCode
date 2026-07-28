//! 存储域 Host Functions（插件键值存储，按 plugin_id 隔离）

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use crate::plugin::permission::PERMISSION_STORAGE;

/// 存储：获取值
///
/// 参数：(key_ptr, key_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_storage_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string_consume(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to read key from WASM memory");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_STORAGE, "host_storage_get") {
        return -1;
    }

    let storage = host_ctx.storage.clone();
    let result = block_on_async(storage.get(&plugin_id, &key));

    match result {
        Ok(Some(value)) => {
            let json_str = match serde_json::to_string(&value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_get: JSON serialization failed");
                    return -1;
                }
            };
            match write_wasm_string(&mut caller, &json_str) {
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to write result to WASM memory");
                    -1
                }
            }
        }
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_get: storage error");
            -1
        }
    }
}

/// 存储：设置值
///
/// 参数：(key_ptr, key_len, val_ptr, val_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_storage_set(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    val_ptr: u32,
    val_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string_consume(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_set: failed to read key");
            return -1;
        }
    };

    let val_str = match read_wasm_string_consume(&mut caller, val_ptr, val_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, key = %key, "host_storage_set: failed to read value");
            return -1;
        }
    };

    let json_value: serde_json::Value = match serde_json::from_str(&val_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_set: invalid JSON value");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_STORAGE, "host_storage_set") {
        return -1;
    }

    let storage = host_ctx.storage.clone();
    match block_on_async(storage.set(&plugin_id, &key, json_value)) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_set: storage error");
            -1
        }
    }
}

/// 存储：删除值
///
/// 参数：(key_ptr, key_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_storage_delete(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string_consume(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_delete: failed to read key");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_STORAGE, "host_storage_delete") {
        return -1;
    }

    let storage = host_ctx.storage.clone();
    match block_on_async(storage.delete(&plugin_id, &key)) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_delete: storage error");
            -1
        }
    }
}
