//! Control Types
//!
//! 会话控制与终端消息类型 — 定义已迁移到 SDK `bedcode-plugin-api::wire::control`
//! （线协议单一事实来源），此处 re-export 保持宿主侧导入路径不变。
//!
//! 宿主仍持有 `Message::{SessionControl, Terminal}` 枚举（传输面），但**只转发不
//! 解动作语义**（ADR 0022 裁剪线）；全变体 serde 往返与 type 标签锁见 SDK。

pub use bedcode_plugin_api::wire::control::{
    SessionControlAction, SessionControlPayload, TerminalAction, TerminalPayload,
};
