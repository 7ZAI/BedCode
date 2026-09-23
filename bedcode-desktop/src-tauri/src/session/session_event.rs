//! Session Event Types
//!
//! 内核会话线的状态变更广播事件（`SessionManager` → 前端事件转发 / WS 终端通道）。
//!
//! 会话记录与对外视图（`SessionInfo` / `SessionInfoView`）已迁 `crate::protocol::session`
//! （票 02）；本事件只服务内核登记线的进程内广播，其消费面随票 09「事件面收口」退役。

use serde::{Deserialize, Serialize};

use crate::enums::SessionStatus;

/// 会话状态变化事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusEvent {
    pub session_id: String,
    pub old_status: Option<SessionStatus>,
    pub new_status: SessionStatus,
    pub session_name: String,
}
