//! WASM 插件入口 (Mobile) — Component Model 形态（迁移 ticket 04）
//!
//! WasmPlugin trait — 移动端 WASM 插件核心接口
//! wasm_entry! 宏 — 生成组件世界（WIT `bedcode:plugin` world）的全部导出实现
//!
//! 插件开发者只需实现 WasmPlugin trait，然后调用 wasm_entry!(MyPlugin)。
//! 宏展开为 wit-bindgen 生成的 8 组 `Guest` trait 实现 + `export!` 导出，
//! 产物为组件（component）而非旧 ABI 的 core module：
//! - 内存搬运由绑定层处理，不再有 (ptr,len) 与 alloc/dealloc 配对
//! - 契约定义在 `wit/bedcode.wit`（单一事实来源），接口漂移编译期即暴露
//!
//! 相比桌面端，移动端额外支持 on_auth_success / on_disconnect /
//! on_session_created / on_session_stopped（WS 认证/断开/会话生命周期事件）。
//!
//! 绑定生成（`wit_bindgen::generate!`）：
//! - import 接口 → `crate::wasm::bedcode::plugin::<iface>::<fn>` 自由函数，
//!   由 [`crate::wasm_host::WasmHost`] 内部调用（0.60 的 string 参数为 `&str`）
//! - export 接口 → `crate::wasm::exports::bedcode::plugin::<iface>::Guest` trait，
//!   由 `wasm_entry!` 宏对插件类型实现
//! - `pub_export_macro` 使 `export!` 可跨 crate 调用（re-export 在 `wasm` 模块，
//!   插件 crate 内经 `$crate::wasm::export!` 展开）；`default_bindings_module`
//!   指向本 SDK 的 `wasm` 模块（`$crate::wasm`），导出函数内的类型引用
//!   （`exports::bedcode::plugin::<iface>::Guest`）随宏体解析到 SDK

use crate::types::PluginManifest;

wit_bindgen::generate!({
    path: "wit/bedcode.wit",
    world: "plugin",
    pub_export_macro: true,
    default_bindings_module: "$crate::wasm",
});

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

    /// 批量传输请求钩子（v2，可选，默认 fail-closed 拒绝）
    ///
    /// 宿主在 POST /transfer-request 时调用一次（批级三路分流 allow/ask/deny），
    /// 同步阻塞握手，宿主外层 2 秒超时（复用上传钩子超时常量）。
    /// 接收策略在此实现："accept" → allow；"reject" → deny("policy-denied")；
    /// "ask"（默认）→ ask（批进入 pending 等待用户应答）。
    fn on_transfer_request(_meta: &crate::types::TransferRequestMeta) -> crate::types::UploadHookDecision {
        crate::types::UploadHookDecision::deny("plugin does not implement on_transfer_request")
    }
}

#[cfg(all(test, feature = "wasm"))]
mod tests {
    use super::*;
    use crate::types::{PluginContributes, PluginType, UploadHookDecision, UploadRequestMeta, TransferRequestMeta};
    use crate::BusMessage;

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
                plugin_type: PluginType::Wasm,
                permissions: vec![],
                contributes: PluginContributes::default(),
                icon: None,
                wasm_hash: String::new(),
                rust_library: String::new(),
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
                plugin_type: PluginType::Wasm,
                permissions: vec![],
                contributes: PluginContributes::default(),
                icon: None,
                wasm_hash: String::new(),
                rust_library: String::new(),
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

    /// 覆盖批钩子的插件：ask 模式（请求用户批准）
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
                plugin_type: PluginType::Wasm,
                permissions: vec![],
                contributes: PluginContributes::default(),
                icon: None,
                wasm_hash: String::new(),
                rust_library: String::new(),
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

        fn on_transfer_request(_meta: &TransferRequestMeta) -> UploadHookDecision {
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
    fn test_default_lifecycle_hooks_succeed() {
        // 未覆盖的启动/关闭/认证/断开/会话回调默认成功，不干扰宿主流程
        assert!(TestWasmPlugin::on_startup().is_ok());
        assert!(TestWasmPlugin::on_shutdown().is_ok());
        // 移动端特有：WebSocket 认证成功/断开/会话创建/停止
        assert!(TestWasmPlugin::on_auth_success().is_ok());
        assert!(TestWasmPlugin::on_disconnect("ws closed").is_ok());
        assert!(TestWasmPlugin::on_session_created("s1").is_ok());
        assert!(TestWasmPlugin::on_session_stopped("s1").is_ok());
    }

    #[test]
    fn test_default_bus_message_hook_succeeds() {
        let msg = BusMessage {
            topic: "t".into(),
            sender: "s".into(),
            payload: serde_json::Value::Null,
            timestamp: 0,
        };
        assert!(TestWasmPlugin::on_bus_message(&msg).is_ok());
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
        // 安全契约：未实现批钩子的插件默认拒绝一切批请求（fail-closed）
        let meta = TransferRequestMeta {
            batch_id: "b1".to_string(),
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
        // 插件可覆盖批钩子表达 ask（批进入 pending 等待用户应答）
        let meta = TransferRequestMeta {
            batch_id: "b1".to_string(),
            files: vec![],
            total_size: 0,
        };
        let decision = AskTransferPlugin::on_transfer_request(&meta);
        assert!(!decision.allow);
        assert!(decision.ask);
    }
}

/// 生成组件 world（`bedcode:plugin`）的全部导出实现
///
/// 展开为 wit-bindgen 生成的 8 组 `Guest` trait 实现（command / lifecycle /
/// events / terminal-hooks / upload-hook / transfer-request-hook / manifest /
/// abi）并调用 `export!` 导出。语义与旧 `__bedcode_*` 导出 1:1 对应（见各 impl
/// 注释）。
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

        // ==================== events（移动端子集：原 __bedcode_on_bus_message/on_auth_success/on_disconnect/on_session_created/on_session_stopped） ====================

        impl $crate::wasm::exports::bedcode::plugin::events::Guest for $plugin_type {
            fn on_bus_message(topic: String, payload: String) -> Result<(), String> {
                let payload: serde_json::Value =
                    serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
                // 移动端 WIT events.on-bus-message 无 sender 通道（宿主侧仅传 topic+payload，
                // 旧 ABI 的 sender 段随组件契约定稿裁剪）—— sender 置空、timestamp 恒 0
                let msg = $crate::BusMessage {
                    topic,
                    sender: String::new(),
                    payload,
                    timestamp: 0,
                };
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_bus_message(&msg) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_bus_message failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_auth_success() -> Result<(), String> {
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_auth_success() {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_auth_success failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_disconnect(reason: String) -> Result<(), String> {
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_disconnect(&reason) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_disconnect failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_session_created(session_id: String) -> Result<(), String> {
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_session_created(&session_id) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_session_created failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_session_stopped(session_id: String) -> Result<(), String> {
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_session_stopped(&session_id) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_session_stopped failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }
        }

        // ==================== terminal-hooks（原 __bedcode_on_terminal_input/output） ====================

        impl $crate::wasm::exports::bedcode::plugin::terminal_hooks::Guest for $plugin_type {
            fn on_terminal_input(session_id: String, text: String) -> Option<String> {
                <$plugin_type as $crate::wasm::WasmPlugin>::on_terminal_input(&session_id, &text)
            }

            fn on_terminal_output(session_id: String, data: String) -> Option<String> {
                <$plugin_type as $crate::wasm::WasmPlugin>::on_terminal_output(&session_id, &data)
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
                serde_json::to_string(&decision).unwrap_or_else(|_| {
                    r#"{"allow":false,"ask":false,"reason":"serialize decision failed"}"#.to_string()
                })
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
                serde_json::to_string(&decision).unwrap_or_else(|_| {
                    r#"{"allow":false,"ask":false,"reason":"serialize decision failed"}"#.to_string()
                })
            }
        }

        // ==================== manifest（原 __bedcode_manifest） ====================

        impl $crate::wasm::exports::bedcode::plugin::manifest::Guest for $plugin_type {
            fn get() -> String {
                serde_json::to_string(&<$plugin_type as $crate::wasm::WasmPlugin>::manifest())
                    .unwrap_or_else(|_| "{}".to_string())
            }
        }

        // ==================== abi（原 __bedcode_abi_version） ====================

        impl $crate::wasm::exports::bedcode::plugin::abi::Guest for $plugin_type {
            /// ABI 版本：语义与 `abi::ABI_VERSION`（当前 v6）完全一致。
            /// 无 form 字段：项目未发布、一次性切割，不存在 core 形态共存
            fn version() -> u32 {
                $crate::abi::ABI_VERSION as u32
            }
        }

        // ==================== 组件导出 ====================

        // 生成 #[no_mangle] 导出函数（command/lifecycle/... 全部 8 组接口的 cabi 导出）。
        // 宏展开处 `$crate` 为插件依赖的 SDK：绑定类型路径经 lib.rs 的
        // `pub use wasm::bedcode` re-export 定位（generate! 的 default_bindings_module）
        $crate::wasm::export!($plugin_type);
    };
}
