//! Mobile-to-Desktop Request Builders
//!
//! 票 04：会话控制 / 终端输入 / 配置查询已迁桌面 HTTP 面；票 13 会话控制进一步
//! 下沉插件 `com.bedcode.terminal-session`（宿主 `SessionHttpClient` /
//! `SessionManager` 已退役，前端经 `src/plugin/sessionCommands.ts`）。
//! `SessionRequest` / `TerminalRequest` / `ConfigRequest` /
//! `ResponseParser` 随之退役删除。本文件只保留认证信封构建器 `AuthRequest`——
//! 其消费者为 `ws_protocol_integration` 的 legacy 场景（真实 WsClient→router→
//! handler 链路的 WS 首消息 JWT 认证），生产路径零使用（认证已 HTTP 化）。

use crate::enums::auth::{AuthPayload, AuthStage};
use crate::model::message::Message;
use crate::state::get_global_token;

/// 获取当前全局 Token 并应用到消息
fn with_token(message: Message) -> Message {
    let token = get_global_token();
    if token.is_empty() {
        message
    } else {
        message.with_token(&token)
    }
}

// ==================== Auth Requests ====================

/// 认证相关请求构建器
pub struct AuthRequest;

impl AuthRequest {
    /// 构建 JWT Token 重新认证消息（WS 首消息 JWT 认证）
    ///
    /// 认证已 HTTP 化后，移动端正常路径不再经 WS 握手——本构造器服务集成测试
    /// 驱动真实 WsClient→router→handler 链路（`ws_protocol_integration` 的
    /// `connect_and_pair`）。
    pub fn reauthenticate(device_id: &str, fingerprint: &str, session_token: &str) -> Message {
        with_token(Message::Auth {
            message_id: uuid::Uuid::new_v4().to_string(),
            expect_response: true,
            timestamp: chrono::Utc::now().timestamp_millis(),
            session_id: None,
            token: String::new(),
            payload: AuthPayload {
                stage: AuthStage::Reauthenticate,
                device_id: Some(device_id.to_string()),
                device_fingerprint: Some(fingerprint.to_string()),
                session_token: Some(session_token.to_string()),
                ..Default::default()
            },
        })
    }
}

// ==================== Constants ====================

/// 默认超时常量
pub mod timeouts {
    use std::time::Duration;

    /// 认证请求超时
    pub const AUTH: Duration = Duration::from_secs(30);
    /// 生物认证请求超时（含系统生物识别弹窗，需更长等待）
    pub const BIO_AUTH: Duration = Duration::from_secs(120);
    /// 会话控制请求超时
    pub const SESSION_CONTROL: Duration = Duration::from_secs(15);
}
