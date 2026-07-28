//! WASM 插件入口
//!
//! WasmPlugin trait — WASM 插件核心接口
//! wasm_entry! 宏 — 自动生成 WASM 导出函数和内存分配器
//!
//! 插件开发者只需实现 WasmPlugin trait，然后调用 wasm_entry!(MyPlugin)

use crate::events::SessionLifecycleEvent;
use crate::types::PluginManifest;
use crate::BusMessage;

/// WASM 插件核心 trait
///
/// 所有 WASM 插件必须实现此 trait，并通过 `wasm_entry!` 宏生成导出函数。
/// 宏负责 WASM ABI 层的 JSON 字符串 ↔ 类型化载荷转换，插件代码只处理类型。
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
    ///
    /// `args` 为类型化 JSON（宏已从 ABI 字符串解析，解析失败时为 `Value::Null`）
    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value>;

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
    ///
    /// `timestamp` 字段当前 ABI 未传递，值为 0（ABI v2 计划补齐）
    fn on_message(_msg: &BusMessage) -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收会话生命周期事件（可选，默认忽略）
    ///
    /// 由宿主 SessionManager 直接分发，不走消息总线。
    /// 事件为类型化枚举（宏已从 JSON 载荷解析）
    fn on_session_lifecycle(_event: &SessionLifecycleEvent) -> anyhow::Result<()> {
        Ok(())
    }
}

/// 自动生成 WASM 导出函数 + 线性内存分配器
///
/// 生成以下导出：
/// - `__bedcode_abi_version() -> i32` — ABI 版本协商（v2 起）
/// - `__bedcode_allocate(size) -> ptr` — 内存分配器，供宿主写入字符串
/// - `__bedcode_deallocate(ptr, len)` — 内存回收器（v2 起，缺失时宿主退化 v1 不回收行为）
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

        /// ABI 版本协商 — 宿主实例化后读取，与 `abi::ABI_VERSION` 比对
        ///
        /// 插件要求的版本高于宿主支持时，宿主拒绝加载并给出明确错误
        #[no_mangle]
        pub extern "C" fn __bedcode_abi_version() -> i32 {
            $crate::abi::ABI_VERSION as i32
        }

        /// 内存分配器 — 供宿主写入字符串到 WASM 线性内存
        ///
        /// 宿主调用此函数分配 len 字节的内存，然后将字符串字节写入返回的指针位置。
        /// 使用 std::alloc 精确 Layout 分配，与 `__bedcode_deallocate` 配对回收
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

        /// 内存回收器 — 释放 `__bedcode_allocate` 或 `wasm_alloc_string` 分配的内存
        ///
        /// 双向配对回收，消除长驻插件线性内存单调增长：
        /// - 宿主在读取完插件传入的字符串后调用（host function 参数回收）
        /// - 插件侧在读取完宿主写入的参数/结果后调用（导出函数参数与 out_ptr 回收）
        /// - 宿主在读取完插件返回的结果后调用（导出函数结果回收）
        #[no_mangle]
        pub extern "C" fn __bedcode_deallocate(ptr: u32, len: u32) {
            if ptr == 0 || len == 0 {
                return;
            }
            if let Ok(layout) = std::alloc::Layout::from_size_align(len as usize, 1) {
                // SAFETY: ptr 由同模块 __bedcode_allocate / wasm_alloc_string 以相同 Layout 分配
                unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
            }
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
            // WasmHost 是无状态 unit struct；插件身份由宿主侧 Caller state 维护。
            // 日志走 UFCS 调用，宏展开处无需导入 HostLog trait
            let host = $crate::wasm_host::WasmHost;
            match <$plugin_type>::activate() {
                Ok(()) => {
                    $crate::host::HostLog::log_info(&host, "Plugin activated (wasm)");
                    0
                }
                Err(e) => {
                    $crate::host::HostLog::log_error(&host, &format!("activate failed: {}", e));
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
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(&host, &format!("deactivate failed: {}", e));
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
            let args_str = $crate::wasm_host::wasm_read_string(args_ptr, args_len);
            // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
            $crate::wasm_host::wasm_dealloc_string(name_ptr, name_len);
            $crate::wasm_host::wasm_dealloc_string(args_ptr, args_len);
            // ABI 字符串 → 类型化 JSON（解析失败时为 Null，由插件自行容错）
            let args: serde_json::Value =
                serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);

            let result_str = match <$plugin_type>::invoke_command(&name, args) {
                Ok(value) => {
                    match serde_json::to_string(&value) {
                        Ok(s) => s,
                        // 错误信息经 serde_json 转义，避免引号/反斜杠产生非法 JSON
                        // 导致宿主侧反序列化失败、屏蔽真实错误原因
                        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
                    }
                }
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
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
            // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
            $crate::wasm_host::wasm_dealloc_string(sid_ptr, sid_len);
            $crate::wasm_host::wasm_dealloc_string(text_ptr, text_len);

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
            // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
            $crate::wasm_host::wasm_dealloc_string(sid_ptr, sid_len);
            $crate::wasm_host::wasm_dealloc_string(data_ptr, data_len);

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
            // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
            $crate::wasm_host::wasm_dealloc_string(topic_ptr, topic_len);
            $crate::wasm_host::wasm_dealloc_string(sender_ptr, sender_len);
            $crate::wasm_host::wasm_dealloc_string(payload_ptr, payload_len);
            let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
                Ok(v) => v,
                Err(_) => serde_json::Value::Null,
            };
            // ABI 三段字符串 → 类型化 BusMessage（timestamp 待 ABI v2 传递）
            let msg = $crate::BusMessage {
                topic,
                sender,
                payload,
                timestamp: 0,
            };
            match <$plugin_type>::on_message(&msg) {
                Ok(()) => 0,
                Err(e) => {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(&host, &format!("on_message failed: {}", e));
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
            // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
            $crate::wasm_host::wasm_dealloc_string(payload_ptr, payload_len);
            // ABI JSON 字符串 → 类型化 SessionLifecycleEvent（解析失败视为协议错误）
            let event: $crate::events::SessionLifecycleEvent = match serde_json::from_str(&payload_str) {
                Ok(e) => e,
                Err(e) => {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(
                        &host,
                        &format!("on_session_lifecycle: invalid event payload: {}", e),
                    );
                    return -1;
                }
            };
            match <$plugin_type>::on_session_lifecycle(&event) {
                Ok(()) => 0,
                Err(e) => {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(
                        &host,
                        &format!("on_session_lifecycle failed: {}", e),
                    );
                    -1
                }
            }
        }
    };
}
