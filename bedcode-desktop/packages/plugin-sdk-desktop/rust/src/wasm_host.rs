//! WASM 插件侧宿主 API 绑定
//!
//! [`WasmHost`] 以 WASM import 后端实现 `host/*` 全部功能 trait，
//! 插件通过这些调用访问宿主能力。编译为 WASM 时，调用对应宿主在
//! wasmtime Linker 中注册的 host functions。
//! 线性内存字符串通过 (ptr, len) 对传递，使用 wasm_alloc_string/wasm_read_string 辅助。
//!
//! 插件身份（plugin_id）由宿主侧 Caller state 维护并注入各 host function，
//! 插件侧无需持有 —— `WasmHost` 是无状态 unit struct。

use crate::host::{
    ConfigKey, HostBus, HostConfig, HostDatabase, HostError, HostEvents, HostFs, HostHttp, HostLog,
    HostPluginDatabase, HostSession, HostStorage, HostTerminal,
};

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
        if status != 0 {
            return Err(HostError::call_failed("storage_get"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("storage_get: invalid JSON from host: {}", e)))
    }

    fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), HostError> {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let val_str = serde_json::to_string(value).unwrap_or_default();
        let (val_ptr, val_len) = wasm_alloc_string(&val_str);
        let status = unsafe { host_storage_set(key_ptr, key_len, val_ptr, val_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("storage_set"))
        }
    }

    fn storage_delete(&self, key: &str) -> Result<(), HostError> {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let status = unsafe { host_storage_delete(key_ptr, key_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("storage_delete"))
        }
    }
}

// ==================== HostDatabase / HostPluginDatabase ====================

impl HostDatabase for WasmHost {
    fn db_execute(&self, sql: &str) -> Result<i32, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let affected = unsafe { host_db_execute(sql_ptr, sql_len) };
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
        if status != 0 {
            return Err(HostError::call_failed("db_query"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("db_query: invalid JSON from host: {}", e)))
    }

    fn db_execute_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<i32, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        let (params_ptr, params_len) = wasm_alloc_string(&params_str);
        let affected = unsafe { host_db_execute_params(sql_ptr, sql_len, params_ptr, params_len) };
        if affected >= 0 {
            Ok(affected)
        } else {
            Err(HostError::call_failed("db_execute_params"))
        }
    }

    fn db_query_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<Option<serde_json::Value>, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        let (params_ptr, params_len) = wasm_alloc_string(&params_str);
        let mut out = [0u32; 2];
        let status = unsafe {
            host_db_query_params(sql_ptr, sql_len, params_ptr, params_len, out.as_mut_ptr() as u32)
        };
        if status != 0 {
            return Err(HostError::call_failed("db_query_params"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("db_query_params: invalid JSON from host: {}", e)))
    }
}

impl HostPluginDatabase for WasmHost {
    fn plugin_db_execute(&self, sql: &str) -> Result<i32, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let affected = unsafe { host_plugin_db_execute(sql_ptr, sql_len) };
        if affected >= 0 {
            Ok(affected)
        } else {
            Err(HostError::call_failed("plugin_db_execute"))
        }
    }

    fn plugin_db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let mut out = [0u32; 2];
        let status = unsafe { host_plugin_db_query(sql_ptr, sql_len, out.as_mut_ptr() as u32) };
        if status != 0 {
            return Err(HostError::call_failed("plugin_db_query"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("plugin_db_query: invalid JSON from host: {}", e)))
    }

    fn plugin_db_execute_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<i32, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        let (params_ptr, params_len) = wasm_alloc_string(&params_str);
        let affected = unsafe {
            host_plugin_db_execute_params(sql_ptr, sql_len, params_ptr, params_len)
        };
        if affected >= 0 {
            Ok(affected)
        } else {
            Err(HostError::call_failed("plugin_db_execute_params"))
        }
    }

    fn plugin_db_query_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<Option<serde_json::Value>, HostError> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let params_str = serde_json::to_string(params).unwrap_or_else(|_| "[]".to_string());
        let (params_ptr, params_len) = wasm_alloc_string(&params_str);
        let mut out = [0u32; 2];
        let status = unsafe {
            host_plugin_db_query_params(
                sql_ptr,
                sql_len,
                params_ptr,
                params_len,
                out.as_mut_ptr() as u32,
            )
        };
        if status != 0 {
            return Err(HostError::call_failed("plugin_db_query_params"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("plugin_db_query_params: invalid JSON from host: {}", e)))
    }
}

// ==================== HostTerminal ====================

impl HostTerminal for WasmHost {
    fn terminal_send(&self, session_id: &str, data: &str) -> Result<(), HostError> {
        let (sid_ptr, sid_len) = wasm_alloc_string(session_id);
        let (data_ptr, data_len) = wasm_alloc_string(data);
        let status = unsafe { host_terminal_send(sid_ptr, sid_len, data_ptr, data_len) };
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
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("session_list: invalid JSON from host: {}", e)))
    }

    fn session_get(&self, session_id: &str) -> Result<Option<serde_json::Value>, HostError> {
        let (sid_ptr, sid_len) = wasm_alloc_string(session_id);
        let mut out = [0u32; 2];
        let status = unsafe { host_session_get(sid_ptr, sid_len, out.as_mut_ptr() as u32) };
        if status != 0 {
            return Err(HostError::call_failed("session_get"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("session_get: invalid JSON from host: {}", e)))
    }

    fn session_config_list(&self) -> Result<Option<serde_json::Value>, HostError> {
        let mut out = [0u32; 2];
        let status = unsafe { host_session_config_list(out.as_mut_ptr() as u32) };
        if status != 0 {
            return Err(HostError::call_failed("session_config_list"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
        serde_json::from_str(&json_str)
            .map(Some)
            .map_err(|e| HostError::custom(-1, format!("session_config_list: invalid JSON from host: {}", e)))
    }

    fn session_lifecycle_register(&self) -> Result<(), HostError> {
        let status = unsafe { host_session_lifecycle_register() };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("session_lifecycle_register"))
        }
    }

    fn session_input_register(&self) -> Result<(), HostError> {
        let status = unsafe { host_session_input_register() };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("session_input_register"))
        }
    }
}

// ==================== HostEvents ====================

impl HostEvents for WasmHost {
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value) {
        let (name_ptr, name_len) = wasm_alloc_string(event_name);
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = wasm_alloc_string(&payload_str);
        unsafe { host_emit_event(name_ptr, name_len, payload_ptr, payload_len) }
    }

    fn broadcast_sync(&self, event: &crate::events::SyncEvent) {
        // SyncEvent serde 表示即线协议（tag = "type"），宿主侧反序列化为同一类型
        let payload_str = serde_json::to_string(event).unwrap_or_default();
        let (ptr, len) = wasm_alloc_string(&payload_str);
        unsafe { host_broadcast_sync(ptr, len) }
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), HostError> {
        let (title_ptr, title_len) = wasm_alloc_string(title);
        let (body_ptr, body_len) = wasm_alloc_string(body);
        let status = unsafe { host_notify(title_ptr, title_len, body_ptr, body_len) };
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
        if status != 0 {
            return Err(HostError::call_failed("http_fetch"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        let json_str = read_and_free_result(out[0], out[1]);
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
        if status != 0 {
            return Err(HostError::call_failed("fs_read"));
        }
        if out[0] == 0 && out[1] == 0 {
            return Ok(None);
        }
        Ok(Some(read_and_free_result(out[0], out[1])))
    }

    fn fs_write(&self, path: &str, data: &str) -> Result<(), HostError> {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let (data_ptr, data_len) = wasm_alloc_string(data);
        let status = unsafe { host_fs_write(path_ptr, path_len, data_ptr, data_len) };
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
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("fs_copy"))
        }
    }

    fn fs_delete(&self, path: &str) -> Result<(), HostError> {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let status = unsafe { host_fs_delete(path_ptr, path_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("fs_delete"))
        }
    }
}

// ==================== HostLog ====================

impl HostLog for WasmHost {
    fn log_info(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_info(ptr, len) }
    }

    fn log_debug(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_debug(ptr, len) }
    }

    fn log_warn(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_warn(ptr, len) }
    }

    fn log_error(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_error(ptr, len) }
    }

    fn mark_plugin_error(&self, error: &str) {
        let (ptr, len) = wasm_alloc_string(error);
        unsafe { host_mark_plugin_error(ptr, len) }
    }
}

// ==================== HostBus ====================

impl HostBus for WasmHost {
    fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), HostError> {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = wasm_alloc_string(&payload_str);
        let status = unsafe { host_bus_publish(topic_ptr, topic_len, payload_ptr, payload_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("bus_publish"))
        }
    }

    fn bus_subscribe(&self, topic: &str) -> Result<(), HostError> {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        let status = unsafe { host_bus_subscribe(topic_ptr, topic_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("bus_subscribe"))
        }
    }

    fn bus_unsubscribe(&self, topic: &str) -> Result<(), HostError> {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        let status = unsafe { host_bus_unsubscribe(topic_ptr, topic_len) };
        if status == 0 {
            Ok(())
        } else {
            Err(HostError::call_failed("bus_unsubscribe"))
        }
    }
}

// ==================== HostConfig ====================

impl HostConfig for WasmHost {
    fn config_get(&self, key: ConfigKey) -> Result<Option<String>, HostError> {
        let (key_ptr, key_len) = wasm_alloc_string(key.as_str());
        let mut out = [0u32; 2];
        let status = unsafe { host_config_get(key_ptr, key_len, out.as_mut_ptr() as u32) };
        // 宿主对不可用的配置项返回 -1（如 home_dir 解析失败），语义为"无此配置"而非调用错误
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return Ok(None);
        }
        Ok(Some(read_and_free_result(out[0], out[1])))
    }
}

// ==================== WASM Import Declarations ====================
//
// 这些 extern "C" 声明在编译为 WASM 时对应宿主在 wasmtime Linker 中
// 注册的 abi::NAMESPACE（"bedcode"）命名空间下的 host functions。
// 函数名与宿主注册名的一致性由 SDK abi 模块常量保证（宿主侧注册引用同一组常量，
// 且宿主测试 test_host_fn_registration_matches_abi 校验签名）。
// #[link(wasm_import_module)] 确保 WASM 模块从 "bedcode" 命名空间导入，
// 而非默认的 "env" 命名空间。
//
// WASM ABI 约定：返回 (ptr, len) 的函数通过输出参数（out_ptr）传递结果，
// 因为 C ABI 不支持多值返回，Rust 的 (u32, u32) 元组会被编译器
// 拆解为额外的指针参数，导致签名不匹配。
// 宿主端将结果写入 out_ptr 指向的 8 字节内存（ptr: u32 + len: u32）。

#[link(wasm_import_module = "bedcode")]
extern "C" {
    /// 存储：获取值 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_storage_get(key_ptr: u32, key_len: u32, out_ptr: u32) -> i32;
    /// 存储：设置值 — 返回 0 成功，-1 失败
    fn host_storage_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;
    /// 存储：删除值 — 返回 0 成功，-1 失败
    fn host_storage_delete(key_ptr: u32, key_len: u32) -> i32;
    /// 数据库：执行 SQL — 返回受影响行数
    fn host_db_execute(sql_ptr: u32, sql_len: u32) -> i32;
    /// 数据库：查询 SQL — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_db_query(sql_ptr: u32, sql_len: u32, out_ptr: u32) -> i32;
    /// 数据库：执行 SQL 参数绑定版 — params 为 JSON 数组字符串，返回受影响行数
    fn host_db_execute_params(sql_ptr: u32, sql_len: u32, params_ptr: u32, params_len: u32) -> i32;
    /// 数据库：查询 SQL 参数绑定版 — 结果写入 out_ptr（8 字节: ptr + len）
    fn host_db_query_params(sql_ptr: u32, sql_len: u32, params_ptr: u32, params_len: u32, out_ptr: u32) -> i32;
    /// 插件独立数据库：执行 SQL — 返回受影响行数
    fn host_plugin_db_execute(sql_ptr: u32, sql_len: u32) -> i32;
    /// 插件独立数据库：查询 SQL — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_plugin_db_query(sql_ptr: u32, sql_len: u32, out_ptr: u32) -> i32;
    /// 插件独立数据库：执行 SQL 参数绑定版 — params 为 JSON 数组字符串，返回受影响行数
    fn host_plugin_db_execute_params(sql_ptr: u32, sql_len: u32, params_ptr: u32, params_len: u32) -> i32;
    /// 插件独立数据库：查询 SQL 参数绑定版 — 结果写入 out_ptr（8 字节: ptr + len）
    fn host_plugin_db_query_params(sql_ptr: u32, sql_len: u32, params_ptr: u32, params_len: u32, out_ptr: u32) -> i32;
    /// 终端：发送输入 — 返回 0 成功，-1 失败
    fn host_terminal_send(sid_ptr: u32, sid_len: u32, data_ptr: u32, data_len: u32) -> i32;
    /// 会话：列出所有 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_session_list(out_ptr: u32) -> i32;
    /// 会话：获取单个 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_session_get(sid_ptr: u32, sid_len: u32, out_ptr: u32) -> i32;
    /// 会话配置：列出所有 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_session_config_list(out_ptr: u32) -> i32;
    /// 事件：向前端发送
    fn host_emit_event(name_ptr: u32, name_len: u32, payload_ptr: u32, payload_len: u32);
    /// HTTP 代理 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_http_fetch(req_ptr: u32, req_len: u32, out_ptr: u32) -> i32;
    /// 文件系统：读取文件 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_fs_read(path_ptr: u32, path_len: u32, out_ptr: u32) -> i32;
    /// 文件系统：写入文件 — 返回 0 成功，-1 失败
    fn host_fs_write(path_ptr: u32, path_len: u32, data_ptr: u32, data_len: u32) -> i32;
    /// 文件系统：复制文件 — 返回 0 成功，-1 失败
    fn host_fs_copy(src_ptr: u32, src_len: u32, dst_ptr: u32, dst_len: u32) -> i32;
    /// 文件系统：删除文件 — 返回 0 成功（含文件不存在），-1 失败
    fn host_fs_delete(path_ptr: u32, path_len: u32) -> i32;
    /// 配置：读取配置项 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_config_get(key_ptr: u32, key_len: u32, out_ptr: u32) -> i32;
    /// 日志：info
    fn host_log_info(msg_ptr: u32, msg_len: u32);
    /// 日志：debug
    fn host_log_debug(msg_ptr: u32, msg_len: u32);
    /// 日志：warn
    fn host_log_warn(msg_ptr: u32, msg_len: u32);
    /// 日志：error
    fn host_log_error(msg_ptr: u32, msg_len: u32);
    /// 插件状态：标记错误（无返回值）
    fn host_mark_plugin_error(err_ptr: u32, err_len: u32);
    /// 广播：同步事件到所有客户端
    fn host_broadcast_sync(payload_ptr: u32, payload_len: u32);
    /// 通知：发送系统通知 — 返回 0 成功，-1 失败
    fn host_notify(title_ptr: u32, title_len: u32, body_ptr: u32, body_len: u32) -> i32;
    /// 消息总线：发布消息 — 返回 0 成功，-1 失败
    fn host_bus_publish(topic_ptr: u32, topic_len: u32, payload_ptr: u32, payload_len: u32) -> i32;
    /// 消息总线：订阅 topic — 返回 0 成功，-1 失败
    fn host_bus_subscribe(topic_ptr: u32, topic_len: u32) -> i32;
    /// 消息总线：取消订阅 — 返回 0 成功，-1 失败
    fn host_bus_unsubscribe(topic_ptr: u32, topic_len: u32) -> i32;
    /// 会话生命周期：注册监听器 — 返回 0 成功，-1 失败
    fn host_session_lifecycle_register() -> i32;
    /// 会话输入：注册提交输入行监听器 — 返回 0 成功，-1 失败（含权限拒绝）
    fn host_session_input_register() -> i32;
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
fn read_and_free_result(ptr: u32, len: u32) -> String {
    let s = wasm_read_string(ptr, len);
    wasm_dealloc_string(ptr, len);
    s
}

/// 将 (ptr, len) 结果写入 WASM 线性内存中的 out_ptr 位置（8 字节: ptr:u32 + len:u32）
///
/// 用于 wasm_entry! 宏生成的导出函数，将返回值通过 out_ptr 输出参数传递给宿主，
/// 而非 Rust 元组返回值（C ABI 会将元组拆解为额外指针参数，导致签名不匹配）。
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
