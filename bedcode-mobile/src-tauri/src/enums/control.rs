//! Control Types
//!
//! 会话控制消息类型（票 04：信封协议退役后仅保留仍有消费者的部分）
//!
//! 已退役删除：`SessionConfig*`（配置查询迁 HTTP `GET /api/configs`）、
//! `Terminal*`（终端流迁票 05 新协议）、`SubscribeMode`（终端订阅随旧协议退役）、
//! `SessionControlAction` 的 `ResizeSession`（resize 走 HTTP）/ `JoinSession` /
//! `LeaveSession` / `SessionChanged`（旧会话流控制帧，桌面端点已删）。

use serde::{Deserialize, Serialize};

// ==================== Session Control ====================

/// 会话控制载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionControlPayload {
    /// 控制动作
    pub action: SessionControlAction,
}

/// 会话控制动作
///
/// 保留变体仅服务 `ws_protocol_integration` 的 legacy 场景（请求-响应匹配 /
/// token 注入断言）；会话控制生产调用面已迁 HTTP（`session::http`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionControlAction {
    /// 列出会话
    ListSessions,
    /// 启动会话
    StartSession { config_id: String },
    /// 停止会话
    StopSession { session_id: String },
    /// 删除会话
    RemoveSession { session_id: String },
}
