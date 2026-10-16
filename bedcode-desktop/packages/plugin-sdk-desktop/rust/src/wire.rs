//! 跨端线协议类型（单一事实源）
//!
//! 宿主 `bedcode-desktop/src-tauri/src/enums/` 与移动端
//! `bedcode-mobile/src-tauri/src/enums/` 里的会话同步 / WS 控制形状收编到此处：
//! 改字段名只改本模块一处，宿主与插件编译期同步（会话引擎下沉专项票 01）。
//!
//! **口径边界**（ADR 0022 裁剪线）：
//! - 本模块只定义**形状**，不含业务解释。
//! - 认证线协议（`enums/auth.rs`）与会话记录 / 状态机线协议
//!   （宿主 `protocol/session.rs`）不在本模块：前者另线演进，后者是宿主传输面
//!   自有形状。
//! - 移动端保留平行副本（双端契约分叉先例，ADR 0018/0019 口径），不要求
//!   移动端直接依赖桌面 SDK crate。
//!
//! **websocket 业务下沉票 08 退役**：`sync`（`SyncPayload`）与 `control`
//! （`SessionControlAction` / `TerminalAction`）两个子模块已删除——宿主
//! `/ws/event` 旧 `Message` 业务协议整体退役，插件 wire 面不再有宿主同步载荷
//! 与 WS 控制/终端帧形状。本模块只剩仍在生产路径的类型：`summary`
//! （`SessionSummary`，插件 session-control 端点响应形状）与 `key`
//! （`KeyCombo` / `KeyCode`，宿主 `pty_process::send_special_key` 引擎翻译面）。

pub mod key;
pub mod summary;

pub use key::{KeyCode, KeyCombo};
pub use summary::SessionSummary;