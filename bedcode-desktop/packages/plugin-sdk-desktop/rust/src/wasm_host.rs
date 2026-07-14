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
        let (ptr, len) = unsafe { host_storage_get(key_ptr, key_len) };
        if ptr == 0 && len == 0 {
            return None;
        }
        let json_str = wasm_read_string(ptr, len);
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
        let (ptr, len) = unsafe { host_db_query(sql_ptr, sql_len) };
        if ptr == 0 && len == 0 {
            return None;
        }
        let json_str = wasm_read_string(ptr, len);
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
        let (ptr, len) = unsafe { host_session_list() };
        if ptr == 0 && len == 0 {
            return None;
        }
        let json_str = wasm_read_string(ptr, len);
        serde_json::from_str(&json_str).ok()
    }

    /// 会话：获取单个
    pub fn session_get(&self, session_id: &str) -> Option<serde_json::Value> {
        let (sid_ptr, sid_len) = wasm_alloc_string(session_id);
        let (ptr, len) = unsafe { host_session_get(sid_ptr, sid_len) };
        if ptr == 0 && len == 0 {
            return None;
        }
        let json_str = wasm_read_string(ptr, len);
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
        let (ptr, len) = unsafe { host_http_fetch(req_ptr, req_len) };
        if ptr == 0 && len == 0 {
            return None;
        }
        let json_str = wasm_read_string(ptr, len);
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

extern "C" {
    /// 存储：获取值 — 返回 (ptr, len) JSON 或 (0,0)
    fn host_storage_get(key_ptr: u32, key_len: u32) -> (u32, u32);
    /// 存储：设置值 — 返回 0 成功，-1 失败
    fn host_storage_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;
    /// 存储：删除值 — 返回 0 成功，-1 失败
    fn host_storage_delete(key_ptr: u32, key_len: u32) -> i32;
    /// 数据库：执行 SQL — 返回受影响行数
    fn host_db_execute(sql_ptr: u32, sql_len: u32) -> i32;
    /// 数据库：查询 SQL — 返回 (ptr, len) JSON 或 (0,0)
    fn host_db_query(sql_ptr: u32, sql_len: u32) -> (u32, u32);
    /// 终端：发送输入 — 返回 0 成功，-1 失败
    fn host_terminal_send(sid_ptr: u32, sid_len: u32, data_ptr: u32, data_len: u32) -> i32;
    /// 会话：列出所有 — 返回 (ptr, len) JSON
    fn host_session_list() -> (u32, u32);
    /// 会话：获取单个 — 返回 (ptr, len) JSON 或 (0,0)
    fn host_session_get(sid_ptr: u32, sid_len: u32) -> (u32, u32);
    /// 事件：向前端发送
    fn host_emit_event(name_ptr: u32, name_len: u32, payload_ptr: u32, payload_len: u32);
    /// HTTP 代理 — 返回 (ptr, len) JSON 或 (0,0)
    fn host_http_fetch(req_ptr: u32, req_len: u32) -> (u32, u32);
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
}

// ==================== WASM Memory Helpers ====================

/// 分配字符串到 WASM 线性内存，返回 (ptr, len)
///
/// 通过宿主的 __bedcode_allocate 或 WASM 线性内存分配器分配空间
pub fn wasm_alloc_string(s: &str) -> (u32, u32) {
    if s.is_empty() {
        return (0, 0);
    }
    let bytes = s.as_bytes();
    let len = bytes.len();
    let ptr = __bedcode_allocate(len);
    if ptr.is_null() {
        return (0, 0);
    }
    // SAFETY: ptr 由 __bedcode_allocate 分配，大小为 len 字节
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

/// WASM 线性内存分配器（与 wasm_entry! 宏中生成的 __bedcode_allocate 相同）
///
/// 在 WASM 模块内部调用时使用本模块生成的版本，
/// 宿主侧也通过此函数分配内存写入字符串
#[no_mangle]
pub extern "C" fn __bedcode_allocate(len: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}
