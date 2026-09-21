//! WASM 插件入口（Component Model 形态，迁移阶段 B）
//!
//! WasmPlugin trait — WASM 插件核心接口
//! wasm_entry! 宏 — 生成组件世界（WIT `bedcode:plugin` world）的全部导出实现
//!
//! 插件开发者只需实现 WasmPlugin trait，然后调用 wasm_entry!(MyPlugin)。
//! 宏展开为 wit-bindgen 生成的 5 组 `Guest` trait 实现 + `export!` 导出，
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
use crate::types::PluginManifest;
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
    ///
    /// 启动初始化（建表、注册扩展点、加载资源等）在此执行；
    /// 返回 Err 时宿主将插件置为 Degraded（实例可用但启动未就绪），
    /// 不再被静默忽略。panic 由宿主捕获并按故障处理
    fn on_startup() -> anyhow::Result<()> {
        Ok(())
    }

    /// 应用即将关闭回调（可选）
    ///
    /// 返回 Err 仅记录日志（停用流程继续），不影响插件状态机
    fn on_shutdown() -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收总线消息（可选，默认忽略）
    ///
    /// `timestamp` 字段当前 ABI 未传递，值为 0（ABI v2 计划补齐）
    fn on_message(_msg: &BusMessage) -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收总线二进制消息（v11，可选，默认忽略）
    ///
    /// 订阅方须以 `host-bus.subscribe-binary` 声明二进制格式偏好，
    /// 宿主才把 `publish-binary` 载荷投递到本回调；否则按格式不匹配拒绝。
    /// `msg.payload` 为 Null、`msg.payload_binary` 携带原始字节
    /// （零 JSON 编解码，可传非 UTF-8 与大载荷）
    fn on_message_binary(_msg: &BusMessage) -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收 WS 客户端域消息帧（v14，可选，默认忽略）
    ///
    /// 由宿主 host-websocket 出站连接任务投递（`events-ws` 可选导出，宿主
    /// 实例化后动态探测）；`kind` = `"text"` | `"binary"`，`payload` 统一字节列
    /// （text 为 UTF-8 字节，零 JSON 转义）；**同一连接内按到达序投递**（保序）。
    /// 无返回值（观察型回调）：处理失败经 `host-log` 记录，不中断后续帧投递。
    /// 状态事件请另行在 activate 期订阅 `ws:open/error/close.<owner>`（host-bus）。
    fn on_ws_message(_handle: &str, _kind: &str, _payload: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }

    /// 接收 WS 服务端域（插件端点）对端消息帧（v14，可选，默认忽略）
    ///
    /// 语义同 [`WasmPlugin::on_ws_message`]，标识为端点句柄 + 对端 client-id。
    fn on_ws_client_message(
        _endpoint_id: &str,
        _client_id: &str,
        _kind: &str,
        _payload: &[u8],
    ) -> anyhow::Result<()> {
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

    /// 接收宿主并发任务事件（v20，可选，默认忽略）
    ///
    /// 由宿主 host-task 分发（`submit` 登记的任务经 `events-task` 可选导出回调，
    /// 宿主动态探测；`event_json` 为 camelCase：
    /// `{ jobId, phase: "started"|"progress"|"completed"|"failed"|"cancelled",
    ///   doneUnits?, failedUnits?, result? }`——result 仅终态携带，同 execute-batch
    ///   返回）。无返回值（观察型回调，同 `on_ws_message`）：处理失败经 host-log
    ///   记录，不影响任务执行与其余投递。
    ///
    /// **重入纪律（红线）**：本回调由宿主单线程串行投递（与其他宿主→插件导出
    /// 调用共用同一把实例锁）；**禁止在 guest 调用栈内同步等待自己任务的事件**
    /// （回调投递需要该锁，等待即自死锁）——等待语义一律走 `execute-batch`（宿主
    /// 侧 join，不经 Store）；异步任务的结果消费只能在事件回调 / 后续空闲调用里
    /// 做。回调内再 `submit` 允许（新调用、新拿锁）但受宿主每插件在册任务配额
    /// 约束，避免「回调风暴」模式。
    fn on_task_event(_event_json: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// 认证策略导出实现（v17，可选，默认**拒绝**）
    ///
    /// `auth-policy` 能力导出（`verify-device-token`，票 12 C3）：宿主 server
    /// 中间件验签后取认证中心策略（claims 结构/时效 + 信任撤销检查）。默认拒绝
    /// ——非认证中心插件不提供策略（宿主动态探测命中但不消费）；认证中心
    /// （`com.bedcode.session`）覆盖为真实策略。入参 = 宿主已验签通过的 JWT
    /// token；返回 claims JSON（放行）或错误（拒绝原因）。
    fn verify_device_token_policy(_token: &str) -> Result<String, String> {
        Err("auth-policy not provided by this plugin".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PluginContributes, PluginKind, PluginType};

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
                wasi_preopen_dirs: vec![],
                kind: PluginKind::Application,
                dependencies: vec![],
                resource_overrides: None,
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
            payload_binary: None,
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
}


/// 生成组件 world（`bedcode:plugin`）的全部导出实现
///
/// 展开为 wit-bindgen 生成的 5 组 `Guest` trait 实现（command / lifecycle /
/// events / terminal-hooks / manifest / abi）并调用 `export!`
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

        // 固化流程：四个生命周期导出的结果一律如实上抛宿主（v8 契约），
        // SDK 骨架只负责日志与错误字符串化，不吞任何失败——
        // 启动初始化失败由宿主置 Degraded，不再被静默降级为「已激活」
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

            fn on_startup() -> Result<(), String> {
                let host = $crate::wasm_host::WasmHost;
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_startup() {
                    Ok(()) => {
                        $crate::host::HostLog::log_info(&host, "Plugin startup init completed");
                        Ok(())
                    }
                    Err(e) => {
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_startup failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
            }

            fn on_shutdown() -> Result<(), String> {
                match <$plugin_type as $crate::wasm::WasmPlugin>::on_shutdown() {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let host = $crate::wasm_host::WasmHost;
                        $crate::host::HostLog::log_error(
                            &host,
                            &format!("on_shutdown failed: {}", e),
                        );
                        Err(e.to_string())
                    }
                }
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
                    payload_binary: None,
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

        // ==================== events-binary（v11，可选导出，宿主动态探测） ====================

        impl $crate::wasm_binary::exports::bedcode::plugin::events_binary::Guest for $plugin_type {
            fn on_message_binary(topic: String, sender: String, payload: Vec<u8>) {
                // 字节列 → 类型化 BusMessage（payload 为 Null，payload_binary 携带原始字节）；
                // 无返回值：处理失败经 host-log 记录（观察型回调，语义同 terminal-hooks）
                let msg = $crate::BusMessage {
                    topic,
                    sender,
                    payload: serde_json::Value::Null,
                    payload_binary: Some(payload),
                    timestamp: 0,
                };
                if let Err(e) = <$plugin_type as $crate::wasm::WasmPlugin>::on_message_binary(&msg) {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(
                        &host,
                        &format!("on_message_binary failed: {}", e),
                    );
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

        // ==================== manifest（原 __bedcode_manifest） ====================

        impl $crate::wasm::exports::bedcode::plugin::manifest::Guest for $plugin_type {
            fn get() -> String {
                serde_json::to_string(&<$plugin_type as $crate::wasm::WasmPlugin>::manifest())
                    .unwrap_or_else(|_| "{}".to_string())
            }
        }

        // ==================== abi（原 __bedcode_abi_version + form 形态字段） ====================

        impl $crate::wasm::exports::bedcode::plugin::abi::Guest for $plugin_type {
            /// ABI 版本：语义与 `abi::ABI_VERSION` 完全一致
            fn version() -> u32 {
                $crate::abi::ABI_VERSION as u32
            }

            /// 产物形态：组件（Component Model），宿主按 `form()==1` 识别
            fn form() -> u32 {
                $crate::abi::FORM_COMPONENT as u32
            }
        }

        // ==================== events-ws（v14，可选导出，宿主动态探测） ====================

        impl $crate::wasm_ws::exports::bedcode::plugin::events_ws::Guest for $plugin_type {
            /// 客户端域消息帧（handle = `wsc-<uuid>`）
            fn on_message(handle: String, kind: String, payload: Vec<u8>) {
                // 无返回值（观察型回调）：处理失败经 host-log 记录，宿主不断开连接
                if let Err(e) =
                    <$plugin_type as $crate::wasm::WasmPlugin>::on_ws_message(&handle, &kind, &payload)
                {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(&host, &format!("on_ws_message failed: {}", e));
                }
            }

            /// 服务端域消息帧（endpoint-id = `wse-<uuid>`，client-id 为对端连接 id）
            fn on_client_message(endpoint_id: String, client_id: String, kind: String, payload: Vec<u8>) {
                if let Err(e) = <$plugin_type as $crate::wasm::WasmPlugin>::on_ws_client_message(
                    &endpoint_id,
                    &client_id,
                    &kind,
                    &payload,
                ) {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(&host, &format!("on_ws_client_message failed: {}", e));
                }
            }
        }

        // ==================== auth-policy（v17，可选导出，宿主动态探测） ====================

        impl $crate::wasm_auth_policy::exports::bedcode::plugin::auth_policy::Guest for $plugin_type {
            /// 认证策略裁决：宿主已验签 → 取插件策略（默认拒绝——非认证中心
            /// 插件不提供策略；宿主动态探测命中但不消费）
            fn verify_device_token(token: String) -> Result<String, String> {
                <$plugin_type as $crate::wasm::WasmPlugin>::verify_device_token_policy(&token)
            }
        }

        // ==================== events-task（v20，可选导出，宿主动态探测） ====================

        impl $crate::wasm_task::exports::bedcode::plugin::events_task::Guest for $plugin_type {
            /// 宿主并发任务进度/终态回调（event-json 为 camelCase，见
            /// [`WasmPlugin::on_task_event`]）：无返回值（观察型回调）；处理失败经
            /// host-log 记录，宿主仅 trap 时 error! 并计数，不影响任务执行与其余投递
            fn on_task_event(event_json: String) {
                if let Err(e) = <$plugin_type as $crate::wasm::WasmPlugin>::on_task_event(&event_json) {
                    let host = $crate::wasm_host::WasmHost;
                    $crate::host::HostLog::log_error(&host, &format!("on_task_event failed: {}", e));
                }
            }
        }

        // ==================== 组件导出 ====================

        // 生成 #[no_mangle] 导出函数（command/lifecycle/... 全部 5 组接口的 cabi 导出）。
        // 宏展开处 `$crate` 为插件依赖的 SDK：绑定类型路径经 lib.rs 的
        // `pub use wasm::bedcode` re-export 定位（generate! 的 default_bindings_module）
        $crate::wasm::export!($plugin_type);
        // v11：events-binary 可选导出的 cabi 导出（宿主动态探测，非 world 必选）
        $crate::wasm_binary::export!($plugin_type);
        // v14：events-ws 可选导出的 cabi 导出（宿主动态探测，非 world 必选）
        $crate::wasm_ws::export!($plugin_type);
        // v17：auth-policy 可选导出的 cabi 导出（宿主动态探测，非 world 必选）
        $crate::wasm_auth_policy::export!($plugin_type);
        // v20：events-task 可选导出的 cabi 导出（宿主动态探测，非 world 必选）
        $crate::wasm_task::export!($plugin_type);
    };
}
