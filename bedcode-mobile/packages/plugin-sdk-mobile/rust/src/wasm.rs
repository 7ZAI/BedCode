//! WASM 插件入口 (Mobile)
//!
//! WasmPlugin trait — 移动端 WASM 插件核心接口
//! wasm_entry! 宏 — 自动生成 WASM 导出函数
//!
//! 相比桌面端，移动端额外支持 on_auth_success / on_disconnect /
//! on_session_created / on_session_stopped / on_terminal_input / on_terminal_output
//!
//! ABI v3：结果传递走 out_ptr（8 字节: ptr + len），新增
//! `__bedcode_abi_version` 版本协商与 `__bedcode_deallocate` 内存回收，
//! 消除元组返回的 FFI-safe 警告与线性内存单调增长。

use crate::types::PluginManifest;

/// WASM 插件核心 trait
pub trait WasmPlugin: Send + Sync + 'static {
    const ID: &'static str;
    fn manifest() -> PluginManifest;
    fn activate() -> anyhow::Result<()>;
    fn deactivate() -> anyhow::Result<()>;
    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value>;

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

    /// 上传请求策略钩子（可选，默认 fail-closed 拒绝）
    ///
    /// 宿主在文件服务上传会话创建时调用一次（写任何字节前），
    /// 同步阻塞上传握手，宿主外层 2 秒超时。「同名即拒」等策略
    /// 由插件在此实现（目标目录存在同名文件 → deny("duplicate-name")）。
    ///
    /// 默认拒绝：插件未覆盖此方法时，所有上传都会被拒绝（fail-closed，安全优先）
    fn on_upload_request(_meta: &crate::types::UploadRequestMeta) -> crate::types::UploadHookDecision {
        crate::types::UploadHookDecision::deny("plugin does not implement on_upload_request")
    }
}

/// 自动生成 WASM 导出函数 + 线性内存分配器/回收器
///
/// 生成以下导出：
/// - `__bedcode_abi_version() -> i32` — ABI 版本协商（宿主比对后决定是否加载）
/// - `__bedcode_allocate(len) -> ptr` — 内存分配器，供宿主写入字符串
/// - `__bedcode_deallocate(ptr, len)` — 内存回收器（与分配器同 Layout 配对）
/// - `__bedcode_manifest(out_ptr)` — 返回 manifest JSON，结果写入 out_ptr
/// - `__bedcode_activate() -> i32` — 激活插件
/// - `__bedcode_deactivate() -> i32` — 停用插件
/// - `__bedcode_invoke_command(name, name_len, args, args_len, out_ptr)` — 调用命令
/// - `__bedcode_on_terminal_input(sid, sid_len, text, text_len, out_ptr)` — 终端输入
/// - `__bedcode_on_terminal_output(sid, sid_len, data, data_len, out_ptr)` — 终端输出
/// - `__bedcode_on_startup()` / `__bedcode_on_shutdown()` / `__bedcode_on_auth_success()`
/// - `__bedcode_on_disconnect(reason, reason_len)` — 连接断开
/// - `__bedcode_on_session_created(sid, sid_len)` / `__bedcode_on_session_stopped(sid, sid_len)`
/// - `__bedcode_on_bus_message(topic, topic_len, sender, sender_len, payload, payload_len, timestamp) -> i32`
/// - `__bedcode_on_upload_request(meta_ptr, meta_len, out_ptr) -> i32` — 上传策略钩子，决定写入 out_ptr
///
/// # WASM ABI 约定
///
/// 返回 (ptr, len) 对的函数通过 out_ptr 输出参数传递结果（8 字节: ptr:u32 + len:u32），
/// 而非 Rust 元组返回值（C ABI 多值返回不跨工具链稳定）。
///
/// 参数与结果内存均由 `wasm_alloc_string`（std::alloc Layout(len, 1)）分配，
/// 读取为 Rust String 后立即 `wasm_dealloc_string` 归还，防止线性内存单调增长。
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
        // 导出函数仅 wasm32 目标需要（宿主 wasmtime 加载）；host 目标（cargo test）
        // 不生成任何导出，避免 `#[no_mangle]` 强制保留对 host_* import 符号的未定义引用
        #[cfg(target_arch = "wasm32")]
        mod __bedcode_wasm_exports {
            use super::*;
            static PLUGIN: std::sync::OnceLock<$plugin_type> = std::sync::OnceLock::new();
            static HOST: std::sync::OnceLock<$crate::wasm_host::WasmHost> = std::sync::OnceLock::new();
            use $crate::host::HostLog as _;

        /// ABI 版本协商 — 宿主实例化后读取，与 `abi::ABI_VERSION` 比对
        #[no_mangle]
        pub extern "C" fn __bedcode_abi_version() -> i32 {
            $crate::abi::ABI_VERSION as i32
        }

        /// 内存分配器 — 供宿主写入字符串到 WASM 线性内存
        ///
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

        /// 内存回收器 — 释放 `__bedcode_allocate` / `wasm_alloc_string` 分配的内存
        ///
        /// 双向配对回收，消除长驻插件线性内存单调增长
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

        #[no_mangle]
        pub extern "C" fn __bedcode_activate() -> i32 {
            let host = HOST.get_or_init(|| $crate::wasm_host::WasmHost);
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
            name_ptr: u32, name_len: u32, args_ptr: u32, args_len: u32, out_ptr: u32,
        ) {
            let name = $crate::wasm_host::wasm_read_string(name_ptr, name_len);
            let args_json = $crate::wasm_host::wasm_read_string(args_ptr, args_len);
            // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
            $crate::wasm_host::wasm_dealloc_string(name_ptr, name_len);
            $crate::wasm_host::wasm_dealloc_string(args_ptr, args_len);
            let args: serde_json::Value = match serde_json::from_str(&args_json) {
                Ok(v) => v,
                // 解析失败时传 Value::Null（与桌面端宏行为一致）
                Err(_) => serde_json::Value::Null,
            };
            let result_str = match <$plugin_type>::invoke_command(&name, args) {
                Ok(value) => {
                    // 错误信息经 serde_json 转义，避免引号/反斜杠产生非法 JSON
                    // 导致宿主侧反序列化失败、屏蔽真实错误原因
                    match serde_json::to_string(&value) {
                        Ok(s) => s,
                        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
                    }
                }
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            };
            let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&result_str);
            $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_terminal_input(
            sid_ptr: u32, sid_len: u32, text_ptr: u32, text_len: u32, out_ptr: u32,
        ) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let text = $crate::wasm_host::wasm_read_string(text_ptr, text_len);
            $crate::wasm_host::wasm_dealloc_string(sid_ptr, sid_len);
            $crate::wasm_host::wasm_dealloc_string(text_ptr, text_len);
            match <$plugin_type>::on_terminal_input(&session_id, &text) {
                Some(modified) => {
                    let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&modified);
                    $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
                }
                None => $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, 0, 0),
            }
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_terminal_output(
            sid_ptr: u32, sid_len: u32, data_ptr: u32, data_len: u32, out_ptr: u32,
        ) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            let data = $crate::wasm_host::wasm_read_string(data_ptr, data_len);
            $crate::wasm_host::wasm_dealloc_string(sid_ptr, sid_len);
            $crate::wasm_host::wasm_dealloc_string(data_ptr, data_len);
            match <$plugin_type>::on_terminal_output(&session_id, &data) {
                Some(modified) => {
                    let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&modified);
                    $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
                }
                None => $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, 0, 0),
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
            $crate::wasm_host::wasm_dealloc_string(reason_ptr, reason_len);
            let _ = <$plugin_type>::on_disconnect(&reason);
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_session_created(sid_ptr: u32, sid_len: u32) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            $crate::wasm_host::wasm_dealloc_string(sid_ptr, sid_len);
            let _ = <$plugin_type>::on_session_created(&session_id);
        }

        #[no_mangle]
        pub extern "C" fn __bedcode_on_session_stopped(sid_ptr: u32, sid_len: u32) {
            let session_id = $crate::wasm_host::wasm_read_string(sid_ptr, sid_len);
            $crate::wasm_host::wasm_dealloc_string(sid_ptr, sid_len);
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
            $crate::wasm_host::wasm_dealloc_string(topic_ptr, topic_len);
            $crate::wasm_host::wasm_dealloc_string(sender_ptr, sender_len);
            $crate::wasm_host::wasm_dealloc_string(payload_ptr, payload_len);
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

        /// 上传请求策略钩子 — 决定 JSON 写入 out_ptr（8 字节: ptr + len）
        ///
        /// fail-closed：入参解析失败时直接拒绝，不调用插件逻辑。
        /// 返回值仅用于宿主侧错误日志，拒绝语义完全由决定 JSON 表达
        #[no_mangle]
        pub extern "C" fn __bedcode_on_upload_request(
            meta_ptr: u32,
            meta_len: u32,
            out_ptr: u32,
        ) -> i32 {
            let meta_str = $crate::wasm_host::wasm_read_string(meta_ptr, meta_len);
            // 参数已拷贝为 Rust String，立即归还宿主写入时分配的线性内存
            $crate::wasm_host::wasm_dealloc_string(meta_ptr, meta_len);

            let decision = match serde_json::from_str::<$crate::types::UploadRequestMeta>(&meta_str) {
                Ok(meta) => <$plugin_type>::on_upload_request(&meta),
                Err(e) => {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(
                        &host,
                        &format!("on_upload_request: invalid meta payload: {}", e),
                    );
                    $crate::types::UploadHookDecision::deny("invalid upload request meta")
                }
            };

            // 序列化失败时退化为裸 JSON 拒绝，保证宿主永远拿到合法决定
            let json = serde_json::to_string(&decision)
                .unwrap_or_else(|_| r#"{"allow":false,"reason":"serialize decision failed"}"#.to_string());
            let (ptr, len) = $crate::wasm_host::wasm_alloc_string(&json);
            $crate::wasm_host::wasm_write_result_to_out_ptr(out_ptr, ptr, len);
            0
            }
        }
    };
}
