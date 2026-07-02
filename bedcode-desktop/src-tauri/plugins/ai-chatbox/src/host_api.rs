//! Host API FFI Types
//!
//! cdylib 插件侧的 HostContext 镜像定义
//! 与宿主 host_context.rs 中的 HostContext 结构完全一致

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// 宿主注入的能力上下文（插件侧镜像）
#[repr(C)]
pub struct HostContext {
    pub plugin_id: *const c_char,
    pub free_string: extern "C" fn(*mut c_char),
    pub storage_get: extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
    pub storage_set: extern "C" fn(*const c_char, *const c_char, *const c_char) -> i32,
    pub storage_delete: extern "C" fn(*const c_char, *const c_char) -> i32,
    pub db_execute: extern "C" fn(*const c_char, *const c_char) -> i32,
    pub db_query: extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
    pub terminal_send_input: extern "C" fn(*const c_char, *const c_char) -> i32,
    pub session_list: extern "C" fn() -> *mut c_char,
    pub session_get: extern "C" fn(*const c_char) -> *mut c_char,
    pub emit_event: extern "C" fn(*const c_char, *const c_char),
}

// SAFETY: HostContext 由宿主在 activate 时注入，所有指针在插件生命周期内有效。
// 宿主保证：plugin_id 指向的字符串在 deactivate 前不会释放；
// 函数指针指向宿主代码段，始终可调用。跨线程访问宿主函数是线程安全的
// （宿主侧通过 Mutex/RwLock 保护共享状态）。
unsafe impl Send for HostContext {}
unsafe impl Sync for HostContext {}

impl HostContext {
    /// 获取 plugin_id 字符串
    pub fn plugin_id_str(&self) -> String {
        if self.plugin_id.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(self.plugin_id) }
            .to_str()
            .unwrap_or("")
            .to_string()
    }

    /// 通过 storage_get 获取值并反序列化
    pub fn storage_get_json(&self, key: &str) -> Option<serde_json::Value> {
        let key_cstr = CString::new(key).ok()?;
        let plugin_id = self.plugin_id;
        let result_ptr = (self.storage_get)(plugin_id, key_cstr.as_ptr());
        if result_ptr.is_null() {
            return None;
        }
        let result_str = unsafe { CStr::from_ptr(result_ptr) }
            .to_str()
            .ok()?
            .to_string();
        (self.free_string)(result_ptr);
        serde_json::from_str(&result_str).ok()
    }

    /// 通过 storage_set 设置值
    pub fn storage_set_json(&self, key: &str, value: &serde_json::Value) -> bool {
        let key_cstr = match CString::new(key) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let value_str = match serde_json::to_string(value) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let value_cstr = match CString::new(value_str) {
            Ok(c) => c,
            Err(_) => return false,
        };
        (self.storage_set)(self.plugin_id, key_cstr.as_ptr(), value_cstr.as_ptr()) == 0
    }

    /// 通过 storage_delete 删除值
    pub fn storage_delete_json(&self, key: &str) -> bool {
        let key_cstr = match CString::new(key) {
            Ok(c) => c,
            Err(_) => return false,
        };
        (self.storage_delete)(self.plugin_id, key_cstr.as_ptr()) == 0
    }

    /// 执行 SQL 语句（INSERT/UPDATE/DELETE/CREATE TABLE）
    pub fn db_execute_sql(&self, sql: &str) -> i32 {
        let sql_cstr = match CString::new(sql) {
            Ok(c) => c,
            Err(_) => return -1,
        };
        (self.db_execute)(self.plugin_id, sql_cstr.as_ptr())
    }

    /// 查询 SQL（SELECT），返回 JSON 数组
    pub fn db_query_sql(&self, sql: &str) -> Option<serde_json::Value> {
        let sql_cstr = CString::new(sql).ok()?;
        let result_ptr = (self.db_query)(self.plugin_id, sql_cstr.as_ptr());
        if result_ptr.is_null() {
            return None;
        }
        let result_str = unsafe { CStr::from_ptr(result_ptr) }
            .to_str()
            .ok()?
            .to_string();
        (self.free_string)(result_ptr);
        serde_json::from_str(&result_str).ok()
    }

    /// 向终端会话发送输入
    pub fn terminal_send(&self, session_id: &str, data: &str) -> bool {
        let sid_cstr = match CString::new(session_id) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let data_cstr = match CString::new(data) {
            Ok(c) => c,
            Err(_) => return false,
        };
        (self.terminal_send_input)(sid_cstr.as_ptr(), data_cstr.as_ptr()) == 0
    }

    /// 向前端发送事件
    pub fn emit(&self, event_name: &str, payload: &serde_json::Value) {
        let name_cstr = match CString::new(event_name) {
            Ok(c) => c,
            Err(_) => return,
        };
        let payload_str = match serde_json::to_string(payload) {
            Ok(s) => s,
            Err(_) => return,
        };
        let payload_cstr = match CString::new(payload_str) {
            Ok(c) => c,
            Err(_) => return,
        };
        (self.emit_event)(name_cstr.as_ptr(), payload_cstr.as_ptr());
    }
}
