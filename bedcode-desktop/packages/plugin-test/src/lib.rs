//! 测试用 WASM 插件（core module 形态，自研 ABI）
//!
//! 用于宿主 core 路径（`wasm_runtime` 测试套件的 `build_test_wasm` 路径）的
//! 连通性测试：覆盖全部 host function 调用路径（存储 / 数据库 / 配置 / 日志 /
//! 事件 / 会话 / 文件系统 / 广播 / 消息总线 / 通知）。
//!
//! 迁移阶段 B 起 SDK 只产出组件（Component Model），本插件为保留 core 形态
//! 测试载体而自包含：不依赖 SDK 的 wasm feature（宏 / WasmHost），
//! 自行声明 `bedcode` 命名空间 import 与 `__bedcode_*` 导出（旧 ABI 胶水）。
//! 阶段 C 宿主清理 core 路径后本插件随之删除。

use bedcode_plugin_api::events::SyncEvent;

// ==================== WASM Import Declarations ====================
//
// 对应宿主在 wasmtime Linker 的 abi::NAMESPACE（"bedcode"）下注册的 host
// functions。#[link(wasm_import_module)] 确保从 "bedcode" 命名空间导入。
// 宿主侧注册同名常量引用同一组常量，签名一致性由宿主测试
// test_host_fn_registration_matches_abi 校验。

#[link(wasm_import_module = "bedcode")]
extern "C" {
    fn host_storage_get(key_ptr: u32, key_len: u32, out_ptr: u32) -> i32;
    fn host_storage_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;
    fn host_plugin_db_execute(sql_ptr: u32, sql_len: u32) -> i32;
    fn host_plugin_db_query(sql_ptr: u32, sql_len: u32, out_ptr: u32) -> i32;
    fn host_config_get(key_ptr: u32, key_len: u32, out_ptr: u32) -> i32;
    fn host_log_info(msg_ptr: u32, msg_len: u32, file_ptr: u32, file_len: u32, line: u32);
    fn host_log_debug(msg_ptr: u32, msg_len: u32, file_ptr: u32, file_len: u32, line: u32);
    fn host_log_warn(msg_ptr: u32, msg_len: u32, file_ptr: u32, file_len: u32, line: u32);
    fn host_log_error(msg_ptr: u32, msg_len: u32, file_ptr: u32, file_len: u32, line: u32);
    fn host_emit_event(name_ptr: u32, name_len: u32, payload_ptr: u32, payload_len: u32);
    fn host_session_list(out_ptr: u32) -> i32;
    fn host_fs_write(path_ptr: u32, path_len: u32, data_ptr: u32, data_len: u32) -> i32;
    fn host_fs_read(path_ptr: u32, path_len: u32, out_ptr: u32) -> i32;
    fn host_broadcast_sync(payload_ptr: u32, payload_len: u32);
    fn host_bus_publish(topic_ptr: u32, topic_len: u32, payload_ptr: u32, payload_len: u32) -> i32;
    fn host_notify(title_ptr: u32, title_len: u32, body_ptr: u32, body_len: u32) -> i32;
}

// ==================== WASM Memory Helpers ====================

/// 分配字符串到 WASM 线性内存，返回 (ptr, len)
///
/// 与 `__bedcode_allocate` 同 Layout(len, 1) 分配，确保回收配对
fn alloc_string(s: &str) -> (u32, u32) {
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
fn read_string(ptr: u32, len: u32) -> String {
    if ptr == 0 && len == 0 {
        return String::new();
    }
    // SAFETY: ptr/len 由宿主传入，指向 WASM 线性内存中的有效区域
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).into_owned()
    }
}

/// 释放 WASM 线性内存中的字符串
fn dealloc_string(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    if let Ok(layout) = std::alloc::Layout::from_size_align(len as usize, 1) {
        // SAFETY: ptr 由 alloc_string / __bedcode_allocate 以相同 Layout 分配
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
    }
}

/// 读取宿主写入的结果字符串并立即释放对应线性内存
fn read_and_free_result(ptr: u32, len: u32) -> String {
    let s = read_string(ptr, len);
    dealloc_string(ptr, len);
    s
}

/// 将 (ptr, len) 结果写入 out_ptr（8 字节: ptr:u32 + len:u32）
fn write_result_to_out_ptr(out_ptr: u32, ptr: u32, len: u32) {
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

// ==================== Host 调用包装（旧 ABI 语义） ====================

fn call_host_storage_get(key: &str) -> Option<serde_json::Value> {
    let (key_ptr, key_len) = alloc_string(key);
    let mut out = [0u32; 2];
    let status = unsafe { host_storage_get(key_ptr, key_len, out.as_mut_ptr() as u32) };
    if status != 0 || (out[0] == 0 && out[1] == 0) {
        return None;
    }
    serde_json::from_str(&read_and_free_result(out[0], out[1])).ok()
}

fn call_host_storage_set(key: &str, value: &serde_json::Value) -> bool {
    let (key_ptr, key_len) = alloc_string(key);
    let val_str = serde_json::to_string(value).unwrap_or_default();
    let (val_ptr, val_len) = alloc_string(&val_str);
    let status = unsafe { host_storage_set(key_ptr, key_len, val_ptr, val_len) };
    status == 0
}

fn call_host_plugin_db_execute(sql: &str) -> bool {
    let (sql_ptr, sql_len) = alloc_string(sql);
    let affected = unsafe { host_plugin_db_execute(sql_ptr, sql_len) };
    affected >= 0
}

fn call_host_plugin_db_query(sql: &str) -> Option<serde_json::Value> {
    let (sql_ptr, sql_len) = alloc_string(sql);
    let mut out = [0u32; 2];
    let status = unsafe { host_plugin_db_query(sql_ptr, sql_len, out.as_mut_ptr() as u32) };
    if status != 0 || (out[0] == 0 && out[1] == 0) {
        return None;
    }
    serde_json::from_str(&read_and_free_result(out[0], out[1])).ok()
}

fn call_host_config_get(key: &str) -> Option<String> {
    let (key_ptr, key_len) = alloc_string(key);
    let mut out = [0u32; 2];
    let status = unsafe { host_config_get(key_ptr, key_len, out.as_mut_ptr() as u32) };
    if status != 0 || (out[0] == 0 && out[1] == 0) {
        return None;
    }
    Some(read_and_free_result(out[0], out[1]))
}

fn host_log(level: u32, message: &str) {
    let (msg_ptr, msg_len) = alloc_string(message);
    // 调用点 file/line：测试插件无真实源码位置，传空串与 0
    let (file_ptr, file_len) = (0u32, 0u32);
    unsafe {
        match level {
            0 => host_log_info(msg_ptr, msg_len, file_ptr, file_len, 0),
            1 => host_log_debug(msg_ptr, msg_len, file_ptr, file_len, 0),
            2 => host_log_warn(msg_ptr, msg_len, file_ptr, file_len, 0),
            _ => host_log_error(msg_ptr, msg_len, file_ptr, file_len, 0),
        }
    }
}

fn call_host_emit_event(event_name: &str, payload: &serde_json::Value) {
    let (name_ptr, name_len) = alloc_string(event_name);
    let payload_str = serde_json::to_string(payload).unwrap_or_default();
    let (payload_ptr, payload_len) = alloc_string(&payload_str);
    unsafe { host_emit_event(name_ptr, name_len, payload_ptr, payload_len) }
}

fn call_host_session_list() -> Option<serde_json::Value> {
    let mut out = [0u32; 2];
    let status = unsafe { host_session_list(out.as_mut_ptr() as u32) };
    if status != 0 || (out[0] == 0 && out[1] == 0) {
        return None;
    }
    serde_json::from_str(&read_and_free_result(out[0], out[1])).ok()
}

fn call_host_fs_write(path: &str, content: &str) -> bool {
    let (path_ptr, path_len) = alloc_string(path);
    let (data_ptr, data_len) = alloc_string(content);
    let status = unsafe { host_fs_write(path_ptr, path_len, data_ptr, data_len) };
    status == 0
}

fn call_host_fs_read(path: &str) -> Option<String> {
    let (path_ptr, path_len) = alloc_string(path);
    let mut out = [0u32; 2];
    let status = unsafe { host_fs_read(path_ptr, path_len, out.as_mut_ptr() as u32) };
    if status != 0 || (out[0] == 0 && out[1] == 0) {
        return None;
    }
    Some(read_and_free_result(out[0], out[1]))
}

fn call_host_broadcast_sync(event: &SyncEvent) {
    let payload_str = serde_json::to_string(event).unwrap_or_default();
    let (ptr, len) = alloc_string(&payload_str);
    unsafe { host_broadcast_sync(ptr, len) }
}

fn call_host_bus_publish(topic: &str, payload: &serde_json::Value) -> bool {
    let (topic_ptr, topic_len) = alloc_string(topic);
    let payload_str = serde_json::to_string(payload).unwrap_or_default();
    let (payload_ptr, payload_len) = alloc_string(&payload_str);
    let status = unsafe { host_bus_publish(topic_ptr, topic_len, payload_ptr, payload_len) };
    status == 0
}

fn call_host_notify(title: &str, body: &str) -> bool {
    let (title_ptr, title_len) = alloc_string(title);
    let (body_ptr, body_len) = alloc_string(body);
    let status = unsafe { host_notify(title_ptr, title_len, body_ptr, body_len) };
    status == 0
}

// ==================== 业务逻辑（命令分发） ====================

fn manifest_json() -> String {
    let json = serde_json::json!({
        "id": "com.bedcode.test",
        "name": "Test Plugin",
        "version": "0.1.0",
        "description": "Glue layer test plugin",
        "author": "BedCode",
        "main": "index.js",
        "sandbox": "inline",
        "pluginType": "rust-ts",
        "rustLibrary": "bedcode_plugin_test",
        "permissions": ["storage", "broadcast", "terminal:input", "terminal:output", "session:read", "fs:read", "fs:write"],
        "contributes": {
            "commands": [{ "id": "test.echo", "title": "Echo" }],
            "views": [],
            "lifecycle": { "onStartup": false, "onShutdown": false },
            "provides": []
        }
    });
    serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string())
}

/// 命令分发（对应旧 SDK WasmPlugin::invoke_command 逻辑）
fn invoke_command(name: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    match name {
        "test.echo" => Ok(args),
        "test_storage" => {
            let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("test_key");
            let value = args.get("value").cloned().unwrap_or(serde_json::json!("test_value"));
            if !call_host_storage_set(key, &value) {
                return Err("storage_set failed".to_string());
            }
            let got = call_host_storage_get(key).unwrap_or(serde_json::Value::Null);
            Ok(serde_json::json!({ "set": value, "got": got }))
        }
        "test_db" => {
            let create = "CREATE TABLE IF NOT EXISTS plugin_com_bedcode_test_data (id INTEGER PRIMARY KEY, val TEXT)";
            if !call_host_plugin_db_execute(create) {
                return Err("plugin_db_execute (create) failed".to_string());
            }
            if !call_host_plugin_db_execute("INSERT OR REPLACE INTO plugin_com_bedcode_test_data (id, val) VALUES (1, 'hello')") {
                return Err("plugin_db_execute (insert) failed".to_string());
            }
            let rows = call_host_plugin_db_query("SELECT val FROM plugin_com_bedcode_test_data WHERE id = 1")
                .unwrap_or(serde_json::Value::Null);
            Ok(serde_json::json!({ "rows": rows }))
        }
        "test_config" => {
            let port = call_host_config_get(bedcode_plugin_api::host::ConfigKey::NetworkPort.as_str())
                .unwrap_or_default();
            Ok(serde_json::json!({ "port": port }))
        }
        "test_log" => {
            host_log(0, "test info");
            host_log(1, "test debug");
            host_log(2, "test warn");
            host_log(3, "test error");
            Ok(serde_json::json!({ "logged": true }))
        }
        "test_emit" => {
            call_host_emit_event("test-event", &serde_json::json!({ "source": "test_plugin" }));
            Ok(serde_json::json!({ "emitted": true }))
        }
        "test_session_list" => {
            let sessions = call_host_session_list().unwrap_or(serde_json::Value::Null);
            Ok(serde_json::json!({ "sessions": sessions }))
        }
        "test_fs" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("hello fs");
            if !call_host_fs_write(path, content) {
                return Err("fs_write failed".to_string());
            }
            let read = call_host_fs_read(path).unwrap_or_default();
            Ok(serde_json::json!({ "wrote": content, "read": read }))
        }
        "test_broadcast" => {
            call_host_broadcast_sync(&SyncEvent::TaskStatusChanged {
                session_id: "test-session".to_string(),
                task_status: "completed".to_string(),
                task_reason: None,
                task_questions: None,
            });
            Ok(serde_json::json!({ "broadcast": true }))
        }
        "test_bus" => {
            if !call_host_bus_publish("test:topic", &serde_json::json!({ "msg": "hello" })) {
                return Err("bus_publish failed".to_string());
            }
            Ok(serde_json::json!({ "published": true }))
        }
        "test_notify" => {
            if !call_host_notify("test title", "test body") {
                return Err("notify failed".to_string());
            }
            Ok(serde_json::json!({ "notified": true }))
        }
        _ => Err(format!("Unknown command: {}", name)),
    }
}

// ==================== WASM Exports（旧 ABI 导出，与宿主 core 路径契约一致） ====================

/// ABI 版本协商 — 宿主实例化后读取，与 `abi::ABI_VERSION` 比对
#[no_mangle]
pub extern "C" fn __bedcode_abi_version() -> i32 {
    bedcode_plugin_api::abi::ABI_VERSION as i32
}

/// 内存分配器 — 供宿主写入字符串到 WASM 线性内存
#[no_mangle]
pub extern "C" fn __bedcode_allocate(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    match std::alloc::Layout::from_size_align(len, 1) {
        Ok(layout) => unsafe { std::alloc::alloc(layout) },
        Err(_) => std::ptr::null_mut(),
    }
}

/// 内存回收器 — 释放 `__bedcode_allocate` / `alloc_string` 分配的内存
#[no_mangle]
pub extern "C" fn __bedcode_deallocate(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    if let Ok(layout) = std::alloc::Layout::from_size_align(len as usize, 1) {
        // SAFETY: ptr 由同模块 __bedcode_allocate / alloc_string 以相同 Layout 分配
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
    }
}

/// 返回 manifest JSON — 结果写入 out_ptr（8 字节: ptr + len）
#[no_mangle]
pub extern "C" fn __bedcode_manifest(out_ptr: u32) {
    let json = manifest_json();
    let (ptr, len) = alloc_string(&json);
    write_result_to_out_ptr(out_ptr, ptr, len);
}

/// 激活插件
#[no_mangle]
pub extern "C" fn __bedcode_activate() -> i32 {
    host_log(0, "Plugin activated (test)");
    0
}

/// 停用插件
#[no_mangle]
pub extern "C" fn __bedcode_deactivate() -> i32 {
    0
}

/// 调用自定义命令 — 结果写入 out_ptr（8 字节: ptr + len）
#[no_mangle]
pub extern "C" fn __bedcode_invoke_command(
    name_ptr: u32,
    name_len: u32,
    args_ptr: u32,
    args_len: u32,
    out_ptr: u32,
) {
    let name = read_string(name_ptr, name_len);
    let args_str = read_string(args_ptr, args_len);
    // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
    dealloc_string(name_ptr, name_len);
    dealloc_string(args_ptr, args_len);
    // ABI 字符串 → 类型化 JSON（解析失败时为 Null，由插件自行容错）
    let args: serde_json::Value =
        serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);

    let result_str = match invoke_command(&name, args) {
        Ok(value) => match serde_json::to_string(&value) {
            Ok(s) => s,
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        },
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    };
    let (ptr, len) = alloc_string(&result_str);
    write_result_to_out_ptr(out_ptr, ptr, len);
}

/// 终端输入处理 — 结果写入 out_ptr（8 字节: ptr + len）
#[no_mangle]
pub extern "C" fn __bedcode_on_terminal_input(
    sid_ptr: u32,
    sid_len: u32,
    text_ptr: u32,
    text_len: u32,
    out_ptr: u32,
) {
    let session_id = read_string(sid_ptr, sid_len);
    let text = read_string(text_ptr, text_len);
    dealloc_string(sid_ptr, sid_len);
    dealloc_string(text_ptr, text_len);

    // 与组件形态 plugin-component-test 行为对齐（大写转换），宿主测试断言同一语义
    let modified = text.to_uppercase();
    let _ = session_id;
    let (ptr, len) = alloc_string(&modified);
    write_result_to_out_ptr(out_ptr, ptr, len);
}

/// 终端输出处理 — 结果写入 out_ptr（8 字节: ptr + len）
#[no_mangle]
pub extern "C" fn __bedcode_on_terminal_output(
    sid_ptr: u32,
    sid_len: u32,
    data_ptr: u32,
    data_len: u32,
    out_ptr: u32,
) {
    let session_id = read_string(sid_ptr, sid_len);
    let data = read_string(data_ptr, data_len);
    dealloc_string(sid_ptr, sid_len);
    dealloc_string(data_ptr, data_len);

    let modified = data.to_uppercase();
    let _ = session_id;
    let (ptr, len) = alloc_string(&modified);
    write_result_to_out_ptr(out_ptr, ptr, len);
}

/// 应用启动完成回调
#[no_mangle]
pub extern "C" fn __bedcode_on_startup() {}

/// 应用即将关闭回调
#[no_mangle]
pub extern "C" fn __bedcode_on_shutdown() {}

/// 接收消息总线消息
#[no_mangle]
pub extern "C" fn __bedcode_on_message(
    topic_ptr: u32,
    topic_len: u32,
    sender_ptr: u32,
    sender_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) -> i32 {
    let topic = read_string(topic_ptr, topic_len);
    let sender = read_string(sender_ptr, sender_len);
    let payload_str = read_string(payload_ptr, payload_len);
    dealloc_string(topic_ptr, topic_len);
    dealloc_string(sender_ptr, sender_len);
    dealloc_string(payload_ptr, payload_len);
    host_log(0, &format!("test plugin on_message: {} from {}", topic, sender));
    let _ = payload_str;
    0
}

/// 接收会话生命周期事件
#[no_mangle]
pub extern "C" fn __bedcode_on_session_lifecycle(payload_ptr: u32, payload_len: u32) -> i32 {
    let payload_str = read_string(payload_ptr, payload_len);
    dealloc_string(payload_ptr, payload_len);
    let _ = payload_str;
    0
}

/// 接收提交输入行事件
#[no_mangle]
pub extern "C" fn __bedcode_on_input_submitted(payload_ptr: u32, payload_len: u32) -> i32 {
    let payload_str = read_string(payload_ptr, payload_len);
    dealloc_string(payload_ptr, payload_len);
    let _ = payload_str;
    0
}

/// 上传请求策略钩子 — 决定 JSON 写入 out_ptr（8 字节: ptr + len）
///
/// fail-closed：入参解析失败时直接拒绝，不调用插件逻辑
#[no_mangle]
pub extern "C" fn __bedcode_on_upload_request(meta_ptr: u32, meta_len: u32, out_ptr: u32) -> i32 {
    let meta_str = read_string(meta_ptr, meta_len);
    dealloc_string(meta_ptr, meta_len);

    let decision = serde_json::from_str::<bedcode_plugin_api::types::UploadRequestMeta>(&meta_str)
        .map(|meta| {
            let _ = meta;
            // 与组件形态 plugin-component-test 同语义：固定拒绝并附原因
            bedcode_plugin_api::types::UploadHookDecision::deny(format!(
                "test-plugin deny ({})",
                meta_str.len()
            ))
        })
        .unwrap_or_else(|_| {
            bedcode_plugin_api::types::UploadHookDecision::deny("invalid upload request meta")
        });

    let json = serde_json::to_string(&decision)
        .unwrap_or_else(|_| r#"{"allow":false,"reason":"serialize decision failed"}"#.to_string());
    let (ptr, len) = alloc_string(&json);
    write_result_to_out_ptr(out_ptr, ptr, len);
    0
}
