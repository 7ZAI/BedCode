//! WASM 插件入口 (Mobile)
//!
//! WasmPlugin trait — 移动端 WASM 插件核心接口
//! wasm_entry! 宏 — 自动生成 WASM 导出函数
//!
//! 相比桌面端，移动端额外支持 on_auth_success / on_disconnect /
//! on_session_created / on_session_stopped / on_terminal_input / on_terminal_output

use crate::types::PluginManifest;

/// WASM 插件核心 trait
pub trait WasmPlugin: Send + Sync + 'static {
    const ID: &'static str;
    fn manifest() -> PluginManifest;
    fn activate() -> anyhow::Result<()>;
    fn deactivate() -> anyhow::Result<()>;
    fn invoke_command(name: &str, args_json: &str) -> anyhow::Result<serde_json::Value>;

    fn on_terminal_input(_session_id: &str, _text: &str) -> Option<String> { None }
    fn on_terminal_output(_session_id: &str, _data: &str) -> Option<String> { None }
    fn on_startup() -> anyhow::Result<()> { Ok(()) }
    fn on_shutdown() -> anyhow::Result<()> { Ok(()) }
    fn on_auth_success() -> anyhow::Result<()> { Ok(()) }
    fn on_disconnect(_reason: &str) -> anyhow::Result<()> { Ok(()) }
    fn on_session_created(_session_id: &str) -> anyhow::Result<()> { Ok(()) }
    fn on_session_stopped(_session_id: &str) -> anyhow::Result<()> { Ok(()) }

    /// 收到总线消息回调（可选，默认忽略）
    fn on_bus_message(_msg: &crate::BusMessage) -> anyhow::Result<()> { Ok(()) }
}

/// 自动生成 WASM 导出函数 + 线性内存分配器
#[macro_export]
macro_rules! wasm_entry {
    ($plugin_type:ty) => {
        static PLUGIN: std::sync::OnceLock<$plugin_type> = std::sync::OnceLock::new();
        static HOST: std::sync::OnceLock<$crate::wasm_host::WasmHost> = std::sync::OnceLock::new();

        #[no_mangle]
        pub extern "C" fn __bedcode_allocate(len: usize) -> *mut u8 {
            let mut buf = Vec::with_capacity(len);
            let ptr = buf.as_mut_ptr();
            std::mem::forget(buf);
            ptr
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_manifest() -> (u32, u32) {
            let manifest = <$plugin_type>::manifest();
            let json = match serde_json::to_string(&manifest) {
                Ok(s) => s,
                Err(_) => return (0, 0),
            };
            $crate::wasm_host::wasm_alloc_string(&json)
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_activate() -> i32 {
            let host = HOST.get_or_init(|| $crate::wasm_host::WasmHost::new(<$plugin_type>::ID));
            match <$plugin_type>::activate() {
                Ok(()) => { host.log_info("Plugin activated (wasm)"); 0 }
                Err(e) => { host.log_error(&format!("activate failed: {}", e)); 1 }
            }
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_deactivate() -> i32 {
            match <$plugin_type>::deactivate() {
                Ok(()) => 0,
                Err(e) => {
                    if let Some(host) = HOST.get() { host.log_error(&format!("deactivate failed: {}", e)); }
                    1
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_invoke_command(
            name_ptr: u32, name_len: u32, args_ptr: u32, args_len: u32,
        ) -> (u32, u32) {
            let name = $crate::wasm_host::wasm_read_string(name_ptr, name_len);
            let args = $crate::wasm_host::wasm_read_string(args_ptr, args_len);
            match <$plugin_type>::invoke_command(&name, &args) {
                Ok(value) => {
                    let json = match serde_json::to_string(&value) {
                        Ok(s) => s,
                        Err(e) => { let err = format!("{{\"error\": \"{}\"}}", e); return $crate::wasm_host::wasm_alloc_string(&err); }
                    };
                    $crate::wasm_host::wasm_alloc_string(&json)
                }
                Err(e) => {
                    let err = format!("{{\"error\": \"{}\"}}", e);
                    $crate::wasm_host::wasm_alloc_string(&err)
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_terminal_input(
            sid_ptr: u32, sid_len: u32, text_ptr: u32, text_len: u32,
        ) -> (u32, u32) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let text = $crate::wasm_host::wasm_read_string(text_ptr, text_len);
            match <$plugin_type>::on_terminal_input(&session_id, &text) {
                Some(modified) => $crate::wasm_host::wasm_alloc_string(&modified),
                None => (0, 0),
            }
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_terminal_output(
            sid_ptr: u32, sid_len: u32, data_ptr: u32, data_len: u32,
        ) -> (u32, u32) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let data = $crate::wasm_host::wasm_read_string(data_ptr, data_len);
            match <$plugin_type>::on_terminal_output(&session_id, &data) {
                Some(modified) => $crate::wasm_host::wasm_alloc_string(&modified),
                None => (0, 0),
            }
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_startup() { let _ = <$plugin_type>::on_startup(); }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_shutdown() { let _ = <$plugin_type>::on_shutdown(); }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_auth_success() { let _ = <$plugin_type>::on_auth_success(); }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_disconnect(reason_ptr: u32, reason_len: u32) {
            let reason = $crate::wasm_host::wasm_read_string(reason_ptr, reason_len);
            let _ = <$plugin_type>::on_disconnect(&reason);
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_session_created(sid_ptr: u32, sid_len: u32) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let _ = <$plugin_type>::on_session_created(&session_id);
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_session_stopped(sid_ptr: u32, sid_len: u32) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let _ = <$plugin_type>::on_session_stopped(&session_id);
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_bus_message(
            topic_ptr: u32, topic_len: u32,
            sender_ptr: u32, sender_len: u32,
            payload_ptr: u32, payload_len: u32,
            timestamp: u64,
        ) -> i32 {
            let topic = $crate::wasm_host::wasm_read_string(topic_ptr, topic_len);
            let sender = $crate::wasm_host::wasm_read_string(sender_ptr, sender_len);
            let payload_str = $crate::wasm_host::wasm_read_string(payload_ptr, payload_len);
            let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
                Ok(v) => v,
                Err(_) => serde_json::Value::Null,
            };
            let msg = $crate::BusMessage {
                topic,
                sender,
                payload,
                timestamp,
            };
            match <$plugin_type>::on_bus_message(&msg) {
                Ok(()) => 0,
                Err(e) => {
                    if let Some(host) = HOST.get() { host.log_error(&format!("on_bus_message failed: {}", e)); }
                    1
                }
            }
        }
    };
}
