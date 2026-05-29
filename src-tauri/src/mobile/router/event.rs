//! Mobile Event - 业务事件定义
//!
//! 发送给前端的业务事件

use serde::{Deserialize, Serialize};
use crate::shared::enums::sumary::{SessionConfigSummary, SessionSummary};

/// Mobile 业务事件（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MobileEvent {
    /// 连接成功
    Connected,
    /// 断开连接
    Disconnected,
    /// 收到输出
    Output {
        session_id: String,
        data: String,
        is_waiting: bool,
        /// 全局递增索引，用于去重
        index: usize,
    },
    /// 订阅响应
    SubscribeResponse {
        session_id: String,
        min_seq: u64,
        max_seq: u64,
        history_count: usize,
    },
    /// 取消订阅响应
    UnsubscribeResponse {
        session_id: String,
    },
    /// 认证成功
    AuthSuccess {
        device_id: String,
        session_token: String,
    },
    /// 认证失败
    AuthFailed {
        reason: String,
    },
    /// 配对请求
    PairingRequest {
        device_name: String,
    },
    /// 配对码验证
    PairingVerified,
    /// 错误
    Error {
        message: String,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
    /// 确认响应（服务端默认响应）
    Ack {
        request_id: String,
    },

    // === 同步数据事件 ===
    /// 会话创建同步
    SyncSessionCreated {
        session: SessionSummary,
        source_device: String,
    },
    /// 会话状态变化同步
    SyncSessionStatusChanged {
        session_id: String,
        old_status: String,
        new_status: String,
        session_name: String,
    },
    /// 会话停止同步
    SyncSessionStopped {
        session_id: String,
        session_name: String,
    },
    /// 会话删除同步
    SyncSessionRemoved {
        session_id: String,
        session_name: String,
    },
    /// 配置创建同步
    SyncConfigCreated {
        config: SessionConfigSummary,
        source_device: String,
    },
    /// 配置更新同步
    SyncConfigUpdated {
        config: SessionConfigSummary,
        source_device: String,
    },
    /// 配置删除同步
    SyncConfigRemoved {
        config_id: String,
        config_name: String,
    },
}