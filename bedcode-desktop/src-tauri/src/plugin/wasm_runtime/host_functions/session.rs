//! 会话域 Host Functions（会话查询、配置列表与会话创建）

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext, WasmPluginState};
use crate::plugin::permission::{PERMISSION_SESSION_READ, PERMISSION_SESSION_WRITE};
use crate::system::error_boundary::spawn_with_error_boundary;
use uuid::Uuid;

// ==================== 逻辑层（core 胶水与 Component Model 绑定共用） ====================

/// 逻辑层：列出所有会话（权限 + SessionManager 查询），返回 JSON 数组字符串
pub(crate) fn session_list(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_list") {
        return Err("permission denied".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    let sessions = block_on_async(sm.list_sessions());
    serde_json::to_string(&sessions)
        .map(Some)
        .map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

/// 逻辑层：获取单个会话（权限 + 查询），不存在返回 None
pub(crate) fn session_get(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_get") {
        return Err("permission denied".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    match block_on_async(sm.get_session(session_id)) {
        Some(info) => serde_json::to_string(&info)
            .map(Some)
            .map_err(|e| format!("session error: JSON serialization failed: {}", e)),
        None => Ok(None),
    }
}

/// 逻辑层：列出会话配置精简列表（id/name/workingDir/command）
pub(crate) fn session_config_list(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_config_list") {
        return Err("permission denied".to_string());
    }
    let cm = host_ctx.config_manager.clone();
    let configs = block_on_async(cm.list_configs())
        .map_err(|e| format!("session error: {}", e))?;
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
    serde_json::to_string(&simplified)
        .map(Some)
        .map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

/// 逻辑层：按配置创建新会话（v6，ADR 0003），返回预生成的 session_id
///
/// 创建为宿主异步执行：wasm 调用栈内同步创建会死锁 —— `create_session`
/// 会同步分发 Creating/Created 生命周期事件，而事件回灌同一插件实例需要
/// 重新获取 `wasm_plugins` 写锁（该锁正被当前 wasm 调用持有，tokio RwLock
/// 不可重入），且 wasmtime Store 不可重入。因此此处预生成会话 ID 立即返回，
/// 实际创建在宿主上下文异步执行：事件分发发生在 wasm 调用返回（锁释放）后，
/// hooks 仍先于 PTY 启动就位。
pub(crate) fn session_create(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    config_id: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_create") {
        return Err("permission denied".to_string());
    }
    if config_id.is_empty() {
        return Err("session error: empty config_id".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    // 预生成会话 ID 并异步创建：插件侧照常将 job 置 creating 并记录 session_id，
    // 等待 Created 事件（携带同一 session_id）完成匹配，语义与同步创建一致
    let session_id = Uuid::new_v4().to_string();
    let cid = config_id.to_string();
    let sid = session_id.clone();
    let pid = plugin_id.to_string();
    spawn_with_error_boundary("host_session_create", async move {
        match sm.create_session_with_id(&cid, &sid).await {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %pid,
                    config_id = %cid,
                    session_id = %sid,
                    "host_session_create: session created (async)"
                );
            }
            Err(e) => {
                // 创建失败无同步返回通道：插件侧由 creating 超时看门狗置 failed
                tracing::error!(
                    plugin_id = %pid,
                    config_id = %cid,
                    session_id = %sid,
                    error = %e,
                    "host_session_create: create_session failed (async)"
                );
            }
        }
    });
    Ok(session_id)
}

// ==================== Host Functions（core module 胶水） ====================

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

    match session_list(&host_ctx, &plugin_id) {
        Ok(Some(json)) => match write_wasm_string(&mut caller, &json) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_session_list: failed to write result to WASM memory");
                -1
            }
        },
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_session_list: list failed");
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

    match session_get(&host_ctx, &plugin_id, &session_id) {
        Ok(Some(json)) => match write_wasm_string(&mut caller, &json) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(session_id = %session_id, "host_session_get: failed to write result to WASM memory");
                -1
            }
        },
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "host_session_get: get failed");
            -1
        }
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

    match session_config_list(&host_ctx, &plugin_id) {
        Ok(Some(json)) => match write_wasm_string(&mut caller, &json) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_session_config_list: failed to write result to WASM memory");
                -1
            }
        },
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_session_config_list: list failed");
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
/// **创建为宿主异步执行**：wasm 调用栈内同步创建会死锁 —— `create_session`
/// 会同步分发 Creating/Created 生命周期事件，而事件回灌同一插件实例需要
/// 重新获取 `wasm_plugins` 写锁（该锁正被当前 wasm 调用持有，tokio RwLock
/// 不可重入），且 wasmtime Store 不可重入。因此此处预生成会话 ID 立即返回，
/// 实际创建在宿主上下文异步执行：事件分发发生在 wasm 调用返回（锁释放）后，
/// hooks 仍先于 PTY 启动就位。
///
/// 参数：(config_id_ptr, config_id_len, out_ptr)
/// 返回：0 成功（预生成的 session_id 写入 out_ptr），-1 失败（含权限拒绝、配置不存在）
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

    match session_create(&host_ctx, &plugin_id, &config_id) {
        Ok(session_id) => match write_wasm_string(&mut caller, &session_id) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_session_create: failed to write session_id to WASM memory");
                -1
            }
        },
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_session_create: create failed");
            -1
        }
    }
}
