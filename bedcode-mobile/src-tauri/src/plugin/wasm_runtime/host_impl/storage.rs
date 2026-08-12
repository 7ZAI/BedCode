//! host_storage_* — 插件键值存储

use super::super::WasmPluginState;
use super::support::{guarded_host_call, has_permission, read_wasm_string, write_result_to_out_ptr, write_wasm_string};

pub(crate) fn host_storage_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_storage_get: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to read key");
            return -1;
        }
    };

    let storage = host_ctx.storage.clone();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_storage_get",
        Err(crate::AppError::Internal("host_storage_get panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(storage.get(&plugin_id, &key))),
    );

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
                Some((ptr, len)) => {
                    if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                        0
                    } else {
                        -1
                    }
                }
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to write result to WASM memory");
                    -1
                }
            }
        }
        Ok(None) => {
            let _ = write_result_to_out_ptr(&mut caller, out_ptr, 0, 0);
            0
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_get: storage error");
            -1
        }
    }
}

pub(crate) fn host_storage_set(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    val_ptr: u32,
    val_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_storage_set: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_set: failed to read key");
            return -1;
        }
    };

    let val_str = match read_wasm_string(&mut caller, val_ptr, val_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_set: failed to read value");
            return -1;
        }
    };

    let json_value: serde_json::Value = match serde_json::from_str(&val_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_set: invalid JSON value");
            return -1;
        }
    };

    let storage = host_ctx.storage.clone();
    let handle = caller.data().runtime_handle.clone();
    match guarded_host_call(
        &plugin_id,
        "host_storage_set",
        Err(crate::AppError::Internal("host_storage_set panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(storage.set(&plugin_id, &key, json_value))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_set: storage error");
            -1
        }
    }
}

pub(crate) fn host_storage_delete(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_storage_delete: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_delete: failed to read key");
            return -1;
        }
    };

    let storage = host_ctx.storage.clone();
    let handle = caller.data().runtime_handle.clone();
    match guarded_host_call(
        &plugin_id,
        "host_storage_delete",
        Err(crate::AppError::Internal("host_storage_delete panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(storage.delete(&plugin_id, &key))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_delete: storage error");
            -1
        }
    }
}
