//! 文件服务域 Host Functions（挂载/卸载/更新根目录/对端信息）
//!
//! 注册表经 [`WasmHostContext`] 注入（在 PluginHost::new() 中早于插件
//! auto-activate 创建并注入），host function 直接从 caller 的宿主上下文获取 ——
//! 不依赖 AppContext 全局单例（其初始化晚于插件激活，激活期挂载会失败）。
//! 挂载的上传策略钩子目标记为 Wasm（WASM 插件导出 __bedcode_on_upload_request）

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::file_service::HookTarget;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_FILESERVICE;
use bedcode_plugin_api::{MountOptions, MountResult};

/// 获取文件服务注册表（经宿主上下文注入，激活期始终可用）
fn file_service_registry(
    host_ctx: &WasmHostContext,
) -> std::sync::Arc<crate::plugin::file_service::FileServiceRegistry> {
    host_ctx.file_service().clone()
}

/// 文件服务：挂载
///
/// 参数：(options_ptr, options_len, out_ptr) — options 为 MountOptions JSON
/// 返回：0 成功，-1 失败。成功时 MountResult JSON 写入 out_ptr
pub(super) fn host_filesrv_mount(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    options_ptr: u32,
    options_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let options_str = match read_wasm_string_consume(&mut caller, options_ptr, options_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_mount: failed to read options");
            return -1;
        }
    };

    let options: MountOptions = match serde_json::from_str(&options_str) {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_mount: invalid MountOptions JSON");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FILESERVICE, "host_filesrv_mount")
    {
        return -1;
    }

    let registry = file_service_registry(&host_ctx);

    let mount_path = options.mount_path.clone();
    match block_on_async(registry.mount(&plugin_id, options, HookTarget::Wasm)) {
        Ok(entry) => {
            let result = MountResult {
                mount_path: entry.mount_path.clone(),
                base_path: format!("/api/plugins/{}/{}", plugin_id, entry.mount_path),
            };
            let json = match serde_json::to_string(&result) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_mount: serialize result failed");
                    return -1;
                }
            };
            match write_wasm_string(&mut caller, &json) {
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_filesrv_mount: failed to write result");
                    -1
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, mount = %mount_path, "host_filesrv_mount: mount failed");
            -1
        }
    }
}

/// 文件服务：卸载挂载点
///
/// 参数：(mount_path_ptr, mount_path_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_filesrv_unmount(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    mount_path_ptr: u32,
    mount_path_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let mount_path = match read_wasm_string_consume(&mut caller, mount_path_ptr, mount_path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_unmount: failed to read mount path");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FILESERVICE, "host_filesrv_unmount")
    {
        return -1;
    }

    let registry = file_service_registry(&host_ctx);

    match block_on_async(registry.unmount(&plugin_id, &mount_path)) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, mount = %mount_path, "host_filesrv_unmount: unmount failed");
            -1
        }
    }
}

/// 文件服务：更新挂载点允许目录根
///
/// 参数：(mount_path_ptr, mount_path_len, roots_ptr, roots_len) — roots 为 JSON 字符串数组
/// 返回：0 成功，-1 失败
pub(super) fn host_filesrv_update_roots(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    mount_path_ptr: u32,
    mount_path_len: u32,
    roots_ptr: u32,
    roots_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let mount_path = match read_wasm_string_consume(&mut caller, mount_path_ptr, mount_path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_update_roots: failed to read mount path");
            return -1;
        }
    };

    let roots_str = match read_wasm_string_consume(&mut caller, roots_ptr, roots_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, mount = %mount_path, "host_filesrv_update_roots: failed to read roots");
            return -1;
        }
    };

    let roots: Vec<String> = match serde_json::from_str(&roots_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, mount = %mount_path, "host_filesrv_update_roots: invalid roots JSON");
            return -1;
        }
    };

    if !super::check_permission(
        &host_ctx,
        &plugin_id,
        PERMISSION_FILESERVICE,
        "host_filesrv_update_roots",
    ) {
        return -1;
    }

    let registry = file_service_registry(&host_ctx);

    match block_on_async(registry.update_roots(&plugin_id, &mount_path, roots)) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, mount = %mount_path, "host_filesrv_update_roots: update failed");
            -1
        }
    }
}

/// 文件服务：获取对端文件服务信息
///
/// 参数：(peer_id_ptr, peer_id_len, out_ptr)
/// 返回：0 成功（对端未公告时 out_ptr 写 (0,0)），-1 失败
pub(super) fn host_filesrv_get_peer(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    peer_id_ptr: u32,
    peer_id_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let peer_id = match read_wasm_string_consume(&mut caller, peer_id_ptr, peer_id_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_get_peer: failed to read peer id");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FILESERVICE, "host_filesrv_get_peer")
    {
        return -1;
    }

    let registry = file_service_registry(&host_ctx);

    match block_on_async(registry.get_peer(&peer_id)) {
        Some(info) => {
            let json = match serde_json::to_string(&info) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, peer_id = %peer_id, "host_filesrv_get_peer: serialize failed");
                    return -1;
                }
            };
            match write_wasm_string(&mut caller, &json) {
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => -1,
            }
        }
        None => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
    }
}
