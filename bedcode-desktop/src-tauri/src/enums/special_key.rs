//! Key Combo Types
//!
//! 动态按键组合解析 — 定义已迁移到 SDK `bedcode-plugin-api::wire::key`（线协议
//! 单一事实来源），此处 re-export 保持宿主侧导入路径不变；类型上的
//! `to_pty_bytes` 引擎方法随类型一并可用（宿主 PTY 写入路径
//! `pty/pty_process.rs::send_special_key` 调用点零改动）。
//!
//! 解析与转义规则的既有平行副本：`wasm-apps/terminal-session/rust/src/keys.rs`
//! （插件自带，票 06 下沉时按「宿主不代译」原则复制）。规则若变更，两处都要改
//! 或把插件切到本 SDK 定义——属后续去重项，不在本次收编范围。

pub use bedcode_plugin_api::wire::key::{KeyCode, KeyCombo};
