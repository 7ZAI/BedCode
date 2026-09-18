//! BedCode Plugin API
//!
//! 插件系统核心接口和共享类型定义。
//! 插件 crate 依赖此 crate 实现 `BedcodePlugin` trait，
//! 主应用通过 `inventory::collect()` 收集所有静态注册的插件。
//!
//! 启用 `wasm` feature 后，额外提供 `WasmPlugin` trait 和 `wasm_entry!` 宏，
//! 用于编译为 WASM 模块的插件。

pub mod abi;
pub mod args;
pub mod command;
pub mod constants;
pub mod context;
pub mod events;
pub mod host;
pub mod http_response;
pub mod permission;
pub mod sql;
pub mod terminal;
pub mod traits;
pub mod types;

/// 消息总线消息
///
/// 插件间通信的统一消息封装，通过 Topic 消息总线传递
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BusMessage {
    /// 消息主题（格式：domain:action，如 task:status-changed）
    pub topic: String,
    /// 发送者插件 ID
    pub sender: String,
    /// 消息负载（任意 JSON）——二进制消息此字段恒为 Null
    pub payload: serde_json::Value,
    /// 二进制负载（v11，仅 `publish-binary` 消息）：零 JSON 编解码，
    /// 可传非 UTF-8 字节与大载荷（MB 级）；JSON 消息此字段为 None。
    /// 消费方以 `payload_binary.is_some()` 区分载荷格式
    #[serde(default)]
    pub payload_binary: Option<Vec<u8>>,
    /// 时间戳（毫秒 Unix）
    pub timestamp: u64,
}

#[cfg(feature = "wasm")]
pub mod api_call;
#[cfg(feature = "wasm")]
#[cfg(feature = "wasm")]
pub mod wasm;
#[cfg(feature = "wasm")]
pub mod wasm_binary;
#[cfg(feature = "wasm")]
pub mod wasm_host;
#[cfg(feature = "wasm")]
pub mod wasm_ws;
// 组件导出宏（wasm_entry! 内 export!）的绑定类型路径：generate! 的
// default_bindings_module 指向 `$crate`，绑定模块树在 wasm.rs 下，
// 此处 re-export 到 crate 根使 `$crate::bedcode::plugin::<iface>::Guest` 可解析
#[cfg(feature = "wasm")]
pub use wasm::bedcode;

pub use args::CommandArgs;
pub use command::{PluginCommand, PluginCommandEntry};
pub use context::RustPluginContext;
pub use events::{InputSubmittedEvent, PluginQuestion, PluginQuestionOption, SessionLifecycleEvent, SyncEvent};
pub use host::{ConfigKey, HostApi, HostError};
pub use permission::PermissionManager;
pub use terminal::TerminalHandler;
pub use traits::{BedcodePlugin, BedcodePluginEntry};
pub use types::*;

#[cfg(test)]
pub(crate) mod test_utils;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_message_serde_round_trip() {
        // 总线消息线协议：topic/sender/payload/payload_binary/timestamp 五字段
        // （payload_binary 为 v11 增量，serde(default) 保证老端 JSON 缺字段可解析）
        let msg = BusMessage {
            topic: "task:status-changed".to_string(),
            sender: "com.bedcode.demo".to_string(),
            payload: serde_json::json!({ "status": "in_progress" }),
            payload_binary: None,
            timestamp: 1700000000123,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "topic": "task:status-changed",
                "sender": "com.bedcode.demo",
                "payload": { "status": "in_progress" },
                "payload_binary": null,
                "timestamp": 1700000000123_i64
            })
        );
        // 老端形状（无 payload_binary 字段）仍可反序列化：增量字段演进原则
        let legacy = serde_json::json!({
            "topic": "task:status-changed",
            "sender": "com.bedcode.demo",
            "payload": { "status": "in_progress" },
            "timestamp": 1700000000123_i64
        });
        let from_legacy: BusMessage = serde_json::from_value(legacy).unwrap();
        assert_eq!(from_legacy.payload_binary, None);

        let back: BusMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back.topic, "task:status-changed");
        assert_eq!(back.sender, "com.bedcode.demo");
        assert_eq!(back.timestamp, 1700000000123);
        assert_eq!(back.payload, serde_json::json!({ "status": "in_progress" }));
        assert_eq!(back.payload_binary, None);
    }

    #[test]
    fn test_bus_message_rejects_unknown_topic_type() {
        // 线协议锁死：topic 必须是字符串，数字载荷应反序列化失败
        let bad = serde_json::json!({ "topic": 42, "sender": "s", "payload": {}, "timestamp": 0 });
        assert!(serde_json::from_value::<BusMessage>(bad).is_err());
    }
}

#[cfg(feature = "wasm")]
pub use wasm::WasmPlugin;
#[cfg(feature = "wasm")]
pub use wasm_host::WasmHost;
// 插件互调 IDL 属性宏（rust-macros crate；经本 crate 再导出，插件侧
// 以 `#[bedcode_plugin_api::plugin_api]` 引用，无需额外依赖）
#[cfg(feature = "wasm")]
pub use bedcode_plugin_api_macros::plugin_api;
