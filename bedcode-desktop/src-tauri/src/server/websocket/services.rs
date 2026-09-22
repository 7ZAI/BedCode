//! WS 面服务（临时住处，非 WS 传输原语）
//!
//! 唯一消费者是 `websocket/channel/*`（event / terminal 两通道的输入与控制分发）；
//! HTTP 面对它零调用。名字里的「服务」是既有错位：这里承载的是会话控制与终端输入
//! 业务，不是 WS 传输能力——按 ADR 0022 裁剪线归属应为会话业务，后续随
//! host-business-decarriage 线下沉插件。**勿把它当 WS 原语扩展**（新 WS 能力
//! 进 `conn` / `channel` / `subscription` 等传输层件）。

pub mod session_control;
pub mod terminal_service;
