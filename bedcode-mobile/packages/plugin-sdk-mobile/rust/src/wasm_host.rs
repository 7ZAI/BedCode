//! WASM 插件侧宿主 API 绑定
//!
//! [`WasmHost`] 以 WASM import 后端实现 `host/*` 全部功能 trait，
//! 插件通过这些调用访问宿主能力。编译为 WASM 时，调用对应宿主在
//! wasmtime Linker 中注册的 host functions。
//! 线性内存字符串通过 (ptr, len) 对传递；返回 (ptr, len) 的结果通过
//! out_ptr 输出参数传递（8 字节: ptr + len，见 [`crate::abi`]）。
//!
//! 内存配对回收：参数/结果均为 `wasm_alloc_string` 分配（与 `__bedcode_deallocate`
//! 同 Layout 配对），读取后立即归还，防止长驻插件线性内存单调增长。
//!
//! 插件身份（plugin_id）由宿主侧 Caller state 维护并注入各 host function，
//! 插件侧无需持有 —— `WasmHost` 是无状态 unit struct（与桌面端 SDK 一致）。

use crate::host::{
    ConfigKey, HostBus, HostConfig, HostDatabase, HostError, HostEvents, HostFileService, HostFs,
    HostHttp, HostLog, HostSession, HostStorage, HostTerminal, HostTransfer,
};
use crate::types::{MountOptions, MountResult, PeerFileService, TransferRequest};

/// 宿主 API 绑定（WASM 插件侧）
///
/// 无状态 unit struct，通过 WASM import 调用宿主注册的 host functions。
/// 实现了 `host/*` 模块的全部功能 trait（自动获得 `HostApi` 聚合 trait）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WasmHost;

// ==================== HostStorage ====================

impl HostStorage for WasmHost {
    fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, HostError> {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let mut out = [0u32; 2];
        let status = unsafe { host_storage_get(key_ptr, key_len, out.as_mut_ptr() as u32) };
        wasm_dealloc_string(key_ptr, key_len);
        if status != 0 {
            return Err(HostError::call_failed("storage_get"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_string(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("storage_get: invalid JSON from host: {}", e)))
    }

    fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), HostError> {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let val_str = serde_json::to_string(value).unwrap_or_default();
        let (val_ptr, val_len) = wasm_alloc_string(&val_str);
        let status = unsafe { host_storage_set(key_ptr, key_len, val_ptr, val_len) };
        wasm_dealloc_string(key_ptr, key_len);
        wasm_dealloc_string(val_ptr, val_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("storage_set"))
        }
    }

    fn storage_delete(&self, key: &str) -> Result<(), HostError> {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let status = unsafe { host_storage_delete(key_ptr, key_len) };
        wasm_dealloc_string(key_ptr, key_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("storage_delete"))
        }
    }
}

// ==================== HostDatabase ====================

impl HostDatabase for WasmHost {
    fn db_execute(&self, sql: &str) -> Result<i32, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let affected = unsafe { host_db_execute(sql_ptr, sql_len) };
        wasm_dealloc_string(sql_ptr, sql_len);
        if affected >= 0 {
            Ok(affected)
        } else {
            Err(HostError::call_failed("db_execute"))
        }
    }

    fn db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let mut out = [0u32; 2];
        let status = unsafe { host_db_query(sql_ptr, sql_len, out.as_mut_ptr() as u32) };
        wasm_dealloc_string(sql_ptr, sql_len);
        if status != 0 {
            return Err(HostError::call_failed("db_query"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_string(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("db_query: invalid JSON from host: {}", e)))
    }
}

// ==================== HostTerminal ====================

impl HostTerminal for WasmHost {
    fn terminal_send(&self, session_id: &str, data: &str) -> Result<(), HostError> {
        let (sid_ptr, sid_len) = wasm_alloc_string(session_id);
        let (data_ptr, data_len) = wasm_alloc_string(data);
        let status = unsafe { host_terminal_send(sid_ptr, sid_len, data_ptr, data_len) };
        wasm_dealloc_string(sid_ptr, sid_len);
        wasm_dealloc_string(data_ptr, data_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("terminal_send"))
        }
    }
}

// ==================== HostSession ====================

impl HostSession for WasmHost {
    fn session_list(&self) -> Result<Option<serde_json::Value>, HostError> {
        let mut out = [0u32; 2];
        let status = unsafe { host_session_list(out.as_mut_ptr() as u32) };
        if status != 0 {
            return Err(HostError::call_failed("session_list"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_string(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("session_list: invalid JSON from host: {}", e)))
    }

    fn session_get(&self, session_id: &str) -> Result<Option<serde_json::Value>, HostError> {
        let (sid_ptr, sid_len) = wasm_alloc_string(session_id);
        let mut out = [0u32; 2];
        let status = unsafe { host_session_get(sid_ptr, sid_len, out.as_mut_ptr() as u32) };
        wasm_dealloc_string(sid_ptr, sid_len);
        if status != 0 {
            return Err(HostError::call_failed("session_get"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_string(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("session_get: invalid JSON from host: {}", e)))
    }
}

// ==================== HostEvents ====================

impl HostEvents for WasmHost {
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value) {
        let (name_ptr, name_len) = wasm_alloc_string(event_name);
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = wasm_alloc_string(&payload_str);
        unsafe { host_emit_event(name_ptr, name_len, payload_ptr, payload_len) };
        wasm_dealloc_string(name_ptr, name_len);
        wasm_dealloc_string(payload_ptr, payload_len);
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), HostError> {
        let (title_ptr, title_len) = wasm_alloc_string(title);
        let (body_ptr, body_len) = wasm_alloc_string(body);
        let status = unsafe { host_notify(title_ptr, title_len, body_ptr, body_len) };
        wasm_dealloc_string(title_ptr, title_len);
        wasm_dealloc_string(body_ptr, body_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("notify"))
        }
    }
}

// ==================== HostHttp ====================

impl HostHttp for WasmHost {
    fn http_fetch(&self, request: &serde_json::Value) -> Result<Option<serde_json::Value>, HostError> {
        let req_str = serde_json::to_string(request).unwrap_or_default();
        let (req_ptr, req_len) = wasm_alloc_string(&req_str);
        let mut out = [0u32; 2];
        let status = unsafe { host_http_fetch(req_ptr, req_len, out.as_mut_ptr() as u32) };
        wasm_dealloc_string(req_ptr, req_len);
        if status != 0 {
            return Err(HostError::call_failed("http_fetch"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_string(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("http_fetch: invalid JSON from host: {}", e)))
    }
}

// ==================== HostFs ====================

impl HostFs for WasmHost {
    fn fs_read(&self, path: &str) -> Result<Option<String>, HostError> {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let mut out = [0u32; 2];
        let status = unsafe { host_fs_read(path_ptr, path_len, out.as_mut_ptr() as u32) };
        wasm_dealloc_string(path_ptr, path_len);
        if status != 0 {
            return Err(HostError::call_failed("fs_read"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let content = read_and_free_string(out[0], out[1]);
        Ok(Some(content))
    }

    fn fs_write(&self, path: &str, data: &str) -> Result<(), HostError> {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let (data_ptr, data_len) = wasm_alloc_string(data);
        let status = unsafe { host_fs_write(path_ptr, path_len, data_ptr, data_len) };
        wasm_dealloc_string(path_ptr, path_len);
        wasm_dealloc_string(data_ptr, data_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("fs_write"))
        }
    }

    fn fs_copy(&self, src: &str, dst: &str) -> Result<(), HostError> {
        let (src_ptr, src_len) = wasm_alloc_string(src);
        let (dst_ptr, dst_len) = wasm_alloc_string(dst);
        let status = unsafe { host_fs_copy(src_ptr, src_len, dst_ptr, dst_len) };
        wasm_dealloc_string(src_ptr, src_len);
        wasm_dealloc_string(dst_ptr, dst_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("fs_copy"))
        }
    }

    fn fs_exists(&self, path: &str) -> Result<bool, HostError> {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let result = unsafe { host_fs_exists(path_ptr, path_len) };
        wasm_dealloc_string(path_ptr, path_len);
        match result {
            1 => Ok(true),
            0 => Ok(false),
            _ => Err(HostError::call_failed("fs_exists")),
        }
    }

    fn fs_delete(&self, path: &str) -> Result<(), HostError> {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let status = unsafe { host_fs_delete(path_ptr, path_len) };
        wasm_dealloc_string(path_ptr, path_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("fs_delete"))
        }
    }

    fn fs_request_auth(&self, paths: &[String]) -> Result<bool, HostError> {
        let json = serde_json::to_string(paths)
            .map_err(|e| HostError::custom(-1, format!("fs_request_auth: serialize failed: {}", e)))?;
        let (paths_ptr, paths_len) = wasm_alloc_string(&json);
        let result = unsafe { host_fs_request_auth(paths_ptr, paths_len) };
        wasm_dealloc_string(paths_ptr, paths_len);
        match result {
            1 => Ok(true),
            0 => Ok(false),
            _ => Err(HostError::call_failed("fs_request_auth")),
        }
    }

    fn fs_write_media_downloads(
        &self,
        src_path: &str,
        display_name: &str,
        mime_type: &str,
    ) -> Result<(), HostError> {
        let (src_ptr, src_len) = wasm_alloc_string(src_path);
        let (name_ptr, name_len) = wasm_alloc_string(display_name);
        let (mime_ptr, mime_len) = wasm_alloc_string(mime_type);
        let status = unsafe {
            host_fs_write_media_downloads(src_ptr, src_len, name_ptr, name_len, mime_ptr, mime_len)
        };
        wasm_dealloc_string(src_ptr, src_len);
        wasm_dealloc_string(name_ptr, name_len);
        wasm_dealloc_string(mime_ptr, mime_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("fs_write_media_downloads"))
        }
    }

    fn fs_save_to_document(
        &self,
        src_path: &str,
        suggested_name: &str,
        mime_type: &str,
    ) -> Result<(), HostError> {
        let (src_ptr, src_len) = wasm_alloc_string(src_path);
        let (name_ptr, name_len) = wasm_alloc_string(suggested_name);
        let (mime_ptr, mime_len) = wasm_alloc_string(mime_type);
        let status = unsafe {
            host_fs_save_to_document(src_ptr, src_len, name_ptr, name_len, mime_ptr, mime_len)
        };
        wasm_dealloc_string(src_ptr, src_len);
        wasm_dealloc_string(name_ptr, name_len);
        wasm_dealloc_string(mime_ptr, mime_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("fs_save_to_document"))
        }
    }
}

// ==================== HostLog ====================

impl HostLog for WasmHost {
    fn log_info(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_info(ptr, len) };
        wasm_dealloc_string(ptr, len);
    }

    fn log_debug(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_debug(ptr, len) };
        wasm_dealloc_string(ptr, len);
    }

    fn log_warn(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_warn(ptr, len) };
        wasm_dealloc_string(ptr, len);
    }

    fn log_error(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_error(ptr, len) };
        wasm_dealloc_string(ptr, len);
    }

    fn mark_plugin_error(&self, error: &str) {
        let (ptr, len) = wasm_alloc_string(error);
        unsafe { host_mark_plugin_error(ptr, len) };
        wasm_dealloc_string(ptr, len);
    }
}

// ==================== HostBus ====================

impl HostBus for WasmHost {
    fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), HostError> {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = wasm_alloc_string(&payload_str);
        let status = unsafe { host_bus_publish(topic_ptr, topic_len, payload_ptr, payload_len) };
        wasm_dealloc_string(topic_ptr, topic_len);
        wasm_dealloc_string(payload_ptr, payload_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("bus_publish"))
        }
    }

    fn bus_subscribe(&self, topic: &str) -> Result<(), HostError> {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        let status = unsafe { host_bus_subscribe(topic_ptr, topic_len) };
        wasm_dealloc_string(topic_ptr, topic_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("bus_subscribe"))
        }
    }

    fn bus_unsubscribe(&self, topic: &str) -> Result<(), HostError> {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        let status = unsafe { host_bus_unsubscribe(topic_ptr, topic_len) };
        wasm_dealloc_string(topic_ptr, topic_len);
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("bus_unsubscribe"))
        }
    }
}

// ==================== HostFileService ====================

impl HostFileService for WasmHost {
    fn filesrv_mount(&self, options: &MountOptions) -> Result<MountResult, HostError> {
        let opts_str = serde_json::to_string(options)
            .map_err(|e| HostError::custom(-1, format!("filesrv_mount: serialize options failed: {}", e)))?;
        let (opts_ptr, opts_len) = wasm_alloc_string(&opts_str);
        let mut out = [0u32; 2];
        let status = unsafe { host_filesrv_mount(opts_ptr, opts_len, out.as_mut_ptr() as u32) };
        if status != 0 {
            return Err(HostError::call_failed("filesrv_mount"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Err(HostError::call_failed("filesrv_mount"));
        }
        let json_str = read_and_free_string(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map_err(|e| HostError::custom(-1, format!("filesrv_mount: invalid JSON from host: {}", e)))
    }

    fn filesrv_unmount(&self, mount_path: &str) -> Result<(), HostError> {
        let (mp_ptr, mp_len) = wasm_alloc_string(mount_path);
        let status = unsafe { host_filesrv_unmount(mp_ptr, mp_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("filesrv_unmount"))
        }
    }

    fn filesrv_update_roots(&self, mount_path: &str, roots: &[String]) -> Result<(), HostError> {
        let (mp_ptr, mp_len) = wasm_alloc_string(mount_path);
        let roots_str = serde_json::to_string(roots).unwrap_or_else(|_| "[]".to_string());
        let (roots_ptr, roots_len) = wasm_alloc_string(&roots_str);
        let status = unsafe { host_filesrv_update_roots(mp_ptr, mp_len, roots_ptr, roots_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("filesrv_update_roots"))
        }
    }

    fn filesrv_get_peer(&self, peer_id: &str) -> Result<Option<PeerFileService>, HostError> {
        let (peer_ptr, peer_len) = wasm_alloc_string(peer_id);
        let mut out = [0u32; 2];
        let status = unsafe { host_filesrv_get_peer(peer_ptr, peer_len, out.as_mut_ptr() as u32) };
        if status != 0 {
            return Err(HostError::call_failed("filesrv_get_peer"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_string(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("filesrv_get_peer: invalid JSON from host: {}", e)))
    }

    fn filesrv_query_peer(&self, peer_id: &str) -> Result<(), HostError> {
        let (peer_ptr, peer_len) = wasm_alloc_string(peer_id);
        let status = unsafe { host_filesrv_query_peer(peer_ptr, peer_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("filesrv_query_peer"))
        }
    }
}

// ==================== HostTransfer ====================

impl HostTransfer for WasmHost {
    fn transfer_start(&self, request: &TransferRequest) -> Result<String, HostError> {
        let req_str = serde_json::to_string(request)
            .map_err(|e| HostError::custom(-1, format!("transfer_start: serialize request failed: {}", e)))?;
        let (req_ptr, req_len) = wasm_alloc_string(&req_str);
        let mut out = [0u32; 2];
        let status = unsafe { host_transfer_start(req_ptr, req_len, out.as_mut_ptr() as u32) };
        if status != 0 {
            return Err(HostError::call_failed("transfer_start"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Err(HostError::call_failed("transfer_start"));
        }
        Ok(read_and_free_string(out[0], out[1]))
    }

    fn transfer_cancel(&self, task_id: &str) -> Result<(), HostError> {
        let (task_ptr, task_len) = wasm_alloc_string(task_id);
        let status = unsafe { host_transfer_cancel(task_ptr, task_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("transfer_cancel"))
        }
    }
}

// ==================== HostConfig ====================

impl HostConfig for WasmHost {
    fn config_get(&self, key: ConfigKey) -> Result<Option<String>, HostError> {
        let (key_ptr, key_len) = wasm_alloc_string(key.as_str());
        let mut out = [0u32; 2];
        let status = unsafe { host_config_get(key_ptr, key_len, out.as_mut_ptr() as u32) };
        wasm_dealloc_string(key_ptr, key_len);
        if status != 0 {
            return Err(HostError::call_failed("config_get"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        Ok(Some(read_and_free_string(out[0], out[1])))
    }
}

// ==================== WASM Import Declarations ====================
//
// 这些 extern "C" 声明在编译为 WASM 时对应宿主在 wasmtime Linker 中
// 注册的 abi::NAMESPACE（"bedcode"）命名空间下的 host functions。
// v3 起返回 (ptr, len) 结果的函数通过 out_ptr 输出（8 字节: ptr + len），
// 返回 i32 状态码（0 成功，非 0 失败）—— 消除元组返回的 FFI-safe 警告。
//
// #[link(wasm_import_module)] 确保 WASM 模块从 "bedcode" 命名空间导入，
// 而非默认的 "env" 命名空间 —— 与宿主 Linker 注册命名空间一致
// （缺少该属性会导致实例化失败：unknown import `env::host_*`）。

#[link(wasm_import_module = "bedcode")]
extern "C" {
    /// 存储：获取值 — 结果写入 out_ptr，返回 0 成功
    fn host_storage_get(key_ptr: u32, key_len: u32, out_ptr: u32) -> i32;
    /// 存储：设置值 — 返回 0 成功，-1 失败
    fn host_storage_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;
    /// 存储：删除值 — 返回 0 成功，-1 失败
    fn host_storage_delete(key_ptr: u32, key_len: u32) -> i32;
    /// 数据库：执行 SQL — 返回受影响行数（-1 失败）
    fn host_db_execute(sql_ptr: u32, sql_len: u32) -> i32;
    /// 数据库：查询 SQL — 结果写入 out_ptr，返回 0 成功
    fn host_db_query(sql_ptr: u32, sql_len: u32, out_ptr: u32) -> i32;
    /// 终端：发送输入 — 返回 0 成功，-1 失败
    fn host_terminal_send(sid_ptr: u32, sid_len: u32, data_ptr: u32, data_len: u32) -> i32;
    /// 会话：列出所有 — 结果写入 out_ptr，返回 0 成功
    fn host_session_list(out_ptr: u32) -> i32;
    /// 会话：获取单个 — 结果写入 out_ptr，返回 0 成功
    fn host_session_get(sid_ptr: u32, sid_len: u32, out_ptr: u32) -> i32;
    /// 事件：向前端发送
    fn host_emit_event(name_ptr: u32, name_len: u32, payload_ptr: u32, payload_len: u32);
    /// HTTP 代理 — 结果写入 out_ptr，返回 0 成功
    fn host_http_fetch(req_ptr: u32, req_len: u32, out_ptr: u32) -> i32;
    /// 日志：info
    fn host_log_info(msg_ptr: u32, msg_len: u32);
    /// 日志：debug
    fn host_log_debug(msg_ptr: u32, msg_len: u32);
    /// 日志：warn
    fn host_log_warn(msg_ptr: u32, msg_len: u32);
    /// 日志：error
    fn host_log_error(msg_ptr: u32, msg_len: u32);
    /// 通知：发送系统通知 — 返回 0 成功，-1 失败
    fn host_notify(title_ptr: u32, title_len: u32, body_ptr: u32, body_len: u32) -> i32;
    /// 文件系统：读取文件 — 结果写入 out_ptr，返回 0 成功
    fn host_fs_read(path_ptr: u32, path_len: u32, out_ptr: u32) -> i32;
    /// 文件系统：写入文件 — 返回 0 成功，-1 失败
    fn host_fs_write(path_ptr: u32, path_len: u32, data_ptr: u32, data_len: u32) -> i32;
    /// 文件系统：复制文件 — 返回 0 成功，-1 失败
    fn host_fs_copy(src_ptr: u32, src_len: u32, dst_ptr: u32, dst_len: u32) -> i32;
    /// 文件系统：检查文件是否存在 — 返回 1 存在，0 不存在，-1 错误
    fn host_fs_exists(path_ptr: u32, path_len: u32) -> i32;
    /// 文件系统：批量请求目录授权（paths 为 JSON 字符串数组）— 返回 1 全部同意，0 拒绝/超时，-1 失败
    fn host_fs_request_auth(paths_ptr: u32, paths_len: u32) -> i32;
    /// 文件系统：删除文件 — 返回 0 成功（不存在也视为成功），-1 失败
    fn host_fs_delete(path_ptr: u32, path_len: u32) -> i32;
    /// 文件系统：写入 MediaStore 公共下载目录 — 返回 0 成功，-1 失败
    fn host_fs_write_media_downloads(
        src_ptr: u32,
        src_len: u32,
        name_ptr: u32,
        name_len: u32,
        mime_ptr: u32,
        mime_len: u32,
    ) -> i32;
    /// 文件系统：「保存到…」弹系统保存对话框并拷贝 — 返回 0 成功，-1 失败/取消
    fn host_fs_save_to_document(
        src_ptr: u32,
        src_len: u32,
        name_ptr: u32,
        name_len: u32,
        mime_ptr: u32,
        mime_len: u32,
    ) -> i32;
    /// 消息总线：发布消息 — 返回 0 成功，-1 失败
    fn host_bus_publish(topic_ptr: u32, topic_len: u32, payload_ptr: u32, payload_len: u32) -> i32;
    /// 消息总线：订阅 topic — 返回 0 成功，-1 失败
    fn host_bus_subscribe(topic_ptr: u32, topic_len: u32) -> i32;
    /// 消息总线：取消订阅 — 返回 0 成功，-1 失败
    fn host_bus_unsubscribe(topic_ptr: u32, topic_len: u32) -> i32;
    /// 插件状态：标记插件为错误状态 — 宿主置 Error + 持久化未启用 + 通知前端
    fn host_mark_plugin_error(msg_ptr: u32, msg_len: u32);
    /// 文件服务：挂载 — MountOptions JSON → out_ptr 输出 MountResult JSON，返回 0 成功 -1 失败
    fn host_filesrv_mount(opts_ptr: u32, opts_len: u32, out_ptr: u32) -> i32;
    /// 文件服务：卸载挂载点 — 返回 0 成功，-1 失败
    fn host_filesrv_unmount(mp_ptr: u32, mp_len: u32) -> i32;
    /// 文件服务：更新允许目录根（roots 为 JSON 数组字符串）— 返回 0 成功，-1 失败
    fn host_filesrv_update_roots(mp_ptr: u32, mp_len: u32, roots_ptr: u32, roots_len: u32) -> i32;
    /// 文件服务：获取对端信息 — out_ptr 输出 PeerFileService JSON（(0,0) 表示未公告）
    fn host_filesrv_get_peer(peer_ptr: u32, peer_len: u32, out_ptr: u32) -> i32;
    /// 文件服务：主动询问对端状态 — 经 WS 控制面发送 Query，返回 0 成功 -1 失败
    fn host_filesrv_query_peer(peer_ptr: u32, peer_len: u32) -> i32;
    /// 传输引擎：启动任务 — TransferRequest JSON → out_ptr 输出 task_id，返回 0 成功 -1 失败
    fn host_transfer_start(req_ptr: u32, req_len: u32, out_ptr: u32) -> i32;
    /// 传输引擎：取消任务 — 返回 0 成功，-1 失败
    fn host_transfer_cancel(task_ptr: u32, task_len: u32) -> i32;
    /// 配置：读取宿主配置项 — 结果写入 out_ptr，返回 0 成功，-1 失败
    fn host_config_get(key_ptr: u32, key_len: u32, out_ptr: u32) -> i32;
}

// ==================== WASM Memory Helpers ====================

/// 分配字符串到 WASM 线性内存，返回 (ptr, len)
///
/// 使用与 `wasm_entry!` 宏生成的 `__bedcode_allocate` 相同的
/// `std::alloc` Layout(len, 1) 分配，确保与 `__bedcode_deallocate`
/// 的回收 Layout 精确配对（宿主读参数后回收、插件读结果后回收均依赖此配对）
pub fn wasm_alloc_string(s: &str) -> (u32, u32) {
    if s.is_empty() {
        return (0, 0);
    }
    let bytes = s.as_bytes();
    let len = bytes.len();
    let Ok(layout) = std::alloc::Layout::from_size_align(len, 1) else {
        return (0, 0);
    };
    // SAFETY: layout 大小非零（已判空），ptr 由同 Layout 的 dealloc 配对回收
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        return (0, 0);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len);
    }
    (ptr as u32, len as u32)
}

/// 从 WASM 线性内存读取字符串
pub fn wasm_read_string(ptr: u32, len: u32) -> String {
    if ptr == 0 && len == 0 {
        return String::new();
    }
    // SAFETY: ptr 和 len 由宿主传入，指向 WASM 线性内存中的有效区域
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).into_owned()
    }
}

/// 释放 WASM 线性内存中的字符串（`__bedcode_allocate` / `wasm_alloc_string` 分配的内存）
///
/// 与分配器同 Layout 配对调用，防止长驻插件线性内存单调增长
pub fn wasm_dealloc_string(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    // __bedcode_deallocate 由 wasm_entry! 宏在插件二进制中定义（此处为本地符号声明）
    extern "C" {
        fn __bedcode_deallocate(ptr: u32, len: u32);
    }
    unsafe { __bedcode_deallocate(ptr, len) };
}

/// 读取宿主写入的结果字符串并立即释放对应线性内存
///
/// 宿主通过 `__bedcode_allocate` 写入 out_ptr 结果，插件拷贝为 Rust String 后
/// 原缓冲区即失效 —— 读后立即归还是安全的
pub fn read_and_free_string(ptr: u32, len: u32) -> String {
    let s = wasm_read_string(ptr, len);
    wasm_dealloc_string(ptr, len);
    s
}

/// 将 (ptr, len) 结果写入 out_ptr（8 字节: ptr:u32 + len:u32，小端序）
pub fn wasm_write_result_to_out_ptr(out_ptr: u32, ptr: u32, len: u32) {
    if out_ptr == 0 {
        return;
    }
    // SAFETY: out_ptr 由宿主传入，指向 WASM 线性内存中的 8 字节有效区域
    unsafe {
        let out = out_ptr as *mut u8;
        std::ptr::copy_nonoverlapping(ptr.to_le_bytes().as_ptr(), out, 4);
        std::ptr::copy_nonoverlapping(len.to_le_bytes().as_ptr(), out.add(4), 4);
    }
}
