//! 终端链路子模块（控制帧 wire 协议 / 输出帧编码 / 订阅者执行体）
//!
//! 连接骨架见 [`crate::server::websocket::conn`]，终端通道协议见
//! [`crate::server::websocket::channel::terminal`]；本模块只保留终端链路自身的
//! wire 定义与编解码实现（帧大小上限、心跳间隔、认证超时等由骨架与路由负责）。

pub(crate) mod control_frame;
pub(crate) mod forward;
pub(crate) mod subscriber;
