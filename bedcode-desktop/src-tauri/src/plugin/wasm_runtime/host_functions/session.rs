//! 会话域 Host Functions（会话查询、配置列表与会话创建）

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use crate::plugin::permission::{PERMISSION_SESSION_READ, PERMISSION_SESSION_WRITE};

/// 会话：列出所有会话
///
/// 参数：(out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_session_list(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    // 权限校验：与 RustPluginContext::list_sessions 保持一致
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_SESSION_READ, "host_session_list") {
        return -1;
    }

    let sm = host_ctx.session_manager.clone();

    let sessions = block_on_async(sm.list_sessions());

    match serde_json::to_string(&sessions) {
        Ok(json) => match write_wasm_string(&mut caller, &json) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!("host_session_list: failed to write result to WASM memory");
                -1
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "host_session_list: serialization failed");
            -1
        }
    }
}

/// 会话：获取单个会话
///
/// 参数：(session_id_ptr, session_id_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_session_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let session_id = match read_wasm_string_consume(&mut caller, sid_ptr, sid_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_session_get: failed to read session_id");
            return -1;
        }
    };

    let host_ctx = caller.data().host_ctx.clone();

    // 权限校验：与 RustPluginContext::get_session 保持一致
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_SESSION_READ, "host_session_get") {
        return -1;
    }

    let sm = host_ctx.session_manager.clone();

    match block_on_async(sm.get_session(&session_id)) {
        Some(info) => match serde_json::to_string(&info) {
            Ok(json) => match write_wasm_string(&mut caller, &json) {
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!(session_id = %session_id, "host_session_get: failed to write result to WASM memory");
                    -1
                }
            },
            Err(e) => {
                tracing::error!(error = %e, session_id = %session_id, "host_session_get: serialization failed");
                -1
            }
        },
        None => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
    }
}

/// 会话配置：列出所有会话配置
///
/// 返回所有会话配置的精简列表（仅 id、working_dir、command 字段），
/// 供插件在 deactivate 时遍历所有项目目录清理 hooks 配置。
///
/// 参数：(out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_session_config_list(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    // 权限校验：会话配置含 working_dir 等敏感路径，要求 session:read
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_SESSION_READ, "host_session_config_list") {
        return -1;
    }

    let cm = host_ctx.config_manager.clone();

    let configs = match block_on_async(cm.list_configs()) {
        Ok(configs) => configs,
        Err(e) => {
            tracing::error!(error = %e, "host_session_config_list: failed to list configs");
            return -1;
        }
    };

    // 精简输出：仅包含插件需要的字段，避免传输不必要的数据
    // （name 供插件 UI 展示会话配置选择列表，如定时任务选配置）
    let simplified: Vec<serde_json::Value> = configs.iter().map(|c| {
        serde_json::json!({
            "id": c.id,
            "name": c.name,
            "workingDir": c.working_dir,
            "command": c.command,
        })
    }).collect();

    match serde_json::to_string(&simplified) {
        Ok(json) => match write_wasm_string(&mut caller, &json) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!("host_session_config_list: failed to write result to WASM memory");
                -1
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "host_session_config_list: serialization failed");
            -1
        }
    }
}

/// 会话：按配置创建新会话（v6，ADR 0003）
///
/// 包一层核心已有的 `SessionManager::create_session`，供插件（如 auto-task
/// 定时自动任务）在指定时刻新建会话。创建成功后宿主照常分发 `Created`
/// 生命周期事件，插件可据此感知新会话就绪。
///
/// 参数：(config_id_ptr, config_id_len, out_ptr)
/// 返回：0 成功（session_id 写入 out_ptr），-1 失败（含权限拒绝、配置不存在）
pub(super) fn host_session_create(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    cid_ptr: u32,
    cid_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let config_id = match read_wasm_string_consume(&mut caller, cid_ptr, cid_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_session_create: failed to read config_id");
            return -1;
        }
    };

    let host_ctx = caller.data().host_ctx.clone();

    // 创建会话属写操作，要求 session:write（与 PERMISSION_API_MAP 中
    // session.create 的映射一致）
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_SESSION_WRITE, "host_session_create") {
        return -1;
    }

    if config_id.is_empty() {
        tracing::error!(plugin_id = %plugin_id, "host_session_create: empty config_id");
        return -1;
    }

    let sm = host_ctx.session_manager.clone();

    match block_on_async(sm.create_session(&config_id)) {
        Ok(session_id) => {
            tracing::info!(
                plugin_id = %plugin_id,
                config_id = %config_id,
                session_id = %session_id,
                "host_session_create: session created"
            );
            match write_wasm_string(&mut caller, &session_id) {
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!("host_session_create: failed to write session_id to WASM memory");
                    -1
                }
            }
        }
        Err(e) => {
            tracing::error!(
                plugin_id = %plugin_id,
                config_id = %config_id,
                error = %e,
                "host_session_create: create_session failed"
            );
            -1
        }
    }
}
