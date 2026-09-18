//! WS 通道处理器实现
//!
//! 骨架（`server::ws::conn`）负责连接生命周期，通道负责协议语义。
//! 阶段 A 落地两个实现：
//!
//! - [`terminal`]：终端路由 `/ws/terminal/session/{id}` 控制帧协议；
//! - [`event`]：事件通道 `/ws/event` 旧 `Message` 协议兼容面；
//! - [`plugin`]：插件端点 `/ws/plugin/{plugin_id}/{path}`（宿主零业务语义，
//!   认证策略由端点声明，帧转投属主插件）。
//!
//! 新增通道 = 新增一个 `ChannelHandler` 实现 + 路由构造点，不改骨架
//! （阶段 A 抽取骨架的全部意义所在，spec §3.2 A1）。

pub mod event;
pub mod plugin;
pub mod terminal;
