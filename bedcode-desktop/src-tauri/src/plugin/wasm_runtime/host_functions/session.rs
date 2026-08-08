//! 会话域 Host Functions（会话查询、配置列表与会话创建）

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use crate::plugin::permission::{PERMISSION_SESSION_READ, PERMISSION_SESSION_WRITE};
use crate::system::error_boundary::spawn_with_error_boundary;
use uuid::Uuid;

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

    // 预生成会话 ID 并异步创建：插件侧照常将 job 置 creating 并记录 session_id，
    // 等待 Created 事件（携带同一 session_id）完成匹配，语义与同步创建一致
    let session_id = Uuid::new_v4().to_string();
    let cid = config_id.clone();
    let sid = session_id.clone();
    spawn_with_error_boundary("host_session_create", async move {
        match sm.create_session_with_id(&cid, &sid).await {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    config_id = %cid,
                    session_id = %sid,
                    "host_session_create: session created (async)"
                );
            }
            Err(e) => {
                // 创建失败无同步返回通道：插件侧由 creating 超时看门狗置 failed
                tracing::error!(
                    plugin_id = %plugin_id,
                    config_id = %cid,
                    session_id = %sid,
                    error = %e,
                    "host_session_create: create_session failed (async)"
                );
            }
        }
    });

    match write_wasm_string(&mut caller, &session_id) {
        Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
        None => {
            tracing::error!("host_session_create: failed to write session_id to WASM memory");
            -1
        }
    }
}

/// 会话：关闭（终止）会话（v7）
///
/// 包一层核心已有的 `SessionManager::kill_session_with_source`，供插件
/// （如 auto-task 定时自动任务）在执行完毕后关闭自己创建的会话。
/// 停止 PTY 并置 Stopped，会话记录保留（与用户手动关闭一致）。
///
/// **异步执行**：`kill_session_with_source` 会同步分发 Stopping/Stopped
/// 生命周期事件，事件回灌同一插件实例需要重新获取 `wasm_plugins` 写锁
/// （该锁正被当前 wasm 调用持有，tokio RwLock 不可重入）——与
/// `host_session_create` 同理，此处 spawn 异步执行，wasm 调用立即返回。
///
/// 参数：(session_id_ptr, session_id_len, out_ptr)
/// 返回：0 成功，-1 失败（含权限拒绝、参数为空）
pub(super) fn host_session_close(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let session_id = match read_wasm_string_consume(&mut caller, sid_ptr, sid_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_session_close: failed to read session_id");
            return -1;
        }
    };

    let host_ctx = caller.data().host_ctx.clone();

    // 关闭会话属写操作，要求 session:write（与 host_session_create 一致）
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_SESSION_WRITE, "host_session_close") {
        return -1;
    }

    if session_id.is_empty() {
        tracing::error!(plugin_id = %plugin_id, "host_session_close: empty session_id");
        return -1;
    }

    let sm = host_ctx.session_manager.clone();
    let sid = session_id.clone();
    spawn_with_error_boundary("host_session_close", async move {
        match sm.kill_session_with_source(&sid, None).await {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    session_id = %sid,
                    "host_session_close: session closed (async)"
                );
            }
            Err(e) => {
                tracing::error!(
                    plugin_id = %plugin_id,
                    session_id = %sid,
                    error = %e,
                    "host_session_close: kill_session failed (async)"
                );
            }
        }
    });

    // 异步执行无同步结果通道：统一返回成功，失败仅记录日志（幂等，重复关闭无害）
    write_result_to_out_ptr(&mut caller, out_ptr, 0, 0)
}
