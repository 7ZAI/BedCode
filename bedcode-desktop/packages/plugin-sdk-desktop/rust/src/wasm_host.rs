//! WASM 插件侧宿主 API 绑定
//!
//! 插件通过这些函数调用宿主能力。
//! 编译为 WASM 时，这些函数对应宿主在 wasmtime Linker 中注册的 host functions。
//! 线性内存字符串通过 (ptr, len) 对传递，使用 wasm_alloc_string/wasm_read_string 辅助

use std::sync::OnceLock;

/// 宿主 API 绑定（WASM 插件侧）
///
/// 通过 WASM import 调用宿主注册的 host functions
pub struct WasmHost {
    /// 插件 ID（注入到部分 host function 参数中）
    plugin_id: OnceLock<String>,
}

impl WasmHost {
    /// 创建 WasmHost
    pub fn new(plugin_id: &str) -> Self {
        let host = Self {
            plugin_id: OnceLock::new(),
        };
        let _ = host.plugin_id.set(plugin_id.to_string());
        host
    }

    /// 获取插件 ID
    pub fn plugin_id(&self) -> &str {
        self.plugin_id.get().map(|s| s.as_str()).unwrap_or("")
    }

    // ==================== Storage ====================

    /// 存储：获取值
    pub fn storage_get(&self, key: &str) -> Option<serde_json::Value> {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let mut out = [0u32; 2];
        let status = unsafe { host_storage_get(key_ptr, key_len, out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let json_str = wasm_read_string(out[0], out[1]);
        serde_json::from_str(&json_str).ok()
    }

    /// 存储：设置值
    pub fn storage_set(&self, key: &str, value: &serde_json::Value) -> bool {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let val_str = serde_json::to_string(value).unwrap_or_default();
        let (val_ptr, val_len) = wasm_alloc_string(&val_str);
        unsafe { host_storage_set(key_ptr, key_len, val_ptr, val_len) == 0 }
    }

    /// 存储：删除值
    pub fn storage_delete(&self, key: &str) -> bool {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        unsafe { host_storage_delete(key_ptr, key_len) == 0 }
    }

    // ==================== Database ====================

    /// 数据库：执行 SQL
    pub fn db_execute(&self, sql: &str) -> i32 {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        unsafe { host_db_execute(sql_ptr, sql_len) }
    }

    /// 数据库：查询 SQL
    pub fn db_query(&self, sql: &str) -> Option<serde_json::Value> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let mut out = [0u32; 2];
        let status = unsafe { host_db_query(sql_ptr, sql_len, out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let json_str = wasm_read_string(out[0], out[1]);
        serde_json::from_str(&json_str).ok()
    }

    // ==================== Plugin Database ====================

    /// 插件独立数据库：执行 SQL
    ///
    /// 操作插件专属 .db 文件，无表名前缀限制
    pub fn plugin_db_execute(&self, sql: &str) -> i32 {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        unsafe { host_plugin_db_execute(sql_ptr, sql_len) }
    }

    /// 插件独立数据库：查询 SQL
    ///
    /// 操作插件专属 .db 文件，无表名前缀限制
    pub fn plugin_db_query(&self, sql: &str) -> Option<serde_json::Value> {
        let (sql_ptr, sql_len) = wasm_alloc_string(sql);
        let mut out = [0u32; 2];
        let status = unsafe { host_plugin_db_query(sql_ptr, sql_len, out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let json_str = wasm_read_string(out[0], out[1]);
        serde_json::from_str(&json_str).ok()
    }

    // ==================== Terminal ====================

    /// 终端：发送输入
    pub fn terminal_send(&self, session_id: &str, data: &str) -> bool {
        let (sid_ptr, sid_len) = wasm_alloc_string(session_id);
        let (data_ptr, data_len) = wasm_alloc_string(data);
        unsafe { host_terminal_send(sid_ptr, sid_len, data_ptr, data_len) == 0 }
    }

    // ==================== Session ====================

    /// 会话：列出所有
    pub fn session_list(&self) -> Option<serde_json::Value> {
        let mut out = [0u32; 2];
        let status = unsafe { host_session_list(out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let json_str = wasm_read_string(out[0], out[1]);
        serde_json::from_str(&json_str).ok()
    }

    /// 会话：获取单个
    pub fn session_get(&self, session_id: &str) -> Option<serde_json::Value> {
        let (sid_ptr, sid_len) = wasm_alloc_string(session_id);
        let mut out = [0u32; 2];
        let status = unsafe { host_session_get(sid_ptr, sid_len, out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let json_str = wasm_read_string(out[0], out[1]);
        serde_json::from_str(&json_str).ok()
    }

    // ==================== Event ====================

    /// 事件：向前端发送
    pub fn emit_event(&self, event_name: &str, payload: &serde_json::Value) {
        let (name_ptr, name_len) = wasm_alloc_string(event_name);
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = wasm_alloc_string(&payload_str);
        unsafe { host_emit_event(name_ptr, name_len, payload_ptr, payload_len) }
    }

    // ==================== HTTP ====================

    /// HTTP 代理：发起 HTTP 请求
    pub fn http_fetch(&self, request: &serde_json::Value) -> Option<serde_json::Value> {
        let req_str = serde_json::to_string(request).unwrap_or_default();
        let (req_ptr, req_len) = wasm_alloc_string(&req_str);
        let mut out = [0u32; 2];
        let status = unsafe { host_http_fetch(req_ptr, req_len, out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let json_str = wasm_read_string(out[0], out[1]);
        serde_json::from_str(&json_str).ok()
    }

    // ==================== Logging ====================

    /// 日志：info 级别
    pub fn log_info(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_info(ptr, len) }
    }

    /// 日志：debug 级别
    pub fn log_debug(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_debug(ptr, len) }
    }

    /// 日志：warn 级别
    pub fn log_warn(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_warn(ptr, len) }
    }

    /// 日志：error 级别
    pub fn log_error(&self, message: &str) {
        let (ptr, len) = wasm_alloc_string(message);
        unsafe { host_log_error(ptr, len) }
    }

    // ==================== File System ====================

    /// 文件系统：读取文件内容
    pub fn fs_read(&self, path: &str) -> Option<String> {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let mut out = [0u32; 2];
        let status = unsafe { host_fs_read(path_ptr, path_len, out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let content = wasm_read_string(out[0], out[1]);
        Some(content)
    }

    /// 文件系统：写入文件内容（自动创建父目录）
    pub fn fs_write(&self, path: &str, data: &str) -> bool {
        let (path_ptr, path_len) = wasm_alloc_string(path);
        let (data_ptr, data_len) = wasm_alloc_string(data);
        unsafe { host_fs_write(path_ptr, path_len, data_ptr, data_len) == 0 }
    }

    /// 文件系统：复制文件（自动创建目标父目录）
    pub fn fs_copy(&self, src: &str, dst: &str) -> bool {
        let (src_ptr, src_len) = wasm_alloc_string(src);
        let (dst_ptr, dst_len) = wasm_alloc_string(dst);
        unsafe { host_fs_copy(src_ptr, src_len, dst_ptr, dst_len) == 0 }
    }

    // ==================== Config ====================

    /// 配置：读取宿主配置项（白名单限制）
    pub fn config_get(&self, key: &str) -> Option<String> {
        let (key_ptr, key_len) = wasm_alloc_string(key);
        let mut out = [0u32; 2];
        let status = unsafe { host_config_get(key_ptr, key_len, out.as_mut_ptr() as u32) };
        if status != 0 || (out[0] == 0 && out[1] == 0) {
            return None;
        }
        let val = wasm_read_string(out[0], out[1]);
        Some(val)
    }

    // ==================== Broadcast ====================

    /// 广播同步事件到所有客户端（移动端同步通道）
    ///
    /// payload 必须包含 `type` 字段，支持的类型：
    /// - `TaskStatusChanged`: { type, session_id, task_status, task_reason?, task_questions? }
    /// - `SessionModeChanged`: { type, session_id, auto_approve }
    pub fn broadcast_sync(&self, payload: &serde_json::Value) {
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (ptr, len) = wasm_alloc_string(&payload_str);
        unsafe { host_broadcast_sync(ptr, len) }
    }

    // ==================== Message Bus ====================

    /// 发布消息到总线
    pub fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> bool {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = wasm_alloc_string(&payload_str);
        unsafe { host_bus_publish(topic_ptr, topic_len, payload_ptr, payload_len) == 0 }
    }

    /// 订阅 topic
    pub fn bus_subscribe(&self, topic: &str) -> bool {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        unsafe { host_bus_subscribe(topic_ptr, topic_len) == 0 }
    }

    /// 取消订阅
    pub fn bus_unsubscribe(&self, topic: &str) -> bool {
        let (topic_ptr, topic_len) = wasm_alloc_string(topic);
        unsafe { host_bus_unsubscribe(topic_ptr, topic_len) == 0 }
    }

    // ==================== Session Lifecycle ====================

    /// 注册会话生命周期监听器
    ///
    /// 调用后，宿主为该插件创建一个 SessionLifecycleListener 并注册到 SessionManager。
    /// 生命周期事件通过 on_session_lifecycle 回调接收（不走消息总线）：
    /// - event_type="creating": 会话创建前（同步阻塞）
    /// - event_type="created": 会话创建后
    /// - event_type="stopping": 会话停止前
    /// - event_type="stopped": 会话停止后
    pub fn session_lifecycle_register(&self) -> bool {
        unsafe { host_session_lifecycle_register() == 0 }
    }

    // ==================== Notification ====================

    /// 通知：发送系统通知（移动端特有，桌面端为空操作）
    pub fn notify(&self, title: &str, body: &str) -> bool {
        let (title_ptr, title_len) = wasm_alloc_string(title);
        let (body_ptr, body_len) = wasm_alloc_string(body);
        unsafe { host_notify(title_ptr, title_len, body_ptr, body_len) == 0 }
    }
}

// ==================== WASM Import Declarations ====================
//
// 这些 extern "C" 声明在编译为 WASM 时对应宿主在 wasmtime Linker 中
// 注册的 "bedcode" 命名空间下的 host functions
// #[link(wasm_import_module)] 确保 WASM 模块从 "bedcode" 命名空间导入，
// 而非默认的 "env" 命名空间
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
    /// 插件独立数据库：执行 SQL — 返回受影响行数
    fn host_plugin_db_execute(sql_ptr: u32, sql_len: u32) -> i32;
    /// 插件独立数据库：查询 SQL — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_plugin_db_query(sql_ptr: u32, sql_len: u32, out_ptr: u32) -> i32;
    /// 终端：发送输入 — 返回 0 成功，-1 失败
    fn host_terminal_send(sid_ptr: u32, sid_len: u32, data_ptr: u32, data_len: u32) -> i32;
    /// 会话：列出所有 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_session_list(out_ptr: u32) -> i32;
    /// 会话：获取单个 — 结果写入 out_ptr（8 字节: ptr + len），返回 0 成功 -1 失败
    fn host_session_get(sid_ptr: u32, sid_len: u32, out_ptr: u32) -> i32;
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
}

// ==================== WASM Memory Helpers ====================

/// 分配字符串到 WASM 线性内存，返回 (ptr, len)
///
/// 直接在 WASM 线性内存中分配空间（不依赖外部 __bedcode_allocate，
/// 避免与 wasm_entry! 宏生成的 __bedcode_allocate 重复符号冲突）
pub fn wasm_alloc_string(s: &str) -> (u32, u32) {
    if s.is_empty() {
        return (0, 0);
    }
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    let ptr: *mut u8 = buf.as_mut_ptr();
    std::mem::forget(buf);
    if ptr.is_null() {
        return (0, 0);
    }
    // SAFETY: ptr 由 Vec::with_capacity 分配，大小为 len 字节
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
