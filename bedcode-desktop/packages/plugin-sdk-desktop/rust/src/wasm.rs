//! WASM 插件入口（Component Model 形态，迁移阶段 B）
//!
//! WasmPlugin trait — WASM 插件核心接口
//! wasm_entry! 宏 — 生成组件世界（WIT `bedcode:plugin` world）的全部导出实现
//!
//! 插件开发者只需实现 WasmPlugin trait，然后调用 wasm_entry!(MyPlugin)。
//! 宏展开为 wit-bindgen 生成的 7 组 `Guest` trait 实现 + `export!` 导出，
//! 产物为组件（component）而非旧 ABI 的 core module：
//! - 内存搬运由绑定层处理，不再有 (ptr,len) 与 alloc/dealloc 配对
//! - 契约定义在 `wit/bedcode.wit`（单一事实来源），接口漂移编译期即暴露
//! - 宿主以 `load_plugin_from_file` 按产物格式自动选择加载路径（阶段 A 共存）
//!
//! 绑定生成（`wit_bindgen::generate!`）：
//! - import 接口 → `crate::wasm::bedcode::plugin::<iface>::<fn>` 自由函数，
//!   由 [`crate::wasm_host::WasmHost`] 内部调用
//! - export 接口 → `crate::wasm::exports::bedcode::plugin::<iface>::Guest` trait，
//!   由 `wasm_entry!` 宏对插件类型实现
//! - `pub_export_macro` 使 `export!` 可跨 crate 调用（re-export 在 `wasm` 模块，
//!   插件 crate 内经 `$crate::wasm::export!` 展开）；`default_bindings_module`
//!   指向本 SDK 的 `wasm` 模块（`$crate::wasm`），导出函数内的类型引用
//!   （`exports::bedcode::plugin::<iface>::Guest`）随宏体解析到 SDK

use crate::events::{InputSubmittedEvent, ProcessDoneEvent, SessionLifecycleEvent};
use crate::types::{PluginManifest, TransferRequestMeta, UploadHookDecision, UploadRequestMeta};
use crate::BusMessage;

wit_bindgen::generate!({
    path: "wit/bedcode.wit",
    world: "plugin",
    pub_export_macro: true,
    default_bindings_module: "$crate::wasm",
});

/// WASM 插件核心 trait
///
/// 所有 WASM 插件必须实现此 trait，并通过 `wasm_entry!` 宏生成导出函数。
/// 宏负责组件 ABI 层的 JSON 字符串 ↔ 类型化载荷转换，插件代码只处理类型。
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

    /// 接收提交输入行事件（可选，默认忽略）
    ///
    /// 由宿主 SessionManager 异步分发（需先调用 `session_input_register()`
    /// 注册并获得 `terminal:observe` 授权），不走消息总线。
    /// 纯观察通知：回调出错不影响输入本身。
    /// 事件为类型化结构体（宏已从 JSON 载荷解析）
    fn on_input_submitted(_event: &InputSubmittedEvent) -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收进程执行完成事件（可选，默认忽略）
    ///
    /// 由宿主 host-process 分发（`process_run` 启动的进程结束时触发），
    /// 不走消息总线。事件为类型化结构体（宏已从 JSON 载荷解析）。
    fn on_process_done(_event: &ProcessDoneEvent) -> anyhow::Result<()> {
        Ok(())
    }

    /// 上传请求策略钩子（可选，默认 fail-closed 拒绝）
    ///
    /// 宿主在文件服务上传会话创建时调用一次（写任何字节前），
    /// 同步阻塞上传握手，宿主外层 2 秒超时。「同名即拒」等策略
    /// 由插件在此实现（目标目录存在同名文件 → deny("duplicate-name")）。
    ///
    /// 默认拒绝：插件未覆盖此方法时，所有上传都会被拒绝（fail-closed，安全优先）
    fn on_upload_request(_meta: &UploadRequestMeta) -> UploadHookDecision {
        UploadHookDecision::deny("plugin does not implement on_upload_request")
    }

    /// 批量传输请求钩子（v2，可选，默认 fail-closed 拒绝）
    ///
    /// 宿主在 POST /transfer-request 时调用一次（批 ID + 文件清单 + 总大小）。
    /// 三路决定：allow = 直接批准；ask = 批置 pending 等待用户应答；
    /// deny = 403 拒绝（reason 如 policy-denied）。
    /// 默认拒绝：插件未覆盖此方法时，批量传输一律被拒（fail-closed，安全优先）
    fn on_transfer_request(_meta: &TransferRequestMeta) -> UploadHookDecision {
        UploadHookDecision::deny("plugin does not implement on_transfer_request")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PluginContributes, PluginType};

    /// 最小 WASM 测试插件：仅实现必需方法，其余走 trait 默认
    struct TestWasmPlugin;

    impl WasmPlugin for TestWasmPlugin {
        const ID: &'static str = "com.bedcode.test-wasm";

        fn manifest() -> PluginManifest {
            PluginManifest {
                id: Self::ID.to_string(),
                name: "Test Wasm".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                main: String::new(),
                sandbox: "inline".to_string(),
                permissions: vec![],
                contributes: PluginContributes::default(),
                plugin_type: PluginType::Rust,
                rust_library: String::new(),
                api: vec![],
                icon: None,
            }
        }

        fn activate() -> anyhow::Result<()> {
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(_name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }
    }

    /// 覆盖上传钩子的插件：验证插件可自主决定允许/拒绝
    struct AllowUploadPlugin;

    impl WasmPlugin for AllowUploadPlugin {
        const ID: &'static str = "com.bedcode.test-allow";

        fn manifest() -> PluginManifest {
            PluginManifest {
                id: Self::ID.to_string(),
                name: "Allow".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                main: String::new(),
                sandbox: "inline".to_string(),
                permissions: vec![],
                contributes: PluginContributes::default(),
                plugin_type: PluginType::Rust,
                rust_library: String::new(),
                api: vec![],
                icon: None,
            }
        }

        fn activate() -> anyhow::Result<()> {
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(_name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }

        fn on_upload_request(_meta: &UploadRequestMeta) -> UploadHookDecision {
            UploadHookDecision::allow()
        }
    }

    /// 覆盖批钩子的插件：验证插件可返回 ask（v2 异步批准）
    struct AskTransferPlugin;

    impl WasmPlugin for AskTransferPlugin {
        const ID: &'static str = "com.bedcode.test-ask";

        fn manifest() -> PluginManifest {
            PluginManifest {
                id: Self::ID.to_string(),
                name: "Ask".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                main: String::new(),
                sandbox: "inline".to_string(),
                permissions: vec![],
                contributes: PluginContributes::default(),
                plugin_type: PluginType::Rust,
                rust_library: String::new(),
                api: vec![],
                icon: None,
            }
        }

        fn activate() -> anyhow::Result<()> {
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(_name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }

        fn on_transfer_request(_meta: &crate::types::TransferRequestMeta) -> UploadHookDecision {
            UploadHookDecision::ask()
        }
    }

    #[test]
    fn test_default_terminal_hooks_are_pass_through() {
        // 默认行为 = 不修改管道（None），宿主按原样放行
        assert_eq!(TestWasmPlugin::on_terminal_input("s1", "ls"), None);
        assert_eq!(TestWasmPlugin::on_terminal_output("s1", "out"), None);
    }

    #[test]
    fn test_default_lifecycle_and_observer_hooks_succeed() {
        // 未覆盖的启动/关闭/总线/生命周期/输入观察回调默认成功，不干扰宿主流程
        assert!(TestWasmPlugin::on_startup().is_ok());
        assert!(TestWasmPlugin::on_shutdown().is_ok());
        let msg = BusMessage {
            topic: "t".into(),
            sender: "s".into(),
            payload: serde_json::Value::Null,
            timestamp: 0,
        };
        assert!(TestWasmPlugin::on_message(&msg).is_ok());
        let lifecycle = SessionLifecycleEvent::Stopped {
            session_id: "s1".into(),
            source_device: None,
            resource_dir: String::new(),
        };
        assert!(TestWasmPlugin::on_session_lifecycle(&lifecycle).is_ok());
        let input = InputSubmittedEvent { session_id: "s1".into(), text: "x".into() };
        assert!(TestWasmPlugin::on_input_submitted(&input).is_ok());
    }

    #[test]
    fn test_default_upload_hook_is_fail_closed() {
        // 安全契约：未实现钩子的插件默认拒绝一切上传，并给出明确原因
        let meta = UploadRequestMeta { relative_path: "a.txt".into(), size: 1 };
        let decision = TestWasmPlugin::on_upload_request(&meta);
        assert!(!decision.allow);
        assert_eq!(decision.reason.as_deref(), Some("plugin does not implement on_upload_request"));
    }

    #[test]
    fn test_upload_hook_override_can_allow() {
        // 插件可覆盖钩子放行上传（决策完全由插件表达）
        let meta = UploadRequestMeta { relative_path: "a.txt".into(), size: 1 };
        assert!(AllowUploadPlugin::on_upload_request(&meta).allow);
    }

    #[test]
    fn test_default_transfer_hook_is_fail_closed() {
        // 安全契约：未实现批钩子的插件默认拒绝一切批量传输请求
        let meta = crate::types::TransferRequestMeta {
            batch_id: "b1".into(),
            files: vec![UploadRequestMeta { relative_path: "a.txt".into(), size: 1 }],
            total_size: 1,
        };
        let decision = TestWasmPlugin::on_transfer_request(&meta);
        assert!(!decision.allow);
        assert!(!decision.ask);
        assert_eq!(
            decision.reason.as_deref(),
            Some("plugin does not implement on_transfer_request")
        );
    }

    #[test]
    fn test_transfer_hook_override_can_ask() {
        // 插件可覆盖批钩子返回 ask（批置 pending 等待用户应答）
        let meta = crate::types::TransferRequestMeta {
            batch_id: "b2".into(),
            files: Vec::new(),
            total_size: 0,
        };
        let decision = AskTransferPlugin::on_transfer_request(&meta);
        assert!(decision.ask);
        assert!(!decision.allow);
    }
}


/// 生成组件 world（`bedcode:plugin`）的全部导出实现
///
/// 展开为 wit-bindgen 生成的 7 组 `Guest` trait 实现（command / lifecycle /
/// events / terminal-hooks / upload-hook / manifest / abi）并调用 `export!`
/// 导出。语义与旧 `__bedcode_*` 导出 1:1 对应（见各 impl 注释）。
///
/// # 用法
/// ```ignore
/// struct MyPlugin;
/// impl WasmPlugin for MyPlugin { ... }
/// wasm_entry!(MyPlugin);
/// ```
/// 参数为 `ident` 而非 `ty`：Rust 宏的片段卫生限制 —— `ty` 片段不能作为
/// `ident` 传给 `export!` 宏（报 "no rules expected ty metavariable"）。
#[macro_export]
macro_rules! wasm_entry {
    ($plugin_type:ident) => {
        // ==================== command（原 __bedcode_invoke_command） ====================

        impl $crate::wasm::exports::bedcode::plugin::command::Guest for $plugin_type {
            /// 调用自定义命令：JSON 载荷保留（args-json → 类型化 Value → 结果 JSON）
            fn invoke(name: String, args: String) -> String {
                // 组件绑定层保证 UTF-8 合法，解析失败时退化为 Null（由插件自行容错）
                let args: serde_json::Value =
                    serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);

                match <$plugin_type as $crate::wasm::WasmPlugin>::invoke_command(&name, args) {
                    Ok(value) => {
                        match serde_json::to_string(&value) {
                            Ok(s) => s,
                            // 错误信息经 serde_json 转义，避免引号/反斜杠产生非法 JSON
                            // 导致宿主侧反序列化失败、屏蔽真实错误原因
                            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
                        }
                    }
                    Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
                }
            }
        }

        // ==================== lifecycle（原 __bedcode_activate/deactivate/on_startup/on_shutdown） ====================

        impl $crate::wasm::exports::bedcode::plugin::lifecycle::Guest for $plugin_type {
            fn activate() -> Result<(), String> {
                // WasmHost 是无状态 unit struct；插件身份由宿主侧 Caller state 维护。
                // 日志走 UFCS 调用，宏展开处无需导入 HostLog trait
                let host = $crate::wasm_host::WasmHost;
                match <$plugin_type as $crate::wasm::WasmPlugin>::activate() {
                    Ok(()) => {
                        $crate::host::HostLog::log_info(&host, "Plugin activated (wasm)");
                        Ok(())
                    }
                    Err(e) => {
                        $crate::host::HostLog::log_error(&host, &format!("activate failed: {}", e));
                        Err(e.to_string())
                    }
                }
            }

            fn deactivate() -> Result<(), String> {
                match <$plugin_type as $crate::wasm::WasmPlugin>::deactivate() {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("deactivate failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_startup() {
                let _ = <$plugin_type as $crate::wasm::WasmPlugin>::on_startup();
            }

            fn on_shutdown() {
                let _ = <$plugin_type as $crate::wasm::WasmPlugin>::on_shutdown();
            }
        }

        // ==================== events（原 __bedcode_on_message/on_session_lifecycle/on_input_submitted） ====================

        impl $crate::wasm::exports::bedcode::plugin::events::Guest for $plugin_type {
            fn on_message(topic: String, sender: String, payload: String) -> Result<(), String> {
                let payload: serde_json::Value =
                    serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
                // ABI 三段字符串 → 类型化 BusMessage（timestamp 待 ABI v2 传递）
                let msg = $crate::BusMessage {
                    topic,
                    sender,
                    payload,
                    timestamp: 0,
                };
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_message(&msg) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_message failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_session_lifecycle(payload: String) -> Result<(), String> {
                // JSON 字符串 → 类型化 SessionLifecycleEvent（解析失败视为协议错误）
                let event: $crate::events::SessionLifecycleEvent = serde_json::from_str(&payload)
                    .map_err(|e| format!("on_session_lifecycle: invalid event payload: {}", e))?;
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_session_lifecycle(&event) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_session_lifecycle failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_input_submitted(payload: String) -> Result<(), String> {
                // JSON 字符串 → 类型化 InputSubmittedEvent（解析失败视为协议错误）
                let event: $crate::events::InputSubmittedEvent = serde_json::from_str(&payload)
                    .map_err(|e| format!("on_input_submitted: invalid event payload: {}", e))?;
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_input_submitted(&event) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_input_submitted failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_process_done(payload: String) -> Result<(), String> {
                // JSON 字符串 → 类型化 ProcessDoneEvent（解析失败视为协议错误）
                let event: $crate::events::ProcessDoneEvent = serde_json::from_str(&payload)
                    .map_err(|e| format!("on_process_done: invalid event payload: {}", e))?;
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_process_done(&event) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_process_done failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }
        }

        // ==================== terminal-hooks（原 __bedcode_on_terminal_input/output） ====================

        impl $crate::wasm::exports::bedcode::plugin::terminal_hooks::Guest for $plugin_type {
            fn on_terminal_input(_session_id: String, text: String) -> Option<String> {
                <$plugin_type as $crate::wasm::WasmPlugin>::on_terminal_input(&_session_id, &text)
            }

            fn on_terminal_output(_session_id: String, data: String) -> Option<String> {
                <$plugin_type as $crate::wasm::WasmPlugin>::on_terminal_output(&_session_id, &data)
            }
        }

        // ==================== upload-hook（原 __bedcode_on_upload_request） ====================

        impl $crate::wasm::exports::bedcode::plugin::upload_hook::Guest for $plugin_type {
            /// 上传策略钩子：fail-closed（入参解析失败时直接拒绝，不调用插件逻辑）。
            /// 拒绝语义完全由决定 JSON 表达，宿主据此 fail-closed。
            fn on_upload_request(meta_json: String) -> String {
                let decision =
                    match serde_json::from_str::<$crate::types::UploadRequestMeta>(&meta_json) {
                        Ok(meta) => <$plugin_type as $crate::wasm::WasmPlugin>::on_upload_request(&meta),
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
                serde_json::to_string(&decision)
                    .unwrap_or_else(|_| r#"{"allow":false,"ask":false,"reason":"serialize decision failed"}"#.to_string())
            }
        }

        // ==================== transfer-request-hook（v2，原 __bedcode_on_transfer_request） ====================

        impl $crate::wasm::exports::bedcode::plugin::transfer_request_hook::Guest for $plugin_type {
            /// 批量传输请求钩子：fail-closed（入参解析失败时直接拒绝，不调用插件逻辑），
            /// 与 upload-hook 同构。
            fn on_transfer_request(meta_json: String) -> String {
                let decision =
                    match serde_json::from_str::<$crate::types::TransferRequestMeta>(&meta_json) {
                        Ok(meta) => <$plugin_type as $crate::wasm::WasmPlugin>::on_transfer_request(&meta),
                        Err(e) => {
                            let host = $crate::wasm_host::WasmHost;
                            $crate::host::HostLog::log_error(
                                &host,
                                &format!("on_transfer_request: invalid meta payload: {}", e),
                            );
                            $crate::types::UploadHookDecision::deny("invalid transfer request meta")
                        }
                    };

                // 序列化失败时退化为裸 JSON 拒绝，保证宿主永远拿到合法决定
                serde_json::to_string(&decision)
                    .unwrap_or_else(|_| r#"{"allow":false,"ask":false,"reason":"serialize decision failed"}"#.to_string())
            }
        }

        // ==================== manifest（原 __bedcode_manifest） ====================

        impl $crate::wasm::exports::bedcode::plugin::manifest::Guest for $plugin_type {
            fn get() -> String {
                serde_json::to_string(&<$plugin_type as $crate::wasm::WasmPlugin>::manifest())
                    .unwrap_or_else(|_| "{}".to_string())
            }
        }

        // ==================== abi（原 __bedcode_abi_version + form 形态字段） ====================

        impl $crate::wasm::exports::bedcode::plugin::abi::Guest for $plugin_type {
            /// ABI 版本：语义与 `abi::ABI_VERSION`（当前 v6）完全一致
            fn version() -> u32 {
                $crate::abi::ABI_VERSION as u32
            }

            /// 产物形态：组件（Component Model），宿主按 `form()==1` 识别
            fn form() -> u32 {
                $crate::abi::FORM_COMPONENT as u32
            }
        }

        // ==================== 组件导出 ====================

        // 生成 #[no_mangle] 导出函数（command/lifecycle/... 全部 7 组接口的 cabi 导出）。
        // 宏展开处 `$crate` 为插件依赖的 SDK：绑定类型路径经 lib.rs 的
        // `pub use wasm::bedcode` re-export 定位（generate! 的 default_bindings_module）
        $crate::wasm::export!($plugin_type);
    };
}
