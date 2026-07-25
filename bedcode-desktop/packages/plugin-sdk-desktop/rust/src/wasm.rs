//! WASM 插件入口
//!
//! WasmPlugin trait — WASM 插件核心接口
//! wasm_entry! 宏 — 自动生成 WASM 导出函数和内存分配器
//!
//! 插件开发者只需实现 WasmPlugin trait，然后调用 wasm_entry!(MyPlugin)

use crate::types::PluginManifest;

/// WASM 插件核心 trait
///
/// 所有 WASM 插件必须实现此 trait，并通过 `wasm_entry!` 宏生成导出函数
pub trait WasmPlugin: Send + Sync + 'static {
    /// 插件唯一标识（反向域名格式，如 com.bedcode.ai-chatbox）
    const ID: &'static str;

    /// 返回插件 manifest
    fn manifest() -> PluginManifest;

    /// 激活插件
    fn activate() -> anyhow::Result<()>;

    /// 停用插件
    fn deactivate() -> anyhow::Result<()>;

    /// 调用自定义命令
    fn invoke_command(name: &str, args_json: &str) -> anyhow::Result<serde_json::Value>;

    /// 终端输入处理（可选，默认不做修改）
    fn on_terminal_input(_session_id: &str, _text: &str) -> Option<String> {
        None
    }

    /// 终端输出处理（可选，默认不做修改）
    fn on_terminal_output(_session_id: &str, _data: &str) -> Option<String> {
        None
    }

    /// 应用启动完成回调（可选）
    fn on_startup() -> anyhow::Result<()> {
        Ok(())
    }

    /// 应用即将关闭回调（可选）
    fn on_shutdown() -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收总线消息（可选，默认忽略）
    fn on_message(_topic: &str, _sender: &str, _payload: &serde_json::Value) -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收会话生命周期事件（可选，默认忽略）
    ///
    /// 由宿主 SessionManager 直接分发，不走消息总线。
    /// payload 中包含 `event_type` 字段区分事件类型（creating/created/stopping/stopped）。
    fn on_session_lifecycle(_event: &serde_json::Value) -> anyhow::Result<()> {
        Ok(())
    }
}

/// 自动生成 WASM 导出函数 + 线性内存分配器
///
/// 生成以下导出：
/// - `__bedcode_allocate(size) -> ptr` — 内存分配器，供宿主写入字符串
/// - `__bedcode_manifest(out_ptr)` — 返回 manifest JSON，结果写入 out_ptr（8 字节: ptr + len）
/// - `__bedcode_activate() -> i32` — 激活插件
/// - `__bedcode_deactivate() -> i32` — 停用插件
/// - `__bedcode_invoke_command(name_ptr, name_len, args_ptr, args_len, out_ptr)` — 调用命令，结果写入 out_ptr
/// - `__bedcode_on_terminal_input(sid_ptr, sid_len, text_ptr, text_len, out_ptr)` — 终端输入，结果写入 out_ptr
/// - `__bedcode_on_terminal_output(sid_ptr, sid_len, data_ptr, data_len, out_ptr)` — 终端输出，结果写入 out_ptr
/// - `__bedcode_on_startup() -> ()` — 启动回调
/// - `__bedcode_on_shutdown() -> ()` — 关闭回调
/// - `__bedcode_on_message(topic, sender, payload) -> i32` — 消息总线消息
/// - `__bedcode_on_session_lifecycle(payload) -> i32` — 会话生命周期事件
///
/// # WASM ABI 约定
///
/// 返回 (ptr, len) 对的函数通过 out_ptr 输出参数传递结果（8 字节: ptr:u32 + len:u32），
/// 而非 Rust 元组返回值。因为 C ABI 不支持多值返回，Rust 的 (u32, u32) 元组
/// 会被编译器拆解为额外的指针参数，导致宿主端签名不匹配。
///
/// # 用法
/// ```ignore
/// struct MyPlugin;
/// impl WasmPlugin for MyPlugin { ... }
/// wasm_entry!(MyPlugin);
/// ```
#[macro_export]
macro_rules! wasm_entry {
    ($plugin_type:ty) => {
        static PLUGIN: std::sync::OnceLock<$plugin_type> = std::sync::OnceLock::new();
        static HOST: std::sync::OnceLock<$crate::wasm_host::WasmHost> = std::sync::OnceLock::new();

        /// 内存分配器 — 供宿主写入字符串到 WASM 线性内存
        ///
        /// 宿主调用此函数分配 len 字节的内存，然后将字符串字节写入返回的指针位置
        #[no_mangle]
        pub extern "C" fn __bedcode_allocate(len: usize) -> *mut u8 {
            let mut buf = Vec::with_capacity(len);
            let ptr = buf.as_mut_ptr();
            std::mem::forget(buf);
            ptr
        }

        /// 返回 manifest JSON — 结果写入 out_ptr（8 字节: ptr + len）
        #[no_mangle]
        pub extern "C" fn __bedcode_manifest(out_ptr: u32) {
            let manifest = <$plugin_type>::manifest();
            let json = match serde_json::to_string(&manifest) {
                Ok(s) => s,
                Err(_) => {
                    $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, 0, 0);
                    return;
                }
            };
            let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&json);
            $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
        }

        /// 激活插件
        #[no_mangle]
        pub extern "C" fn __bedcode_activate() -> i32 {
            // 初始化 WasmHost（首次调用时设置）
            let host = HOST.get_or_init(|| $crate::wasm_host::WasmHost::new(<$plugin_type>::ID));
            match <$plugin_type>::activate() {
                Ok(()) => {
                    host.log_info("Plugin activated (wasm)");
                    0
                }
                Err(e) => {
                    host.log_error(&format!("activate failed: {}", e));
                    1
                }
            }
        }

        /// 停用插件
        #[no_mangle]
        pub extern "C" fn __bedcode_deactivate() -> i32 {
            match <$plugin_type>::deactivate() {
                Ok(()) => 0,
                Err(e) => {
                    if let Some(host) = HOST.get() {
                        host.log_error(&format!("deactivate failed: {}", e));
                    }
                    1
                }
            }
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
            let name = $crate::wasm_host::wasm_read_string(name_ptr, name_len);
            let args = $crate::wasm_host::wasm_read_string(args_ptr, args_len);

            let result_str = match <$plugin_type>::invoke_command(&name, &args) {
                Ok(value) => {
                    match serde_json::to_string(&value) {
                        Ok(s) => s,
                        Err(e) => format!("{{\"error\": \"{}\"}}", e),
                    }
                }
                Err(e) => format!("{{\"error\": \"{}\"}}", e),
            };
            let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&result_str);
            $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
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
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let text = $crate::wasm_host::wasm_read_string(text_ptr, text_len);

            match <$plugin_type>::on_terminal_input(&session_id, &text) {
                Some(modified) => {
                    let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&modified);
                    $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
                }
                None => {
                    $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, 0, 0);
                }
            }
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
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let data = $crate::wasm_host::wasm_read_string(data_ptr, data_len);

            match <$plugin_type>::on_terminal_output(&session_id, &data) {
                Some(modified) => {
                    let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&modified);
                    $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
                }
                None => {
                    $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, 0, 0);
                }
            }
        }

        /// 应用启动完成回调
        #[no_mangle]
        pub extern "C" fn __bedcode_on_startup() {
            let _ = <$plugin_type>::on_startup();
        }

        /// 应用即将关闭回调
        #[no_mangle]
        pub extern "C" fn __bedcode_on_shutdown() {
            let _ = <$plugin_type>::on_shutdown();
        }

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
            let topic = $crate::wasm_host::wasm_read_string(topic_ptr, topic_len);
            let sender = $crate::wasm_host::wasm_read_string(sender_ptr, sender_len);
            let payload_str = $crate::wasm_host::wasm_read_string(payload_ptr, payload_len);
            let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
                Ok(v) => v,
                Err(_) => serde_json::Value::Null,
            };
            match <$plugin_type>::on_message(&topic, &sender, &payload) {
                Ok(()) => 0,
                Err(e) => {
                    if let Some(host) = HOST.get() {
                        host.log_error(&format!("on_message failed: {}", e));
                    }
                    -1
                }
            }
        }

        /// 接收会话生命周期事件
        #[no_mangle]
        pub extern "C" fn __bedcode_on_session_lifecycle(
            payload_ptr: u32,
            payload_len: u32,
        ) -> i32 {
            let payload_str = $crate::wasm_host::wasm_read_string(payload_ptr, payload_len);
            let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
                Ok(v) => v,
                Err(_) => serde_json::Value::Null,
            };
            match <$plugin_type>::on_session_lifecycle(&payload) {
                Ok(()) => 0,
                Err(e) => {
                    if let Some(host) = HOST.get() {
                        host.log_error(&format!("on_session_lifecycle failed: {}", e));
                    }
                    -1
                }
            }
        }
    };
}
